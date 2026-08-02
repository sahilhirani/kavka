//! Kavka core: everything that talks to Kafka lives here. The Tauri shell and
//! (later) the Team Server web console are thin frontends over this crate.
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
