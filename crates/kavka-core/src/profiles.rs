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
