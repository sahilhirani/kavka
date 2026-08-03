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
        }
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
