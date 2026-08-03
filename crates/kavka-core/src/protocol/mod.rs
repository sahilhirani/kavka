//! A hand-rolled Kafka wire-protocol client, for the admin RPCs librdkafka
//! does not expose.
//!
//! This is the module docs/ARCHITECTURE.md D2 anticipated: *"Gaps in
//! librdkafka … are filled by implementing the specific Kafka protocol frames
//! directly in Rust (`kavka-core::protocol`) over the existing authenticated
//! connection — admin-style RPCs are simple request/response and don't need the
//! full client machinery."* rdkafka 0.37 wraps none of the six APIs Phase 3b
//! needs, and librdkafka's C admin API — which [`crate::acl`] already reaches
//! into for ACLs and IncrementalAlterConfigs — has no entry point for
//! DescribeQuorum or the client-quota pair at all.
//!
//! # Why not the `kafka-protocol` crate
//!
//! `kafka-protocol` generates codecs for every message in the Kafka schema —
//! ~200 request/response types, `indexmap`, `uuid`, `crc32c`, `bytes`, `string`
//! and a record-batch layer with four optional compression crates hanging off
//! it. Kavka needs **eleven** message types and no record batches. Weighed
//! against this file's ~450 lines of encoder plus a primitive layer that is
//! fully golden-byte tested:
//!
//! - **Dependency weight.** Every other dependency in this crate's Cargo.toml
//!   carries a paragraph justifying it; a generated codec for 190 messages
//!   nothing calls is the opposite of that discipline, and it would be the
//!   single largest thing in a binary whose selling point is its size (D1).
//! - **Flexible versions.** This is the one thing that would have been worth
//!   paying for — and it turns out to be small: KIP-482 is a varint length and
//!   a tag buffer, ~60 lines in [`wire`], checked against hand-verified hex.
//! - **Control over the version window.** The crate exposes every version,
//!   which quietly invites encoding one nothing has ever run. Here the windows
//!   are deliberately narrow and every one of them is exercised — see below.
//!
//! The crypto follows the same logic: SCRAM (RFC 5802) is built on `ring`,
//! which `rustls` — and so `ureq`, and so the OIDC and Schema Registry clients
//! — already links, rather than on a SCRAM crate that would bring a second
//! HMAC/PBKDF2 implementation into the binary. See [`scram`].
//!
//! # Version windows
//!
//! Negotiated per API from the broker's own ApiVersions table, taking the
//! highest version both sides speak; a broker outside the window is refused
//! with a message naming the API and BOTH windows ([`conn::Api`]).
//!
//! | API | Key | Implemented | Why the floor |
//! |---|---|---|---|
//! | ApiVersions | 18 | 0-3 | v0 is the fallback when a broker rejects v3 |
//! | Metadata | 3 | 9-12 | v9 is the first flexible version |
//! | FindCoordinator | 10 | 4-6 | v4 is the first with the `Coordinators` array |
//! | ListGroups | 16 | 5 | v5 adds the `TypesFilter` share groups are found by |
//! | SaslHandshake | 17 | 1 | v0 puts raw SASL tokens on the socket (pre-Kafka 1.0) |
//! | SaslAuthenticate | 36 | 2 | first flexible version |
//! | ElectLeaders | 43 | 2 | first flexible version (Kafka 2.4) |
//! | AlterPartitionReassignments | 45 | 0 | the only version Kafka defines |
//! | ListPartitionReassignments | 46 | 0 | the only version Kafka defines |
//! | DescribeClientQuotas | 48 | 1 | first flexible version (Kafka 2.7) |
//! | AlterClientQuotas | 49 | 1 | first flexible version (Kafka 2.7) |
//! | DescribeQuorum | 55 | 0-1 | v1 adds the fetch timestamps the contract needs |
//! | ShareGroupDescribe | 77 | 1 | Kafka DELETED v0 in 4.1; v1 is the whole window |
//! | DescribeShareGroupOffsets | 90 | 0 | the only version Kafka defines |
//!
//! The floors are a stance, not an oversight: a legacy encoder for the
//! pre-flexible version of, say, DescribeClientQuotas is code the integration
//! suite could never run (the dev cluster negotiates v1), and an encoder no
//! test can reach is worse than a refusal that names the API and the versions.
//! The practical cost is Kafka 2.6 for quotas and 2.4 for elections; the
//! practical benefit is that every byte this module can emit has been checked.
//!
//! # Feature gating
//!
//! Behind the `kafka` feature, with the rest of the cluster-facing code. The
//! reason is [`crate::connection::auth::TokenSource`]: OAUTHBEARER here is the
//! *same* token source `ClusterConnection` uses — OIDC client credentials and
//! MSK IAM SigV4 — and that type lives behind `kafka` because it hands
//! librdkafka an `rdkafka::client::OAuthToken`. A `protocol`-only feature would
//! therefore have to grow a second OIDC and MSK implementation to be useful,
//! which is precisely the duplication this module exists to avoid. Nothing is
//! lost: a build without `kafka` cannot open a cluster connection at all, so it
//! has nothing to route these RPCs to.
//!
//! TLS, by contrast, needs **no** `kafka-ssl`: that feature's cost is
//! librdkafka's vendored OpenSSL build, and this module speaks TLS through
//! rustls (see [`tls`]).
//!
//! # Blocking
//!
//! Every call here BLOCKS on a socket, like the rest of the core — the Tauri
//! shell wraps each in `spawn_blocking`.
//!
//! # Connection lifetime
//!
//! Each free function below opens a connection, runs its call, and drops it —
//! the one-shot form, for a caller that makes a single user-initiated action.
//!
//! A caller that repeats — the reassignment monitor polls `reassign_list` every
//! two seconds — holds a [`ProtocolClient`] instead and pays for ONE handshake.
//! That matters more than it looks: every call through the one-shot form is a
//! TCP connect, a TLS handshake, ApiVersions, and the full SASL exchange, and
//! for SCRAM that exchange is four round trips *and* a PBKDF2 derivation on the
//! calling thread. At one poll every two seconds that is a login per poll.
//!
//! A held client is NOT concurrency-safe and says so on the type: it owns one
//! socket with one correlation-id sequence and one read cursor, so callers that
//! can overlap must serialize them (the desktop shell keeps one client per
//! profile behind a mutex — see `apps/desktop/src-tauri/src/lib.rs`). When a
//! call fails at the transport, [`ProtocolClient::is_healthy`] goes false and
//! stays false: the socket's framing can no longer be trusted, and the only
//! correct move is to drop the client and dial again.

mod conn;
mod elect;
mod errors;
mod meta;
mod quorum;
mod quotas;
mod reassign;
mod sasl;
mod scram;
mod share;
mod tls;
mod wire;

pub use elect::PartitionResult;
pub use quorum::{QuorumInfo, ReplicaState};
pub use quotas::{QuotaEntity, QuotaEntityPart, QuotaOp, QuotaValue};
pub use reassign::{ReassignmentSpec, ReassignmentState, TopicPartition};
pub use share::{ShareGroupDetail, ShareGroupInfo, ShareGroupMember, ShareGroupOffset};

use crate::connection::ClusterConnection;
use crate::profiles::{AuthConfig, ConnectionProfile};
use crate::{Error, Result};
use conn::BrokerConnection;
use quorum::QuorumReply;
use std::collections::BTreeMap;
use std::sync::Arc;

/// One sentence for the one mechanism Kavka cannot speak, shared by the
/// transport and SASL layers so the two can never drift into telling the user
/// different things about the same gap.
pub(crate) fn kerberos_unsupported() -> Error {
    Error::Other(
        "Kerberos (SASL/GSSAPI) is not supported — it needs a platform GSSAPI implementation \
         (SSPI on Windows, cyrus-sasl elsewhere), which Kavka does not bundle"
            .into(),
    )
}

/// Read-only enforcement, in core rather than in the UI
/// (docs/ARCHITECTURE.md D5).
///
/// The one-shot entry points below apply this through
/// [`ClusterConnection::ensure_writable`] BEFORE a socket exists — a read-only
/// connection must not so much as authenticate on behalf of a write. This is
/// the same judgement made a second time on [`ProtocolClient`]'s own profile,
/// so that holding a client directly cannot route around it.
fn ensure_writable(profile: &ConnectionProfile, op: &'static str) -> Result<()> {
    if profile.read_only {
        return Err(Error::ReadOnly(op));
    }
    Ok(())
}

/// An authenticated protocol connection to one cluster.
///
/// Hold one to batch (or repeat) several calls; the free functions below are
/// the one-shot form.
///
/// **Not concurrency-safe, by construction.** One socket, one correlation-id
/// sequence, one read cursor: two overlapping calls on the same client would
/// interleave two requests on one stream, and the `&mut self` on every method
/// is what forces a caller to decide how it serializes them rather than
/// discovering the interleaving as a decode failure. It is `Send`, so passing
/// one between threads (or parking it behind a mutex) is fine.
pub struct ProtocolClient {
    profile: ConnectionProfile,
    conn: BrokerConnection,
}

impl ProtocolClient {
    /// Connects to the first reachable bootstrap server and authenticates.
    pub fn connect(profile: &ConnectionProfile) -> Result<Self> {
        // The same guard `ClusterConnection` applies, for the same reason: an
        // MSK IAM profile mints a live, replayable AWS credential and hands it
        // to whatever host the profile names, so the host is checked before a
        // socket exists. Shared rather than re-implemented — two copies of a
        // security check is one copy that will be forgotten.
        if matches!(profile.auth, AuthConfig::AwsMskIam { .. }) {
            crate::connection::require_aws_msk_endpoints(
                &profile.bootstrap_servers,
                crate::connection::non_aws_msk_endpoints_allowed(),
            )?;
        }
        if matches!(profile.auth, AuthConfig::Kerberos { .. }) {
            return Err(kerberos_unsupported());
        }
        if profile.bootstrap_servers.is_empty() {
            return Err(Error::Other(
                "this connection has no bootstrap servers".into(),
            ));
        }

        let tls = tls::client_config(&profile.auth)?;
        let mut failures = Vec::new();
        for address in &profile.bootstrap_servers {
            match Self::open(address, tls.clone(), &profile.auth) {
                Ok(conn) => {
                    return Ok(Self {
                        profile: profile.clone(),
                        conn,
                    })
                }
                // Keep going: a bootstrap list exists so that one broker being
                // down is not an outage.
                Err(err) => failures.push(err.to_string()),
            }
        }
        // Every address is named, in order, so the reader can see which one
        // failed how — the desktop error library reads the first host:port out
        // of this string for its title.
        Err(Error::Other(format!(
            "could not reach any bootstrap server: {}",
            failures.join("; ")
        )))
    }

    /// The protocol client for an existing cluster connection — same profile,
    /// same credentials, its own socket.
    pub fn for_connection(cluster: &ClusterConnection) -> Result<Self> {
        Self::connect(cluster.profile())
    }

    fn open(
        address: &str,
        tls: Option<Arc<rustls::ClientConfig>>,
        auth: &AuthConfig,
    ) -> Result<BrokerConnection> {
        let mut conn = BrokerConnection::connect(address, tls)?;
        sasl::authenticate(&mut conn, auth)?;
        Ok(conn)
    }

    fn ensure_writable(&self, op: &'static str) -> Result<()> {
        ensure_writable(&self.profile, op)
    }

    /// Whether this client's socket can still be trusted.
    ///
    /// False once a call has failed at the TRANSPORT — a write that did not go
    /// out, a reply that did not come back, a correlation id that did not
    /// match. Past any of those the read cursor may be sitting mid-frame, so
    /// every later call on this socket would decode one message's bytes as
    /// another's. It never goes back to true: the fix is to drop this client
    /// and connect again, which is what a caller holding one across calls is
    /// expected to do (see the module docs).
    ///
    /// A Kafka-level failure — a rejected reassignment, a read-only refusal, a
    /// version that does not negotiate — leaves this TRUE. Those arrive in a
    /// complete, correctly framed reply; the connection is fine and the next
    /// call on it will work.
    pub fn is_healthy(&self) -> bool {
        self.conn.is_healthy()
    }

    /// The KRaft metadata quorum: leader, voters and observers.
    ///
    /// DescribeQuorum is a controller API. In KRaft a broker is a raft observer
    /// and forwards the request to the active controller, which is the only way
    /// this can work from outside the cluster — the controller listener is
    /// normally not advertised to clients.
    ///
    /// A broker that answers NOT_CONTROLLER (or NOT_LEADER_OR_FOLLOWER) anyway
    /// is REPORTED, not chased. This used to re-ask the broker named by
    /// `Metadata.ControllerId`, which reads like the obvious fix and is not
    /// one: on KRaft that field is not "the controller" — `KafkaApis` answers
    /// it with a *randomly chosen* member of the current voter set, on purpose,
    /// so that clients spread their controller-bound traffic instead of
    /// stampeding one node. Following it therefore has no better odds than
    /// asking again, it costs a second connection and a second full SASL
    /// handshake, and on the ordinary KRaft deployment the address it names is
    /// a controller-listener endpoint no client can reach at all. So the code
    /// goes to the user through [`Self::no_controller`], which says what the
    /// cluster said and why Kavka cannot route around it.
    pub fn quorum_describe(&mut self) -> Result<QuorumInfo> {
        match quorum::describe(&mut self.conn)? {
            QuorumReply::Described(info) => Ok(info),
            QuorumReply::NotHere(code) => Err(self.no_controller(code)),
        }
    }

    fn no_controller(&self, code: i16) -> Error {
        Error::Other(format!(
            "{} would not describe the quorum ({}) — it is a broker, and it forwards this to the \
             active controller, so this usually means the cluster is electing one right now. Try \
             again in a moment; on KRaft the controllers themselves are normally on a listener \
             that is not advertised to clients, so there is nowhere else for Kavka to ask.",
            self.conn.address(),
            errors::describe(code, None)
        ))
    }

    /// Elects preferred leaders. `topic == None` covers every eligible
    /// partition in the cluster; see [`elect::elect`] for what Kafka omits from
    /// that form of the answer.
    pub fn elect_leaders(
        &mut self,
        topic: Option<&str>,
        partitions: Option<&[i32]>,
    ) -> Result<Vec<PartitionResult>> {
        self.ensure_writable("elect_leaders")?;
        elect::elect(&mut self.conn, topic, partitions)
    }

    /// Starts moving partitions onto the replica sets given.
    pub fn reassign_alter(&mut self, specs: &[ReassignmentSpec]) -> Result<Vec<PartitionResult>> {
        self.ensure_writable("reassign_alter")?;
        reassign::alter(&mut self.conn, specs)
    }

    /// Cancels in-flight moves, reverting each partition to the replica set it
    /// had before.
    pub fn reassign_cancel(&mut self, parts: &[TopicPartition]) -> Result<Vec<PartitionResult>> {
        self.ensure_writable("reassign_cancel")?;
        reassign::cancel(&mut self.conn, parts)
    }

    /// The moves still in flight. Empty means the cluster is settled.
    pub fn reassign_list(&mut self, topic: Option<&str>) -> Result<Vec<ReassignmentState>> {
        reassign::list(&mut self.conn, topic)
    }

    /// Every client quota, `<default>` entities included.
    pub fn quotas_list(&mut self) -> Result<Vec<QuotaEntity>> {
        quotas::list(&mut self.conn)
    }

    /// Sets or removes quota values on one entity. An op with `value: None`
    /// removes that setting.
    pub fn quotas_alter(&mut self, entity: &[QuotaEntityPart], ops: &[QuotaOp]) -> Result<()> {
        self.ensure_writable("quotas_alter")?;
        quotas::alter(&mut self.conn, entity, ops)
    }

    /// Every share group on the cluster (KIP-932), with its state and member
    /// count.
    ///
    /// A FAN-OUT, and it has to be: ListGroups is answered by each broker about
    /// the groups THAT broker coordinates, so asking the one we happen to be
    /// connected to returns a fraction of the answer that looks exactly like
    /// all of it. Every broker in the metadata table is asked, and a broker
    /// that cannot be reached fails the whole call rather than silently
    /// shortening the list — a share group missing from this view reads as "the
    /// group is gone", which is the one wrong answer worth failing over.
    ///
    /// The per-broker second round trip (ShareGroupDescribe, for the member
    /// counts ListGroups does not carry) needs no routing of its own: the
    /// broker that listed a group is by definition its coordinator.
    pub fn share_groups_list(&mut self) -> Result<Vec<ShareGroupInfo>> {
        share::ensure_enabled(&self.conn)?;
        let brokers = meta::brokers(&mut self.conn)?;

        // Keyed by group id: a group is coordinated by exactly one broker, so
        // this deduplicates nothing in the steady state — it is what keeps a
        // coordinator move DURING the fan-out from listing one group twice.
        let mut found: BTreeMap<String, ShareGroupInfo> = BTreeMap::new();
        for broker in &brokers {
            let listed = self.with_broker(&broker.host, broker.port, share::list_on)?;
            for group in listed {
                found.insert(group.group_id.clone(), group);
            }
        }
        Ok(found.into_values().collect())
    }

    /// One share group's members, their assignments, and the share-partition
    /// start offsets it has state for.
    ///
    /// Routed through FindCoordinator: a broker that does not coordinate this
    /// group answers NOT_COORDINATOR rather than forwarding, so the two calls
    /// go to the broker that does. They share that connection — one lookup, one
    /// dial, two calls.
    pub fn share_group_detail(&mut self, group_id: &str) -> Result<ShareGroupDetail> {
        share::ensure_enabled(&self.conn)?;
        let coordinator = share::find_coordinator(&mut self.conn, group_id)?;
        let group = group_id.to_string();
        self.with_broker(&coordinator.host, coordinator.port, move |conn| {
            let (state, members) = share::detail_on(conn, &group)?;
            let offsets = share::offsets_on(conn, &group)?;
            Ok(ShareGroupDetail {
                group_id: group,
                state,
                members,
                offsets,
            })
        })
    }

    /// Runs `call` against one named broker: this connection when it is already
    /// that broker, a fresh one otherwise.
    ///
    /// The reuse is not just an optimization — on a single-broker cluster every
    /// share-group call would otherwise pay a second TCP connect, TLS handshake
    /// and full SASL exchange to reach the socket it is already holding. The
    /// comparison is on host and port rather than on the address string, so the
    /// scheme a user typed in a bootstrap entry does not defeat it.
    fn with_broker<T>(
        &mut self,
        host: &str,
        port: u16,
        call: impl FnOnce(&mut BrokerConnection) -> Result<T>,
    ) -> Result<T> {
        if self.conn.is_at(host, port) {
            return call(&mut self.conn);
        }
        // Bracketed for an IPv6 literal, which otherwise re-parses as a host
        // with a port made of colons.
        let address = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let tls = tls::client_config(&self.profile.auth)?;
        let mut conn = Self::open(&address, tls, &self.profile.auth).map_err(|e| {
            Error::Other(format!(
                "could not reach {address}, which this cluster named as one of its own brokers: {e}"
            ))
        })?;
        call(&mut conn)
    }
}

// ---------------------------------------------------------------------------
// One-shot entry points — the IPC surface.
//
// Every mutating one calls `ClusterConnection::ensure_writable` FIRST, before
// a socket is opened (docs/ARCHITECTURE.md D5): a read-only connection must not
// so much as authenticate on behalf of a write.
// ---------------------------------------------------------------------------

pub fn quorum_describe(cluster: &ClusterConnection) -> Result<QuorumInfo> {
    ProtocolClient::for_connection(cluster)?.quorum_describe()
}

pub fn elect_leaders(
    cluster: &ClusterConnection,
    topic: Option<&str>,
    partitions: Option<&[i32]>,
) -> Result<Vec<PartitionResult>> {
    cluster.ensure_writable("elect_leaders")?;
    ProtocolClient::for_connection(cluster)?.elect_leaders(topic, partitions)
}

pub fn reassign_alter(
    cluster: &ClusterConnection,
    specs: &[ReassignmentSpec],
) -> Result<Vec<PartitionResult>> {
    cluster.ensure_writable("reassign_alter")?;
    ProtocolClient::for_connection(cluster)?.reassign_alter(specs)
}

pub fn reassign_cancel(
    cluster: &ClusterConnection,
    parts: &[TopicPartition],
) -> Result<Vec<PartitionResult>> {
    cluster.ensure_writable("reassign_cancel")?;
    ProtocolClient::for_connection(cluster)?.reassign_cancel(parts)
}

pub fn reassign_list(
    cluster: &ClusterConnection,
    topic: Option<&str>,
) -> Result<Vec<ReassignmentState>> {
    ProtocolClient::for_connection(cluster)?.reassign_list(topic)
}

pub fn quotas_list(cluster: &ClusterConnection) -> Result<Vec<QuotaEntity>> {
    ProtocolClient::for_connection(cluster)?.quotas_list()
}

pub fn quotas_alter(
    cluster: &ClusterConnection,
    entity: &[QuotaEntityPart],
    ops: &[QuotaOp],
) -> Result<()> {
    cluster.ensure_writable("quotas_alter")?;
    ProtocolClient::for_connection(cluster)?.quotas_alter(entity, ops)
}

pub fn share_groups_list(cluster: &ClusterConnection) -> Result<Vec<ShareGroupInfo>> {
    ProtocolClient::for_connection(cluster)?.share_groups_list()
}

pub fn share_group_detail(cluster: &ClusterConnection, group_id: &str) -> Result<ShareGroupDetail> {
    ProtocolClient::for_connection(cluster)?.share_group_detail(group_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Environment;

    fn profile(auth: AuthConfig, bootstrap: Vec<String>) -> ConnectionProfile {
        ConnectionProfile {
            id: "unit".into(),
            name: "unit".into(),
            environment: Environment::Dev,
            bootstrap_servers: bootstrap,
            auth,
            read_only: false,
            schema_registry: None,
            connect_clusters: Vec::new(),
            metrics_endpoint: None,
            sampler_interval_ms: None,
            wasm_serdes: Vec::new(),
        }
    }

    /// `ProtocolClient` holds a live socket and has no `Debug`, so failures are
    /// unwrapped by pattern rather than by `unwrap_err`.
    fn connect_failure(profile: &ConnectionProfile) -> String {
        match ProtocolClient::connect(profile) {
            Ok(_) => panic!("this profile must not connect"),
            Err(err) => err.to_string(),
        }
    }

    /// The MSK guard has to fire before any socket, because the risk is handing
    /// a live AWS credential to whatever host the profile names.
    #[test]
    fn an_msk_profile_pointed_at_a_non_aws_host_is_refused_before_connecting() {
        let err = connect_failure(&profile(
            AuthConfig::AwsMskIam {
                region: "eu-west-1".into(),
                profile: None,
            },
            vec!["evil.example.com:9098".into()],
        ));
        assert!(err.contains("refusing to connect"), "got {err}");
        assert!(err.contains("act as you in AWS"), "got {err}");
    }

    #[test]
    fn kerberos_is_refused_by_name_before_connecting() {
        let err = connect_failure(&profile(
            AuthConfig::Kerberos {
                service_name: "kafka".into(),
                principal: "alice@EXAMPLE".into(),
            },
            vec!["broker:9092".into()],
        ));
        assert!(err.contains("Kerberos"), "got {err}");
        assert!(err.contains("GSSAPI"), "got {err}");
    }

    #[test]
    fn a_profile_with_no_bootstrap_servers_says_so_rather_than_timing_out() {
        let err = connect_failure(&profile(AuthConfig::Plaintext, Vec::new()));
        assert!(err.contains("no bootstrap servers"), "got {err}");
    }

    /// Every address is tried and every failure is named, so a bootstrap list
    /// with one bad entry does not hide which one it was.
    #[test]
    fn unreachable_bootstrap_servers_are_all_named_in_the_failure() {
        // Port 1 on the loopback: refused immediately on every platform the
        // app ships on, so this stays a fast test rather than a timeout.
        let err = connect_failure(&profile(
            AuthConfig::Plaintext,
            vec!["127.0.0.1:1".into(), "127.0.0.1:2".into()],
        ));
        assert!(
            err.contains("could not reach any bootstrap server"),
            "got {err}"
        );
        assert!(err.contains("127.0.0.1:1"), "got {err}");
        assert!(err.contains("127.0.0.1:2"), "got {err}");
    }

    /// D5: read-only is enforced in core. The judgement is the profile's flag
    /// alone, so it is provable without a cluster — and the refusal names the
    /// operation, which is what the desktop error library's `read-only` row
    /// shows the user.
    #[test]
    fn read_only_refuses_every_mutator_by_name() {
        let writable = profile(AuthConfig::Plaintext, vec!["127.0.0.1:1".into()]);
        let read_only = ConnectionProfile {
            read_only: true,
            ..writable.clone()
        };
        for op in [
            "elect_leaders",
            "reassign_alter",
            "reassign_cancel",
            "quotas_alter",
        ] {
            assert!(ensure_writable(&writable, op).is_ok());
            let err = ensure_writable(&read_only, op).unwrap_err().to_string();
            assert!(err.contains("read-only"), "got {err}");
            assert!(err.contains(op), "got {err}");
        }
    }

    /// The read-only checks that are NOT here are as deliberate as the ones
    /// that are: describing a quorum, listing reassignments and listing quotas
    /// read nothing and write nothing, and refusing them would make read-only
    /// mode mean "cannot look", which is the opposite of its purpose.
    #[test]
    fn read_only_does_not_block_the_read_paths() {
        let read_only = ConnectionProfile {
            read_only: true,
            ..profile(AuthConfig::Plaintext, vec!["127.0.0.1:1".into()])
        };
        // The bootstrap address refuses connections, so each of these must fail
        // — the assertion is HOW. A read path that reached the socket was not
        // refused as a mutation; one that answered `ReadOnly` never tried.
        let outcomes: Vec<Result<()>> = vec![
            ProtocolClient::connect(&read_only)
                .and_then(|mut client| client.quorum_describe())
                .map(|_| ()),
            ProtocolClient::connect(&read_only)
                .and_then(|mut client| client.reassign_list(None))
                .map(|_| ()),
            ProtocolClient::connect(&read_only)
                .and_then(|mut client| client.quotas_list())
                .map(|_| ()),
            ProtocolClient::connect(&read_only)
                .and_then(|mut client| client.share_groups_list())
                .map(|_| ()),
            ProtocolClient::connect(&read_only)
                .and_then(|mut client| client.share_group_detail("anything"))
                .map(|_| ()),
        ];
        for outcome in outcomes {
            let Err(err) = outcome else {
                panic!("nothing is listening on 127.0.0.1:1");
            };
            assert!(
                !matches!(err, Error::ReadOnly(_)),
                "a read path was refused as a mutation: {err}"
            );
        }
    }
}
