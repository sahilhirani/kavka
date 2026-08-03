//! Kavka core: everything that talks to Kafka lives here. The Tauri shell and
//! (later) the companion CLI are thin frontends over this crate.
//!
//! Layout mirrors docs/ARCHITECTURE.md:
//! - [`profiles`]: connection profiles; secrets live in the OS keychain only.
//! - [`connection`]: authenticated cluster connections, read-only enforcement.
//! - [`admin`]: topics, configs, quotas, groups, reassignment.
//! - [`acl`]: access-control bindings — list, create, delete.
//! - [`connect`]: Kafka Connect REST clients (multi-cluster, per profile).
//! - [`consume`]: bounded fetch and live tail.
//! - [`produce`]: single records and bulk generation; every path is
//!   read-only-checked before it touches the network.
//! - [`cancel`]: the cooperative stop flag long reads check.
//! - [`serdes`]: bytes -> canonical JSON with schema metadata.
//! - [`sr`]: Schema Registry clients.
//! - [`search`]: the streaming unbounded search engine.
//! - [`sql`]: SQL over a bounded scan window, on DataFusion.
//! - [`history`]: the lag sampler and its durable store (redb).
//! - [`metrics`]: Prometheus/JMX-exporter scraping, in memory only.
//! - [`alerts`]: rules, hysteresis, and the webhook that carries an event out.
//! - [`streams`]: Kafka Streams topology inference — pure, and carrying its own
//!   list of what it cannot know.
//! - [`xcluster`]: work that spans two clusters — copying a topic's records
//!   into another one, diffing two topics' configurations, and carrying a
//!   consumer group's position across. It owns no Kafka machinery of its own:
//!   it composes [`search`]'s filter, [`produce`]'s producer settings and
//!   [`admin`]'s offset rules, which is what keeps one answer to "is this
//!   record durable" and one sentence for "this group is still running".
//! - [`protocol`]: hand-rolled Kafka wire frames for the admin RPCs librdkafka
//!   does not expose — quorum, leader election, reassignment, client quotas,
//!   share groups (docs/ARCHITECTURE.md D2).
//!
//! The three Phase 4 modules divide along one line, and it is a durability
//! line rather than a subject-matter one: [`history`] owns everything written
//! to disk, [`metrics`] owns everything that only ever lives in RAM, and
//! [`alerts`] reads both and owns nothing but its rules. That is why the alert
//! evaluator is a pure function over observations it is handed — it can be
//! driven through a synthetic week in a millisecond, which is the only way a
//! `for_ms`/cooldown state machine ever gets properly tested.

pub mod acl;
pub mod admin;
pub mod alerts;
pub mod cancel;
pub mod connect;
pub mod connection;
pub mod consume;
pub mod history;
/// Ungated: masking is a regex over decoded text, and the whole feature is
/// testable — rules, store and all — with no cluster in sight.
pub mod masking;
pub mod metrics;
/// Ungated: a grammar is a table of strings, and the whole point of it is that
/// it can be checked exhaustively without a cluster, a model or a network.
pub mod nlq;
pub mod produce;
pub mod profiles;
pub mod search;
pub mod serdes;
/// Ungated, like [`search`], and for the same reason: the wire types, the
/// column list and the surface documentation are strings and structs the UI
/// needs, and none of them involves librdkafka. The engine inside is gated —
/// `datafusion` rides on the `kafka` feature, because a build that cannot open
/// a cluster connection has no records to run SQL over (see Cargo.toml).
pub mod sql;
pub mod sr;
pub mod streams;
/// Behind `wasm-serdes` (which `kafka-ssl` includes), because `wasmtime` is:
/// see Cargo.toml for what that costs and why it is worth it. The *trait* the
/// pipeline calls through — [`serdes::CustomDecoder`] — is ungated, so the
/// decode ladder has the step whether or not this module is compiled.
#[cfg(feature = "wasm-serdes")]
pub mod wasm_serde;
pub mod xcluster;

/// Behind `kafka` for one reason, spelled out at the top of the module: its
/// OAUTHBEARER path reuses [`connection::auth::TokenSource`], which is gated on
/// `kafka` because it returns an `rdkafka` type. A build without the feature
/// cannot open a cluster connection at all, so it has nothing to route these
/// RPCs to.
#[cfg(feature = "kafka")]
pub mod protocol;

/// OS-keychain access (macOS Keychain / Windows Credential Manager). The only
/// place secret VALUES ever pass through; everything else holds
/// [`profiles::SecretRef`]s.
pub mod secrets {
    use crate::profiles::SecretRef;
    use crate::{Error, Result};

    const SERVICE: &str = "kavka";

    /// Every keychain entry a profile can own, as the suffix after its id:
    /// the entry name is always `{profile_id}/{suffix}`.
    ///
    /// **This list and the ProfileEditor's `entry` vocabulary
    /// (`apps/desktop/src/ProfileEditor.tsx`, the `entry` object in `save`)
    /// must match exactly.** They are the two halves of one contract: the
    /// editor writes these entries, and deleting a profile purges them. When
    /// the two drifted, `sr_password` was written on save and left behind on
    /// delete — a Schema Registry password outliving the connection it
    /// belonged to. Anything that enumerates secrets iterates this constant
    /// rather than repeating the strings.
    ///
    /// **This list is not the whole story, and cannot be.** A Connect cluster's
    /// password is named after the *cluster*
    /// (`{profile_id}/connect_password/{cluster}`, see
    /// [`crate::profiles::connect_password_suffix`]), so its suffix is per
    /// profile rather than a fixed word and no constant can enumerate it. A
    /// full purge is therefore this constant **plus**
    /// [`crate::profiles::ConnectionProfile::connect_secret_entries`], which
    /// means reading the profile *before* deleting it from the store.
    pub const SECRET_SUFFIXES: &[&str] = &[
        "password",
        "client_key",
        "client_secret",
        "sr_password",
        "metrics_password",
    ];

    /// The keychain entry name for one of a profile's secrets.
    pub fn entry_name(profile_id: &str, suffix: &str) -> String {
        format!("{profile_id}/{suffix}")
    }

    pub fn set(entry: &str, value: &str) -> Result<()> {
        keyring::Entry::new(SERVICE, entry)?.set_password(value)?;
        Ok(())
    }

    /// A missing entry is the normal state for an imported profile (exports
    /// carry refs, never values), so it gets an actionable message rather than
    /// a raw keychain error. Kind-neutral wording: the same call resolves
    /// passwords, OIDC client secrets and TLS private keys, and "no password
    /// stored" reads as a bug when the profile has no password field at all.
    pub fn resolve(secret: &SecretRef) -> Result<String> {
        match keyring::Entry::new(SERVICE, &secret.entry)?.get_password() {
            Ok(value) => Ok(value),
            Err(keyring::Error::NoEntry) => Err(Error::Other(
                "no stored secret for this connection on this machine — edit the profile to \
                 enter it again"
                    .into(),
            )),
            Err(e) => Err(e.into()),
        }
    }

    /// Whether the keychain holds a value for this entry, without reading it.
    ///
    /// The UI needs this to tell the truth about a stored secret: "leave this
    /// blank to keep the stored password" is a lie if nothing is stored, and
    /// the profile alone can't answer — it carries a [`SecretRef`], not the
    /// value. A real keychain failure propagates rather than reading as "no",
    /// so a locked keychain never gets mistaken for an empty one.
    pub fn exists(entry: &str) -> Result<bool> {
        match keyring::Entry::new(SERVICE, entry)?.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Idempotent: deleting a missing entry is not an error.
    pub fn delete(entry: &str) -> Result<()> {
        match keyring::Entry::new(SERVICE, entry)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// The keychain vocabulary is a contract with the ProfileEditor, so it gets a
/// tripwire rather than a comment alone.
#[cfg(test)]
mod secret_vocabulary {
    use super::secrets::{entry_name, SECRET_SUFFIXES};

    /// The regression this constant exists for: `sr_password` was written by
    /// the editor on save and missed by `profiles_delete`, so a Schema
    /// Registry password outlived the connection it belonged to.
    #[test]
    fn every_entry_the_editor_writes_is_purged_on_delete() {
        for suffix in [
            "password",
            "client_key",
            "client_secret",
            "sr_password",
            // Phase 4. A metrics endpoint's password is the fourth secret a
            // profile can own, and the first one added since this tripwire
            // existed — the whole point of the list is that adding a field to
            // the editor without adding it here is caught by a test rather
            // than by a user finding a credential that outlived its cluster.
            "metrics_password",
        ] {
            assert!(
                SECRET_SUFFIXES.contains(&suffix),
                "{suffix} is written by ProfileEditor but not in SECRET_SUFFIXES"
            );
        }
    }

    #[test]
    fn an_entry_name_is_the_profile_id_then_the_suffix() {
        assert_eq!(entry_name("abc-123", "sr_password"), "abc-123/sr_password");
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
