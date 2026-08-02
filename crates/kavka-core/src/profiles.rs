//! Connection profiles. Serialized to JSON in the app data dir; any secret is a
//! [`SecretRef`] into the OS keychain, so exported profiles are secret-free by
//! construction (never add a String secret field to these types).

use serde::{Deserialize, Serialize};

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
    SaslPlain { username: String, password: SecretRef, tls: bool },
    SaslScram { mechanism: ScramMechanism, username: String, password: SecretRef, tls: bool },
    AwsMskIam { region: String, profile: Option<String> },
    OauthBearer { token_endpoint: String, client_id: String, client_secret: SecretRef },
    Kerberos { service_name: String, principal: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ScramMechanism {
    #[serde(rename = "SCRAM-SHA-256")]
    Sha256,
    #[serde(rename = "SCRAM-SHA-512")]
    Sha512,
}
