//! Kavka core: everything that talks to Kafka lives here. The Tauri shell and
//! (later) the companion CLI are thin frontends over this crate.
//!
//! Layout mirrors docs/ARCHITECTURE.md:
//! - [`profiles`]: connection profiles; secrets live in the OS keychain only.
//! - [`connection`]: authenticated cluster connections, read-only enforcement.
//! - [`admin`]: topics, configs, ACLs, quotas, groups, reassignment.
//! - [`serdes`]: bytes -> canonical JSON with schema metadata.
//! - [`search`]: the streaming unbounded search engine.

pub mod admin;
pub mod connection;
pub mod profiles;
pub mod search;
pub mod serdes;

/// OS-keychain access (macOS Keychain / Windows Credential Manager). The only
/// place secret VALUES ever pass through; everything else holds
/// [`profiles::SecretRef`]s.
pub mod secrets {
    use crate::profiles::SecretRef;
    use crate::Result;

    const SERVICE: &str = "kavka";

    pub fn set(entry: &str, value: &str) -> Result<()> {
        keyring::Entry::new(SERVICE, entry)?.set_password(value)?;
        Ok(())
    }

    pub fn resolve(secret: &SecretRef) -> Result<String> {
        Ok(keyring::Entry::new(SERVICE, &secret.entry)?.get_password()?)
    }

    /// Idempotent: deleting a missing entry is not an error.
    pub fn delete(entry: &str) -> Result<()> {
        match keyring::Entry::new(SERVICE, entry)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connection is read-only: {0} is a mutating operation")]
    ReadOnly(&'static str),
    #[error("keychain: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
