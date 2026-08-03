//! Confluent-compatible Schema Registry client (docs/ARCHITECTURE.md D4).
//!
//! Two directions, and they are deliberately not symmetric:
//!
//! - **Decode** ([`SchemaRegistry::lookup`]) resolves a schema id off a
//!   message's framing, plus the subject/version shown beside it.
//! - **Encode** ([`SchemaRegistry::latest_schema`]) resolves a subject's latest
//!   schema, which is what a produce writes against (Phase 2).
//!
//! Apicurio and AWS Glue arrive behind the same shape later; Apicurio ships a
//! Confluent-compatible API surface, so this client already reaches it.
//!
//! # Degradation is the design of the decode path, not of both paths
//!
//! A registry the app cannot reach must not stop a user browsing a topic. Every
//! failure in `lookup` returns `None`, the caller falls back to hex **with the
//! schema id still attached**, and the reason is recorded once per session
//! ([`SchemaRegistry::note_error`]) rather than once per message — a
//! 2000-message fetch against a wedged registry is one line in the log, not two
//! thousand.
//!
//! Failures are cached alongside successes for the same reason: without that,
//! every message in the fetch pays a fresh 10-second connect timeout.
//!
//! **`latest_schema` errors instead.** There is no honest degradation for
//! "encode this against a schema I could not fetch": guessing would put bytes
//! on a topic that no consumer of that subject can read, which is worse than
//! not producing at all. Its failures are also not cached — a produce is one
//! call, not two thousand, and the user who fixes the registry and presses
//! send again must not be answered from a cache of the outage.
//!
//! Secret discipline (D5): the password is resolved from the OS keychain here,
//! is folded immediately into an `Authorization` header value, never appears in
//! an error message (the registry URL carries no credentials), and is redacted
//! from `Debug`.

use crate::connection::ClusterConnection;
use crate::profiles::SchemaRegistryConfig;
use crate::secrets;
use crate::Error;
use apache_avro::Schema as AvroSchema;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};

/// A registry lookup happens inline in the decode path, so a wedged registry
/// must not hold a fetch open for longer than the fetch's own budget.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// The registry's own media type, with plain JSON as the fallback every
/// Confluent-compatible implementation also answers.
const ACCEPT: &str = "application/vnd.schemaregistry.v1+json, application/json";

/// What the registry wants on a body it is about to parse. Confluent accepts
/// `application/json` too, but Apicurio's Confluent-compatible endpoint is
/// stricter, so the registry's own media type is what goes on the wire.
const CONTENT_TYPE: &str = "application/vnd.schemaregistry.v1+json";

/// Cap on how much of a registry error body is quoted back.
const ERROR_BODY_LIMIT: usize = 200;

/// The schema types this build will send to a registry. Checked before the
/// request so a typo comes back as "Kavka doesn't know that one, here are the
/// three" rather than as a 422 from three network hops away.
///
/// Kavka can *register* a Protobuf or JSON-Schema schema without being able to
/// *decode* one: registering is a text upload, and the registry is the thing
/// that validates it. That asymmetry with [`SchemaKind`] is deliberate — a user
/// who has to switch tools to add a version to a Protobuf subject has no idea
/// why.
pub const SCHEMA_TYPES: [&str; 3] = ["AVRO", "JSON", "PROTOBUF"];

/// The compatibility levels a Confluent-compatible registry accepts.
pub const COMPATIBILITY_LEVELS: [&str; 7] = [
    "BACKWARD",
    "BACKWARD_TRANSITIVE",
    "FORWARD",
    "FORWARD_TRANSITIVE",
    "FULL",
    "FULL_TRANSITIVE",
    "NONE",
];

/// What a schema id turned out to be. Only Avro needs the schema itself to
/// decode a payload; a JSON-Schema subject frames plain JSON, and Protobuf
/// needs a descriptor pool this build does not carry yet.
#[derive(Debug)]
pub enum SchemaKind {
    Avro(Box<AvroSchema>),
    Json,
    /// The registry's `schemaType` verbatim, for a message that names it.
    Unsupported(String),
}

/// One resolved schema id: how to read the payload, plus the subject and
/// version the inspector shows beside it (docs/DESIGN.md §5.6, the schema chip
/// `orders-value · v4 · id 217`).
pub struct RegisteredSchema {
    pub subject: Option<String>,
    pub version: Option<i32>,
    pub kind: SchemaKind,
}

/// A subject's latest registered schema: what a produce encodes against, and
/// the id that goes into the framing so a consumer can find it again.
///
/// Every field is known here, unlike [`RegisteredSchema`] — the lookup went the
/// other way, from a subject the user named to the id the registry assigned.
#[derive(Debug)]
pub struct SubjectSchema {
    pub subject: String,
    pub schema_id: u32,
    pub version: i32,
    pub kind: SchemaKind,
}

// ---------------------------------------------------------------------------
// The management surface (Phase 3a). Field names are the IPC contract — the
// TypeScript in apps/desktop/src mirrors them exactly, so renaming one is a
// breaking change on both sides of the bridge.
// ---------------------------------------------------------------------------

/// One registered version of a subject, **with its schema text** — which is
/// what separates this from the decode path's id→subject lookup: the version
/// browser diffs two of these against each other, so the text is the payload,
/// not a detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectVersion {
    pub subject: String,
    pub version: i32,
    pub schema_id: u32,
    /// `AVRO` when the registry omits it, which is Confluent's default.
    pub schema_type: String,
    pub schema: String,
}

/// The registry's verdict on a candidate schema.
///
/// `messages` is the registry's own prose, verbatim and unsummarised: it names
/// the field and the rule that broke ("reader field X is missing a default
/// value"), and no wording Kavka could invent would be more useful than that.
/// Empty is normal — a registry old enough to ignore `?verbose=true` answers
/// with the boolean alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityCheck {
    pub compatible: bool,
    pub messages: Vec<String>,
}

/// The id a registration produced — the number that goes into the wire framing
/// so a consumer can find the schema again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredId {
    pub schema_id: u32,
}

/// The compatibility level actually in force, and where it came from.
///
/// **`inherited` is the whole reason this is not a bare `String`.** "This
/// subject is set to BACKWARD" and "this subject follows a registry default
/// that happens to be BACKWARD" are different facts, and the difference is the
/// one a user acts on: setting a level on the first changes one subject,
/// setting one on the second stops it tracking the default forever. A client
/// that only gets the word back cannot tell them apart, so it cannot say either
/// — which is how a UI ends up with a "uses the registry default" line that can
/// never render.
///
/// `inherited` is false for the global query itself: the registry-wide default
/// inherits from nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityInForce {
    pub level: String,
    pub inherited: bool,
}

/// A Schema Registry, scoped to one consume/tail session.
///
/// The cache is per-session on purpose: a session is short, schemas are
/// immutable once registered, and a process-wide cache would need invalidation
/// machinery for no gain.
pub struct SchemaRegistry {
    base_url: String,
    /// The complete `Authorization` header value, built once. Redacted in
    /// `Debug` — it embeds the registry password.
    authorization: Option<String>,
    /// `None` marks an id we already failed to resolve this session, so a
    /// failing registry costs one round trip per id rather than one per
    /// message.
    cache: Mutex<HashMap<u32, Option<Arc<RegisteredSchema>>>>,
    /// The encode side's cache, keyed by subject. Successes only — see the
    /// module docs.
    subjects: Mutex<HashMap<String, Arc<SubjectSchema>>>,
    first_error: Mutex<Option<String>>,
}

impl SchemaRegistry {
    /// Builds a client for a profile's registry. Never fails: a keychain that
    /// cannot produce the password is recorded as the session's error and the
    /// client goes on unauthenticated, which is exactly what the user needs to
    /// see — "the registry rejected these credentials" — rather than a
    /// connection that refuses to start.
    pub fn new(config: &SchemaRegistryConfig) -> Self {
        let mut registry = Self::unauthenticated(&config.url);
        let Some(username) = config.username.as_deref() else {
            return registry;
        };
        let password = match &config.password {
            Some(secret) => match secrets::resolve(secret) {
                Ok(password) => password,
                Err(e) => {
                    registry.note_error(format!("schema registry credentials: {e}"));
                    return registry;
                }
            },
            // An API key with no secret half is legal (some registries take the
            // key as the username alone).
            None => String::new(),
        };
        registry.authorization = Some(basic_auth(username, &password));
        registry
    }

    fn unauthenticated(url: &str) -> Self {
        Self {
            base_url: url.trim_end_matches('/').to_string(),
            authorization: None,
            cache: Mutex::new(HashMap::new()),
            subjects: Mutex::new(HashMap::new()),
            first_error: Mutex::new(None),
        }
    }

    /// Resolves a schema id, or `None` if it cannot be resolved at all — the
    /// caller then renders hex and keeps the id.
    pub fn lookup(&self, schema_id: u32) -> Option<Arc<RegisteredSchema>> {
        if let Some(cached) = self.cache().get(&schema_id) {
            return cached.clone();
        }
        let resolved = self.fetch(schema_id).map(Arc::new);
        self.cache().insert(schema_id, resolved.clone());
        resolved
    }

    /// The schema a produce should encode against: `GET
    /// /subjects/{subject}/versions/latest`.
    ///
    /// **Errors rather than degrading**, unlike [`lookup`](Self::lookup) — see
    /// the module docs. The message always names the subject and, when the
    /// registry answered at all, quotes what it said, so "the registry is
    /// down" and "there is no such subject" are never the same sentence.
    ///
    /// "Latest" is read once per registry instance and then cached. A produce
    /// builds its own registry, so a run never encodes against a schema that
    /// was superseded before it started — and a bulk run never re-fetches per
    /// record.
    pub fn latest_schema(&self, subject: &str) -> crate::Result<Arc<SubjectSchema>> {
        if let Some(cached) = self.subjects().get(subject) {
            return Ok(Arc::clone(cached));
        }
        let path = format!("/subjects/{}/versions/latest", path_segment(subject));
        let body = self.get(&path).map_err(|cause| {
            Error::Other(format!(
                "schema registry at {} could not give Kavka the latest schema for {subject}: \
                 {cause}{}",
                self.base_url,
                // A keychain failure recorded at construction is the real
                // cause of an HTTP 401, and the only one the user can act on.
                self.error()
                    .map_or_else(String::new, |noted| format!(" — {noted}"))
            ))
        })?;

        let response: VersionResponse = serde_json::from_str(&body).map_err(|e| {
            Error::Other(format!(
                "schema registry at {} returned an unexpected body for {subject}: {e}",
                self.base_url
            ))
        })?;
        let kind = match response.schema_type.as_deref() {
            // `schemaType` is omitted for Avro (Confluent's default).
            None | Some("AVRO") => SchemaKind::Avro(Box::new(
                AvroSchema::parse_str(&response.schema).map_err(|e| {
                    Error::Other(format!(
                        "{subject} version {} is registered as Avro but did not parse: {e}",
                        response.version
                    ))
                })?,
            )),
            Some("JSON") => SchemaKind::Json,
            Some(other) => SchemaKind::Unsupported(other.to_string()),
        };

        let resolved = Arc::new(SubjectSchema {
            subject: response.subject,
            schema_id: response.id,
            version: response.version,
            kind,
        });
        self.subjects()
            .insert(subject.to_string(), Arc::clone(&resolved));
        Ok(resolved)
    }

    // -----------------------------------------------------------------------
    // The management surface (Phase 3a).
    //
    // Nothing here caches. The decode path caches because it asks the same
    // question two thousand times in a fetch; this path answers a person who
    // just pressed a button, and an answer out of a cache is how a user sees
    // the version they registered ten seconds ago missing from the list.
    // -----------------------------------------------------------------------

    /// Every registered version of a subject, with its schema text.
    ///
    /// `GET /subjects/{s}/versions` gives the numbers only, so this is one more
    /// round trip per version. Deliberate: the text is the point — the version
    /// browser diffs consecutive versions against each other — and a list that
    /// made the caller fetch each one would just move the same N requests
    /// somewhere with less context to report a failure from.
    ///
    /// Returned in the registry's own order, oldest first.
    pub fn subject_versions(&self, subject: &str) -> crate::Result<Vec<SubjectVersion>> {
        let listing = self
            .call(
                Method::Get,
                &format!("/subjects/{}/versions", path_segment(subject)),
                None,
            )
            .map_err(|failure| {
                if failure.is_unknown_subject() {
                    Error::Other(format!(
                        "the registry has no subject named {subject} — check the name, or \
                         register its first version"
                    ))
                } else {
                    self.refused(&format!("list the versions of {subject}"), &failure)
                }
            })?;
        let numbers: Vec<i32> = serde_json::from_str(&listing).map_err(|e| {
            Error::Other(format!(
                "schema registry at {} returned an unexpected version list for {subject}: {e}",
                self.base_url
            ))
        })?;

        let mut versions = Vec::with_capacity(numbers.len());
        for version in numbers {
            let body = self
                .call(
                    Method::Get,
                    &format!("/subjects/{}/versions/{version}", path_segment(subject)),
                    None,
                )
                .map_err(|failure| {
                    self.refused(&format!("read {subject} version {version}"), &failure)
                })?;
            let response: VersionResponse = serde_json::from_str(&body).map_err(|e| {
                Error::Other(format!(
                    "schema registry at {} returned an unexpected body for {subject} version \
                     {version}: {e}",
                    self.base_url
                ))
            })?;
            versions.push(SubjectVersion {
                subject: response.subject,
                version: response.version,
                schema_id: response.id,
                // Confluent omits `schemaType` for Avro; the UI must never have
                // to know that.
                schema_type: response.schema_type.unwrap_or_else(|| "AVRO".to_string()),
                schema: response.schema,
            });
        }
        Ok(versions)
    }

    /// Asks the registry whether a candidate schema may follow the subject's
    /// current latest — `POST /compatibility/subjects/{s}/versions/latest`.
    ///
    /// **A subject with no versions is compatible, not an error.** The registry
    /// answers 404 / `40401` there, and treating that as a failure would make
    /// registering a subject's *first* version impossible through this path —
    /// which is the most common registration there is.
    ///
    /// `?verbose=true` is what turns "false" into a reason. A registry old
    /// enough to ignore the parameter answers the boolean alone and
    /// `messages` comes back empty; nothing here invents prose to fill it.
    pub fn check_compatibility(
        &self,
        subject: &str,
        schema: &str,
        schema_type: &str,
    ) -> crate::Result<CompatibilityCheck> {
        let schema_type = normalize_schema_type(schema_type)?;
        let payload = schema_payload(schema, &schema_type);
        let path = format!(
            "/compatibility/subjects/{}/versions/latest?verbose=true",
            path_segment(subject)
        );

        let body = match self.call(Method::Post, &path, Some(&payload)) {
            Ok(body) => body,
            Err(failure) if failure.is_unknown_subject() => {
                return Ok(CompatibilityCheck {
                    compatible: true,
                    messages: vec![format!(
                        "{subject} has no registered versions yet, so there is nothing for this \
                         schema to be incompatible with."
                    )],
                })
            }
            Err(failure) if failure.is_status(422) => {
                return Err(Error::Other(format!(
                    "the registry rejected this schema as invalid: {}",
                    failure.detail
                )))
            }
            Err(failure) => {
                return Err(self.refused(&format!("check a schema against {subject}"), &failure))
            }
        };

        let response: CompatibilityResponse = serde_json::from_str(&body).map_err(|e| {
            Error::Other(format!(
                "schema registry at {} returned an unexpected compatibility verdict for \
                 {subject}: {e}",
                self.base_url
            ))
        })?;
        Ok(CompatibilityCheck {
            compatible: response.is_compatible,
            messages: response.messages.unwrap_or_default(),
        })
    }

    /// Registers a schema under a subject — `POST /subjects/{s}/versions`.
    ///
    /// **Checks compatibility first, in core.** The UI checks too, but the two
    /// are not the same guarantee: the UI's check is a preview the user may
    /// have run five minutes and one concurrent registration ago, and this one
    /// is on the same code path as the write. The registry's own 409 is still
    /// mapped, because between the check and the POST is a window nothing can
    /// close.
    ///
    /// Private: the only public door is [`register`], which gates on
    /// [`ClusterConnection::ensure_writable`] first (D5).
    fn register_schema(
        &self,
        subject: &str,
        schema: &str,
        schema_type: &str,
    ) -> crate::Result<RegisteredId> {
        let schema_type = normalize_schema_type(schema_type)?;
        let check = self.check_compatibility(subject, schema, &schema_type)?;
        if !check.compatible {
            return Err(Error::Other(format!(
                "the registry rejected this schema as incompatible with {subject}{}",
                quote_messages(&check.messages)
            )));
        }

        let payload = schema_payload(schema, &schema_type);
        let body = self
            .call(
                Method::Post,
                &format!("/subjects/{}/versions", path_segment(subject)),
                Some(&payload),
            )
            .map_err(|failure| {
                if failure.is_status(409) {
                    Error::Other(format!(
                        "the registry rejected this schema as incompatible with {subject}: {}",
                        failure.detail
                    ))
                } else if failure.is_status(422) {
                    Error::Other(format!(
                        "the registry rejected this schema as invalid: {}",
                        failure.detail
                    ))
                } else {
                    self.refused(&format!("register a schema for {subject}"), &failure)
                }
            })?;

        let response: RegisterResponse = serde_json::from_str(&body).map_err(|e| {
            Error::Other(format!(
                "schema registry at {} accepted the schema for {subject} but did not say what id \
                 it got: {e}",
                self.base_url
            ))
        })?;
        Ok(RegisteredId {
            schema_id: response.id,
        })
    }

    /// The compatibility level in force — `GET /config/{subject}` for a
    /// subject, `GET /config` for the global default.
    ///
    /// A subject with no override of its own falls back to the global level,
    /// because that is the level that actually applies to it — but it comes
    /// back **marked** ([`CompatibilityInForce::inherited`]) rather than as an
    /// indistinguishable string. Reporting "no level" instead is not an option
    /// either: it reads as "anything goes", which is the opposite of the truth.
    ///
    /// **`?defaultToGlobal=true` is deliberately not sent.** It asks the
    /// registry to do this fallback itself, which answers 200 with the global
    /// level and destroys the only signal there is: a plain `GET
    /// /config/{subject}` 404s (`40408`) for a subject with no setting of its
    /// own, and that 404 *is* the "inherits" answer. Doing the fallback here
    /// costs one extra round trip on a subject that has no override and works
    /// the same on every registry, old or new.
    pub fn compatibility(&self, subject: Option<&str>) -> crate::Result<CompatibilityInForce> {
        let Some(subject) = subject else {
            // The registry-wide default inherits from nothing.
            return Ok(CompatibilityInForce {
                level: self.global_level()?,
                inherited: false,
            });
        };

        match self.call(
            Method::Get,
            &format!("/config/{}", path_segment(subject)),
            None,
        ) {
            Ok(body) => Ok(CompatibilityInForce {
                level: self.level_named_by(&body)?,
                inherited: false,
            }),
            // A 404 here is the registry saying "this subject has no setting of
            // its own" — a state, not a failure. Any 404 counts: registries
            // disagree on the code (40401 vs 40408) and some send no envelope at
            // all, and the fallback is correct under every one of them.
            Err(failure) if failure.is_status(404) => Ok(CompatibilityInForce {
                level: self.global_level()?,
                inherited: true,
            }),
            Err(failure) => Err(self.refused(
                &format!("read the compatibility level of {subject}"),
                &failure,
            )),
        }
    }

    /// `GET /config` — the level every subject without one of its own follows.
    fn global_level(&self) -> crate::Result<String> {
        let body = self
            .call(Method::Get, "/config", None)
            .map_err(|failure| self.refused("read the global compatibility level", &failure))?;
        self.level_named_by(&body)
    }

    /// The level a `/config` body names, whichever of the two spellings in the
    /// wild it used.
    fn level_named_by(&self, body: &str) -> crate::Result<String> {
        let config: CompatibilityConfig = serde_json::from_str(body).map_err(|e| {
            Error::Other(format!(
                "schema registry at {} returned an unexpected compatibility config: {e}",
                self.base_url
            ))
        })?;
        config.level().ok_or_else(|| {
            Error::Other(format!(
                "schema registry at {} answered without naming a compatibility level",
                self.base_url
            ))
        })
    }

    /// Sets the compatibility level — `PUT /config/{subject}`, or `PUT /config`
    /// for the global default.
    ///
    /// Private: the only public door is [`set_compatibility`], which gates on
    /// [`ClusterConnection::ensure_writable`] first (D5).
    fn set_compatibility_level(&self, subject: Option<&str>, level: &str) -> crate::Result<()> {
        let level = normalize_level(level)?;
        let path = match subject {
            Some(subject) => format!("/config/{}", path_segment(subject)),
            None => "/config".to_string(),
        };
        let payload = serde_json::json!({ "compatibility": level }).to_string();

        self.call(Method::Put, &path, Some(&payload))
            .map_err(|failure| {
                if failure.is_status(422) {
                    Error::Other(format!(
                        "the registry rejected the compatibility level {level}: {}",
                        failure.detail
                    ))
                } else if let (Some(subject), true) = (subject, failure.is_status(404)) {
                    Error::Other(format!(
                        "the registry has no subject named {subject}: {}",
                        failure.detail
                    ))
                } else {
                    self.refused(
                        &match subject {
                            Some(subject) => {
                                format!("set the compatibility level of {subject} to {level}")
                            }
                            None => format!("set the global compatibility level to {level}"),
                        },
                        &failure,
                    )
                }
            })?;
        Ok(())
    }

    /// The message for a call that never got an answer it could use. Always
    /// names the registry, and appends a keychain failure recorded at
    /// construction — which is the real cause of an HTTP 401 and the only part
    /// of it the user can act on.
    fn refused(&self, what: &str, failure: &SrFailure) -> Error {
        Error::Other(format!(
            "schema registry at {} could not {what}: {failure}{}",
            self.base_url,
            self.error()
                .map_or_else(String::new, |noted| format!(" — {noted}"))
        ))
    }

    /// Records the session's first failure. Later ones are dropped: the first
    /// is the one that explains the rest, and a per-message error is noise the
    /// UI has nowhere to put.
    pub fn note_error(&self, message: String) {
        let mut slot = self.first_error();
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    /// The session's first failure, if there was one.
    pub fn error(&self) -> Option<String> {
        self.first_error().clone()
    }

    /// The session's first failure, clearing it — for a caller that reports it
    /// and must not report it twice.
    pub fn take_error(&self) -> Option<String> {
        self.first_error().take()
    }

    fn fetch(&self, schema_id: u32) -> Option<RegisteredSchema> {
        let body = match self.get(&format!("/schemas/ids/{schema_id}")) {
            Ok(body) => body,
            Err(cause) => {
                self.note_error(format!(
                    "schema registry at {} could not resolve schema {schema_id}: {cause}",
                    self.base_url
                ));
                return None;
            }
        };
        let response: SchemaResponse = match serde_json::from_str(&body) {
            Ok(response) => response,
            Err(e) => {
                self.note_error(format!(
                    "schema registry at {} returned an unexpected body for schema \
                     {schema_id}: {e}",
                    self.base_url
                ));
                return None;
            }
        };

        let kind = match response.schema_type.as_deref() {
            // `schemaType` is omitted for Avro (Confluent's default).
            None | Some("AVRO") => match AvroSchema::parse_str(&response.schema) {
                Ok(schema) => SchemaKind::Avro(Box::new(schema)),
                Err(e) => {
                    self.note_error(format!(
                        "schema {schema_id} is registered as Avro but did not parse: {e}"
                    ));
                    return None;
                }
            },
            Some("JSON") => SchemaKind::Json,
            Some(other) => SchemaKind::Unsupported(other.to_string()),
        };

        // Best-effort: the payload decodes without it, and a registry that
        // withholds the subject listing (a common ACL split) should cost the
        // chip, not the message.
        let (subject, version) = self.subject_version(schema_id);
        Some(RegisteredSchema {
            subject,
            version,
            kind,
        })
    }

    /// `GET /schemas/ids/{id}/versions` — the subject/version pairs an id is
    /// registered under. An id can be shared by several subjects; the first is
    /// the one to show.
    fn subject_version(&self, schema_id: u32) -> (Option<String>, Option<i32>) {
        let Ok(body) = self.get(&format!("/schemas/ids/{schema_id}/versions")) else {
            return (None, None);
        };
        match serde_json::from_str::<Vec<IdVersion>>(&body) {
            Ok(versions) => match versions.into_iter().next() {
                Some(first) => (Some(first.subject), Some(first.version)),
                None => (None, None),
            },
            Err(_) => (None, None),
        }
    }

    /// The read path's transport, unchanged in behaviour: the `String` error is
    /// exactly what [`SrFailure`] renders, so every message the decode and
    /// encode paths already produce is byte-identical.
    fn get(&self, path: &str) -> Result<String, String> {
        self.call(Method::Get, path, None)
            .map_err(|failure| failure.to_string())
    }

    /// One registry round trip. The write side needs the *status* to tell
    /// "incompatible" (409) from "not a schema" (422) from "no such subject"
    /// (404), so this hands back a structured failure and [`get`](Self::get)
    /// flattens it.
    fn call(&self, method: Method, path: &str, body: Option<&str>) -> Result<String, SrFailure> {
        let url = format!("{}{path}", self.base_url);
        let mut response = match method {
            Method::Get => self.tune(ureq::get(&url), false).call(),
            Method::Post => match body {
                Some(payload) => self.tune(ureq::post(&url), true).send(payload),
                None => self.tune(ureq::post(&url), false).send_empty(),
            },
            Method::Put => match body {
                Some(payload) => self.tune(ureq::put(&url), true).send(payload),
                None => self.tune(ureq::put(&url), false).send_empty(),
            },
        }
        .map_err(|e| SrFailure::unreachable(format!("request to {url} failed: {e}")))?;

        let status = response.status();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| SrFailure::unreachable(format!("reading the response from {url}: {e}")))?;
        if !status.is_success() {
            let (error_code, detail) = parse_error(&body);
            return Err(SrFailure {
                status: Some(status.as_u16()),
                error_code,
                detail,
            });
        }
        Ok(body)
    }

    /// The parts of a request that never vary. Generic over ureq's body
    /// typestate so the four verbs share one place where the TLS config, the
    /// timeout and the credentials are decided.
    fn tune<Any>(
        &self,
        builder: ureq::RequestBuilder<Any>,
        has_body: bool,
    ) -> ureq::RequestBuilder<Any> {
        let mut builder = builder
            .config()
            // Same reasoning as src/auth/oidc.rs: a self-hosted registry is
            // routinely fronted by a private CA that lives in the OS trust
            // store and nowhere else. ureq's `platform-verifier` feature does
            // not switch this on by itself.
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            // Read the body ourselves: the registry puts its own error_code and
            // message in a 404/409 body, and that is the useful diagnosis.
            .http_status_as_error(false)
            .timeout_global(Some(HTTP_TIMEOUT))
            .build()
            .header("Accept", ACCEPT);
        if has_body {
            builder = builder.header("Content-Type", CONTENT_TYPE);
        }
        if let Some(authorization) = &self.authorization {
            builder = builder.header("Authorization", authorization);
        }
        builder
    }

    /// Poison-tolerant, like the rest of the crate: the guarded state is a
    /// cache and a string, neither of which a panicking holder can leave
    /// inconsistent, and turning one panic into a permanently unusable registry
    /// would be the worse failure.
    fn cache(&self) -> MutexGuard<'_, HashMap<u32, Option<Arc<RegisteredSchema>>>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn subjects(&self) -> MutexGuard<'_, HashMap<String, Arc<SubjectSchema>>> {
        self.subjects.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn first_error(&self) -> MutexGuard<'_, Option<String>> {
        self.first_error.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Hand-written so the `Authorization` header — which carries the registry
/// password — can never reach a log line or a panic message (D5).
impl fmt::Debug for SchemaRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SchemaRegistry")
            .field("base_url", &self.base_url)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .field("cached_ids", &self.cache().len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// The IPC entry points.
//
// Every one of these takes a [`ClusterConnection`] rather than a
// [`SchemaRegistryConfig`], for one reason: the mutating ones have to call
// [`ClusterConnection::ensure_writable`] **first**, and a function that cannot
// see the connection cannot do that. The read-only flag belongs to the
// connection, so the registry client is built *after* the gate, never before —
// a read-only profile therefore costs no keychain read and no socket.
// ---------------------------------------------------------------------------

/// The registry this connection's profile names, or a message saying it names
/// none.
fn registry_for(conn: &ClusterConnection) -> crate::Result<SchemaRegistry> {
    match conn.profile().schema_registry.as_ref() {
        Some(config) => Ok(SchemaRegistry::new(config)),
        None => Err(Error::Other(format!(
            "{} has no Schema Registry — add one in the connection's settings",
            conn.profile().name
        ))),
    }
}

/// Every registered version of a subject, with its schema text.
pub fn subject_versions(
    conn: &ClusterConnection,
    subject: &str,
) -> crate::Result<Vec<SubjectVersion>> {
    registry_for(conn)?.subject_versions(subject)
}

/// Whether a candidate schema may follow the subject's latest version.
pub fn check_compatibility(
    conn: &ClusterConnection,
    subject: &str,
    schema: &str,
    schema_type: &str,
) -> crate::Result<CompatibilityCheck> {
    registry_for(conn)?.check_compatibility(subject, schema, schema_type)
}

/// The compatibility level in force for a subject, or globally for `None`,
/// with whether the subject merely inherits it.
pub fn compatibility(
    conn: &ClusterConnection,
    subject: Option<&str>,
) -> crate::Result<CompatibilityInForce> {
    registry_for(conn)?.compatibility(subject)
}

/// Registers a new version of a subject. Read-only-checked first (D5), then
/// compatibility-checked against the registry before the write.
pub fn register(
    conn: &ClusterConnection,
    subject: &str,
    schema: &str,
    schema_type: &str,
) -> crate::Result<RegisteredId> {
    conn.ensure_writable("register a schema")?;
    registry_for(conn)?.register_schema(subject, schema, schema_type)
}

/// Sets a subject's compatibility level, or the global default for `None`.
/// Read-only-checked first (D5).
pub fn set_compatibility(
    conn: &ClusterConnection,
    subject: Option<&str>,
    level: &str,
) -> crate::Result<()> {
    conn.ensure_writable("set a compatibility level")?;
    registry_for(conn)?.set_compatibility_level(subject, level)
}

/// The registry's request body for a schema: the text plus the type it should
/// be read as. `schemaType` is sent even for Avro — Confluent's default —
/// because a subject that was created as JSON and is then sent an Avro schema
/// with no type is the one case where guessing silently registers the wrong
/// thing.
fn schema_payload(schema: &str, schema_type: &str) -> String {
    serde_json::json!({ "schema": schema, "schemaType": schema_type }).to_string()
}

/// Validates a schema type before it reaches the wire, so a typo reads as
/// "Kavka doesn't know that one" rather than as a 422 from the registry.
fn normalize_schema_type(schema_type: &str) -> crate::Result<String> {
    let normalized = schema_type.trim().to_ascii_uppercase();
    if SCHEMA_TYPES.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(Error::Other(format!(
        "{schema_type:?} is not a schema type Kavka can register; use one of {}",
        SCHEMA_TYPES.join(", ")
    )))
}

/// Same, for a compatibility level.
fn normalize_level(level: &str) -> crate::Result<String> {
    let normalized = level.trim().to_ascii_uppercase();
    if COMPATIBILITY_LEVELS.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(Error::Other(format!(
        "{level:?} is not a compatibility level; use one of {}",
        COMPATIBILITY_LEVELS.join(", ")
    )))
}

/// The registry's own reasons, appended to a sentence. Empty stays empty
/// rather than becoming "because: " with nothing after it.
fn quote_messages(messages: &[String]) -> String {
    if messages.is_empty() {
        return String::new();
    }
    format!(": {}", messages.join("; "))
}

/// RFC 7617 basic credentials. Built here rather than by putting the
/// credentials in the URL, which would leak them into every ureq error string.
fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

/// The registry's error envelope: `{"error_code": 40403, "message": "..."}`.
#[derive(Deserialize)]
struct ErrorResponse {
    error_code: Option<i64>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct SchemaResponse {
    schema: String,
    #[serde(rename = "schemaType")]
    schema_type: Option<String>,
}

/// One entry of `GET /schemas/ids/{id}/versions` — the decode path's
/// id→subject lookup. Deliberately *not* the public [`SubjectVersion`]: that
/// one carries the schema text, this one is the registry's two-field summary.
#[derive(Deserialize)]
struct IdVersion {
    subject: String,
    version: i32,
}

/// `POST /compatibility/subjects/{s}/versions/latest?verbose=true`.
///
/// `is_compatible` is required rather than defaulted: a 200 whose body this
/// build cannot read is reported as an unexpected body, because both defaults
/// are wrong — `true` waves through an unchecked schema, `false` blocks a
/// legitimate one and gives no reason.
#[derive(Deserialize)]
struct CompatibilityResponse {
    is_compatible: bool,
    messages: Option<Vec<String>>,
}

/// `POST /subjects/{s}/versions`.
#[derive(Deserialize)]
struct RegisterResponse {
    id: u32,
}

/// `GET /config` and `GET /config/{subject}` answer with `compatibilityLevel`;
/// `PUT` echoes `compatibility`, and some Confluent-compatible builds use that
/// key on the read too. Both are accepted rather than picking one and being
/// wrong on half the registries in the wild.
#[derive(Deserialize)]
struct CompatibilityConfig {
    #[serde(rename = "compatibilityLevel")]
    compatibility_level: Option<String>,
    compatibility: Option<String>,
}

impl CompatibilityConfig {
    fn level(self) -> Option<String> {
        self.compatibility_level.or(self.compatibility)
    }
}

/// The verbs this client uses. GET and DELETE take no body; POST and PUT may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Get,
    Post,
    Put,
}

/// A registry call that did not succeed.
///
/// `status` is `None` when the request never reached the registry at all — the
/// distinction the write side needs to say "the registry is down" rather than
/// "the registry rejected this".
#[derive(Debug)]
struct SrFailure {
    status: Option<u16>,
    /// The registry's `error_code`, when it sent its own envelope. 4xx codes
    /// are five digits: `40401` is "subject not found", `40403` "schema not
    /// found", `42201` "invalid schema".
    error_code: Option<i64>,
    /// The registry's own words, already summarised by [`parse_error`].
    detail: String,
}

impl SrFailure {
    fn unreachable(detail: String) -> Self {
        Self {
            status: None,
            error_code: None,
            detail,
        }
    }

    fn is_status(&self, code: u16) -> bool {
        self.status == Some(code)
    }

    /// Whether the registry said "no such subject" — a 404 carrying its
    /// `40401`. A bare 404 with no envelope is the *path* being wrong (a proxy,
    /// a wrong base URL), which is a different diagnosis and must not be
    /// reported as an empty subject.
    fn is_unknown_subject(&self) -> bool {
        self.is_status(404) && self.error_code == Some(40401)
    }
}

impl fmt::Display for SrFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(status) => write!(f, "HTTP {status}: {}", self.detail),
            None => f.write_str(&self.detail),
        }
    }
}

/// `GET /subjects/{subject}/versions/latest`.
#[derive(Deserialize)]
struct VersionResponse {
    subject: String,
    id: u32,
    version: i32,
    schema: String,
    #[serde(rename = "schemaType")]
    schema_type: Option<String>,
}

/// Percent-encodes one path segment (RFC 3986 unreserved set).
///
/// A subject name is user data — Confluent allows very nearly anything in one,
/// and the `TopicNameStrategy` default puts a *topic* name in it. A subject
/// with a `/`, a space or a `#` in it would otherwise be pasted straight into
/// the URL and either 404 against the wrong path or silently drop everything
/// after the fragment.
fn path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

/// The registry's `error_code` and its own words, from whatever it sent.
///
/// Split out of [`describe_error`] because the write side has to *branch* on
/// the code — `40401` (no such subject) is a normal state for a compatibility
/// check on a brand-new subject, and reporting it as a failure would make
/// registering a subject's first version impossible.
fn parse_error(body: &str) -> (Option<i64>, String) {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(ErrorResponse {
            error_code: Some(code),
            message: Some(message),
        }) => (Some(code), format!("{message} (error_code {code})")),
        Ok(ErrorResponse {
            error_code,
            message: Some(message),
        }) => (error_code, message),
        Ok(ErrorResponse {
            error_code: Some(code),
            message: None,
        }) => (Some(code), truncate(body.trim(), ERROR_BODY_LIMIT)),
        _ => (None, truncate(body.trim(), ERROR_BODY_LIMIT)),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// A canned Schema Registry over a real socket, shared with the serde
/// pipeline's tests and with [`crate::connect`]'s. A std
/// [`std::net::TcpListener`] rather than a mock trait: the thing worth testing
/// is that ureq sends the headers, verbs and bodies we think it sends, and a
/// trait boundary is exactly where that bug hides.
///
/// A route key is either a bare path (`"/config"` — any verb) or a
/// verb-qualified one (`"PUT /config"`). Lookup tries the qualified form
/// first, so one server can answer `GET /config` and `PUT /config`
/// differently — which is the whole shape of the Schema Registry's
/// compatibility endpoints and of Connect's `pause`/`resume`.
#[cfg(test)]
pub(crate) mod canned {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// What the server saw, so tests can assert on the request rather than
    /// only on the response.
    #[derive(Debug, Clone)]
    pub(crate) struct Seen {
        pub method: String,
        pub path: String,
        pub authorization: Option<String>,
        pub accept: Option<String>,
        pub content_type: Option<String>,
        /// The request body, verbatim. Empty for a bodyless verb.
        pub body: String,
    }

    impl Seen {
        /// The body as JSON, for asserting on what Kavka sent rather than on
        /// its exact serialization.
        pub(crate) fn json(&self) -> serde_json::Value {
            serde_json::from_str(&self.body)
                .unwrap_or_else(|e| panic!("body was not JSON ({e}): {:?}", self.body))
        }
    }

    pub(crate) struct CannedRegistry {
        url: String,
        seen: Arc<Mutex<Vec<Seen>>>,
        shutdown: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl CannedRegistry {
        /// `routes` maps a request path — optionally verb-qualified, see the
        /// module docs — to `(status, body)`. Anything else gets a 404 with the
        /// registry's own error envelope.
        pub(crate) fn start(routes: Vec<(&'static str, u16, &'static str)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let url = format!("http://{}", listener.local_addr().expect("addr"));
            listener.set_nonblocking(true).expect("nonblocking");

            let routes: HashMap<String, (u16, String)> = routes
                .into_iter()
                .map(|(path, status, body)| (path.to_string(), (status, body.to_string())))
                .collect();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let shutdown = Arc::new(AtomicBool::new(false));

            let worker = {
                let seen = Arc::clone(&seen);
                let shutdown = Arc::clone(&shutdown);
                std::thread::spawn(move || {
                    while !shutdown.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, _)) => serve(stream, &routes, &seen),
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(std::time::Duration::from_millis(2));
                            }
                            Err(_) => break,
                        }
                    }
                })
            };

            Self {
                url,
                seen,
                shutdown,
                worker: Some(worker),
            }
        }

        pub(crate) fn url(&self) -> &str {
            &self.url
        }

        pub(crate) fn seen(&self) -> Vec<Seen> {
            self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    impl Drop for CannedRegistry {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn serve(
        mut stream: TcpStream,
        routes: &HashMap<String, (u16, String)>,
        seen: &Mutex<Vec<Seen>>,
    ) {
        // An accepted socket inherits the listener's non-blocking mode on
        // Windows; the per-connection read below is meant to block.
        let _ = stream.set_nonblocking(false);
        let Some(request) = read_request(&stream) else {
            return;
        };
        // Verb-qualified first, so `GET /config` and `PUT /config` can answer
        // differently; a bare path stays a wildcard over verbs.
        let (status, body) = routes
            .get(&format!("{} {}", request.method, request.path))
            .or_else(|| routes.get(&request.path))
            .cloned()
            .unwrap_or((
                404,
                r#"{"error_code":40403,"message":"Schema not found"}"#.to_string(),
            ));
        seen.lock().unwrap_or_else(|e| e.into_inner()).push(request);

        // `Connection: close` keeps this server a one-request-per-socket toy;
        // ureq then opens a fresh connection per request and never blocks
        // waiting on a keep-alive we would have to implement.
        //
        // 204 is written without a body or a Content-Length, because
        // "no content" is what Connect answers to a DELETE and a restart and
        // the framing has to be the real one.
        let response = if status == 204 {
            "HTTP/1.1 204 X\r\nConnection: close\r\n\r\n".to_string()
        } else {
            format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            )
        };
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn read_request(stream: &TcpStream) -> Option<Seen> {
        let mut reader = BufReader::new(stream);
        let mut start = String::new();
        reader.read_line(&mut start).ok()?;
        let mut parts = start.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();

        let mut authorization = None;
        let mut accept = None;
        let mut content_type = None;
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 {
                break;
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                let value = value.trim();
                match name.to_ascii_lowercase().as_str() {
                    "authorization" => authorization = Some(value.to_string()),
                    "accept" => accept = Some(value.to_string()),
                    "content-type" => content_type = Some(value.to_string()),
                    "content-length" => content_length = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        // Content-Length only: nothing here sends chunked, and reading to EOF
        // would deadlock against a client waiting for the response.
        let mut body = vec![0u8; content_length];
        if content_length > 0 && reader.read_exact(&mut body).is_err() {
            body.clear();
        }

        Some(Seen {
            method,
            path,
            authorization,
            accept,
            content_type,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::canned::CannedRegistry;
    use super::*;
    use crate::profiles::SecretRef;

    const ORDER_SCHEMA: &str = r#"{"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#;
    const ORDER_VERSIONS: &str = r#"[{"subject":"orders-value","version":4}]"#;

    fn config(url: &str) -> SchemaRegistryConfig {
        SchemaRegistryConfig {
            url: url.to_string(),
            username: None,
            password: None,
        }
    }

    #[test]
    fn resolves_a_schema_with_its_subject_and_version() {
        let server = CannedRegistry::start(vec![
            ("/schemas/ids/217", 200, ORDER_SCHEMA),
            ("/schemas/ids/217/versions", 200, ORDER_VERSIONS),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let resolved = registry.lookup(217).expect("resolves");
        assert!(matches!(resolved.kind, SchemaKind::Avro(_)));
        assert_eq!(resolved.subject.as_deref(), Some("orders-value"));
        assert_eq!(resolved.version, Some(4));
        assert_eq!(registry.error(), None);

        let seen = server.seen();
        assert_eq!(seen.len(), 2, "schema then subject lookup: {seen:?}");
        assert_eq!(seen[0].path, "/schemas/ids/217");
        assert_eq!(seen[1].path, "/schemas/ids/217/versions");
        assert!(
            seen[0]
                .accept
                .as_deref()
                .is_some_and(|a| a.contains("schemaregistry")),
            "{seen:?}"
        );
        // No credentials configured means no header at all, not an empty one.
        assert_eq!(seen[0].authorization, None);
    }

    #[test]
    fn basic_auth_is_sent_when_the_profile_carries_credentials() {
        let server = CannedRegistry::start(vec![
            ("/schemas/ids/1", 200, ORDER_SCHEMA),
            ("/schemas/ids/1/versions", 200, ORDER_VERSIONS),
        ]);
        let mut registry = SchemaRegistry::unauthenticated(server.url());
        registry.authorization = Some(basic_auth("alice", "s3cr3t"));

        assert!(registry.lookup(1).is_some());
        // base64("alice:s3cr3t")
        assert_eq!(
            server.seen()[0].authorization.as_deref(),
            Some("Basic YWxpY2U6czNjcjN0")
        );
    }

    #[test]
    fn the_authorization_header_never_reaches_debug() {
        let mut registry = SchemaRegistry::unauthenticated("http://registry.example");
        registry.authorization = Some(basic_auth("alice", "s3cr3t"));
        let rendered = format!("{registry:?}");
        assert!(!rendered.contains("s3cr3t"), "leaked: {rendered}");
        assert!(!rendered.contains("YWxpY2U6czNjcjN0"), "leaked: {rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }

    #[test]
    fn a_schema_id_is_fetched_once_per_session() {
        let server = CannedRegistry::start(vec![
            ("/schemas/ids/217", 200, ORDER_SCHEMA),
            ("/schemas/ids/217/versions", 200, ORDER_VERSIONS),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        for _ in 0..5 {
            assert!(registry.lookup(217).is_some());
        }
        assert_eq!(server.seen().len(), 2, "cached after the first lookup");
    }

    #[test]
    fn an_unreachable_registry_degrades_and_reports_once() {
        // Port 1 is never listening; the connection is refused immediately.
        let registry = SchemaRegistry::new(&config("http://127.0.0.1:1"));

        assert!(registry.lookup(217).is_none());
        assert!(registry.lookup(217).is_none());
        assert!(registry.lookup(9).is_none());

        let error = registry.error().expect("one recorded failure");
        assert!(error.contains("schema 217"), "got {error}");
        // Reported once: the second id's failure did not overwrite the first.
        assert!(!error.contains("schema 9"), "got {error}");
        assert_eq!(registry.take_error(), Some(error));
        assert_eq!(registry.take_error(), None);
    }

    #[test]
    fn a_failed_id_is_not_retried_per_message() {
        let server = CannedRegistry::start(vec![]);
        let registry = SchemaRegistry::new(&config(server.url()));

        for _ in 0..4 {
            assert!(registry.lookup(404).is_none());
        }
        assert_eq!(server.seen().len(), 1, "the 404 is remembered");
        let error = registry.error().expect("recorded");
        assert!(error.contains("HTTP 404"), "got {error}");
        assert!(error.contains("Schema not found"), "got {error}");
    }

    #[test]
    fn a_protobuf_schema_is_named_rather_than_guessed_at() {
        let server = CannedRegistry::start(vec![
            (
                "/schemas/ids/5",
                200,
                r#"{"schema":"syntax = \"proto3\";","schemaType":"PROTOBUF"}"#,
            ),
            ("/schemas/ids/5/versions", 200, r#"[]"#),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let resolved = registry.lookup(5).expect("resolves the metadata");
        match &resolved.kind {
            SchemaKind::Unsupported(kind) => assert_eq!(kind, "PROTOBUF"),
            _ => panic!("protobuf must not be read as avro"),
        }
        assert_eq!(resolved.subject, None);
    }

    #[test]
    fn a_json_schema_subject_decodes_as_plain_json() {
        let server = CannedRegistry::start(vec![
            (
                "/schemas/ids/8",
                200,
                r#"{"schema":"{\"type\":\"object\"}","schemaType":"JSON"}"#,
            ),
            (
                "/schemas/ids/8/versions",
                200,
                r#"[{"subject":"events-value","version":2}]"#,
            ),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let resolved = registry.lookup(8).expect("resolves");
        assert!(matches!(resolved.kind, SchemaKind::Json));
        assert_eq!(resolved.subject.as_deref(), Some("events-value"));
    }

    #[test]
    fn a_registry_that_hides_subjects_still_decodes() {
        let server = CannedRegistry::start(vec![
            ("/schemas/ids/217", 200, ORDER_SCHEMA),
            ("/schemas/ids/217/versions", 403, r#"{"message":"nope"}"#),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let resolved = registry.lookup(217).expect("the schema itself resolved");
        assert!(matches!(resolved.kind, SchemaKind::Avro(_)));
        assert_eq!(resolved.subject, None);
        assert_eq!(resolved.version, None);
        // The chip is missing, but nothing failed that the user must act on.
        assert_eq!(registry.error(), None);
    }

    #[test]
    fn an_unparseable_schema_is_reported_rather_than_half_used() {
        let server = CannedRegistry::start(vec![(
            "/schemas/ids/3",
            200,
            r#"{"schema":"{\"type\":\"nonsense\"}"}"#,
        )]);
        let registry = SchemaRegistry::new(&config(server.url()));

        assert!(registry.lookup(3).is_none());
        assert!(registry.error().unwrap().contains("did not parse"));
    }

    #[test]
    fn a_missing_keychain_secret_is_recorded_not_fatal() {
        let registry = SchemaRegistry::new(&SchemaRegistryConfig {
            url: "http://registry.example".into(),
            username: Some("alice".into()),
            password: Some(SecretRef {
                entry: format!("kavka-unit-test/absent/{}", std::process::id()),
            }),
        });

        assert!(registry.authorization.is_none(), "no credentials to send");
        let error = registry.error().expect("the reason is kept for the UI");
        assert!(error.contains("schema registry credentials"), "got {error}");
    }

    #[test]
    fn error_bodies_are_summarised() {
        assert_eq!(
            parse_error(r#"{"error_code":40403,"message":"Schema not found"}"#),
            (
                Some(40403),
                "Schema not found (error_code 40403)".to_string()
            )
        );
        assert_eq!(
            parse_error(r#"{"message":"nope"}"#),
            (None, "nope".to_string())
        );
        assert_eq!(
            parse_error(" <html>502</html> "),
            (None, "<html>502</html>".to_string())
        );
        // The code survives even when the registry sends no prose with it —
        // the write side branches on it, so losing it would turn "no such
        // subject" into "something went wrong".
        assert_eq!(
            parse_error(r#"{"error_code":40401}"#),
            (Some(40401), r#"{"error_code":40401}"#.to_string())
        );
    }

    #[test]
    fn trailing_slashes_in_the_configured_url_do_not_double_up() {
        let registry = SchemaRegistry::unauthenticated("http://registry.example/");
        assert_eq!(registry.base_url, "http://registry.example");
    }

    // -----------------------------------------------------------------------
    // The encode side.
    // -----------------------------------------------------------------------

    const ORDER_LATEST: &str = r#"{"subject":"orders-value","version":4,"id":217,"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#;

    #[test]
    fn the_latest_schema_of_a_subject_carries_the_id_the_framing_needs() {
        let server = CannedRegistry::start(vec![(
            "/subjects/orders-value/versions/latest",
            200,
            ORDER_LATEST,
        )]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let latest = registry.latest_schema("orders-value").expect("resolves");
        assert_eq!(latest.subject, "orders-value");
        assert_eq!(latest.schema_id, 217);
        assert_eq!(latest.version, 4);
        assert!(matches!(latest.kind, SchemaKind::Avro(_)));
        assert_eq!(
            server.seen()[0].path,
            "/subjects/orders-value/versions/latest"
        );
        // The decode side's `first_error` is untouched by a success.
        assert_eq!(registry.error(), None);
    }

    #[test]
    fn a_subject_is_fetched_once_per_registry() {
        let server = CannedRegistry::start(vec![(
            "/subjects/orders-value/versions/latest",
            200,
            ORDER_LATEST,
        )]);
        let registry = SchemaRegistry::new(&config(server.url()));

        for _ in 0..5 {
            assert_eq!(
                registry
                    .latest_schema("orders-value")
                    .expect("resolves")
                    .schema_id,
                217
            );
        }
        assert_eq!(server.seen().len(), 1, "cached after the first lookup");
    }

    /// The asymmetry that matters: producing cannot fall back to "show it as
    /// hex", so this path errors where `lookup` returns `None`.
    #[test]
    fn an_unreachable_registry_is_an_error_on_the_encode_side() {
        // Port 1 is never listening; the connection is refused immediately.
        let registry = SchemaRegistry::new(&config("http://127.0.0.1:1"));
        let err = registry
            .latest_schema("orders-value")
            .expect_err("nothing to encode against")
            .to_string();

        assert!(err.contains("orders-value"), "got {err}");
        assert!(err.contains("schema registry at"), "got {err}");
    }

    #[test]
    fn an_unknown_subject_quotes_what_the_registry_said() {
        let server = CannedRegistry::start(vec![]);
        let registry = SchemaRegistry::new(&config(server.url()));
        let err = registry
            .latest_schema("no-such-value")
            .expect_err("404")
            .to_string();

        assert!(err.contains("no-such-value"), "got {err}");
        assert!(err.contains("HTTP 404"), "got {err}");
        assert!(err.contains("Schema not found"), "got {err}");
    }

    /// A failure is not cached: the user who fixes the registry and presses
    /// send again must reach it, not a memory of the outage.
    #[test]
    fn a_failed_subject_lookup_is_retried() {
        let server = CannedRegistry::start(vec![]);
        let registry = SchemaRegistry::new(&config(server.url()));

        assert!(registry.latest_schema("orders-value").is_err());
        assert!(registry.latest_schema("orders-value").is_err());
        assert_eq!(server.seen().len(), 2, "asked again, not remembered");
    }

    #[test]
    fn a_keychain_failure_is_carried_into_the_encode_side_error() {
        let registry = SchemaRegistry::new(&SchemaRegistryConfig {
            url: "http://127.0.0.1:1".into(),
            username: Some("alice".into()),
            password: Some(SecretRef {
                entry: format!("kavka-unit-test/absent-encode/{}", std::process::id()),
            }),
        });
        let err = registry
            .latest_schema("orders-value")
            .expect_err("no credentials, no registry")
            .to_string();
        assert!(err.contains("schema registry credentials"), "got {err}");
    }

    #[test]
    fn a_non_avro_subject_is_named_rather_than_parsed_as_avro() {
        let server = CannedRegistry::start(vec![
            (
                "/subjects/events-value/versions/latest",
                200,
                r#"{"subject":"events-value","version":2,"id":8,"schema":"{\"type\":\"object\"}","schemaType":"JSON"}"#,
            ),
            (
                "/subjects/traces-value/versions/latest",
                200,
                r#"{"subject":"traces-value","version":1,"id":9,"schema":"syntax = \"proto3\";","schemaType":"PROTOBUF"}"#,
            ),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        assert!(matches!(
            registry
                .latest_schema("events-value")
                .expect("resolves")
                .kind,
            SchemaKind::Json
        ));
        match &registry
            .latest_schema("traces-value")
            .expect("resolves")
            .kind
        {
            SchemaKind::Unsupported(kind) => assert_eq!(kind, "PROTOBUF"),
            _ => panic!("protobuf must not be read as avro"),
        }
    }

    #[test]
    fn an_unparseable_avro_subject_names_itself() {
        let server = CannedRegistry::start(vec![(
            "/subjects/broken-value/versions/latest",
            200,
            r#"{"subject":"broken-value","version":1,"id":3,"schema":"{\"type\":\"nonsense\"}"}"#,
        )]);
        let registry = SchemaRegistry::new(&config(server.url()));
        let err = registry
            .latest_schema("broken-value")
            .expect_err("not a schema")
            .to_string();
        assert!(err.contains("broken-value version 1"), "got {err}");
        assert!(err.contains("did not parse"), "got {err}");
    }

    /// A subject name is user data, and `TopicNameStrategy` puts a topic name
    /// in it — so it has to survive the trip into a URL path.
    #[test]
    fn subject_names_are_percent_encoded_into_the_path() {
        assert_eq!(path_segment("orders-value"), "orders-value");
        assert_eq!(path_segment("a/b"), "a%2Fb");
        assert_eq!(path_segment("with space"), "with%20space");
        assert_eq!(path_segment("q?x=1#frag"), "q%3Fx%3D1%23frag");
        assert_eq!(path_segment("héllo"), "h%C3%A9llo");
    }

    // -----------------------------------------------------------------------
    // The management surface (Phase 3a).
    // -----------------------------------------------------------------------

    const V1: &str = r#"{"subject":"orders-value","version":1,"id":100,"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"}]}"}"#;
    const V2: &str = r#"{"subject":"orders-value","version":2,"id":217,"schemaType":"AVRO","schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\",\"default\":\"new\"}]}"}"#;
    const CANDIDATE: &str = r#"{"type":"record","name":"Order","fields":[{"name":"orderId","type":"int"},{"name":"status","type":"string","default":"new"},{"name":"note","type":"string","default":""}]}"#;
    const COMPAT_PATH: &str = "/compatibility/subjects/orders-value/versions/latest?verbose=true";

    #[test]
    fn every_version_of_a_subject_comes_back_with_its_schema_text() {
        let server = CannedRegistry::start(vec![
            ("/subjects/orders-value/versions", 200, "[1,2]"),
            ("/subjects/orders-value/versions/1", 200, V1),
            ("/subjects/orders-value/versions/2", 200, V2),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let versions = registry.subject_versions("orders-value").expect("lists");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 1);
        assert_eq!(versions[0].schema_id, 100);
        // `schemaType` omitted is Avro, and the UI must never have to know that.
        assert_eq!(versions[0].schema_type, "AVRO");
        assert!(versions[0].schema.contains("orderId"));
        assert_eq!(versions[1].version, 2);
        assert_eq!(versions[1].schema_id, 217);
        assert_eq!(versions[1].schema_type, "AVRO");
        // The text is the payload — the version diff has nothing without it.
        assert!(versions[1].schema.contains("status"));

        let seen = server.seen();
        assert!(seen.iter().all(|s| s.method == "GET"), "{seen:?}");
        assert_eq!(seen.len(), 3, "the listing, then one per version");
    }

    #[test]
    fn an_unknown_subject_is_named_rather_than_reported_as_an_empty_list() {
        let server = CannedRegistry::start(vec![(
            "/subjects/no-such-value/versions",
            404,
            r#"{"error_code":40401,"message":"Subject 'no-such-value' not found."}"#,
        )]);
        let err = SchemaRegistry::new(&config(server.url()))
            .subject_versions("no-such-value")
            .expect_err("40401")
            .to_string();

        assert!(err.contains("no subject named no-such-value"), "got {err}");
        assert!(err.contains("register its first version"), "got {err}");
    }

    #[test]
    fn a_compatibility_check_sends_the_schema_its_type_and_asks_for_reasons() {
        let server = CannedRegistry::start(vec![(COMPAT_PATH, 200, r#"{"is_compatible":true}"#)]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let verdict = registry
            .check_compatibility("orders-value", CANDIDATE, "avro")
            .expect("checks");
        assert!(verdict.compatible);
        // A registry too old for `?verbose=true` gives the boolean alone, and
        // nothing here invents prose to fill the gap.
        assert!(verdict.messages.is_empty());

        let seen = server.seen();
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].path, COMPAT_PATH);
        assert_eq!(
            seen[0].content_type.as_deref(),
            Some("application/vnd.schemaregistry.v1+json")
        );
        // The type is normalised on the way out, so `avro` reaches the wire as
        // the registry's own spelling.
        assert_eq!(
            seen[0].json(),
            serde_json::json!({"schema": CANDIDATE, "schemaType": "AVRO"})
        );
    }

    #[test]
    fn an_incompatible_schema_comes_back_with_the_registrys_own_reasons() {
        let server = CannedRegistry::start(vec![(
            COMPAT_PATH,
            200,
            r#"{"is_compatible":false,"messages":["READER_FIELD_MISSING_DEFAULT_VALUE: note","{oldSchemaVersion: 2}"]}"#,
        )]);
        let verdict = SchemaRegistry::new(&config(server.url()))
            .check_compatibility("orders-value", CANDIDATE, "AVRO")
            .expect("the check itself succeeded");

        assert!(!verdict.compatible);
        assert_eq!(verdict.messages.len(), 2);
        assert!(verdict.messages[0].contains("READER_FIELD_MISSING_DEFAULT_VALUE"));
    }

    /// Registering a subject's *first* version is the most common registration
    /// there is, and the registry answers the compatibility check for it with a
    /// 404. Treating that as a failure would make the common case impossible.
    #[test]
    fn a_subject_with_no_versions_is_compatible_rather_than_a_404() {
        let server = CannedRegistry::start(vec![(
            COMPAT_PATH,
            404,
            r#"{"error_code":40401,"message":"Subject 'orders-value' not found."}"#,
        )]);
        let verdict = SchemaRegistry::new(&config(server.url()))
            .check_compatibility("orders-value", CANDIDATE, "AVRO")
            .expect("a new subject is not a failure");

        assert!(verdict.compatible);
        assert!(
            verdict.messages[0].contains("no registered versions yet"),
            "{verdict:?}"
        );
    }

    /// A bare 404 with no envelope is the *path* being wrong — a proxy, a bad
    /// base URL — which is a different diagnosis and must not read as "this
    /// subject is new".
    #[test]
    fn a_404_with_no_error_code_is_not_mistaken_for_a_new_subject() {
        let server = CannedRegistry::start(vec![(COMPAT_PATH, 404, "<html>not found</html>")]);
        let err = SchemaRegistry::new(&config(server.url()))
            .check_compatibility("orders-value", CANDIDATE, "AVRO")
            .expect_err("this is a wrong URL, not a new subject")
            .to_string();

        assert!(err.contains("HTTP 404"), "got {err}");
        assert!(
            err.contains("check a schema against orders-value"),
            "got {err}"
        );
    }

    #[test]
    fn registering_checks_compatibility_before_it_writes() {
        let server = CannedRegistry::start(vec![
            (COMPAT_PATH, 200, r#"{"is_compatible":true}"#),
            ("POST /subjects/orders-value/versions", 200, r#"{"id":218}"#),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let id = registry
            .register_schema("orders-value", CANDIDATE, "AVRO")
            .expect("registers");
        assert_eq!(id.schema_id, 218);

        let seen = server.seen();
        assert_eq!(seen.len(), 2, "the check, then the write: {seen:?}");
        assert_eq!(seen[0].path, COMPAT_PATH, "the check came first");
        assert_eq!(seen[1].path, "/subjects/orders-value/versions");
        assert_eq!(seen[1].method, "POST");
        assert_eq!(
            seen[1].json(),
            serde_json::json!({"schema": CANDIDATE, "schemaType": "AVRO"})
        );
    }

    #[test]
    fn an_incompatible_schema_is_never_written() {
        let server = CannedRegistry::start(vec![(
            COMPAT_PATH,
            200,
            r#"{"is_compatible":false,"messages":["READER_FIELD_MISSING_DEFAULT_VALUE: note"]}"#,
        )]);
        let err = SchemaRegistry::new(&config(server.url()))
            .register_schema("orders-value", CANDIDATE, "AVRO")
            .expect_err("incompatible")
            .to_string();

        assert!(err.contains("incompatible with orders-value"), "got {err}");
        // The registry's own reason, quoted.
        assert!(
            err.contains("READER_FIELD_MISSING_DEFAULT_VALUE"),
            "got {err}"
        );
        assert_eq!(server.seen().len(), 1, "the write never happened");
    }

    /// Between the check and the POST is a window nothing can close, so the
    /// registry's own 409 is mapped too.
    #[test]
    fn a_409_on_the_write_still_reads_as_incompatible_quoting_the_registry() {
        let server = CannedRegistry::start(vec![
            (COMPAT_PATH, 200, r#"{"is_compatible":true}"#),
            (
                "POST /subjects/orders-value/versions",
                409,
                r#"{"error_code":409,"message":"Schema being registered is incompatible with an earlier schema for subject \"orders-value\""}"#,
            ),
        ]);
        let err = SchemaRegistry::new(&config(server.url()))
            .register_schema("orders-value", CANDIDATE, "AVRO")
            .expect_err("409")
            .to_string();

        assert!(err.contains("incompatible with orders-value"), "got {err}");
        assert!(err.contains("earlier schema for subject"), "got {err}");
        assert!(err.contains("error_code 409"), "got {err}");
    }

    #[test]
    fn a_422_reads_as_invalid_rather_than_incompatible() {
        let server = CannedRegistry::start(vec![
            (COMPAT_PATH, 200, r#"{"is_compatible":true}"#),
            (
                "POST /subjects/orders-value/versions",
                422,
                r#"{"error_code":42201,"message":"Invalid schema {\"type\":\"nonsense\"} with refs [] of type AVRO"}"#,
            ),
        ]);
        let err = SchemaRegistry::new(&config(server.url()))
            .register_schema("orders-value", CANDIDATE, "AVRO")
            .expect_err("422")
            .to_string();

        assert!(err.contains("as invalid"), "got {err}");
        assert!(!err.contains("incompatible"), "42201 is not 409: {err}");
        assert!(err.contains("of type AVRO"), "got {err}");
        assert!(err.contains("error_code 42201"), "got {err}");
    }

    /// A 500 is neither: it names the registry, so a user does not go hunting
    /// their schema for a fault that is not in it.
    #[test]
    fn a_registry_that_falls_over_names_itself_rather_than_the_schema() {
        let server = CannedRegistry::start(vec![
            (COMPAT_PATH, 200, r#"{"is_compatible":true}"#),
            (
                "POST /subjects/orders-value/versions",
                500,
                r#"{"error_code":50001,"message":"Error while forwarding register schema request to the leader"}"#,
            ),
        ]);
        let err = SchemaRegistry::new(&config(server.url()))
            .register_schema("orders-value", CANDIDATE, "AVRO")
            .expect_err("500")
            .to_string();

        assert!(err.contains("schema registry at"), "got {err}");
        assert!(
            err.contains("register a schema for orders-value"),
            "got {err}"
        );
        assert!(err.contains("HTTP 500"), "got {err}");
        assert!(
            err.contains("forwarding register schema request"),
            "got {err}"
        );
        assert!(
            !err.contains("invalid") && !err.contains("incompatible"),
            "got {err}"
        );
    }

    /// The registry-wide default inherits from nothing, so it is never marked.
    #[test]
    fn the_global_compatibility_level_is_read_from_config() {
        let server = CannedRegistry::start(vec![(
            "GET /config",
            200,
            r#"{"compatibilityLevel":"BACKWARD"}"#,
        )]);
        let registry = SchemaRegistry::new(&config(server.url()));

        assert_eq!(
            registry.compatibility(None).expect("reads"),
            CompatibilityInForce {
                level: "BACKWARD".to_string(),
                inherited: false,
            }
        );
        let seen = server.seen();
        assert_eq!(seen.len(), 1, "one call: {seen:?}");
        assert_eq!(seen[0].path, "/config");
    }

    /// A subject WITH a setting of its own answers 200, and the global is never
    /// asked for — the subject's own level is the one in force, full stop.
    #[test]
    fn a_subjects_own_level_comes_back_unmarked_and_costs_one_call() {
        let server = CannedRegistry::start(vec![
            (
                "/config/orders-value",
                200,
                r#"{"compatibilityLevel":"FULL_TRANSITIVE"}"#,
            ),
            // Present so the assertion below is about Kavka not asking, rather
            // than about this server not answering.
            ("GET /config", 200, r#"{"compatibilityLevel":"BACKWARD"}"#),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        assert_eq!(
            registry.compatibility(Some("orders-value")).expect("reads"),
            CompatibilityInForce {
                level: "FULL_TRANSITIVE".to_string(),
                inherited: false,
            }
        );
        let seen = server.seen();
        assert_eq!(seen.len(), 1, "the global was not asked for: {seen:?}");
        // `?defaultToGlobal=true` would have the registry do the fallback
        // itself, which answers 200 with the global level and leaves nothing to
        // tell an override from an inheritance.
        assert_eq!(seen[0].path, "/config/orders-value");
    }

    /// A subject with no setting of its own answers 404. The global level is
    /// what actually applies to it — so it comes back, **marked**: reporting
    /// "no level" reads as "anything goes", and reporting it unmarked would
    /// make "set to BACKWARD" and "follows a global that is BACKWARD" the same
    /// answer.
    #[test]
    fn a_subject_with_no_override_of_its_own_inherits_the_global_level() {
        let server = CannedRegistry::start(vec![
            (
                "/config/orders-value",
                404,
                r#"{"error_code":40408,"message":"Subject 'orders-value' does not have subject-level compatibility configured"}"#,
            ),
            ("GET /config", 200, r#"{"compatibility":"FORWARD"}"#),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        // `compatibility` rather than `compatibilityLevel`: both spellings are
        // in the wild and both are read.
        assert_eq!(
            registry.compatibility(Some("orders-value")).expect("reads"),
            CompatibilityInForce {
                level: "FORWARD".to_string(),
                inherited: true,
            }
        );
        assert_eq!(server.seen().len(), 2, "the subject, then the global");
    }

    /// The fallback is for a 404 and nothing else: a 403 on the subject config
    /// is a permission problem, and answering it with the global level would
    /// tell the user a level is in force that Kavka never actually read.
    #[test]
    fn a_refusal_on_the_subject_config_is_not_read_as_an_inheritance() {
        let server = CannedRegistry::start(vec![
            ("/config/orders-value", 403, r#"{"message":"Forbidden"}"#),
            ("GET /config", 200, r#"{"compatibility":"FORWARD"}"#),
        ]);
        let err = SchemaRegistry::new(&config(server.url()))
            .compatibility(Some("orders-value"))
            .expect_err("403")
            .to_string();

        assert!(
            err.contains("read the compatibility level of orders-value"),
            "got {err}"
        );
        assert!(err.contains("HTTP 403"), "got {err}");
    }

    #[test]
    fn setting_a_level_puts_it_and_normalises_it_on_the_way_out() {
        let server = CannedRegistry::start(vec![
            (
                "PUT /config/orders-value",
                200,
                r#"{"compatibility":"NONE"}"#,
            ),
            ("PUT /config", 200, r#"{"compatibility":"FULL"}"#),
        ]);
        let registry = SchemaRegistry::new(&config(server.url()));

        registry
            .set_compatibility_level(Some("orders-value"), " none ")
            .expect("sets the subject");
        registry
            .set_compatibility_level(None, "full")
            .expect("sets the global default");

        let seen = server.seen();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(seen[0].path, "/config/orders-value");
        assert_eq!(seen[0].json(), serde_json::json!({"compatibility": "NONE"}));
        assert_eq!(seen[1].path, "/config");
        assert_eq!(seen[1].json(), serde_json::json!({"compatibility": "FULL"}));
    }

    #[test]
    fn a_rejected_level_quotes_the_registrys_reason() {
        let server = CannedRegistry::start(vec![(
            "PUT /config/orders-value",
            422,
            r#"{"error_code":42203,"message":"Invalid compatibility level. Valid values are none, backward, forward, full, backward_transitive, forward_transitive, and full_transitive"}"#,
        )]);
        let err = SchemaRegistry::new(&config(server.url()))
            .set_compatibility_level(Some("orders-value"), "BACKWARD")
            .expect_err("422")
            .to_string();

        assert!(
            err.contains("rejected the compatibility level BACKWARD"),
            "got {err}"
        );
        assert!(err.contains("Valid values are none"), "got {err}");
    }

    #[test]
    fn setting_the_level_of_a_subject_that_is_not_there_names_the_subject() {
        let server = CannedRegistry::start(vec![(
            "PUT /config/ghost-value",
            404,
            r#"{"error_code":40401,"message":"Subject 'ghost-value' not found."}"#,
        )]);
        let err = SchemaRegistry::new(&config(server.url()))
            .set_compatibility_level(Some("ghost-value"), "BACKWARD")
            .expect_err("404")
            .to_string();

        assert!(err.contains("no subject named ghost-value"), "got {err}");
    }

    /// A typo comes back as "Kavka doesn't know that one, here are the ones it
    /// does" rather than as a 422 from three network hops away.
    #[test]
    fn unknown_levels_and_schema_types_never_reach_the_wire() {
        let server = CannedRegistry::start(vec![]);
        let registry = SchemaRegistry::new(&config(server.url()));

        let level = registry
            .set_compatibility_level(None, "BACKWARDS")
            .expect_err("not a level")
            .to_string();
        assert!(
            level.contains("BACKWARD_TRANSITIVE"),
            "lists the set: {level}"
        );
        assert!(level.contains("\"BACKWARDS\""), "quotes the input: {level}");

        let schema_type = registry
            .check_compatibility("orders-value", CANDIDATE, "avroo")
            .expect_err("not a schema type")
            .to_string();
        assert!(
            schema_type.contains("AVRO, JSON, PROTOBUF"),
            "got {schema_type}"
        );

        assert!(server.seen().is_empty(), "nothing was sent");
    }

    /// Protobuf and JSON Schema cannot be *decoded* by this build, but they can
    /// be registered: registering is a text upload the registry validates.
    #[test]
    fn a_protobuf_schema_can_be_registered_even_though_it_cannot_be_decoded() {
        let server = CannedRegistry::start(vec![
            (
                "/compatibility/subjects/traces-value/versions/latest?verbose=true",
                200,
                r#"{"is_compatible":true}"#,
            ),
            ("POST /subjects/traces-value/versions", 200, r#"{"id":9}"#),
        ]);
        let id = SchemaRegistry::new(&config(server.url()))
            .register_schema("traces-value", "syntax = \"proto3\";", "protobuf")
            .expect("registers");

        assert_eq!(id.schema_id, 9);
        assert_eq!(
            server.seen()[1].json(),
            serde_json::json!({"schema": "syntax = \"proto3\";", "schemaType": "PROTOBUF"})
        );
    }

    #[test]
    fn the_write_side_sends_the_profiles_credentials() {
        let server = CannedRegistry::start(vec![
            (COMPAT_PATH, 200, r#"{"is_compatible":true}"#),
            ("POST /subjects/orders-value/versions", 200, r#"{"id":1}"#),
            ("PUT /config", 200, r#"{"compatibility":"NONE"}"#),
        ]);
        let mut registry = SchemaRegistry::unauthenticated(server.url());
        registry.authorization = Some(basic_auth("alice", "s3cr3t"));

        registry
            .register_schema("orders-value", CANDIDATE, "AVRO")
            .expect("registers");
        registry
            .set_compatibility_level(None, "NONE")
            .expect("sets");

        for request in server.seen() {
            // base64("alice:s3cr3t")
            assert_eq!(
                request.authorization.as_deref(),
                Some("Basic YWxpY2U6czNjcjN0"),
                "{request:?}"
            );
        }
    }

    #[test]
    fn an_unreachable_registry_names_itself_on_the_write_side_too() {
        // Port 1 is never listening; the connection is refused immediately.
        let registry = SchemaRegistry::new(&config("http://127.0.0.1:1"));
        let err = registry
            .set_compatibility_level(Some("orders-value"), "BACKWARD")
            .expect_err("nothing is listening")
            .to_string();

        assert!(err.contains("http://127.0.0.1:1"), "got {err}");
        assert!(
            err.contains("set the compatibility level of orders-value to BACKWARD"),
            "got {err}"
        );
    }

    // -----------------------------------------------------------------------
    // The read-only gate (D5).
    // -----------------------------------------------------------------------

    fn integration() -> bool {
        if std::env::var("KAVKA_IT").is_err() {
            eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
            return false;
        }
        true
    }

    /// A connection whose registry would fail loudly if it were ever consulted,
    /// so a passing read-only assertion proves the gate ran before the client
    /// was built.
    fn connection(
        read_only: bool,
        schema_registry: Option<SchemaRegistryConfig>,
    ) -> ClusterConnection {
        use crate::profiles::{AuthConfig, ConnectionProfile, Environment};
        ClusterConnection::connect(ConnectionProfile {
            id: "it-sr".into(),
            name: "local docker".into(),
            environment: Environment::Dev,
            bootstrap_servers: vec![
                std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
            ],
            auth: AuthConfig::Plaintext,
            read_only,
            schema_registry,
            connect_clusters: Vec::new(),
        })
        .expect("connect")
    }

    #[test]
    fn a_read_only_connection_refuses_every_mutating_registry_call() {
        if !integration() {
            return;
        }
        let conn = connection(true, Some(config("http://127.0.0.1:1")));
        let started = std::time::Instant::now();

        for err in [
            super::register(&conn, "orders-value", CANDIDATE, "AVRO").expect_err("register"),
            super::set_compatibility(&conn, Some("orders-value"), "NONE").expect_err("set"),
        ] {
            assert!(matches!(err, Error::ReadOnly(_)), "got {err}");
            assert!(err.to_string().contains("read-only"), "got {err}");
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "nothing was attempted over the network"
        );
    }

    #[test]
    fn a_connection_with_no_registry_says_so_rather_than_failing_to_connect() {
        if !integration() {
            return;
        }
        let err = super::subject_versions(&connection(false, None), "orders-value")
            .expect_err("no registry configured")
            .to_string();
        assert!(err.contains("no Schema Registry"), "got {err}");
        assert!(err.contains("connection's settings"), "got {err}");
    }
}
