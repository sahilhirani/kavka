//! Authenticated cluster connections. Wraps rdkafka clients when the `kafka`
//! feature is enabled; the facade compiles without it so tooling can build
//! this crate on a toolchain without CMake.
//!
//! All methods here are BLOCKING (librdkafka metadata calls block). Callers in
//! async contexts (the Tauri shell) wrap them in `spawn_blocking`.

use crate::admin::TopicInfo;
use crate::profiles::ConnectionProfile;
#[cfg(feature = "kafka")]
use crate::profiles::{AuthConfig, ScramMechanism};
#[cfg(feature = "kafka")]
use crate::secrets;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// The OAUTHBEARER token providers. They live in their own directory at the
/// crate root (`src/auth/`) but hang off this module: they exist only to serve
/// [`ClusterConnection`], and declaring them here keeps the whole tree behind
/// the single `kafka` cfg gate this file already maintains.
#[cfg(feature = "kafka")]
#[path = "auth/mod.rs"]
pub mod auth;

#[cfg(feature = "kafka")]
use auth::{KavkaClientContext, TokenSource};
#[cfg(feature = "kafka")]
use rdkafka::admin::AdminClient;
#[cfg(feature = "kafka")]
use rdkafka::config::ClientConfig;
#[cfg(feature = "kafka")]
use rdkafka::consumer::{BaseConsumer, Consumer};
#[cfg(feature = "kafka")]
use std::sync::OnceLock;
#[cfg(feature = "kafka")]
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerInfo {
    pub id: i32,
    pub host: String,
    pub port: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterOverview {
    pub cluster_id: Option<String>,
    pub brokers: Vec<BrokerInfo>,
    pub topic_count: usize,
    pub partition_count: usize,
}

#[cfg(feature = "kafka")]
pub(crate) const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to let librdkafka's event queue drain before a metadata call.
/// Only spent on OAUTHBEARER connections — see [`ClusterConnection::service_events`].
#[cfg(feature = "kafka")]
const EVENT_DRAIN_TIMEOUT: Duration = Duration::from_millis(200);

/// Dev-only escape hatch: run OAUTHBEARER over `sasl_plaintext` instead of
/// `sasl_ssl`, for local IdP/broker setups with no TLS. Never set this against
/// a real cluster — an OAuth token is a bearer credential and this puts it on
/// the wire in the clear.
#[cfg(feature = "kafka")]
const OAUTH_ALLOW_PLAINTEXT_ENV: &str = "KAVKA_DEV_OAUTH_ALLOW_PLAINTEXT";

/// Escape hatch for MSK clusters reached through custom DNS (a private zone, a
/// CNAME, an on-prem resolver) rather than their `*.amazonaws.com` names — see
/// [`require_aws_msk_endpoints`].
#[cfg(feature = "kafka")]
const ALLOW_NON_AWS_MSK_ENV: &str = "KAVKA_ALLOW_NON_AWS_MSK_ENDPOINTS";

/// The only host suffixes an MSK IAM profile may point at by default
/// (global partition and AWS China).
#[cfg(feature = "kafka")]
const AWS_HOST_SUFFIXES: [&str; 2] = [".amazonaws.com", ".amazonaws.com.cn"];

pub struct ClusterConnection {
    profile: ConnectionProfile,
    #[cfg(feature = "kafka")]
    consumer: BaseConsumer<KavkaClientContext>,
    /// Built on first admin call, then reused — see [`ClusterConnection::admin`].
    #[cfg(feature = "kafka")]
    admin: OnceLock<AdminClient<KavkaClientContext>>,
}

impl ClusterConnection {
    /// Creates the client and verifies connectivity with a metadata fetch, so
    /// a returned connection is known-good.
    #[cfg(feature = "kafka")]
    pub fn connect(profile: ConnectionProfile) -> Result<Self> {
        let (config, context) = client_config(&profile)?;
        let consumer: BaseConsumer<KavkaClientContext> =
            config.create_with_context(context).map_err(client_error)?;
        let conn = Self {
            profile,
            consumer,
            admin: OnceLock::new(),
        };
        conn.metadata()?;
        Ok(conn)
    }

    /// The AdminClient for this connection, created on first use and then
    /// reused for the connection's lifetime.
    ///
    /// Lazy rather than built in [`connect`](Self::connect) because an
    /// AdminClient is not free: rdkafka spawns a dedicated OS polling thread
    /// per client and joins it on drop, and the client opens its own
    /// connections to every broker. Connecting and browsing topics — what most
    /// sessions only ever do — issues no admin call at all, so building one
    /// eagerly would cost a thread and a second set of broker sessions per
    /// cluster for nothing. `OnceLock` then keeps it to exactly one client, so
    /// a burst of admin calls doesn't re-resolve secrets or re-handshake.
    ///
    /// Caveat, OAUTHBEARER only: librdkafka delivers token-refresh events on a
    /// client's main queue, and rdkafka's AdminClient polls only its own admin
    /// queue (`Client::poll_event` is `pub(crate)`, so nothing outside rdkafka
    /// can drain the main one). An OAUTHBEARER admin call can therefore stall
    /// until its timeout; [`crate::admin`] says so in the error rather than
    /// letting it read as a network fault.
    #[cfg(feature = "kafka")]
    pub(crate) fn admin(&self) -> Result<&AdminClient<KavkaClientContext>> {
        if let Some(admin) = self.admin.get() {
            return Ok(admin);
        }
        let (config, context) = client_config(&self.profile)?;
        let admin: AdminClient<KavkaClientContext> =
            config.create_with_context(context).map_err(client_error)?;
        // A concurrent caller may have won the race; its client is as good as
        // ours, so drop the loser rather than serialize every admin call behind
        // a lock.
        let _ = self.admin.set(admin);
        Ok(self
            .admin
            .get()
            .expect("OnceLock was just set and never clears"))
    }

    /// Runs one blocking librdkafka call on the shared metadata consumer,
    /// servicing the OAUTHBEARER event queue first and routing a failure
    /// through [`describe_failure`](Self::describe_failure) so a rejected
    /// credential never surfaces as a bare timeout.
    ///
    /// `what` is the operation as the user would name it — it becomes the head
    /// of the error message.
    #[cfg(feature = "kafka")]
    pub(crate) fn with_consumer<T>(
        &self,
        what: &str,
        call: impl FnOnce(&BaseConsumer<KavkaClientContext>) -> rdkafka::error::KafkaResult<T>,
    ) -> Result<T> {
        self.service_events();
        call(&self.consumer).map_err(|e| self.describe_failure(format!("{what} failed: {e}")))
    }

    /// A consumer bound to `group_id`, for reading and writing that group's
    /// committed offsets.
    ///
    /// It never joins the group: callers `assign` partitions statically, which
    /// sends no JoinGroup and no heartbeat, so an Empty group stays Empty and
    /// an active group's membership is untouched. Auto-commit and offset
    /// storing are off so that merely holding this consumer can't move the
    /// offsets it exists to inspect.
    #[cfg(feature = "kafka")]
    pub(crate) fn new_group_consumer(
        &self,
        group_id: &str,
    ) -> Result<BaseConsumer<KavkaClientContext>> {
        let (mut config, context) = client_config(&self.profile)?;
        config
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("allow.auto.create.topics", "false");
        let consumer: BaseConsumer<KavkaClientContext> =
            config.create_with_context(context).map_err(client_error)?;
        // Same reason as `service_events`: nothing else polls this client, so
        // drain once to let the token callback run before the first call.
        if consumer.client().context().needs_oauth_token() {
            let _ = consumer.poll(EVENT_DRAIN_TIMEOUT);
        }
        Ok(consumer)
    }

    /// Whether this connection mints OAUTHBEARER tokens — see the caveat on
    /// [`admin`](Self::admin).
    #[cfg(feature = "kafka")]
    pub(crate) fn needs_oauth_token(&self) -> bool {
        self.consumer.client().context().needs_oauth_token()
    }

    /// A fresh consumer for a consume/tail session — sessions own their
    /// consumer (own seek positions, own lifecycle, dropped on their own
    /// thread) rather than sharing the metadata consumer. Reuses the
    /// profile's full auth mapping; secrets are re-resolved at creation.
    #[cfg(feature = "kafka")]
    pub fn new_session_consumer(&self) -> Result<BaseConsumer<KavkaClientContext>> {
        let (mut config, context) = client_config(&self.profile)?;
        // librdkafka refuses `rd_kafka_assign`/`rd_kafka_consumer_poll` on a
        // client with no group.id ("Requires a consumer with group.id
        // configured") — and browse sessions assign partitions by hand rather
        // than subscribing, so without this every fetch and tail fails at
        // assignment. Nothing here joins a group: no subscribe, and auto-commit
        // is off, so the id never reaches __consumer_offsets and no application's
        // committed offsets can move because someone opened a topic in Kavka.
        config
            .set("group.id", format!("kavka-browse-{}", std::process::id()))
            .set("enable.auto.commit", "false")
            // Browsing a topic that isn't there must report that, not create it.
            .set("allow.auto.create.topics", "false");
        config.create_with_context(context).map_err(client_error)
    }

    #[cfg(not(feature = "kafka"))]
    pub fn connect(_profile: ConnectionProfile) -> Result<Self> {
        Err(Error::Other(
            "kavka-core was built without the `kafka` feature".into(),
        ))
    }

    pub fn profile(&self) -> &ConnectionProfile {
        &self.profile
    }

    /// Every mutating entry point calls this first (D5: read-only is enforced
    /// in core, not the UI).
    pub fn ensure_writable(&self, op: &'static str) -> Result<()> {
        if self.profile.read_only {
            return Err(Error::ReadOnly(op));
        }
        Ok(())
    }

    /// librdkafka asks for OAUTHBEARER tokens by enqueueing a refresh event on
    /// the client's queue, and rdkafka only calls
    /// `KavkaClientContext::generate_oauth_token` while that queue is polled.
    /// This consumer exists purely to serve metadata requests, so nothing else
    /// ever polls it — drain the queue here. The token fetch itself runs inline
    /// on this thread, which is why the wait is not the whole budget.
    ///
    /// Skipped once a token is in hand and clear of expiry: there is no refresh
    /// event to collect then, and the poll would sit out its full timeout on
    /// every metadata call — a flat 200ms tax on browsing an OAUTHBEARER
    /// cluster.
    #[cfg(feature = "kafka")]
    fn service_events(&self) {
        let context = self.consumer.client().context();
        if context.needs_oauth_token() && !context.has_fresh_token() {
            let _ = self.consumer.poll(EVENT_DRAIN_TIMEOUT);
        }
    }

    #[cfg(feature = "kafka")]
    fn metadata(&self) -> Result<rdkafka::metadata::Metadata> {
        self.service_events();
        self.consumer
            .fetch_metadata(None, METADATA_TIMEOUT)
            .map_err(|e| self.describe_failure(format!("metadata fetch failed: {e}")))
    }

    /// A broker that never finished the SASL handshake is indistinguishable, to
    /// librdkafka's caller, from one that is merely slow — so a rejected client
    /// secret or a lapsed SSO session surfaces as "metadata fetch failed:
    /// Operation timed out", which sends the user hunting network faults. When
    /// the token source is the thing that actually failed, say so instead.
    #[cfg(feature = "kafka")]
    pub(crate) fn describe_failure(&self, generic: String) -> Error {
        match self.consumer.client().context().last_auth_error() {
            Some(cause) => Error::Other(format!("authentication failed: {cause}")),
            None => Error::Other(generic),
        }
    }

    #[cfg(feature = "kafka")]
    pub fn overview(&self) -> Result<ClusterOverview> {
        let md = self.metadata()?;
        let cluster_id = self.consumer.client().fetch_cluster_id(METADATA_TIMEOUT);
        let mut brokers: Vec<BrokerInfo> = md
            .brokers()
            .iter()
            .map(|b| BrokerInfo {
                id: b.id(),
                host: b.host().to_string(),
                port: b.port(),
            })
            .collect();
        brokers.sort_by_key(|b| b.id);
        Ok(ClusterOverview {
            cluster_id,
            brokers,
            topic_count: md
                .topics()
                .iter()
                .filter(|t| !is_internal(t.name()))
                .count(),
            partition_count: md
                .topics()
                .iter()
                .filter(|t| !is_internal(t.name()))
                .map(|t| t.partitions().len())
                .sum(),
        })
    }

    #[cfg(not(feature = "kafka"))]
    pub fn overview(&self) -> Result<ClusterOverview> {
        Err(Error::Other("built without the `kafka` feature".into()))
    }

    #[cfg(feature = "kafka")]
    pub fn list_topics(&self) -> Result<Vec<TopicInfo>> {
        let md = self.metadata()?;
        let mut topics: Vec<TopicInfo> = md
            .topics()
            .iter()
            .map(|t| TopicInfo {
                name: t.name().to_string(),
                partitions: t.partitions().len() as u32,
                replication_factor: t
                    .partitions()
                    .first()
                    .map(|p| p.replicas().len())
                    .unwrap_or(0) as u16,
                internal: is_internal(t.name()),
            })
            .collect();
        topics.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(topics)
    }

    #[cfg(not(feature = "kafka"))]
    pub fn list_topics(&self) -> Result<Vec<TopicInfo>> {
        Err(Error::Other("built without the `kafka` feature".into()))
    }
}

/// Kafka's internal topics. Metadata doesn't flag these; the double-underscore
/// prefix convention covers __consumer_offsets/__transaction_state/SR's
/// _schemas is NOT matched (single underscore) — refined via DescribeTopics
/// in Phase 1.
#[cfg(feature = "kafka")]
pub(crate) fn is_internal(name: &str) -> bool {
    name.starts_with("__")
}

/// Maps a profile's auth config onto librdkafka properties, plus the client
/// context that will mint OAUTHBEARER tokens for it. Secrets are resolved from
/// the OS keychain here, at connect time — they never sit in the profile, and
/// key material is passed to librdkafka in memory rather than written to disk.
#[cfg(feature = "kafka")]
fn client_config(profile: &ConnectionProfile) -> Result<(ClientConfig, KavkaClientContext)> {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", profile.bootstrap_servers.join(","))
        .set("client.id", "kavka");
    let mut token_source = None;

    match &profile.auth {
        AuthConfig::Plaintext => {
            cfg.set("security.protocol", "plaintext");
        }
        AuthConfig::Tls {
            ca_pem_path,
            client_cert_pem_path,
            client_key,
        } => {
            cfg.set("security.protocol", ssl_protocol()?);
            if let Some(ca) = ca_pem_path {
                cfg.set("ssl.ca.location", ca);
            }
            if let Some(cert) = client_cert_pem_path {
                cfg.set("ssl.certificate.location", cert);
            }
            if let Some(key) = client_key {
                // `ssl.key.pem` takes the PEM inline; `ssl.key.location` would
                // mean spilling the private key to a file (D5).
                cfg.set("ssl.key.pem", secrets::resolve(key)?);
            }
        }
        AuthConfig::SaslPlain {
            username,
            password,
            tls,
        } => {
            cfg.set("security.protocol", sasl_protocol(*tls)?)
                .set("sasl.mechanism", "PLAIN")
                .set("sasl.username", username)
                .set("sasl.password", secrets::resolve(password)?);
        }
        AuthConfig::SaslScram {
            mechanism,
            username,
            password,
            tls,
        } => {
            cfg.set("security.protocol", sasl_protocol(*tls)?)
                .set(
                    "sasl.mechanism",
                    match mechanism {
                        ScramMechanism::Sha256 => "SCRAM-SHA-256",
                        ScramMechanism::Sha512 => "SCRAM-SHA-512",
                    },
                )
                .set("sasl.username", username)
                .set("sasl.password", secrets::resolve(password)?);
        }
        AuthConfig::OauthBearer {
            token_endpoint,
            client_id,
            client_secret,
        } => {
            cfg.set("security.protocol", oauth_protocol()?)
                .set("sasl.mechanism", "OAUTHBEARER");
            token_source = Some(TokenSource::Oidc {
                token_endpoint: token_endpoint.clone(),
                client_id: client_id.clone(),
                client_secret: secrets::resolve(client_secret)?,
            });
        }
        AuthConfig::AwsMskIam {
            region,
            profile: aws_profile,
        } => {
            require_aws_msk_endpoints(&profile.bootstrap_servers, non_aws_msk_endpoints_allowed())?;
            token_source = Some(msk_token_source(region, aws_profile.as_deref())?);
            // MSK IAM is OAUTHBEARER over TLS, always (docs/ARCHITECTURE.md D2).
            cfg.set("security.protocol", sasl_protocol(true)?)
                .set("sasl.mechanism", "OAUTHBEARER");
        }
        AuthConfig::Kerberos { .. } => {
            return Err(Error::Other(
                "Kerberos (SASL/GSSAPI) is not supported yet — it needs a per-platform GSSAPI \
                 build of librdkafka (SSPI on Windows, cyrus-sasl elsewhere)"
                    .into(),
            ));
        }
    }
    Ok((cfg, KavkaClientContext::new(token_source)))
}

/// `KafkaError::ClientConfig`'s `Display` renders the rejected VALUE, and the
/// value librdkafka is most likely to reject here is `ssl.key.pem` — a private
/// key, which must never reach a UI toast or a log (D5). Name the property and
/// quote librdkafka's description; drop the value.
#[cfg(feature = "kafka")]
fn client_error(err: rdkafka::error::KafkaError) -> Error {
    match err {
        rdkafka::error::KafkaError::ClientConfig(_, description, key, _value) => Error::Other(
            format!("failed to create Kafka client: librdkafka rejected `{key}`: {description}"),
        ),
        other => Error::Other(format!("failed to create Kafka client: {other}")),
    }
}

#[cfg(feature = "msk-iam")]
fn msk_token_source(region: &str, aws_profile: Option<&str>) -> Result<TokenSource> {
    Ok(TokenSource::MskIam {
        region: region.to_string(),
        profile: aws_profile.map(str::to_string),
    })
}

/// MSK IAM lives behind its own feature because the AWS SDK it needs is a
/// heavier build than the base `kafka` tier promises (see Cargo.toml). A build
/// without it refuses the profile rather than silently authenticating some
/// other way.
#[cfg(all(feature = "kafka", not(feature = "msk-iam")))]
fn msk_token_source(_region: &str, _aws_profile: Option<&str>) -> Result<TokenSource> {
    Err(Error::Other(
        "this build lacks MSK IAM support — rebuild kavka-core with the `msk-iam` feature \
         (or `kafka-ssl`, which includes it)"
            .into(),
    ))
}

#[cfg(feature = "kafka")]
fn non_aws_msk_endpoints_allowed() -> bool {
    matches!(std::env::var(ALLOW_NON_AWS_MSK_ENV).as_deref(), Ok("1"))
}

/// Refuses to hand an MSK IAM token to a host that is not AWS's.
///
/// A profile is a shareable document, and an imported one carries whatever
/// bootstrap servers its author chose. Connecting mints a SigV4-presigned
/// `kafka-cluster:Connect` URL from the local AWS identity and presents it to
/// those servers, so a profile pointed at an attacker's host collects a live,
/// replayable-for-15-minutes credential for the importer's AWS account — no
/// prompt, no broker of ours involved. The bootstrap host is the only thing
/// that decides who receives it, so it is checked before the client is built.
#[cfg(feature = "kafka")]
fn require_aws_msk_endpoints(bootstrap_servers: &[String], allow_non_aws: bool) -> Result<()> {
    if allow_non_aws {
        return Ok(());
    }
    for server in bootstrap_servers {
        let host = bootstrap_host(server);
        let lower = host.to_ascii_lowercase();
        if !AWS_HOST_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            return Err(Error::Other(format!(
                "refusing to connect: MSK IAM signs a live AWS credential for your identity and \
                 hands it to the bootstrap host, so pointing this profile at {host:?} — which is \
                 not an .amazonaws.com(.cn) host — would let it act as you in AWS for the next 15 \
                 minutes. If this really is your MSK cluster behind custom DNS, set \
                 {ALLOW_NON_AWS_MSK_ENV}=1."
            )));
        }
    }
    Ok(())
}

/// The host part of a `bootstrap.servers` entry: `[proto://]host[:port]`.
/// Bracketed IPv6 literals keep their colons; anything else that ends up
/// mangled here is not an `*.amazonaws.com` name anyway, so it is rejected —
/// which is the safe direction.
#[cfg(feature = "kafka")]
fn bootstrap_host(server: &str) -> &str {
    let server = server.trim();
    let server = server.split_once("://").map_or(server, |(_, rest)| rest);
    match server.strip_prefix('[') {
        Some(rest) => rest.split_once(']').map_or(rest, |(host, _)| host),
        None => server.rsplit_once(':').map_or(server, |(host, _)| host),
    }
}

#[cfg(feature = "kafka")]
fn sasl_protocol(tls: bool) -> Result<&'static str> {
    if tls {
        // sasl_ssl needs the `kafka-ssl` feature (vendored OpenSSL).
        #[cfg(feature = "kafka-ssl")]
        return Ok("sasl_ssl");
        #[cfg(not(feature = "kafka-ssl"))]
        return Err(Error::Other(
            "SASL over TLS requires the `kafka-ssl` build of kavka-core".into(),
        ));
    }
    Ok("sasl_plaintext")
}

#[cfg(feature = "kafka")]
fn ssl_protocol() -> Result<&'static str> {
    #[cfg(feature = "kafka-ssl")]
    return Ok("ssl");
    #[cfg(not(feature = "kafka-ssl"))]
    Err(Error::Other(
        "TLS requires the `kafka-ssl` build of kavka-core".into(),
    ))
}

/// OAUTHBEARER is TLS-only unless the dev escape hatch is set explicitly.
#[cfg(feature = "kafka")]
fn oauth_protocol() -> Result<&'static str> {
    if matches!(std::env::var(OAUTH_ALLOW_PLAINTEXT_ENV).as_deref(), Ok("1")) {
        return Ok("sasl_plaintext");
    }
    sasl_protocol(true)
}

#[cfg(all(test, feature = "kafka"))]
mod tests {
    use super::*;
    use crate::profiles::{Environment, SecretRef};
    use std::sync::{Mutex, MutexGuard};

    /// The OS keychain is process-wide — worse, machine-wide — state, and cargo
    /// runs these tests on parallel threads. Every test that stores, reads or
    /// deletes an entry takes this first, so no two can interleave a write with
    /// another's read or delete.
    static KEYCHAIN: Mutex<()> = Mutex::new(());

    /// Poison-tolerant: the guarded state is the keychain itself, which a failed
    /// assertion elsewhere cannot corrupt, and a poisoned lock would otherwise
    /// turn one failure into a whole-module failure.
    fn keychain_lock() -> MutexGuard<'static, ()> {
        KEYCHAIN.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A keychain entry name unique to one test in one process. Entries outlive
    /// the run and two `cargo test` invocations (or a stale one from an
    /// interrupted run) can overlap on the same machine, so the pid is part of
    /// the name as well as the test's own.
    fn entry_for(test: &str) -> String {
        format!("kavka-unit-test/{test}/{}", std::process::id())
    }

    fn profile(auth: AuthConfig) -> ConnectionProfile {
        profile_at(vec!["broker-1:9093".into(), "broker-2:9093".into()], auth)
    }

    fn profile_at(bootstrap_servers: Vec<String>, auth: AuthConfig) -> ConnectionProfile {
        ConnectionProfile {
            id: "unit".into(),
            name: "unit".into(),
            environment: Environment::Dev,
            bootstrap_servers,
            auth,
            read_only: false,
            schema_registry: None,
        }
    }

    fn config_for(auth: AuthConfig) -> ClientConfig {
        client_config(&profile(auth)).expect("client_config").0
    }

    /// Stores a throwaway secret in the OS keychain. Returns `None` where there
    /// is no usable keychain (headless CI), so the secret-bearing cases skip
    /// rather than fail.
    fn stored_secret(entry: &str, value: &str) -> Option<SecretRef> {
        secrets::set(entry, value).ok().map(|()| SecretRef {
            entry: entry.to_string(),
        })
    }

    #[test]
    fn common_properties_are_set_for_every_auth_kind() {
        let cfg = config_for(AuthConfig::Plaintext);
        assert_eq!(
            cfg.get("bootstrap.servers"),
            Some("broker-1:9093,broker-2:9093")
        );
        assert_eq!(cfg.get("client.id"), Some("kavka"));
    }

    #[test]
    fn plaintext_sets_no_sasl_or_tls_properties() {
        let cfg = config_for(AuthConfig::Plaintext);
        assert_eq!(cfg.get("security.protocol"), Some("plaintext"));
        assert_eq!(cfg.get("sasl.mechanism"), None);
        assert_eq!(cfg.get("ssl.ca.location"), None);
    }

    // Asserts ssl/sasl_ssl protocols, which plain `kafka` builds correctly refuse.
    #[cfg(feature = "kafka-ssl")]
    #[test]
    fn tls_maps_paths_and_keeps_key_material_off_disk() {
        let _keychain = keychain_lock();
        let entry = entry_for("tls_maps_paths_and_keeps_key_material_off_disk");
        let Some(key) = stored_secret(&entry, "-----BEGIN PRIVATE KEY-----\nk\n") else {
            eprintln!("skipped: no usable OS keychain");
            return;
        };
        let cfg = config_for(AuthConfig::Tls {
            ca_pem_path: Some("/etc/kavka/ca.pem".into()),
            client_cert_pem_path: Some("/etc/kavka/client.pem".into()),
            client_key: Some(key),
        });
        let _ = secrets::delete(&entry);

        assert_eq!(cfg.get("security.protocol"), Some("ssl"));
        assert_eq!(cfg.get("ssl.ca.location"), Some("/etc/kavka/ca.pem"));
        assert_eq!(
            cfg.get("ssl.certificate.location"),
            Some("/etc/kavka/client.pem")
        );
        assert_eq!(
            cfg.get("ssl.key.pem"),
            Some("-----BEGIN PRIVATE KEY-----\nk\n")
        );
        // The private key is handed over inline, never as a file location.
        assert_eq!(cfg.get("ssl.key.location"), None);
    }

    // Asserts ssl/sasl_ssl protocols, which plain `kafka` builds correctly refuse.
    #[cfg(feature = "kafka-ssl")]
    #[test]
    fn tls_omits_properties_for_absent_material() {
        let cfg = config_for(AuthConfig::Tls {
            ca_pem_path: None,
            client_cert_pem_path: None,
            client_key: None,
        });
        assert_eq!(cfg.get("security.protocol"), Some("ssl"));
        assert_eq!(cfg.get("ssl.ca.location"), None);
        assert_eq!(cfg.get("ssl.certificate.location"), None);
        assert_eq!(cfg.get("ssl.key.pem"), None);
    }

    // Asserts ssl/sasl_ssl protocols, which plain `kafka` builds correctly refuse.
    #[cfg(feature = "kafka-ssl")]
    #[test]
    fn sasl_plain_switches_protocol_on_the_tls_flag() {
        let _keychain = keychain_lock();
        let entry = entry_for("sasl_plain_switches_protocol_on_the_tls_flag");
        let Some(password) = stored_secret(&entry, "pw") else {
            eprintln!("skipped: no usable OS keychain");
            return;
        };
        let over_tls = config_for(AuthConfig::SaslPlain {
            username: "alice".into(),
            password: password.clone(),
            tls: true,
        });
        let plain = config_for(AuthConfig::SaslPlain {
            username: "alice".into(),
            password,
            tls: false,
        });
        let _ = secrets::delete(&entry);

        assert_eq!(over_tls.get("security.protocol"), Some("sasl_ssl"));
        assert_eq!(plain.get("security.protocol"), Some("sasl_plaintext"));
        for cfg in [&over_tls, &plain] {
            assert_eq!(cfg.get("sasl.mechanism"), Some("PLAIN"));
            assert_eq!(cfg.get("sasl.username"), Some("alice"));
            assert_eq!(cfg.get("sasl.password"), Some("pw"));
        }
    }

    // Asserts ssl/sasl_ssl protocols, which plain `kafka` builds correctly refuse.
    #[cfg(feature = "kafka-ssl")]
    #[test]
    fn scram_maps_the_mechanism_name() {
        let _keychain = keychain_lock();
        let entry = entry_for("scram_maps_the_mechanism_name");
        let Some(password) = stored_secret(&entry, "pw") else {
            eprintln!("skipped: no usable OS keychain");
            return;
        };
        let sha256 = config_for(AuthConfig::SaslScram {
            mechanism: ScramMechanism::Sha256,
            username: "alice".into(),
            password: password.clone(),
            tls: true,
        });
        let sha512 = config_for(AuthConfig::SaslScram {
            mechanism: ScramMechanism::Sha512,
            username: "alice".into(),
            password,
            tls: true,
        });
        let _ = secrets::delete(&entry);

        assert_eq!(sha256.get("sasl.mechanism"), Some("SCRAM-SHA-256"));
        assert_eq!(sha512.get("sasl.mechanism"), Some("SCRAM-SHA-512"));
        assert_eq!(sha256.get("security.protocol"), Some("sasl_ssl"));
    }

    // Asserts ssl/sasl_ssl protocols, which plain `kafka` builds correctly refuse.
    #[cfg(feature = "kafka-ssl")]
    #[test]
    fn oauthbearer_is_sasl_ssl_and_carries_an_oidc_token_source() {
        let _keychain = keychain_lock();
        let entry = entry_for("oauthbearer_is_sasl_ssl_and_carries_an_oidc_token_source");
        let Some(client_secret) = stored_secret(&entry, "shhh") else {
            eprintln!("skipped: no usable OS keychain");
            return;
        };
        let (cfg, context) = client_config(&profile(AuthConfig::OauthBearer {
            token_endpoint: "https://idp.example/token".into(),
            client_id: "kavka".into(),
            client_secret,
        }))
        .expect("client_config");
        let _ = secrets::delete(&entry);

        assert_eq!(cfg.get("security.protocol"), Some("sasl_ssl"));
        assert_eq!(cfg.get("sasl.mechanism"), Some("OAUTHBEARER"));
        // Kavka mints the token itself, so librdkafka's own OIDC machinery
        // (which would need libcurl) must stay untouched.
        assert_eq!(cfg.get("sasl.oauthbearer.method"), None);
        assert!(context.needs_oauth_token());
        // The resolved secret must not be reachable through Debug.
        let rendered = format!("{context:?}");
        assert!(!rendered.contains("shhh"), "secret leaked: {rendered}");
    }

    /// One of the real broker names MSK hands out.
    fn msk_profile() -> ConnectionProfile {
        profile_at(
            vec!["b-1.kavka.abc123.c2.kafka.eu-west-1.amazonaws.com:9098".into()],
            AuthConfig::AwsMskIam {
                region: "eu-west-1".into(),
                profile: Some("prod".into()),
            },
        )
    }

    #[cfg(feature = "msk-iam")]
    #[test]
    fn msk_iam_is_oauthbearer_over_tls_with_no_keychain_secret() {
        let (cfg, context) = client_config(&msk_profile()).expect("client_config");

        assert_eq!(cfg.get("security.protocol"), Some("sasl_ssl"));
        assert_eq!(cfg.get("sasl.mechanism"), Some("OAUTHBEARER"));
        assert_eq!(cfg.get("sasl.username"), None);
        assert_eq!(cfg.get("sasl.password"), None);
        assert!(context.needs_oauth_token());
    }

    /// The `kafka` tier deliberately excludes the AWS SDK (Cargo.toml feature
    /// comments), so such a build has to refuse the profile — clearly, not by
    /// falling through to some other mechanism.
    #[cfg(not(feature = "msk-iam"))]
    #[test]
    fn msk_iam_without_the_feature_says_the_build_lacks_it() {
        let err = client_config(&msk_profile()).unwrap_err().to_string();
        assert!(err.contains("lacks MSK IAM support"), "got {err}");
        assert!(err.contains("msk-iam"), "got {err}");
    }

    #[test]
    fn msk_iam_accepts_aws_bootstrap_hosts() {
        for servers in [
            vec!["b-1.kavka.abc123.c2.kafka.eu-west-1.amazonaws.com:9098".to_string()],
            // Case is not significant in DNS, and a port is optional.
            vec!["B-2.KAVKA.C2.KAFKA.US-EAST-1.AMAZONAWS.COM".to_string()],
            // AWS China partition uses .amazonaws.com.cn.
            vec!["b-1.kavka.c2.kafka.cn-north-1.amazonaws.com.cn:9098".to_string()],
            // Every host is checked, not just the first.
            vec![
                "b-1.kavka.c2.kafka.eu-west-1.amazonaws.com:9098".to_string(),
                "b-2.kavka.c2.kafka.eu-west-1.amazonaws.com:9098".to_string(),
            ],
        ] {
            assert!(
                require_aws_msk_endpoints(&servers, false).is_ok(),
                "rejected {servers:?}"
            );
        }
    }

    #[test]
    fn msk_iam_refuses_to_sign_for_a_non_aws_bootstrap_host() {
        for servers in [
            vec!["evil.example.com:9098".to_string()],
            // A suffix that only *looks* like AWS's.
            vec!["kafka.eu-west-1.amazonaws.com.evil.example:9098".to_string()],
            vec!["10.0.0.7:9098".to_string()],
            vec!["[2001:db8::1]:9098".to_string()],
            // The second entry is the one that would receive the token.
            vec![
                "b-1.kavka.c2.kafka.eu-west-1.amazonaws.com:9098".to_string(),
                "evil.example.com:9098".to_string(),
            ],
        ] {
            let err = require_aws_msk_endpoints(&servers, false)
                .expect_err(&format!("accepted {servers:?}"))
                .to_string();
            assert!(err.contains("refusing to connect"), "got {err}");
            // The message has to explain the risk and name the escape hatch.
            assert!(err.contains("act as you in AWS"), "got {err}");
            assert!(err.contains(ALLOW_NON_AWS_MSK_ENV), "got {err}");
        }
    }

    #[test]
    fn the_escape_hatch_allows_a_custom_dns_msk_endpoint() {
        let servers = vec!["kafka.internal.corp:9098".to_string()];
        assert!(require_aws_msk_endpoints(&servers, false).is_err());
        assert!(require_aws_msk_endpoints(&servers, true).is_ok());
    }

    #[test]
    fn bootstrap_hosts_are_parsed_without_their_port_or_scheme() {
        for (server, host) in [
            ("broker:9092", "broker"),
            ("broker", "broker"),
            (" broker:9092 ", "broker"),
            (
                "SASL_SSL://b-1.kafka.eu-west-1.amazonaws.com:9098",
                "b-1.kafka.eu-west-1.amazonaws.com",
            ),
            ("[2001:db8::1]:9092", "2001:db8::1"),
        ] {
            assert_eq!(bootstrap_host(server), host, "for {server:?}");
        }
    }

    #[test]
    fn client_config_errors_never_echo_the_rejected_value() {
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            "MIIEvQIBADANBg".repeat(64)
        );
        let message = client_error(rdkafka::error::KafkaError::ClientConfig(
            rdkafka::types::RDKafkaConfRes::RD_KAFKA_CONF_INVALID,
            "Invalid value for configuration property".into(),
            "ssl.key.pem".into(),
            pem.clone(),
        ))
        .to_string();

        assert!(message.contains("ssl.key.pem"), "got {message}");
        assert!(
            message.contains("Invalid value for configuration property"),
            "got {message}"
        );
        assert!(!message.contains("BEGIN PRIVATE KEY"), "got {message}");
        assert!(!message.contains("MIIEvQIBADANBg"), "got {message}");
        assert!(!message.contains(&pem), "got {message}");
    }

    #[test]
    fn other_client_errors_keep_librdkafkas_own_wording() {
        let message = client_error(rdkafka::error::KafkaError::ClientCreation(
            "No such configuration property: \"nope\"".into(),
        ))
        .to_string();
        assert!(
            message.contains("No such configuration property"),
            "got {message}"
        );
    }

    #[test]
    fn non_oauth_profiles_get_a_context_without_a_token_source() {
        let (_, context) = client_config(&profile(AuthConfig::Plaintext)).expect("client_config");
        assert!(!context.needs_oauth_token());
    }

    #[test]
    fn kerberos_reports_that_kerberos_specifically_is_pending() {
        let err = client_config(&profile(AuthConfig::Kerberos {
            service_name: "kafka".into(),
            principal: "alice@EXAMPLE".into(),
        }))
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Kerberos"), "got {message}");
        assert!(message.contains("GSSAPI"), "got {message}");
    }

    #[test]
    fn tls_protocols_resolve_under_the_ssl_build() {
        // The gate builds with `kafka-ssl`; without it both must refuse rather
        // than hand librdkafka a protocol it was not linked for.
        #[cfg(feature = "kafka-ssl")]
        {
            assert_eq!(ssl_protocol().unwrap(), "ssl");
            assert_eq!(sasl_protocol(true).unwrap(), "sasl_ssl");
        }
        #[cfg(not(feature = "kafka-ssl"))]
        {
            assert!(ssl_protocol().is_err());
            assert!(sasl_protocol(true).is_err());
        }
        assert_eq!(sasl_protocol(false).unwrap(), "sasl_plaintext");
    }
}
