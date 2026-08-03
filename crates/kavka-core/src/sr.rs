//! Confluent-compatible Schema Registry client (docs/ARCHITECTURE.md D4).
//!
//! Read-only in Phase 1: resolve a schema id off a message's framing, and the
//! subject/version to display next to it. Apicurio and AWS Glue arrive behind
//! the same shape later; Apicurio ships a Confluent-compatible API surface, so
//! this client already reaches it.
//!
//! # Degradation is the design, not the error path
//!
//! A registry the app cannot reach must not stop a user browsing a topic. Every
//! failure here returns `None` from [`SchemaRegistry::lookup`], the caller
//! falls back to hex **with the schema id still attached**, and the reason is
//! recorded once per session ([`SchemaRegistry::note_error`]) rather than once
//! per message — a 2000-message fetch against a wedged registry is one line in
//! the log, not two thousand.
//!
//! Failures are cached alongside successes for the same reason: without that,
//! every message in the fetch pays a fresh 10-second connect timeout.
//!
//! Secret discipline (D5): the password is resolved from the OS keychain here,
//! is folded immediately into an `Authorization` header value, never appears in
//! an error message (the registry URL carries no credentials), and is redacted
//! from `Debug`.

use crate::profiles::SchemaRegistryConfig;
use crate::secrets;
use apache_avro::Schema as AvroSchema;
use base64::Engine as _;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};

/// A registry lookup happens inline in the decode path, so a wedged registry
/// must not hold a fetch open for longer than the fetch's own budget.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// The registry's own media type, with plain JSON as the fallback every
/// Confluent-compatible implementation also answers.
const ACCEPT: &str = "application/vnd.schemaregistry.v1+json, application/json";

/// Cap on how much of a registry error body is quoted back.
const ERROR_BODY_LIMIT: usize = 200;

/// What a schema id turned out to be. Only Avro needs the schema itself to
/// decode a payload; a JSON-Schema subject frames plain JSON, and Protobuf
/// needs a descriptor pool this build does not carry yet.
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
        match serde_json::from_str::<Vec<SubjectVersion>>(&body) {
            Ok(versions) => match versions.into_iter().next() {
                Some(first) => (Some(first.subject), Some(first.version)),
                None => (None, None),
            },
            Err(_) => (None, None),
        }
    }

    fn get(&self, path: &str) -> Result<String, String> {
        let url = format!("{}{path}", self.base_url);
        let mut request = ureq::get(&url)
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
        if let Some(authorization) = &self.authorization {
            request = request.header("Authorization", authorization);
        }

        let mut response = request
            .call()
            .map_err(|e| format!("request to {url} failed: {e}"))?;
        let status = response.status();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("reading the response from {url}: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                describe_error(&body)
            ));
        }
        Ok(body)
    }

    /// Poison-tolerant, like the rest of the crate: the guarded state is a
    /// cache and a string, neither of which a panicking holder can leave
    /// inconsistent, and turning one panic into a permanently unusable registry
    /// would be the worse failure.
    fn cache(&self) -> MutexGuard<'_, HashMap<u32, Option<Arc<RegisteredSchema>>>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
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

#[derive(Deserialize)]
struct SubjectVersion {
    subject: String,
    version: i32,
}

fn describe_error(body: &str) -> String {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(ErrorResponse {
            error_code: Some(code),
            message: Some(message),
        }) => format!("{message} (error_code {code})"),
        Ok(ErrorResponse {
            message: Some(message),
            ..
        }) => message,
        _ => truncate(body.trim(), ERROR_BODY_LIMIT),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// A canned Schema Registry over a real socket, shared with the serde
/// pipeline's tests. A std [`std::net::TcpListener`] rather than a mock trait:
/// the thing worth testing is that ureq sends the headers we think it sends,
/// and a trait boundary is exactly where that bug hides.
#[cfg(test)]
pub(crate) mod canned {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// What the server saw, so tests can assert on the request rather than
    /// only on the response.
    #[derive(Debug, Clone)]
    pub(crate) struct Seen {
        pub path: String,
        pub authorization: Option<String>,
        pub accept: Option<String>,
    }

    pub(crate) struct CannedRegistry {
        url: String,
        seen: Arc<Mutex<Vec<Seen>>>,
        shutdown: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl CannedRegistry {
        /// `routes` maps a request path to `(status, body)`. Anything else gets
        /// a 404 with the registry's own error envelope.
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
        let (status, body) = routes.get(&request.path).cloned().unwrap_or((
            404,
            r#"{"error_code":40403,"message":"Schema not found"}"#.to_string(),
        ));
        seen.lock().unwrap_or_else(|e| e.into_inner()).push(request);

        // `Connection: close` keeps this server a one-request-per-socket toy;
        // ureq then opens a fresh connection per request and never blocks
        // waiting on a keep-alive we would have to implement.
        let response = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn read_request(stream: &TcpStream) -> Option<Seen> {
        let mut reader = BufReader::new(stream);
        let mut start = String::new();
        reader.read_line(&mut start).ok()?;
        let path = start.split_whitespace().nth(1)?.to_string();

        let mut authorization = None;
        let mut accept = None;
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
                match name.to_ascii_lowercase().as_str() {
                    "authorization" => authorization = Some(value.trim().to_string()),
                    "accept" => accept = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }
        Some(Seen {
            path,
            authorization,
            accept,
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
            describe_error(r#"{"error_code":40403,"message":"Schema not found"}"#),
            "Schema not found (error_code 40403)"
        );
        assert_eq!(describe_error(r#"{"message":"nope"}"#), "nope");
        assert_eq!(describe_error(" <html>502</html> "), "<html>502</html>");
    }

    #[test]
    fn trailing_slashes_in_the_configured_url_do_not_double_up() {
        let registry = SchemaRegistry::unauthenticated("http://registry.example/");
        assert_eq!(registry.base_url, "http://registry.example");
    }
}
