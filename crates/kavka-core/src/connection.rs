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

#[cfg(feature = "kafka")]
use rdkafka::config::ClientConfig;
#[cfg(feature = "kafka")]
use rdkafka::consumer::{BaseConsumer, Consumer};
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
const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ClusterConnection {
    profile: ConnectionProfile,
    #[cfg(feature = "kafka")]
    consumer: BaseConsumer,
}

impl ClusterConnection {
    /// Creates the client and verifies connectivity with a metadata fetch, so
    /// a returned connection is known-good.
    #[cfg(feature = "kafka")]
    pub fn connect(profile: ConnectionProfile) -> Result<Self> {
        let consumer: BaseConsumer = client_config(&profile)?
            .create()
            .map_err(|e| Error::Other(format!("failed to create Kafka client: {e}")))?;
        let conn = Self { profile, consumer };
        conn.metadata()?;
        Ok(conn)
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

    #[cfg(feature = "kafka")]
    fn metadata(&self) -> Result<rdkafka::metadata::Metadata> {
        self.consumer
            .fetch_metadata(None, METADATA_TIMEOUT)
            .map_err(|e| Error::Other(format!("metadata fetch failed: {e}")))
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
fn is_internal(name: &str) -> bool {
    name.starts_with("__")
}

/// Maps a profile's auth config onto librdkafka properties. Secrets are
/// resolved from the OS keychain here, at connect time — they never sit in
/// the profile or the config for longer than the client build.
#[cfg(feature = "kafka")]
fn client_config(profile: &ConnectionProfile) -> Result<ClientConfig> {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", profile.bootstrap_servers.join(","))
        .set("client.id", "kavka");

    match &profile.auth {
        AuthConfig::Plaintext => {
            cfg.set("security.protocol", "plaintext");
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
        AuthConfig::Tls { .. }
        | AuthConfig::AwsMskIam { .. }
        | AuthConfig::OauthBearer { .. }
        | AuthConfig::Kerberos { .. } => {
            return Err(Error::Other(
                "this auth method requires the TLS-enabled build — it lands later in Phase 0"
                    .into(),
            ));
        }
    }
    Ok(cfg)
}

#[cfg(feature = "kafka")]
fn sasl_protocol(tls: bool) -> Result<&'static str> {
    if tls {
        // sasl_ssl needs the `kafka-ssl` feature (vendored OpenSSL).
        #[cfg(feature = "kafka-ssl")]
        return Ok("sasl_ssl");
        #[cfg(not(feature = "kafka-ssl"))]
        return Err(Error::Other(
            "SASL over TLS requires the TLS-enabled build — it lands later in Phase 0".into(),
        ));
    }
    Ok("sasl_plaintext")
}
