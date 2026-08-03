//! Connection profiles. Serialized to JSON in the app data dir; any secret is a
//! [`SecretRef`] into the OS keychain, so exported profiles are secret-free by
//! construction (never add a String secret field to these types).

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub environment: Environment,
    pub bootstrap_servers: Vec<String>,
    pub auth: AuthConfig,
    /// Mutating operations are rejected in core when set (docs/ARCHITECTURE.md D5).
    pub read_only: bool,
    /// Where to resolve the schema ids found in message framing, if this
    /// cluster has a registry.
    ///
    /// `#[serde(default)]` is load-bearing, not decoration: profiles written
    /// before Phase 1 have no such key, and they are read off disk on every
    /// launch. Without it, adding this field would make every existing
    /// `profiles.json` fail to parse — the user would open Kavka to an empty
    /// sidebar and a "couldn't read its connection file" banner.
    #[serde(default)]
    pub schema_registry: Option<SchemaRegistryConfig>,
    /// The Kafka Connect clusters this connection can drive (Phase 3a).
    ///
    /// A list rather than an `Option`, because a cluster routinely has several
    /// Connect groups (a source cluster and a sink cluster is the common
    /// shape) and the UI has to name the one an action goes to. Empty is the
    /// normal state and means "this connection has no Connect".
    ///
    /// `#[serde(default)]` for the same reason as `schema_registry`, one phase
    /// later: every profile on disk today has no such key.
    #[serde(default)]
    pub connect_clusters: Vec<ConnectClusterConfig>,
    /// Where to scrape broker metrics from, if this cluster has a
    /// JMX-exporter/Prometheus endpoint (Phase 4).
    ///
    /// An `Option` rather than a list, unlike `connect_clusters`: the charts
    /// this feeds are cluster-level, and a cluster has one aggregation point
    /// for them — a Prometheus that already scrapes every broker, or one
    /// broker's exporter. Nothing here is required for monitoring to work at
    /// all; lag history needs no broker cooperation whatsoever
    /// (docs/ARCHITECTURE.md D6), so `None` is a fully functional Phase 4
    /// connection with throughput charts missing and said so.
    ///
    /// `#[serde(default)]` for the third time and the same reason: every
    /// profile on disk today has no such key, and they are read on every
    /// launch.
    #[serde(default)]
    pub metrics_endpoint: Option<MetricsEndpointConfig>,
    /// How often the lag sampler takes a reading while this connection is open,
    /// in milliseconds. `None` is [`crate::history::DEFAULT_INTERVAL_MS`].
    ///
    /// The profile is where this lives because it is the only per-cluster thing
    /// that survives a restart, and the interval is a per-cluster judgement: a
    /// laptop dev cluster can be read every five seconds, and somebody's
    /// production coordinator should not be. Stored unclamped and clamped on
    /// use ([`crate::history::clamp_interval_ms`]) so a value written by an
    /// older build — or by hand — samples a little less often rather than not
    /// at all.
    ///
    /// `#[serde(default)]` for the fourth time and the same reason as the three
    /// fields above it: every profile on disk today has no such key, and they
    /// are read on every launch.
    #[serde(default)]
    pub sampler_interval_ms: Option<u32>,
    /// Custom decoders for this cluster's payloads, as sandboxed WebAssembly
    /// modules (Phase 5b, [`crate::wasm_serde`]).
    ///
    /// A list rather than an `Option`, like `connect_clusters` and unlike
    /// `metrics_endpoint`: a cluster routinely carries more than one in-house
    /// format, and the whole point of the glob is that each plugin claims the
    /// topics it understands. Empty is the normal state and means the built-in
    /// ladder decides everything.
    ///
    /// `#[serde(default)]` for the fifth time and the same reason as the four
    /// fields above it: every profile on disk today has no such key, and they
    /// are read on every launch. The failure this guards is not "the new
    /// feature is missing", it is "the sidebar is empty and every connection is
    /// gone".
    ///
    /// **Not a secret and not a keychain entry**: a plugin is a file path and a
    /// list of globs, so it travels in a profile export like the rest of the
    /// document. The path on the machine that imports it may not exist, which
    /// is reported the same way a missing CA file is — see
    /// [`crate::wasm_serde::WasmSerdes::load`].
    #[serde(default)]
    pub wasm_serdes: Vec<WasmSerdeConfig>,
}

/// One custom decoder: a WebAssembly module on disk, and the topics it reads.
///
/// The ABI the module has to implement is documented once, on
/// [`crate::wasm_serde`], and an example implementation with build instructions
/// is in `docs/examples/wasm-serde/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmSerdeConfig {
    /// How the user refers to this plugin. Appears in the payload inspector's
    /// provenance line and in any error the plugin produces.
    pub name: String,
    /// Absolute path to the `.wasm` (or `.wat`) file.
    pub path: String,
    /// Topic-name globs. A plugin with no globs reads nothing — an empty list
    /// is "registered but off", not "everything", because the alternative
    /// silently routes every topic on the cluster through a decoder the user
    /// has not finished configuring.
    #[serde(default)]
    pub applies_to_topics: Vec<String>,
}

impl WasmSerdeConfig {
    /// Whether this plugin claims `topic`.
    ///
    /// The glob is `*` (any run of characters, including none) and `?` (exactly
    /// one character), matched against the whole name — the vocabulary a Kafka
    /// user already has from `kafka-topics.sh --topic 'orders.*'` and from ACL
    /// prefixes. There is no `**` and no character class: a topic name has no
    /// path structure to describe, and every extra piece of syntax is another
    /// thing that behaves differently here than in the shell.
    ///
    /// Matching is case-sensitive, because Kafka topic names are.
    pub fn matches_topic(&self, topic: &str) -> bool {
        self.applies_to_topics
            .iter()
            .any(|pattern| glob_matches(pattern, topic))
    }
}

/// `*` and `?` against a whole string, iteratively.
///
/// Iterative rather than recursive on purpose: the pattern is user input, and a
/// recursive matcher on `*a*a*a*a*b` against a long name is the textbook way to
/// turn a topic list into a hang. This is the standard backtracking-once
/// algorithm — linear in the common case, and bounded by `pattern * text` in
/// the worst.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    // Where to resume if the current `*` turns out to have matched too little.
    let mut star: Option<(usize, usize)> = None;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_at, resume)) = star {
            // Give the last `*` one more character and try again.
            p = star_at + 1;
            t = resume + 1;
            star = Some((star_at, resume + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

impl ConnectionProfile {
    /// Every keychain entry this profile's Connect clusters own.
    ///
    /// [`crate::secrets::SECRET_SUFFIXES`] cannot express these and must not
    /// try: a Connect password's entry name carries the *cluster's* name
    /// ([`connect_password_suffix`]), so the set is a property of one profile
    /// rather than a fixed vocabulary. Anything that purges a profile's
    /// secrets iterates that constant **and** calls this — otherwise a Connect
    /// password outlives the connection it belonged to, which is the
    /// `sr_password` regression that constant was written for, one level down.
    ///
    /// Read off the stored [`SecretRef`]s rather than rebuilt from the naming
    /// convention, so a profile written by another build — or renamed by hand —
    /// still purges the entry it actually references.
    pub fn connect_secret_entries(&self) -> Vec<String> {
        self.connect_clusters
            .iter()
            .filter_map(|cluster| cluster.password.as_ref())
            .map(|secret| secret.entry.clone())
            .collect()
    }

    /// The Connect cluster with this name, if the profile has one.
    pub fn connect_cluster(&self, name: &str) -> Option<&ConnectClusterConfig> {
        self.connect_clusters.iter().find(|c| c.name == name)
    }
}

/// One Kafka Connect cluster (a worker group's REST endpoint).
///
/// Credentials follow [`SchemaRegistryConfig`]'s rule exactly: the username is
/// an identifier and lives in the profile, the password is a [`SecretRef`] into
/// the OS keychain and never touches disk (docs/ARCHITECTURE.md D5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectClusterConfig {
    /// How the user refers to this cluster. Also the key every `connect_*` IPC
    /// call passes, and the tail of its keychain entry name.
    pub name: String,
    /// The worker REST endpoint, e.g. `http://connect-1.internal:8083`.
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretRef>,
}

/// The keychain-entry suffix for one Connect cluster's password —
/// `connect_password/{cluster}`, so the full entry name is
/// `{profile_id}/connect_password/{cluster}` via
/// [`crate::secrets::entry_name`].
///
/// The cluster name is user data and lands in a keychain entry name, so it is
/// used verbatim and nothing is parsed back out of it: the purge path reads
/// [`ConnectionProfile::connect_secret_entries`] instead of reconstructing
/// names, which is what makes a cluster rename safe.
pub fn connect_password_suffix(cluster: &str) -> String {
    format!("connect_password/{cluster}")
}

/// A Confluent-compatible Schema Registry (Confluent, Apicurio, Redpanda).
///
/// Credentials follow the same rule as everything else here: the username is
/// an identifier and lives in the profile, the password is a [`SecretRef`] into
/// the OS keychain and never touches disk (docs/ARCHITECTURE.md D5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaRegistryConfig {
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretRef>,
}

/// A Prometheus-format metrics endpoint — a JMX exporter on a broker, or a
/// Prometheus that already scrapes them all (docs/ARCHITECTURE.md D6).
///
/// Credentials follow [`SchemaRegistryConfig`]'s rule exactly, for the third
/// time: the username is an identifier and lives in the profile, the password
/// is a [`SecretRef`] into the OS keychain and never touches disk (D5). Its
/// keychain suffix is the fixed word `metrics_password` — unlike a Connect
/// cluster's, there is only ever one of these per profile — so
/// [`crate::secrets::SECRET_SUFFIXES`] *can* name it, and does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsEndpointConfig {
    /// The scrape URL, e.g. `http://broker-1.internal:9404/metrics`.
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Staging,
    Prod,
}

/// Reference to a secret stored in the OS keychain (macOS Keychain /
/// Windows Credential Manager) via the `keyring` crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    pub entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthConfig {
    Plaintext,
    Tls {
        ca_pem_path: Option<String>,
        client_cert_pem_path: Option<String>,
        client_key: Option<SecretRef>,
    },
    SaslPlain {
        username: String,
        password: SecretRef,
        tls: bool,
    },
    SaslScram {
        mechanism: ScramMechanism,
        username: String,
        password: SecretRef,
        tls: bool,
    },
    AwsMskIam {
        region: String,
        profile: Option<String>,
    },
    OauthBearer {
        token_endpoint: String,
        client_id: String,
        client_secret: SecretRef,
    },
    Kerberos {
        service_name: String,
        principal: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ScramMechanism {
    #[serde(rename = "SCRAM-SHA-256")]
    Sha256,
    #[serde(rename = "SCRAM-SHA-512")]
    Sha512,
}

/// Envelope wrapping an exported profile set. Versioned so a future profile
/// shape can be migrated (or rejected) instead of silently half-parsed.
/// Generic over the payload so exports can borrow and imports can own.
#[derive(Debug, Serialize, Deserialize)]
struct Envelope<P> {
    kavka_profiles: u32,
    profiles: P,
}

const VERSION_FIELD: &str = "kavka_profiles";

/// Envelope version this build writes and reads.
pub const EXPORT_VERSION: u32 = 1;

/// Serializes profiles into the shareable export envelope. The result is
/// secret-free by construction — profiles carry [`SecretRef`]s, never values
/// (docs/ARCHITECTURE.md D5) — and the debug assertion below fails loudly if
/// anyone ever adds a String secret field to the profile types.
pub fn export_json(profiles: &[ConnectionProfile]) -> String {
    let json = serde_json::to_string_pretty(&Envelope {
        kavka_profiles: EXPORT_VERSION,
        profiles,
    })
    .expect("connection profiles are infallibly serializable");
    debug_assert_eq!(
        leaked_secret_field(
            &serde_json::from_str::<serde_json::Value>(&json).expect("just serialized")
        ),
        None,
        "profile export carried a secret value (docs/ARCHITECTURE.md D5)"
    );
    json
}

/// Parses an export envelope produced by [`export_json`]. Rejects malformed
/// JSON, documents that aren't Kavka exports, and versions this build can't
/// read — each with a message that says what to do about it.
pub fn import_json(raw: &str) -> Result<Vec<ConnectionProfile>> {
    let doc: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| Error::Other(format!("not valid JSON: {e}")))?;

    let version = match doc.get(VERSION_FIELD) {
        Some(v) => v.as_u64().ok_or_else(|| {
            Error::Other(format!(
                "malformed profile export: \"{VERSION_FIELD}\" must be a version number"
            ))
        })?,
        None => {
            return Err(Error::Other(format!(
                "not a Kavka profile export: missing the \"{VERSION_FIELD}\" version field"
            )))
        }
    };
    if version != u64::from(EXPORT_VERSION) {
        return Err(Error::Other(if version > u64::from(EXPORT_VERSION) {
            format!(
                "profile export version {version} was written by a newer Kavka; \
                 this build reads version {EXPORT_VERSION} — update Kavka to import it"
            )
        } else {
            format!(
                "profile export version {version} is no longer supported; \
                 this build reads version {EXPORT_VERSION}"
            )
        }));
    }

    let envelope: Envelope<Vec<ConnectionProfile>> = serde_json::from_value(doc)
        .map_err(|e| Error::Other(format!("malformed profile export: {e}")))?;
    Ok(envelope.profiles)
}

/// What to do when an imported profile's id already exists in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStrategy {
    /// Leave the stored profile untouched.
    Skip,
    /// Overwrite the stored profile with the imported one.
    Replace,
}

impl std::str::FromStr for ImportStrategy {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "skip" => Ok(Self::Skip),
            "replace" => Ok(Self::Replace),
            other => Err(Error::Other(format!(
                "unknown import strategy {other:?}; expected \"skip\" or \"replace\""
            ))),
        }
    }
}

/// Outcome of [`ProfileStore::import`]. `imported` counts profiles whose id was
/// new; existing ids land in `skipped` or `replaced` per the strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub replaced: usize,
}

/// Field names that may only ever hold a [`SecretRef`] (or nothing). Returns
/// the first offender, so the export assertion and its test can name it.
fn leaked_secret_field(value: &serde_json::Value) -> Option<String> {
    fn is_secret_shaped(key: &str) -> bool {
        [
            "password",
            "secret",
            "credential",
            "token",
            "passphrase",
            "key",
        ]
        .iter()
        .any(|needle| {
            key == *needle
                || key
                    .strip_suffix(needle)
                    .is_some_and(|prefix| prefix.ends_with('_'))
        })
    }

    /// A `SecretRef` serializes as `{"entry": "..."}`; `None` as `null`.
    fn is_secret_ref(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Null => true,
            serde_json::Value::Object(map) => {
                map.len() == 1 && map.get("entry").is_some_and(serde_json::Value::is_string)
            }
            _ => false,
        }
    }

    match value {
        serde_json::Value::Object(map) => map.iter().find_map(|(key, child)| {
            if is_secret_shaped(key) && !is_secret_ref(child) {
                return Some(key.clone());
            }
            leaked_secret_field(child)
        }),
        serde_json::Value::Array(items) => items.iter().find_map(leaked_secret_field),
        _ => None,
    }
}

/// JSON-file-backed profile store in the app config dir. Writes are
/// write-tmp-then-rename so a crash can't leave a truncated file, and
/// read-modify-write sequences hold `write_lock` so concurrent IPC commands
/// can't lose updates.
pub struct ProfileStore {
    dir: PathBuf,
    write_lock: std::sync::Mutex<()>,
}

impl ProfileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            write_lock: std::sync::Mutex::new(()),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("profiles.json")
    }

    pub fn list(&self) -> Result<Vec<ConnectionProfile>> {
        let path = self.file();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| Error::Other(format!("reading {}: {e}", path.display())))?;
        serde_json::from_str(&raw)
            .map_err(|e| Error::Other(format!("parsing {}: {e}", path.display())))
    }

    pub fn upsert(&self, profile: ConnectionProfile) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut all = self.list()?;
        match all.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile,
            None => all.push(profile),
        }
        self.write(&all)
    }

    /// Merges imported profiles into the store under `strategy`. Imported SASL
    /// profiles reference keychain entries that don't exist on this machine —
    /// that's expected; connecting reports it (see `secrets::resolve`).
    pub fn import(
        &self,
        profiles: Vec<ConnectionProfile>,
        strategy: ImportStrategy,
    ) -> Result<ImportReport> {
        let _guard = self.write_lock.lock().unwrap();
        let mut all = self.list()?;
        let mut report = ImportReport::default();
        for profile in profiles {
            match all.iter_mut().find(|p| p.id == profile.id) {
                Some(slot) => match strategy {
                    ImportStrategy::Skip => report.skipped += 1,
                    ImportStrategy::Replace => {
                        *slot = profile;
                        report.replaced += 1;
                    }
                },
                None => {
                    all.push(profile);
                    report.imported += 1;
                }
            }
        }
        if report.imported + report.replaced > 0 {
            self.write(&all)?;
        }
        Ok(report)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut all = self.list()?;
        all.retain(|p| p.id != id);
        self.write(&all)
    }

    fn write(&self, all: &[ConnectionProfile]) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| Error::Other(format!("creating {}: {e}", self.dir.display())))?;
        let json = serde_json::to_vec_pretty(all)
            .map_err(|e| Error::Other(format!("serializing profiles: {e}")))?;
        let tmp = self.dir.join("profiles.json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| Error::Other(format!("writing {}: {e}", tmp.display())))?;
        fs::rename(&tmp, self.file())
            .map_err(|e| Error::Other(format!("replacing profiles.json: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Scratch config dir, removed on drop — keeps store tests dependency-free.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kavka-profiles-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn store(&self) -> ProfileStore {
            ProfileStore::new(self.0.clone())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn profile(id: &str, name: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: id.into(),
            name: name.into(),
            environment: Environment::Prod,
            bootstrap_servers: vec!["broker-1:9093".into(), "broker-2:9093".into()],
            auth: AuthConfig::SaslScram {
                mechanism: ScramMechanism::Sha512,
                username: "kavka-app".into(),
                password: SecretRef {
                    entry: format!("{id}/password"),
                },
                tls: true,
            },
            read_only: true,
            schema_registry: Some(SchemaRegistryConfig {
                url: "https://registry.example".into(),
                username: Some("sr-key".into()),
                password: Some(SecretRef {
                    entry: format!("{id}/schema_registry_password"),
                }),
            }),
            connect_clusters: vec![
                ConnectClusterConfig {
                    name: "sources".into(),
                    url: "https://connect-1.example:8083".into(),
                    username: Some("connect-user".into()),
                    password: Some(SecretRef {
                        entry: entry_for(id, "sources"),
                    }),
                },
                // Anonymous workers are the common on-prem shape, and the one
                // that would hide a `None`-handling bug in the purge path.
                ConnectClusterConfig {
                    name: "sinks".into(),
                    url: "http://connect-2.example:8083".into(),
                    username: None,
                    password: None,
                },
            ],
            metrics_endpoint: Some(MetricsEndpointConfig {
                url: "https://prometheus.example/federate".into(),
                username: Some("scrape".into()),
                password: Some(SecretRef {
                    entry: crate::secrets::entry_name(id, "metrics_password"),
                }),
            }),
            sampler_interval_ms: Some(30_000),
            wasm_serdes: Vec::new(),
        }
    }

    fn entry_for(profile_id: &str, cluster: &str) -> String {
        crate::secrets::entry_name(profile_id, &connect_password_suffix(cluster))
    }

    /// One of every auth variant, so the no-secret-values check covers every
    /// field that could ever hold one.
    fn every_auth_variant() -> Vec<ConnectionProfile> {
        let variants = [
            AuthConfig::Plaintext,
            AuthConfig::Tls {
                ca_pem_path: Some("/etc/ssl/ca.pem".into()),
                client_cert_pem_path: Some("/etc/ssl/client.pem".into()),
                client_key: Some(SecretRef {
                    entry: "p/client_key".into(),
                }),
            },
            AuthConfig::SaslPlain {
                username: "svc".into(),
                password: SecretRef {
                    entry: "p/password".into(),
                },
                tls: true,
            },
            AuthConfig::SaslScram {
                mechanism: ScramMechanism::Sha256,
                username: "svc".into(),
                password: SecretRef {
                    entry: "p/password".into(),
                },
                tls: false,
            },
            AuthConfig::AwsMskIam {
                region: "eu-west-1".into(),
                profile: Some("default".into()),
            },
            AuthConfig::OauthBearer {
                token_endpoint: "https://idp.example/oauth2/token".into(),
                client_id: "kavka".into(),
                client_secret: SecretRef {
                    entry: "p/client_secret".into(),
                },
            },
            AuthConfig::Kerberos {
                service_name: "kafka".into(),
                principal: "kavka@EXAMPLE".into(),
            },
        ];
        variants
            .into_iter()
            .enumerate()
            .map(|(i, auth)| ConnectionProfile {
                auth,
                ..profile(&format!("p{i}"), "variant")
            })
            .collect()
    }

    #[test]
    fn export_import_roundtrip_preserves_profiles() {
        let original = every_auth_variant();
        let json = export_json(&original);
        let back = import_json(&json).expect("roundtrip");

        assert_eq!(back.len(), original.len());
        // Compared through JSON: the profile types are deliberately not PartialEq.
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
    }

    #[test]
    fn export_writes_the_versioned_envelope() {
        let doc: serde_json::Value =
            serde_json::from_str(&export_json(&[profile("a", "A")])).unwrap();
        assert_eq!(doc[VERSION_FIELD], serde_json::json!(EXPORT_VERSION));
        assert_eq!(doc["profiles"].as_array().unwrap().len(), 1);
        assert_eq!(doc["profiles"][0]["id"], "a");
    }

    #[test]
    fn export_never_contains_secret_values() {
        let json = export_json(&every_auth_variant());
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Every secret-shaped field holds a SecretRef (or nothing), never a value.
        assert_eq!(leaked_secret_field(&doc), None);
        // Entry names are refs, not values — the keychain is the only holder.
        assert!(json.contains("p/password"));

        // The detector isn't vacuous: a planted value is caught.
        let mut leaked = doc.clone();
        leaked["profiles"][2]["password"] = serde_json::json!("hunter2");
        assert_eq!(leaked_secret_field(&leaked).as_deref(), Some("password"));
        // ...including one hidden inside a SecretRef-shaped object.
        let mut nested = doc.clone();
        nested["profiles"][5]["client_secret"] = serde_json::json!({"entry": "e", "value": "s3"});
        assert_eq!(
            leaked_secret_field(&nested).as_deref(),
            Some("client_secret")
        );
        // A non-secret field that merely mentions "token" is not a false positive.
        assert!(json.contains("token_endpoint"));
    }

    /// The field arrived in Phase 1; every profile written before it has no
    /// such key, and those files are read on every launch.
    #[test]
    fn profiles_written_before_the_schema_registry_field_still_load() {
        let legacy = r#"{
            "kavka_profiles": 1,
            "profiles": [{
                "id": "old",
                "name": "Legacy",
                "environment": "dev",
                "bootstrap_servers": ["localhost:9092"],
                "auth": {"kind": "plaintext"},
                "read_only": false
            }]
        }"#;
        let profiles = import_json(legacy).expect("a Phase 0 profile still parses");
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].schema_registry.is_none());
    }

    #[test]
    fn a_schema_registry_password_travels_as_a_reference() {
        let json = export_json(&[profile("a", "A")]);
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        let registry = &doc["profiles"][0]["schema_registry"];

        assert_eq!(registry["url"], "https://registry.example");
        assert_eq!(registry["username"], "sr-key");
        assert_eq!(
            registry["password"],
            serde_json::json!({"entry": "a/schema_registry_password"})
        );
        assert_eq!(leaked_secret_field(&doc), None);

        // The detector reaches into the nested config too.
        let mut leaked = doc.clone();
        leaked["profiles"][0]["schema_registry"]["password"] = serde_json::json!("hunter2");
        assert_eq!(leaked_secret_field(&leaked).as_deref(), Some("password"));
    }

    #[test]
    fn connect_clusters_survive_an_export_import_roundtrip() {
        let original = profile("a", "A");
        let back = import_json(&export_json(&[original])).expect("roundtrip");
        let clusters = &back[0].connect_clusters;

        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].name, "sources");
        assert_eq!(clusters[0].url, "https://connect-1.example:8083");
        assert_eq!(clusters[0].username.as_deref(), Some("connect-user"));
        assert_eq!(
            clusters[0].password.as_ref().map(|s| s.entry.as_str()),
            Some("a/connect_password/sources")
        );
        // The anonymous worker roundtrips as anonymous, not as an empty
        // credential.
        assert_eq!(clusters[1].name, "sinks");
        assert!(clusters[1].username.is_none());
        assert!(clusters[1].password.is_none());
    }

    /// The field arrived in Phase 3a; every profile written before it has no
    /// such key, and those files are read on every launch. This is the same
    /// regression `schema_registry` was `#[serde(default)]`-ed for, one phase
    /// later.
    #[test]
    fn profiles_written_before_the_connect_field_still_load() {
        let legacy = r#"{
            "kavka_profiles": 1,
            "profiles": [{
                "id": "old",
                "name": "Legacy",
                "environment": "dev",
                "bootstrap_servers": ["localhost:9092"],
                "auth": {"kind": "plaintext"},
                "read_only": false,
                "schema_registry": {"url": "https://registry.example"}
            }]
        }"#;
        let profiles = import_json(legacy).expect("a Phase 1 profile still parses");
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].connect_clusters.is_empty());
        assert!(profiles[0].connect_secret_entries().is_empty());
    }

    #[test]
    fn a_connect_password_travels_as_a_reference() {
        let json = export_json(&[profile("a", "A")]);
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        let clusters = &doc["profiles"][0]["connect_clusters"];

        assert_eq!(
            clusters[0]["password"],
            serde_json::json!({"entry": "a/connect_password/sources"})
        );
        assert_eq!(clusters[1]["password"], serde_json::Value::Null);
        assert_eq!(leaked_secret_field(&doc), None);

        // The detector reaches into the new location too — it is an array of
        // objects two levels down, which is exactly the shape a field-name
        // scan is most likely to miss.
        let mut leaked = doc.clone();
        leaked["profiles"][0]["connect_clusters"][0]["password"] = serde_json::json!("hunter2");
        assert_eq!(leaked_secret_field(&leaked).as_deref(), Some("password"));
        // ...including one hidden inside a SecretRef-shaped object.
        let mut nested = doc.clone();
        nested["profiles"][0]["connect_clusters"][1]["password"] =
            serde_json::json!({"entry": "e", "value": "s3"});
        assert_eq!(leaked_secret_field(&nested).as_deref(), Some("password"));
    }

    /// The field arrived in Phase 4; every profile written before it has no
    /// such key. Third verse, same as the first — and the reason this test
    /// exists a third time is that the failure it guards is not "the new
    /// feature is missing", it is "the sidebar is empty and every connection
    /// is gone".
    #[test]
    fn profiles_written_before_the_metrics_field_still_load() {
        let legacy = r#"{
            "kavka_profiles": 1,
            "profiles": [{
                "id": "old",
                "name": "Legacy",
                "environment": "dev",
                "bootstrap_servers": ["localhost:9092"],
                "auth": {"kind": "plaintext"},
                "read_only": false,
                "schema_registry": {"url": "https://registry.example"},
                "connect_clusters": [{"name": "sources", "url": "http://connect:8083"}]
            }]
        }"#;
        let profiles = import_json(legacy).expect("a Phase 3 profile still parses");
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].metrics_endpoint.is_none());
        // Both Phase 4 additions default, not just the one the contract named:
        // an absent interval is "whatever the sampler's default is", which is
        // the only answer that does not need a second default written down.
        assert!(profiles[0].sampler_interval_ms.is_none());
        // The rest of the profile is untouched by the addition.
        assert_eq!(profiles[0].connect_clusters.len(), 1);
        assert!(profiles[0].schema_registry.is_some());
    }

    /// The interval survives a round trip through the store, which is the whole
    /// point of putting it on the profile: an editor that writes a value the
    /// file drops silently is worse than an editor with no field at all, since
    /// the user believes they changed something.
    #[test]
    fn a_sampler_interval_survives_export_and_import() {
        let mut profile = profile("p1", "Orders");
        profile.sampler_interval_ms = Some(45_000);
        let round_tripped = import_json(&export_json(&[profile])).expect("its own export parses");
        assert_eq!(round_tripped[0].sampler_interval_ms, Some(45_000));
    }

    /// An unauthenticated exporter — the common on-prem shape, and the one that
    /// would hide a `None`-handling bug — parses as anonymous rather than as an
    /// empty credential.
    #[test]
    fn a_metrics_endpoint_may_be_anonymous() {
        let legacy = r#"{
            "kavka_profiles": 1,
            "profiles": [{
                "id": "old",
                "name": "Legacy",
                "environment": "dev",
                "bootstrap_servers": ["localhost:9092"],
                "auth": {"kind": "plaintext"},
                "read_only": false,
                "metrics_endpoint": {"url": "http://broker-1:9404/metrics"}
            }]
        }"#;
        let profiles = import_json(legacy).expect("an anonymous exporter parses");
        let endpoint = profiles[0].metrics_endpoint.as_ref().expect("endpoint");
        assert_eq!(endpoint.url, "http://broker-1:9404/metrics");
        assert!(endpoint.username.is_none());
        assert!(endpoint.password.is_none());
    }

    #[test]
    fn a_metrics_password_travels_as_a_reference() {
        let json = export_json(&[profile("a", "A")]);
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        let endpoint = &doc["profiles"][0]["metrics_endpoint"];

        assert_eq!(endpoint["url"], "https://prometheus.example/federate");
        assert_eq!(endpoint["username"], "scrape");
        assert_eq!(
            endpoint["password"],
            serde_json::json!({"entry": "a/metrics_password"})
        );
        assert_eq!(leaked_secret_field(&doc), None);

        // The detector reaches into the new location too.
        let mut leaked = doc.clone();
        leaked["profiles"][0]["metrics_endpoint"]["password"] = serde_json::json!("hunter2");
        assert_eq!(leaked_secret_field(&leaked).as_deref(), Some("password"));
        // ...including one hidden inside a SecretRef-shaped object.
        let mut nested = doc;
        nested["profiles"][0]["metrics_endpoint"]["password"] =
            serde_json::json!({"entry": "e", "value": "s3"});
        assert_eq!(leaked_secret_field(&nested).as_deref(), Some("password"));
    }

    /// Unlike a Connect password, this one has a fixed suffix — so the purge
    /// path that iterates the constant covers it, and this is the tripwire that
    /// says so.
    #[test]
    fn the_metrics_password_is_named_by_the_fixed_vocabulary() {
        assert!(crate::secrets::SECRET_SUFFIXES.contains(&"metrics_password"));
        assert_eq!(
            profile("a", "A")
                .metrics_endpoint
                .and_then(|endpoint| endpoint.password)
                .map(|secret| secret.entry),
            Some(crate::secrets::entry_name("a", "metrics_password"))
        );
    }

    /// `SECRET_SUFFIXES` cannot name these — the cluster's name is in the entry
    /// — so a purge that only iterates the constant leaves a Connect password
    /// behind. That is the `sr_password` regression one level down, and this is
    /// the tripwire for it.
    #[test]
    fn a_profiles_connect_entries_are_enumerable_for_the_purge_path() {
        let profile = profile("a", "A");
        assert_eq!(
            profile.connect_secret_entries(),
            vec!["a/connect_password/sources".to_string()]
        );
        assert_eq!(
            connect_password_suffix("sources"),
            "connect_password/sources"
        );
        assert_eq!(
            crate::secrets::entry_name("a", &connect_password_suffix("sources")),
            "a/connect_password/sources"
        );
        // No fixed suffix could have covered it.
        assert!(!crate::secrets::SECRET_SUFFIXES
            .iter()
            .any(|s| s.contains("connect")));
    }

    /// Read off the stored refs, not rebuilt from the naming convention — so a
    /// renamed cluster still purges the entry it actually holds.
    #[test]
    fn the_purge_list_follows_the_stored_reference_not_the_current_name() {
        let mut profile = profile("a", "A");
        profile.connect_clusters[0].name = "renamed".into();
        assert_eq!(
            profile.connect_secret_entries(),
            vec!["a/connect_password/sources".to_string()]
        );
    }

    #[test]
    fn a_cluster_is_looked_up_by_name() {
        let profile = profile("a", "A");
        assert_eq!(profile.connect_cluster("sinks").unwrap().username, None);
        assert!(profile.connect_cluster("nope").is_none());
    }

    /// The field arrived in Phase 5b; every profile written before it has no
    /// such key. Fifth verse, same as the first — and the reason this test
    /// exists a fifth time is that the failure it guards is not "the new
    /// feature is missing", it is "the sidebar is empty and every connection
    /// is gone".
    #[test]
    fn profiles_written_before_the_wasm_serde_field_still_load() {
        let legacy = r#"{
            "kavka_profiles": 1,
            "profiles": [{
                "id": "old",
                "name": "Legacy",
                "environment": "dev",
                "bootstrap_servers": ["localhost:9092"],
                "auth": {"kind": "plaintext"},
                "read_only": false,
                "metrics_endpoint": {"url": "http://broker-1:9404/metrics"},
                "sampler_interval_ms": 30000
            }]
        }"#;
        let profiles = import_json(legacy).expect("a Phase 4 profile still parses");
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].wasm_serdes.is_empty());
        // The rest of the profile is untouched by the addition.
        assert!(profiles[0].metrics_endpoint.is_some());
        assert_eq!(profiles[0].sampler_interval_ms, Some(30_000));
    }

    /// A plugin is a path and some globs — no secret, so it travels in an
    /// export like the rest of the document, and the secret detector has
    /// nothing to say about it.
    #[test]
    fn wasm_serdes_survive_an_export_import_roundtrip() {
        let mut original = profile("a", "A");
        original.wasm_serdes = vec![WasmSerdeConfig {
            name: "acme-protobuf".into(),
            path: "/opt/kavka/acme.wasm".into(),
            applies_to_topics: vec!["acme.*".into(), "orders.v?".into()],
        }];
        let back = import_json(&export_json(&[original])).expect("roundtrip");

        assert_eq!(back[0].wasm_serdes.len(), 1);
        assert_eq!(back[0].wasm_serdes[0].name, "acme-protobuf");
        assert_eq!(back[0].wasm_serdes[0].path, "/opt/kavka/acme.wasm");
        assert_eq!(
            back[0].wasm_serdes[0].applies_to_topics,
            vec!["acme.*".to_string(), "orders.v?".to_string()]
        );

        let doc: serde_json::Value =
            serde_json::from_str(&export_json(&back)).expect("its own export parses");
        assert_eq!(leaked_secret_field(&doc), None);
    }

    /// The glob vocabulary, exhaustively — this is what decides which decoder
    /// reads a topic, so it is worth a table rather than a spot check.
    #[test]
    fn topic_globs_match_the_way_the_shell_does() {
        let cases = [
            ("*", "orders.v2", true),
            ("orders.v2", "orders.v2", true),
            ("orders.v2", "orders.v3", false),
            ("orders.*", "orders.v2", true),
            ("orders.*", "orders.", true),
            ("orders.*", "orders", false),
            ("*.v2", "orders.v2", true),
            ("*.v2", "orders.v2.dlq", false),
            ("*orders*", "eu.orders.v2", true),
            ("orders.v?", "orders.v2", true),
            ("orders.v?", "orders.v20", false),
            ("orders.v?", "orders.v", false),
            // The pathological shape a recursive matcher hangs on.
            (
                "*a*a*a*a*a*b",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                false,
            ),
            ("", "", true),
            ("", "orders", false),
            ("*", "", true),
            // Case-sensitive, because Kafka topic names are.
            ("Orders.*", "orders.v2", false),
        ];
        for (pattern, topic, expected) in cases {
            let config = WasmSerdeConfig {
                name: "p".into(),
                path: "p.wasm".into(),
                applies_to_topics: vec![pattern.to_string()],
            };
            assert_eq!(
                config.matches_topic(topic),
                expected,
                "{pattern:?} against {topic:?}"
            );
        }
    }

    /// A plugin with no globs reads nothing. The alternative — an empty list
    /// meaning "everything" — routes every topic on the cluster through a
    /// decoder the user has not finished configuring.
    #[test]
    fn a_plugin_with_no_globs_claims_nothing() {
        let config = WasmSerdeConfig {
            name: "p".into(),
            path: "p.wasm".into(),
            applies_to_topics: Vec::new(),
        };
        assert!(!config.matches_topic("orders.v2"));
        assert!(!config.matches_topic(""));
    }

    /// Any glob matching is enough — the list is an OR.
    #[test]
    fn several_globs_are_an_or() {
        let config = WasmSerdeConfig {
            name: "p".into(),
            path: "p.wasm".into(),
            applies_to_topics: vec!["acme.*".into(), "legacy-*".into()],
        };
        assert!(config.matches_topic("acme.orders"));
        assert!(config.matches_topic("legacy-orders"));
        assert!(!config.matches_topic("orders.v2"));
    }

    #[test]
    fn import_rejects_unknown_versions() {
        let newer = import_json(r#"{"kavka_profiles": 99, "profiles": []}"#).unwrap_err();
        assert!(
            newer.to_string().contains("update Kavka"),
            "unhelpful: {newer}"
        );
        let older = import_json(r#"{"kavka_profiles": 0, "profiles": []}"#).unwrap_err();
        assert!(
            older.to_string().contains("no longer supported"),
            "unhelpful: {older}"
        );
    }

    #[test]
    fn import_rejects_malformed_documents() {
        for (raw, expected) in [
            ("{not json", "not valid JSON"),
            ("[]", "not a Kavka profile export"),
            (r#"{"profiles": []}"#, "not a Kavka profile export"),
            (
                r#"{"kavka_profiles": "1", "profiles": []}"#,
                "version number",
            ),
            (
                r#"{"kavka_profiles": 1, "profiles": [{"id": "a"}]}"#,
                "malformed profile export",
            ),
        ] {
            let err = import_json(raw).unwrap_err().to_string();
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn import_skip_keeps_existing_profiles() {
        let dir = TempDir::new();
        let store = dir.store();
        store.upsert(profile("a", "stored A")).unwrap();

        let report = store
            .import(
                vec![profile("a", "imported A"), profile("b", "imported B")],
                ImportStrategy::Skip,
            )
            .unwrap();

        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                skipped: 1,
                replaced: 0,
            }
        );
        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().find(|p| p.id == "a").unwrap().name, "stored A");
        assert_eq!(all.iter().find(|p| p.id == "b").unwrap().name, "imported B");
    }

    #[test]
    fn import_replace_overwrites_existing_profiles() {
        let dir = TempDir::new();
        let store = dir.store();
        store.upsert(profile("a", "stored A")).unwrap();

        let report = store
            .import(
                vec![profile("a", "imported A"), profile("b", "imported B")],
                ImportStrategy::Replace,
            )
            .unwrap();

        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                skipped: 0,
                replaced: 1,
            }
        );
        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().find(|p| p.id == "a").unwrap().name, "imported A");
    }

    #[test]
    fn import_into_empty_store_roundtrips_an_export() {
        let dir = TempDir::new();
        let store = dir.store();
        let report = store
            .import(
                import_json(&export_json(&every_auth_variant())).unwrap(),
                ImportStrategy::Skip,
            )
            .unwrap();
        assert_eq!(report.imported, 7);
        assert_eq!(store.list().unwrap().len(), 7);
    }

    #[test]
    fn import_strategy_parses_from_ipc_strings() {
        assert_eq!(
            "skip".parse::<ImportStrategy>().unwrap(),
            ImportStrategy::Skip
        );
        assert_eq!(
            "replace".parse::<ImportStrategy>().unwrap(),
            ImportStrategy::Replace
        );
        let err = "merge".parse::<ImportStrategy>().unwrap_err().to_string();
        assert!(
            err.contains("\"skip\"") && err.contains("\"replace\""),
            "{err}"
        );
    }
}
