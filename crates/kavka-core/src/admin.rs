//! Admin operations: topics, configs, consumer groups, ACLs, quotas,
//! partition reassignment. Implemented over rdkafka's AdminClient where
//! possible; gaps (e.g. KIP-932 Share Groups) go through hand-rolled protocol
//! frames per docs/ARCHITECTURE.md D2.
//!
//! Everything here BLOCKS, like the rest of the core — the Tauri shell wraps
//! each call in `spawn_blocking`. Every mutating entry point
//! ([`create_topic`], [`delete_topic`], [`offsets_reset`],
//! [`broker_config_set`]) calls [`ClusterConnection::ensure_writable`] first,
//! so read-only mode is enforced here rather than in the UI
//! (docs/ARCHITECTURE.md D5).
//!
//! ACLs live in [`crate::acl`] rather than here: they need librdkafka's C admin
//! API, which rdkafka 0.37 does not wrap, and that module owns the plumbing —
//! which [`broker_config_set`] then borrows for IncrementalAlterConfigs.

use crate::connection::ClusterConnection;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[cfg(feature = "kafka")]
use crate::connection::auth::KavkaClientContext;
#[cfg(feature = "kafka")]
use crate::connection::{is_internal, METADATA_TIMEOUT};
#[cfg(feature = "kafka")]
use rdkafka::admin::{AdminOptions, ConfigSource, NewTopic, ResourceSpecifier, TopicReplication};
#[cfg(feature = "kafka")]
use rdkafka::consumer::{BaseConsumer, CommitMode, Consumer};
#[cfg(feature = "kafka")]
use rdkafka::error::RDKafkaErrorCode;
#[cfg(feature = "kafka")]
use rdkafka::topic_partition_list::{Offset, TopicPartitionList};
#[cfg(feature = "kafka")]
use rdkafka::util::Timeout;
#[cfg(feature = "kafka")]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "kafka")]
use std::time::Duration;

/// Budget for one AdminClient round trip, covering the controller's own work
/// (CreateTopics waits for the topic to exist on every broker before it
/// answers). Deliberately longer than [`METADATA_TIMEOUT`]: a create or delete
/// that has already been accepted is worth waiting out, because reporting a
/// timeout for an operation the cluster went on to perform is the one outcome
/// a user cannot act on.
///
/// `pub(crate)` for [`crate::acl`], which drives librdkafka's C admin API
/// directly (rdkafka 0.37 exposes neither ACLs nor IncrementalAlterConfigs) and
/// has to spend the same budget — one admin timeout in the app means one
/// number, not one per module.
#[cfg(feature = "kafka")]
pub(crate) const ADMIN_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Wire types. Field names are the IPC contract — the TypeScript in
// apps/desktop/src mirrors them exactly, so renaming one is a breaking change
// on both sides of the bridge.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicInfo {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u16,
    pub internal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionDetail {
    pub partition: i32,
    pub leader: i32,
    pub replicas: Vec<i32>,
    pub isr: Vec<i32>,
    pub earliest_offset: i64,
    pub latest_offset: i64,
}

/// One entry of a topic's (or broker's) configuration.
///
/// `value` is `None` for a sensitive entry — see [`ConfigEntry::is_sensitive`].
/// `source` carries Kafka's own provenance name verbatim so nothing is lost by
/// the boolean summary in `is_default`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub name: String,
    pub value: Option<String>,
    /// Whether this value is inherited rather than set on the topic itself —
    /// see [`config_is_default`].
    pub is_default: bool,
    pub is_read_only: bool,
    /// Kafka never sends the value of a sensitive entry, and Kavka never
    /// invents one: `value` is `None` whenever this is set (D5).
    pub is_sensitive: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicDetail {
    pub name: String,
    pub internal: bool,
    pub partitions: Vec<PartitionDetail>,
    pub configs: Vec<ConfigEntry>,
}

/// One `name = value` override for [`create_topic`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupInfo {
    pub group_id: String,
    pub state: String,
    pub protocol_type: String,
    pub member_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TopicPartition {
    pub topic: String,
    pub partition: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMember {
    pub member_id: String,
    pub client_id: String,
    pub client_host: String,
    pub assignments: Vec<TopicPartition>,
}

/// Where a group stands on one partition. `committed` is `None` when the group
/// has never committed there; `lag` is `None` for the same reason, because a
/// group with no position has no measurable backlog — reporting `end` as the
/// lag would invent one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupOffset {
    pub topic: String,
    pub partition: i32,
    pub committed: Option<i64>,
    pub end_offset: i64,
    pub lag: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDetail {
    pub group_id: String,
    pub state: String,
    pub members: Vec<GroupMember>,
    pub offsets: Vec<GroupOffset>,
}

/// Where an offset reset should land, before it is resolved against a
/// particular partition's watermarks (see [`resolve_reset_offset`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResetTarget {
    Earliest,
    Latest,
    Offset { offset: i64 },
    TimestampMs { timestamp_ms: i64 },
    ShiftBy { shift_by: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetResetSpec {
    pub group_id: String,
    pub topic: String,
    /// `None` means every partition of `topic`.
    pub partitions: Option<Vec<i32>>,
    pub target: ResetTarget,
    /// Skip the active-group guard. Kafka itself still rejects a reset on a
    /// group with live members, so this buys a clearer broker error rather
    /// than a different outcome — see [`ensure_group_resettable`].
    pub force: bool,
}

// ---------------------------------------------------------------------------
// Pure logic. No cluster, no rdkafka — unit-tested directly, and reusable by
// the UI, which previews "will reprocess about N messages" from the same
// arithmetic before the user confirms (docs/DESIGN.md §7).
// ---------------------------------------------------------------------------

/// What a reset needs to know about one partition to resolve a [`ResetTarget`]
/// into a concrete offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionBounds {
    /// Low watermark: the oldest offset still retained.
    pub earliest: i64,
    /// High watermark: one past the newest message.
    pub end: i64,
    /// The group's current committed offset, if it has one.
    pub committed: Option<i64>,
    /// What the broker answered for a timestamp lookup, if one was made and it
    /// found a message. `None` means "nothing at or after that time".
    pub at_timestamp: Option<i64>,
}

/// Resolves a [`ResetTarget`] against one partition.
///
/// Every result is clamped into `[earliest, end]`. Kafka would accept an
/// out-of-range commit and the group would then fail on its next fetch with
/// OFFSET_OUT_OF_RANGE — at which point `auto.offset.reset` silently decides
/// where the application actually restarts, which is precisely the surprise a
/// deliberate reset exists to avoid. Clamping makes the number Kavka shows the
/// number the application will use.
///
/// - `earliest` / `latest` are the watermarks themselves.
/// - `offset` and `shift_by` are clamped; `shift_by` counts from the current
///   committed offset, or from `earliest` when the group has never committed
///   here (a group with no position has nothing to shift, and `earliest` is
///   the only endpoint that makes a positive shift mean "skip this much").
/// - `timestamp_ms` uses the broker's answer, falling back to `end` when no
///   message is at or after that time — the same rule Kafka's own
///   `--to-datetime` follows.
pub fn resolve_reset_offset(target: &ResetTarget, bounds: PartitionBounds) -> i64 {
    let PartitionBounds {
        earliest,
        end,
        committed,
        at_timestamp,
    } = bounds;
    // `clamp` panics if the range is inverted, and inverted watermarks are a
    // thing a broker mid-truncation can briefly report.
    let clamp = |value: i64| value.clamp(earliest, end.max(earliest));

    match target {
        ResetTarget::Earliest => earliest,
        ResetTarget::Latest => end.max(earliest),
        ResetTarget::Offset { offset } => clamp(*offset),
        ResetTarget::TimestampMs { .. } => clamp(at_timestamp.unwrap_or(end)),
        ResetTarget::ShiftBy { shift_by } => {
            clamp(committed.unwrap_or(earliest).saturating_add(*shift_by))
        }
    }
}

/// The active-group guard.
///
/// A reset is an OffsetCommit made from outside the group, which the broker
/// answers with UNKNOWN_MEMBER_ID whenever the group has live members. Left to
/// Kafka the user gets that string; here they get the group's state, its member
/// count, and the one action that fixes it. `force` skips the check for the
/// operator who knows the group is about to die anyway — it does not make the
/// broker accept it.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
fn ensure_group_resettable(group_id: &str, state: &str, members: usize, force: bool) -> Result<()> {
    if force || state == "Empty" {
        return Ok(());
    }
    Err(Error::Other(format!(
        "\"{group_id}\" is {state} with {members} member(s) — Kafka rejects an offset reset while \
         members are consuming. Stop the application, wait for the group to report Empty, then \
         reset."
    )))
}

/// Whether a config entry is inherited rather than set on the resource itself.
///
/// Kafka's DescribeConfigs reports provenance as a source, and only
/// `DYNAMIC_TOPIC_CONFIG` means "somebody set this on this topic". Everything
/// else — the hardcoded default, `server.properties`, a cluster-wide dynamic
/// broker setting — is a value the topic merely inherits, and the config view
/// exists to show the difference (docs/DESIGN.md §5.10, diff-from-default).
///
/// Brokers before 1.1.0 don't report a source at all; there the entry's own
/// `is_default` flag is the only signal, so it is used verbatim.
#[cfg(feature = "kafka")]
fn config_is_default(source: &ConfigSource, reported: bool) -> bool {
    match source {
        ConfigSource::DynamicTopic => false,
        ConfigSource::Default
        | ConfigSource::StaticBroker
        | ConfigSource::DynamicBroker
        | ConfigSource::DynamicDefaultBroker => true,
        ConfigSource::Unknown => reported,
    }
}

/// The broker-resource twin of [`config_is_default`].
///
/// The two cannot be one function, because the *same* source means opposite
/// things on the two resources. `DYNAMIC_BROKER_CONFIG` on a topic is
/// something the topic inherits from the broker it happens to live on; on the
/// broker itself it is the override somebody set, and it is the only thing
/// [`broker_config_set`] can revert. Reusing the topic rule here would render
/// every broker override as "default", which is precisely backwards for a view
/// whose job is to show what has been changed from stock.
///
/// So for a broker only `DEFAULT_CONFIG` — Kafka's own compiled-in value —
/// counts as default. `STATIC_BROKER_CONFIG` is `server.properties`: somebody
/// did set it, Kavka cannot unset it, and calling it "default" would hide a
/// deliberate operator decision.
#[cfg(feature = "kafka")]
fn broker_config_is_default(source: &ConfigSource, reported: bool) -> bool {
    match source {
        ConfigSource::Default => true,
        ConfigSource::DynamicBroker
        | ConfigSource::DynamicDefaultBroker
        | ConfigSource::StaticBroker
        | ConfigSource::DynamicTopic => false,
        // Same fallback as the topic rule: a pre-1.1.0 broker sends no source
        // at all, so its own flag is the only signal there is.
        ConfigSource::Unknown => reported,
    }
}

/// Kafka's own name for a config source, so the value in the UI is the value in
/// the protocol and in `kafka-configs.sh` output.
#[cfg(feature = "kafka")]
fn config_source_name(source: &ConfigSource) -> &'static str {
    match source {
        ConfigSource::Unknown => "UNKNOWN",
        ConfigSource::DynamicTopic => "DYNAMIC_TOPIC_CONFIG",
        ConfigSource::DynamicBroker => "DYNAMIC_BROKER_CONFIG",
        ConfigSource::DynamicDefaultBroker => "DYNAMIC_DEFAULT_BROKER_CONFIG",
        ConfigSource::StaticBroker => "STATIC_BROKER_CONFIG",
        ConfigSource::Default => "DEFAULT_CONFIG",
    }
}

/// Parses a classic `ConsumerProtocolAssignment` record, which librdkafka hands
/// back as opaque bytes:
///
/// ```text
/// Version    => int16
/// Assignment => [ Topic => string, Partitions => [int32] ]
/// UserData   => bytes
/// ```
///
/// Big-endian throughout, and every length is *signed* — so a truncated,
/// hostile, or simply newer encoding can claim an array of two billion entries.
/// Each length is therefore checked against what is actually left in the buffer
/// before anything is allocated, and any failure yields an empty assignment: a
/// member row with no partitions is a much better outcome than a whole
/// `group_detail` that fails because one member spoke a dialect we don't read.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
fn parse_member_assignment(bytes: &[u8]) -> Vec<TopicPartition> {
    fn parse(bytes: &[u8]) -> Option<Vec<TopicPartition>> {
        let mut cursor = Cursor { bytes, at: 0 };
        let _version = cursor.i16()?;
        let topic_count = cursor.len()?;
        let mut out = Vec::new();
        for _ in 0..topic_count {
            let topic = cursor.string()?;
            let partition_count = cursor.len()?;
            for _ in 0..partition_count {
                out.push(TopicPartition {
                    topic: topic.clone(),
                    partition: cursor.i32()?,
                });
            }
        }
        Some(out)
    }

    struct Cursor<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Cursor<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> {
            let end = self.at.checked_add(n)?;
            let slice = self.bytes.get(self.at..end)?;
            self.at = end;
            Some(slice)
        }

        fn i16(&mut self) -> Option<i16> {
            Some(i16::from_be_bytes(self.take(2)?.try_into().ok()?))
        }

        fn i32(&mut self) -> Option<i32> {
            Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
        }

        /// An array or string length. Negative means null; anything larger than
        /// the bytes that remain is a lie, and rejecting it here is what keeps
        /// a bogus count from turning into a huge allocation.
        fn len(&mut self) -> Option<usize> {
            let raw = self.i32()?;
            let len = usize::try_from(raw).ok()?;
            (len <= self.bytes.len().saturating_sub(self.at)).then_some(len)
        }

        fn string(&mut self) -> Option<String> {
            let len = usize::try_from(self.i16()?).ok()?;
            String::from_utf8(self.take(len)?.to_vec()).ok()
        }
    }

    parse(bytes).unwrap_or_default()
}

/// A blocking executor for rdkafka's admin futures.
///
/// The admin API is the one async corner of rdkafka, but its futures need no
/// reactor and no timer: `AdminClient` spawns a plain OS thread that polls
/// librdkafka's queue and completes a `oneshot`, so all this has to do is park
/// until that happens. Core is blocking by contract and the shell runs it under
/// `spawn_blocking`, where standing up a tokio runtime would panic with "cannot
/// start a runtime from within a runtime" — so the twenty lines below are the
/// cheap option as well as the safe one.
#[cfg(feature = "kafka")]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct Unpark(std::thread::Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            // `park` may return spuriously; re-polling is the whole handling.
            Poll::Pending => std::thread::park(),
        }
    }
}

// ---------------------------------------------------------------------------
// Topics
// ---------------------------------------------------------------------------

/// Metadata, per-partition watermarks and the full configuration for one topic.
#[cfg(feature = "kafka")]
pub fn topic_detail(conn: &ClusterConnection, topic: &str) -> Result<TopicDetail> {
    let metadata = conn.with_consumer("reading topic metadata", |consumer| {
        consumer.fetch_metadata(Some(topic), METADATA_TIMEOUT)
    })?;
    let described = metadata
        .topics()
        .iter()
        .find(|t| t.name() == topic)
        .ok_or_else(|| unknown_topic(topic))?;
    if let Some(err) = described.error() {
        return Err(topic_metadata_error(topic, err));
    }

    let mut partitions = Vec::with_capacity(described.partitions().len());
    for partition in described.partitions() {
        let (earliest_offset, latest_offset) = watermarks(conn, topic, partition.id())?;
        partitions.push(PartitionDetail {
            partition: partition.id(),
            leader: partition.leader(),
            replicas: partition.replicas().to_vec(),
            isr: partition.isr().to_vec(),
            earliest_offset,
            latest_offset,
        });
    }
    partitions.sort_by_key(|p| p.partition);

    Ok(TopicDetail {
        name: topic.to_string(),
        internal: is_internal(topic),
        partitions,
        configs: topic_configs(conn, topic)?,
    })
}

#[cfg(feature = "kafka")]
fn topic_configs(conn: &ClusterConnection, topic: &str) -> Result<Vec<ConfigEntry>> {
    describe_configs(
        conn,
        &ResourceSpecifier::Topic(topic),
        "reading the topic's configuration",
        config_is_default,
    )
}

/// One DescribeConfigs round trip, mapped to wire rows and sorted by name.
///
/// `is_default` is a parameter rather than a match inside, because "is this
/// value set or inherited" is the one question whose answer depends on which
/// resource was asked — see [`broker_config_is_default`].
#[cfg(feature = "kafka")]
fn describe_configs(
    conn: &ClusterConnection,
    resource: &ResourceSpecifier<'_>,
    what: &str,
    is_default: fn(&ConfigSource, bool) -> bool,
) -> Result<Vec<ConfigEntry>> {
    let described = block_on(conn.admin()?.describe_configs([resource], &admin_options()))
        .map_err(|e| admin_error(conn, what, &e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| admin_error(conn, what, "the broker returned no configuration for it"))?
        .map_err(|code| admin_error(conn, what, &code.to_string()))?;

    let mut configs: Vec<ConfigEntry> = described
        .entries
        .into_iter()
        .map(|entry| ConfigEntry {
            name: entry.name,
            // Kafka nulls a sensitive value out on the wire; this makes that
            // structural rather than a property of the broker's manners (D5).
            value: if entry.is_sensitive {
                None
            } else {
                entry.value
            },
            is_default: is_default(&entry.source, entry.is_default),
            is_read_only: entry.is_read_only,
            is_sensitive: entry.is_sensitive,
            source: config_source_name(&entry.source).to_string(),
        })
        .collect();
    configs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(configs)
}

/// Creates a topic and waits for the controller to finish, so a returned `Ok`
/// means the topic exists on every broker.
///
/// `replication_factor` of 0 means "use the cluster's
/// `default.replication.factor`" — Kafka spells that -1 on the wire, which an
/// unsigned count has no room for.
#[cfg(feature = "kafka")]
pub fn create_topic(
    conn: &ClusterConnection,
    name: &str,
    partitions: u32,
    replication_factor: u16,
    configs: &[TopicConfig],
) -> Result<()> {
    conn.ensure_writable("create topic")?;
    if name.trim().is_empty() {
        return Err(Error::Other("give the topic a name".into()));
    }
    let num_partitions = i32::try_from(partitions)
        .ok()
        .filter(|count| *count >= 1)
        .ok_or_else(|| {
            Error::Other(format!(
                "a topic needs at least one partition, and Kafka's limit is {} — {partitions} was \
                 requested",
                i32::MAX
            ))
        })?;
    let replication = TopicReplication::Fixed(if replication_factor == 0 {
        -1
    } else {
        i32::from(replication_factor)
    });

    let mut topic = NewTopic::new(name, num_partitions, replication);
    for config in configs {
        topic = topic.set(&config.name, &config.value);
    }
    let results = block_on(conn.admin()?.create_topics([&topic], &admin_options()))
        .map_err(|e| admin_error(conn, "creating the topic", &e.to_string()))?;
    first_topic_result(conn, "creating the topic", results)
}

/// Deletes a topic and every message in it, waiting for the controller.
#[cfg(feature = "kafka")]
pub fn delete_topic(conn: &ClusterConnection, topic: &str) -> Result<()> {
    conn.ensure_writable("delete topic")?;
    let results = block_on(conn.admin()?.delete_topics(&[topic], &admin_options()))
        .map_err(|e| admin_error(conn, "deleting the topic", &e.to_string()))?;
    first_topic_result(conn, "deleting the topic", results)
}

#[cfg(feature = "kafka")]
fn first_topic_result(
    conn: &ClusterConnection,
    what: &str,
    results: Vec<rdkafka::admin::TopicResult>,
) -> Result<()> {
    match results.into_iter().next() {
        Some(Ok(_)) => Ok(()),
        Some(Err((topic, code))) => Err(admin_error(conn, what, &format!("{topic}: {code}"))),
        None => Err(admin_error(
            conn,
            what,
            "the broker acknowledged the request without saying what happened",
        )),
    }
}

// ---------------------------------------------------------------------------
// Broker configuration
// ---------------------------------------------------------------------------

/// Everything one broker reports about its own configuration, sorted by name.
///
/// Same [`ConfigEntry`] rows as a topic's configuration — same sensitive-value
/// rule, same provenance string — with the one difference that matters spelled
/// out in [`broker_config_is_default`].
#[cfg(feature = "kafka")]
pub fn broker_configs(conn: &ClusterConnection, broker_id: i32) -> Result<Vec<ConfigEntry>> {
    describe_configs(
        conn,
        &ResourceSpecifier::Broker(broker_id),
        &format!("reading broker {broker_id}'s configuration"),
        broker_config_is_default,
    )
}

/// Changes one broker configuration entry, or reverts it to what the broker
/// inherits when `value` is `None`.
///
/// **IncrementalAlterConfigs, never AlterConfigs.** The older call replaces a
/// resource's *entire* dynamic configuration with what the request carries, so
/// using it to change one key silently deletes every other override on that
/// broker — a footgun Kafka added KIP-339 specifically to remove, and the
/// reason this path drops to librdkafka's C API rather than using rdkafka's
/// `alter_configs` (see [`crate::acl::native`]).
///
/// `None` sends a DELETE operation rather than an empty string: those are
/// different requests with different outcomes, and "revert to default" is the
/// one an operator undoing a change actually wants.
#[cfg(feature = "kafka")]
pub fn broker_config_set(
    conn: &ClusterConnection,
    broker_id: i32,
    name: &str,
    value: Option<&str>,
) -> Result<()> {
    use rdkafka::bindings as rdsys;

    conn.ensure_writable("set broker config")?;
    if name.trim().is_empty() {
        return Err(Error::Other(
            "name the configuration entry to change".into(),
        ));
    }
    let what = match value {
        Some(_) => format!("setting \"{name}\" on broker {broker_id}"),
        None => format!("reverting \"{name}\" on broker {broker_id} to its default"),
    };

    let resource = crate::acl::native::ConfigResource::broker(broker_id)?;
    resource.set(name.trim(), value)?;
    let mut resources = [resource.ptr()];

    let event = crate::acl::native::request(
        conn,
        &what,
        rdkafka::types::RDKafkaAdminOp::RD_KAFKA_ADMIN_OP_INCREMENTALALTERCONFIGS,
        rdsys::RD_KAFKA_EVENT_INCREMENTALALTERCONFIGS_RESULT,
        |client, options, queue| unsafe {
            rdsys::rd_kafka_IncrementalAlterConfigs(
                client,
                resources.as_mut_ptr(),
                resources.len(),
                options,
                queue,
            );
        },
    )?;

    let result = unsafe { rdsys::rd_kafka_event_IncrementalAlterConfigs_result(event.ptr()) };
    if result.is_null() {
        return Err(admin_error(
            conn,
            &what,
            "the broker acknowledged the request without saying what happened",
        ));
    }
    let mut count = 0usize;
    let altered =
        unsafe { rdsys::rd_kafka_IncrementalAlterConfigs_result_resources(result, &mut count) };
    for index in 0..count {
        let resource = unsafe { *altered.add(index) };
        let code = unsafe { rdsys::rd_kafka_ConfigResource_error(resource) };
        if code != rdkafka::types::RDKafkaRespErr::RD_KAFKA_RESP_ERR_NO_ERROR {
            let detail = unsafe { rdsys::rd_kafka_ConfigResource_error_string(resource) };
            let detail = crate::acl::native::owned(detail);
            let cause = if detail.is_empty() {
                RDKafkaErrorCode::from(code).to_string()
            } else {
                detail
            };
            return Err(admin_error(conn, &what, &cause));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Consumer groups
// ---------------------------------------------------------------------------

/// Every consumer group the cluster knows about, sorted by id.
#[cfg(feature = "kafka")]
pub fn groups_list(conn: &ClusterConnection) -> Result<Vec<GroupInfo>> {
    let list = conn.with_consumer("listing consumer groups", |consumer| {
        consumer.fetch_group_list(None, METADATA_TIMEOUT)
    })?;
    let mut groups: Vec<GroupInfo> = list
        .groups()
        .iter()
        .map(|group| GroupInfo {
            group_id: group.name().to_string(),
            state: group.state().to_string(),
            protocol_type: group.protocol_type().to_string(),
            member_count: u32::try_from(group.members().len()).unwrap_or(u32::MAX),
        })
        .collect();
    groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
    Ok(groups)
}

/// One group's members, their assignments, and its committed offsets with lag.
#[cfg(feature = "kafka")]
pub fn group_detail(conn: &ClusterConnection, group_id: &str) -> Result<GroupDetail> {
    let (state, members) = describe_group(conn, group_id)?;

    let assigned: BTreeSet<(String, i32)> = members
        .iter()
        .flat_map(|member| member.assignments.iter())
        .map(|tp| (tp.topic.clone(), tp.partition))
        .collect();
    let candidates = candidate_partitions(conn, &members)?;

    let consumer = conn.new_group_consumer(group_id)?;
    let committed = committed_map(conn, &consumer, &candidates)?;
    // A partition is worth a row if the group is reading it or has ever
    // committed on it. Without the second half a stopped application shows
    // nothing; without the first, a group that has just been assigned work it
    // hasn't finished shows nothing either.
    let reportable: Vec<(String, i32)> = candidates
        .into_iter()
        .filter(|key| assigned.contains(key) || committed.get(key).copied().flatten().is_some())
        .collect();
    let offsets = read_offsets(conn, &committed, &reportable)?;

    Ok(GroupDetail {
        group_id: group_id.to_string(),
        state,
        members,
        offsets,
    })
}

#[cfg(feature = "kafka")]
fn describe_group(conn: &ClusterConnection, group_id: &str) -> Result<(String, Vec<GroupMember>)> {
    let list = conn.with_consumer("describing the consumer group", |consumer| {
        consumer.fetch_group_list(Some(group_id), METADATA_TIMEOUT)
    })?;
    let group = list
        .groups()
        .iter()
        .find(|group| group.name() == group_id)
        .ok_or_else(|| {
            Error::Other(format!(
                "this cluster has no consumer group named \"{group_id}\" — a group appears once an \
                 application joins it or commits an offset, and disappears again when its last \
                 offset expires"
            ))
        })?;
    let members = group
        .members()
        .iter()
        .map(|member| GroupMember {
            member_id: member.id().to_string(),
            client_id: member.client_id().to_string(),
            client_host: member.client_host().to_string(),
            assignments: member
                .assignment()
                .map(parse_member_assignment)
                .unwrap_or_default(),
        })
        .collect();
    Ok((group.state().to_string(), members))
}

/// The topic-partitions a [`group_detail`] should ask the coordinator about.
///
/// Kafka's own answer is an OffsetFetch with a null topic list, which
/// librdkafka doesn't expose — `committed_offsets` needs an explicit list — so
/// the list is reconstructed from every partition of every topic the group's
/// members are assigned to. A group with no members has nothing to reconstruct
/// from, so it falls back to every non-internal topic on the cluster: still one
/// OffsetFetch, and the only way to answer "where did this stopped application
/// get to", which is exactly when a group has no members.
#[cfg(feature = "kafka")]
fn candidate_partitions(
    conn: &ClusterConnection,
    members: &[GroupMember],
) -> Result<Vec<(String, i32)>> {
    let topics: BTreeSet<&str> = members
        .iter()
        .flat_map(|member| member.assignments.iter())
        .map(|tp| tp.topic.as_str())
        .collect();
    let metadata = conn.with_consumer("listing topics", |consumer| {
        consumer.fetch_metadata(None, METADATA_TIMEOUT)
    })?;

    let mut out = Vec::new();
    for topic in metadata.topics() {
        let wanted = if topics.is_empty() {
            !is_internal(topic.name())
        } else {
            topics.contains(topic.name())
        };
        if !wanted {
            continue;
        }
        for partition in topic.partitions() {
            out.push((topic.name().to_string(), partition.id()));
        }
    }
    out.sort();
    Ok(out)
}

/// Committed offsets for `partitions`, as the group's coordinator has them.
/// `None` means the group has never committed there — librdkafka reports that
/// as `Offset::Invalid`, and a negative raw offset means the same thing.
#[cfg(feature = "kafka")]
fn committed_map(
    conn: &ClusterConnection,
    consumer: &BaseConsumer<KavkaClientContext>,
    partitions: &[(String, i32)],
) -> Result<BTreeMap<(String, i32), Option<i64>>> {
    if partitions.is_empty() {
        return Ok(BTreeMap::new());
    }
    let query = partition_list(partitions, Offset::Invalid)?;
    let committed = consumer
        .committed_offsets(query, METADATA_TIMEOUT)
        .map_err(|e| {
            conn.describe_failure(format!("reading the group's committed offsets failed: {e}"))
        })?;
    Ok(committed
        .to_topic_map()
        .into_iter()
        .map(|(key, offset)| {
            let offset = match offset {
                Offset::Offset(at) if at >= 0 => Some(at),
                _ => None,
            };
            (key, offset)
        })
        .collect())
}

/// Turns a committed-offset map into wire rows, adding each partition's end
/// watermark and the lag that follows from it.
#[cfg(feature = "kafka")]
fn read_offsets(
    conn: &ClusterConnection,
    committed: &BTreeMap<(String, i32), Option<i64>>,
    partitions: &[(String, i32)],
) -> Result<Vec<GroupOffset>> {
    let mut out = Vec::with_capacity(partitions.len());
    for key in partitions {
        let (_, end_offset) = watermarks(conn, &key.0, key.1)?;
        let committed = committed.get(key).copied().flatten();
        out.push(GroupOffset {
            topic: key.0.clone(),
            partition: key.1,
            committed,
            end_offset,
            // A commit can sit past the end — a reset to latest that raced a
            // truncation, or a stale watermark. Negative lag is never a true
            // statement about outstanding work, so it floors at zero.
            lag: committed.map(|at| (end_offset - at).max(0)),
        });
    }
    Ok(out)
}

/// Moves a group's committed offsets and reports where they landed.
///
/// The offsets are read back from the coordinator *after* the commit rather
/// than echoed from the request, so the answer is what Kafka stored, not what
/// Kavka asked for.
#[cfg(feature = "kafka")]
pub fn offsets_reset(conn: &ClusterConnection, spec: &OffsetResetSpec) -> Result<Vec<GroupOffset>> {
    conn.ensure_writable("reset consumer group offsets")?;
    let (state, members) = describe_group(conn, &spec.group_id)?;
    ensure_group_resettable(&spec.group_id, &state, members.len(), spec.force)?;

    let partitions = reset_partitions(conn, spec)?;
    let consumer = conn.new_group_consumer(&spec.group_id)?;
    // A static assignment, not a subscription: no JoinGroup and no heartbeat,
    // so this never makes Kavka a member of the group. It exists because
    // librdkafka will only commit for a group handle that has a coordinator
    // and a partition set.
    consumer
        .assign(&partition_list(&partitions, Offset::Invalid)?)
        .map_err(|e| conn.describe_failure(format!("preparing the offset reset failed: {e}")))?;

    let committed = committed_map(conn, &consumer, &partitions)?;
    let at_timestamp = match spec.target {
        ResetTarget::TimestampMs { timestamp_ms } => {
            timestamp_offsets(conn, &consumer, timestamp_ms, &partitions)?
        }
        _ => BTreeMap::new(),
    };

    let mut target = TopicPartitionList::with_capacity(partitions.len());
    for key in &partitions {
        let (earliest, end) = watermarks(conn, &key.0, key.1)?;
        let offset = resolve_reset_offset(
            &spec.target,
            PartitionBounds {
                earliest,
                end,
                committed: committed.get(key).copied().flatten(),
                at_timestamp: at_timestamp.get(key).copied().flatten(),
            },
        );
        target
            .add_partition_offset(&key.0, key.1, Offset::Offset(offset))
            .map_err(|e| Error::Other(format!("preparing the offset reset failed: {e}")))?;
    }

    consumer
        .commit(&target, CommitMode::Sync)
        .map_err(|e| conn.describe_failure(format!("resetting the group's offsets failed: {e}")))?;

    let committed = committed_map(conn, &consumer, &partitions)?;
    read_offsets(conn, &committed, &partitions)
}

/// The partitions one [`OffsetResetSpec`] covers, validated against the topic
/// as the cluster currently has it — an unknown partition index is a typo worth
/// naming, not something to discover as a broker error halfway through.
#[cfg(feature = "kafka")]
fn reset_partitions(
    conn: &ClusterConnection,
    spec: &OffsetResetSpec,
) -> Result<Vec<(String, i32)>> {
    let metadata = conn.with_consumer("reading topic metadata", |consumer| {
        consumer.fetch_metadata(Some(&spec.topic), METADATA_TIMEOUT)
    })?;
    let described = metadata
        .topics()
        .iter()
        .find(|t| t.name() == spec.topic)
        .ok_or_else(|| unknown_topic(&spec.topic))?;
    if let Some(err) = described.error() {
        return Err(topic_metadata_error(&spec.topic, err));
    }
    let existing: BTreeSet<i32> = described.partitions().iter().map(|p| p.id()).collect();

    let wanted: Vec<i32> = match &spec.partitions {
        Some(requested) => {
            let mut requested = requested.clone();
            requested.sort_unstable();
            requested.dedup();
            if let Some(missing) = requested.iter().find(|p| !existing.contains(p)) {
                return Err(Error::Other(format!(
                    "{} has no partition {missing} — it has {}, numbered 0 to {}",
                    spec.topic,
                    existing.len(),
                    existing.len().saturating_sub(1)
                )));
            }
            requested
        }
        None => existing.into_iter().collect(),
    };
    if wanted.is_empty() {
        return Err(Error::Other(format!(
            "no partitions to reset on {}",
            spec.topic
        )));
    }
    Ok(wanted
        .into_iter()
        .map(|partition| (spec.topic.clone(), partition))
        .collect())
}

/// The first offset at or after `timestamp_ms` on each partition. A partition
/// with nothing that new maps to `None`, which
/// [`resolve_reset_offset`] reads as "the end".
#[cfg(feature = "kafka")]
fn timestamp_offsets(
    conn: &ClusterConnection,
    consumer: &BaseConsumer<KavkaClientContext>,
    timestamp_ms: i64,
    partitions: &[(String, i32)],
) -> Result<BTreeMap<(String, i32), Option<i64>>> {
    let query = partition_list(partitions, Offset::Offset(timestamp_ms))?;
    let resolved = consumer
        .offsets_for_times(query, METADATA_TIMEOUT)
        .map_err(|e| {
            conn.describe_failure(format!("looking up offsets for that time failed: {e}"))
        })?;
    Ok(resolved
        .to_topic_map()
        .into_iter()
        .map(|(key, offset)| {
            let offset = match offset {
                Offset::Offset(at) if at >= 0 => Some(at),
                // `Offset::End` is librdkafka's "nothing at or after that time".
                _ => None,
            };
            (key, offset)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

#[cfg(feature = "kafka")]
fn partition_list(partitions: &[(String, i32)], offset: Offset) -> Result<TopicPartitionList> {
    let mut list = TopicPartitionList::with_capacity(partitions.len());
    for (topic, partition) in partitions {
        list.add_partition_offset(topic, *partition, offset)
            .map_err(|e| Error::Other(format!("building the partition list failed: {e}")))?;
    }
    Ok(list)
}

#[cfg(feature = "kafka")]
fn watermarks(conn: &ClusterConnection, topic: &str, partition: i32) -> Result<(i64, i64)> {
    conn.with_consumer("reading partition watermarks", |consumer| {
        consumer.fetch_watermarks(topic, partition, METADATA_TIMEOUT)
    })
}

#[cfg(feature = "kafka")]
fn admin_options() -> AdminOptions {
    AdminOptions::new()
        .request_timeout(Some(Timeout::After(ADMIN_TIMEOUT)))
        .operation_timeout(Some(Timeout::After(ADMIN_TIMEOUT)))
}

/// Wraps an AdminClient failure, preferring a recorded authentication cause
/// over the generic one (see `ClusterConnection::describe_failure`).
///
/// `pub(crate)` for [`crate::acl`]: its calls fail the same ways and carry the
/// same OAUTHBEARER caveat, and a second copy of this wording would drift.
#[cfg(feature = "kafka")]
pub(crate) fn admin_error(conn: &ClusterConnection, what: &str, cause: &str) -> Error {
    let mut message = format!("{what} failed: {cause}");
    if conn.needs_oauth_token() {
        // See the caveat on `ClusterConnection::admin`: nothing outside rdkafka
        // can drain an AdminClient's main queue, so its OAUTHBEARER token is
        // never refreshed and the call stalls. Naming it beats letting the user
        // hunt for a network fault.
        message.push_str(
            " — Kavka can't refresh OAUTHBEARER tokens on the admin client yet, so admin \
             operations on an OAuth or MSK IAM connection can time out even while browsing works",
        );
    }
    conn.describe_failure(message)
}

#[cfg(feature = "kafka")]
fn unknown_topic(topic: &str) -> Error {
    Error::Other(format!(
        "this cluster has no topic named \"{topic}\" — it may have been deleted, or the name may \
         be misspelled"
    ))
}

#[cfg(feature = "kafka")]
fn topic_metadata_error(topic: &str, err: rdkafka::types::RDKafkaRespErr) -> Error {
    match RDKafkaErrorCode::from(err) {
        RDKafkaErrorCode::UnknownTopic | RDKafkaErrorCode::UnknownTopicOrPartition => {
            unknown_topic(topic)
        }
        other => Error::Other(format!(
            "the broker returned an error for topic \"{topic}\": {other}"
        )),
    }
}

// ---------------------------------------------------------------------------
// Builds without the `kafka` feature. Every entry point still exists so the
// crate's API is the same shape on a toolchain with no CMake; each one refuses
// rather than silently doing nothing.
// ---------------------------------------------------------------------------

#[cfg(not(feature = "kafka"))]
fn unsupported<T>() -> Result<T> {
    Err(Error::Other(
        "kavka-core was built without the `kafka` feature".into(),
    ))
}

#[cfg(not(feature = "kafka"))]
pub fn topic_detail(_conn: &ClusterConnection, _topic: &str) -> Result<TopicDetail> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn create_topic(
    _conn: &ClusterConnection,
    _name: &str,
    _partitions: u32,
    _replication_factor: u16,
    _configs: &[TopicConfig],
) -> Result<()> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn delete_topic(_conn: &ClusterConnection, _topic: &str) -> Result<()> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn broker_configs(_conn: &ClusterConnection, _broker_id: i32) -> Result<Vec<ConfigEntry>> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn broker_config_set(
    _conn: &ClusterConnection,
    _broker_id: i32,
    _name: &str,
    _value: Option<&str>,
) -> Result<()> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn groups_list(_conn: &ClusterConnection) -> Result<Vec<GroupInfo>> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn group_detail(_conn: &ClusterConnection, _group_id: &str) -> Result<GroupDetail> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn offsets_reset(
    _conn: &ClusterConnection,
    _spec: &OffsetResetSpec,
) -> Result<Vec<GroupOffset>> {
    unsupported()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(earliest: i64, end: i64, committed: Option<i64>) -> PartitionBounds {
        PartitionBounds {
            earliest,
            end,
            committed,
            at_timestamp: None,
        }
    }

    #[test]
    fn earliest_and_latest_resolve_to_the_watermarks() {
        let b = bounds(120, 480, Some(300));
        assert_eq!(resolve_reset_offset(&ResetTarget::Earliest, b), 120);
        assert_eq!(resolve_reset_offset(&ResetTarget::Latest, b), 480);
    }

    #[test]
    fn an_explicit_offset_inside_the_range_is_used_as_given() {
        let b = bounds(120, 480, Some(300));
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: 300 }, b),
            300
        );
        // The endpoints are inside the range, not outside it.
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: 120 }, b),
            120
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: 480 }, b),
            480
        );
    }

    #[test]
    fn an_explicit_offset_outside_the_range_is_clamped_to_it() {
        let b = bounds(120, 480, Some(300));
        // Below the low watermark: those records are gone, so earliest is the
        // earliest anyone can actually start.
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: 0 }, b),
            120
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: -9 }, b),
            120
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: 10_000 }, b),
            480
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: i64::MAX }, b),
            480
        );
    }

    #[test]
    fn shift_by_counts_from_the_committed_offset() {
        let b = bounds(120, 480, Some(300));
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: 10 }, b),
            310
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: -10 }, b),
            290
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: 0 }, b),
            300
        );
    }

    #[test]
    fn shift_by_clamps_at_both_ends_and_never_overflows() {
        let b = bounds(120, 480, Some(300));
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: 1_000 }, b),
            480
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: -1_000 }, b),
            120
        );
        // Saturating, not wrapping: a shift of i64::MAX must not land below the
        // low watermark, which is exactly what a wrapping add would produce.
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: i64::MAX }, b),
            480
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: i64::MIN }, b),
            120
        );
    }

    #[test]
    fn shift_by_falls_back_to_earliest_when_the_group_never_committed() {
        let b = bounds(120, 480, None);
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: 10 }, b),
            130
        );
        assert_eq!(
            resolve_reset_offset(&ResetTarget::ShiftBy { shift_by: -10 }, b),
            120
        );
    }

    #[test]
    fn a_timestamp_uses_the_brokers_answer_and_falls_back_to_the_end() {
        let mut b = bounds(120, 480, Some(300));
        b.at_timestamp = Some(365);
        assert_eq!(
            resolve_reset_offset(&ResetTarget::TimestampMs { timestamp_ms: 42 }, b),
            365
        );

        // Nothing at or after that time: Kafka's own --to-datetime lands on the
        // end of the partition, and so does this.
        b.at_timestamp = None;
        assert_eq!(
            resolve_reset_offset(&ResetTarget::TimestampMs { timestamp_ms: 42 }, b),
            480
        );

        // A broker answer outside the watermarks is still clamped.
        b.at_timestamp = Some(5);
        assert_eq!(
            resolve_reset_offset(&ResetTarget::TimestampMs { timestamp_ms: 42 }, b),
            120
        );
    }

    #[test]
    fn an_empty_partition_resolves_every_target_to_the_same_offset() {
        // earliest == end is a partition that has never held a message, or one
        // whose every record has been deleted. Nothing may resolve past it.
        let b = bounds(7, 7, None);
        for target in [
            ResetTarget::Earliest,
            ResetTarget::Latest,
            ResetTarget::Offset { offset: 0 },
            ResetTarget::Offset { offset: 900 },
            ResetTarget::ShiftBy { shift_by: 5 },
            ResetTarget::ShiftBy { shift_by: -5 },
            ResetTarget::TimestampMs { timestamp_ms: 1 },
        ] {
            assert_eq!(resolve_reset_offset(&target, b), 7, "for {target:?}");
        }
    }

    #[test]
    fn inverted_watermarks_resolve_instead_of_panicking() {
        // `i64::clamp` panics on an inverted range, and a broker mid-truncation
        // can briefly report one.
        let b = bounds(500, 100, Some(400));
        assert_eq!(
            resolve_reset_offset(&ResetTarget::Offset { offset: 300 }, b),
            500
        );
        assert_eq!(resolve_reset_offset(&ResetTarget::Latest, b), 500);
    }

    #[test]
    fn the_active_group_guard_names_the_state_it_saw() {
        let err = ensure_group_resettable("checkout-service", "Stable", 3, false)
            .expect_err("a Stable group must not be reset");
        let message = err.to_string();
        assert!(message.contains("checkout-service"), "got {message}");
        assert!(message.contains("Stable"), "got {message}");
        assert!(message.contains('3'), "got {message}");
        // The doctrine: say what to do next, not just what failed (§7).
        assert!(message.contains("Stop the application"), "got {message}");
    }

    #[test]
    fn the_active_group_guard_passes_an_empty_group_and_yields_to_force() {
        assert!(ensure_group_resettable("g", "Empty", 0, false).is_ok());
        for state in [
            "Stable",
            "PreparingRebalance",
            "CompletingRebalance",
            "Dead",
        ] {
            assert!(
                ensure_group_resettable("g", state, 1, false).is_err(),
                "{state} was allowed through"
            );
            assert!(
                ensure_group_resettable("g", state, 1, true).is_ok(),
                "force did not override {state}"
            );
        }
    }

    /// A `ConsumerProtocolAssignment` as a broker sends it, so the parser is
    /// tested against the real layout rather than its own output.
    fn encode_assignment(topics: &[(&str, &[i32])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0i16.to_be_bytes());
        out.extend_from_slice(&i32::try_from(topics.len()).unwrap().to_be_bytes());
        for (topic, partitions) in topics {
            out.extend_from_slice(&i16::try_from(topic.len()).unwrap().to_be_bytes());
            out.extend_from_slice(topic.as_bytes());
            out.extend_from_slice(&i32::try_from(partitions.len()).unwrap().to_be_bytes());
            for partition in *partitions {
                out.extend_from_slice(&partition.to_be_bytes());
            }
        }
        // UserData: empty, not null.
        out.extend_from_slice(&0i32.to_be_bytes());
        out
    }

    #[test]
    fn member_assignments_are_parsed_from_the_consumer_protocol() {
        let bytes = encode_assignment(&[("orders", &[0, 3, 5]), ("payments", &[1])]);
        assert_eq!(
            parse_member_assignment(&bytes),
            vec![
                TopicPartition {
                    topic: "orders".into(),
                    partition: 0
                },
                TopicPartition {
                    topic: "orders".into(),
                    partition: 3
                },
                TopicPartition {
                    topic: "orders".into(),
                    partition: 5
                },
                TopicPartition {
                    topic: "payments".into(),
                    partition: 1
                },
            ]
        );
    }

    #[test]
    fn an_unreadable_assignment_yields_no_partitions_rather_than_failing() {
        let good = encode_assignment(&[("orders", &[0, 1])]);
        for (label, bytes) in [
            ("empty", Vec::new()),
            // Cuts into the partition array: the count says two, the bytes
            // hold one and a half.
            ("truncated mid-partition", good[..good.len() - 6].to_vec()),
            ("truncated mid-topic-name", good[..10].to_vec()),
            ("header only", good[..2].to_vec()),
            // A length field that claims far more than the buffer holds must
            // not become a two-billion-element allocation.
            (
                "lying topic count",
                [&0i16.to_be_bytes()[..], &i32::MAX.to_be_bytes()[..]].concat(),
            ),
            (
                "negative topic count",
                [&0i16.to_be_bytes()[..], &(-1i32).to_be_bytes()[..]].concat(),
            ),
        ] {
            assert_eq!(
                parse_member_assignment(&bytes),
                Vec::new(),
                "{label} should parse to nothing"
            );
        }
    }

    #[test]
    fn a_trailing_user_data_field_is_not_required() {
        // UserData is the last field and nothing here reads it, so an
        // assignment cut short after the partitions still yields them. Being
        // strict about bytes we ignore would throw away a member's real
        // assignment over a field we never look at.
        let good = encode_assignment(&[("orders", &[0, 1])]);
        assert_eq!(
            parse_member_assignment(&good[..good.len() - 4]),
            parse_member_assignment(&good)
        );
    }

    #[test]
    fn a_member_with_no_assigned_partitions_parses_to_an_empty_list() {
        // A member that joined a group with more members than partitions.
        assert_eq!(parse_member_assignment(&encode_assignment(&[])), Vec::new());
        assert_eq!(
            parse_member_assignment(&encode_assignment(&[("orders", &[])])),
            Vec::new()
        );
    }

    #[test]
    fn the_reset_target_wire_shape_is_the_ipc_contract() {
        for (target, json) in [
            (
                ResetTarget::Earliest,
                serde_json::json!({"kind": "earliest"}),
            ),
            (ResetTarget::Latest, serde_json::json!({"kind": "latest"})),
            (
                ResetTarget::Offset { offset: 42 },
                serde_json::json!({"kind": "offset", "offset": 42}),
            ),
            (
                ResetTarget::TimestampMs {
                    timestamp_ms: 1_700_000_000_000,
                },
                serde_json::json!({"kind": "timestamp_ms", "timestamp_ms": 1_700_000_000_000i64}),
            ),
            (
                ResetTarget::ShiftBy { shift_by: -10 },
                serde_json::json!({"kind": "shift_by", "shift_by": -10}),
            ),
        ] {
            assert_eq!(serde_json::to_value(&target).unwrap(), json);
            assert_eq!(
                serde_json::from_value::<ResetTarget>(json.clone()).unwrap(),
                target
            );
        }
    }
}

/// Integration tests against the dev cluster in `dev/docker-compose.yml`.
///
/// They live in the crate rather than `tests/` because they need a throwaway
/// rdkafka consumer to create a real group, and rdkafka is a dependency of this
/// crate rather than a dev-dependency — an external test target can't see it.
///
/// Skipped unless `KAVKA_IT=1`, so `cargo test` on a machine with no broker
/// stays green:
///
/// ```text
/// docker compose -f dev/docker-compose.yml up -d --wait
/// KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl
/// ```
#[cfg(all(test, feature = "kafka"))]
mod it {
    use super::*;
    use crate::profiles::ConnectionProfile;
    use rdkafka::config::ClientConfig;
    use rdkafka::message::Message;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    const BOOTSTRAP: &str = "localhost:9092";
    const SEEDED_TOPIC: &str = "orders";
    const SEEDED_PARTITIONS: usize = 6;

    /// `false` (and a note on stderr) unless the dev cluster is meant to be up.
    fn enabled() -> bool {
        if matches!(std::env::var("KAVKA_IT").as_deref(), Ok("1")) {
            return true;
        }
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml up to run this");
        false
    }

    /// Unique per process and per call, so a rerun (or an interrupted one that
    /// left topics behind) can't collide with itself.
    fn unique(prefix: &str) -> String {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!(
            "{prefix}-{}-{nanos:x}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Built through serde rather than a struct literal: `ConnectionProfile`
    /// gains optional fields over time (schema registry, and whatever Phase 2
    /// adds), and a literal here would have to be edited every time one lands.
    fn profile(read_only: bool) -> ConnectionProfile {
        serde_json::from_value(serde_json::json!({
            "id": "kavka-it",
            "name": "dev cluster",
            "environment": "dev",
            "bootstrap_servers": [BOOTSTRAP],
            "auth": {"kind": "plaintext"},
            "read_only": read_only,
        }))
        .expect("the integration profile is a valid ConnectionProfile")
    }

    fn connect(read_only: bool) -> ClusterConnection {
        ClusterConnection::connect(profile(read_only)).expect(
            "the dev cluster must be up: docker compose -f dev/docker-compose.yml up -d --wait",
        )
    }

    /// Retries `check` until it answers `Some`, or gives up after `within`.
    /// Metadata propagation and group-state transitions are both eventually
    /// consistent, and a fixed sleep is either flaky or slow.
    fn eventually<T>(within: Duration, what: &str, mut check: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + within;
        loop {
            if let Some(value) = check() {
                return value;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// A real consumer in `group`, reading `SEEDED_TOPIC` from the beginning.
    /// It joins the group for real — that is the point: it is what makes the
    /// group non-Empty for the guard test.
    fn joined_consumer(group: &str) -> BaseConsumer {
        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", BOOTSTRAP)
            .set("group.id", group)
            .set("client.id", "kavka-it-probe")
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .set("session.timeout.ms", "6000")
            .create()
            .expect("throwaway consumer");
        consumer.subscribe(&[SEEDED_TOPIC]).expect("subscribe");
        consumer
    }

    /// Consumes until `count` messages have arrived, then commits one past the
    /// last offset seen on each partition — i.e. a group that stopped in the
    /// middle of the topic.
    fn consume_and_commit(consumer: &BaseConsumer, count: usize) -> Vec<(i32, i64)> {
        let mut last: BTreeMap<i32, i64> = BTreeMap::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = 0usize;
        while seen < count {
            assert!(
                Instant::now() < deadline,
                "only consumed {seen} of {count} messages from {SEEDED_TOPIC}"
            );
            match consumer.poll(Duration::from_millis(500)) {
                Some(Ok(message)) => {
                    last.insert(message.partition(), message.offset());
                    seen += 1;
                }
                Some(Err(e)) => panic!("consuming {SEEDED_TOPIC} failed: {e}"),
                None => {}
            }
        }

        let committed: Vec<(i32, i64)> = last
            .iter()
            .map(|(partition, offset)| (*partition, offset + 1))
            .collect();
        let mut tpl = TopicPartitionList::with_capacity(committed.len());
        for (partition, offset) in &committed {
            tpl.add_partition_offset(SEEDED_TOPIC, *partition, Offset::Offset(*offset))
                .expect("build commit list");
        }
        consumer.commit(&tpl, CommitMode::Sync).expect("commit");
        committed
    }

    fn state_of(conn: &ClusterConnection, group: &str) -> String {
        group_detail(conn, group).expect("group_detail").state
    }

    #[test]
    fn topic_detail_reports_partitions_watermarks_and_configs() {
        if !enabled() {
            return;
        }
        let conn = connect(true);
        let detail = topic_detail(&conn, SEEDED_TOPIC).expect("topic_detail");

        assert_eq!(detail.name, SEEDED_TOPIC);
        assert!(!detail.internal);
        assert_eq!(detail.partitions.len(), SEEDED_PARTITIONS);
        for (index, partition) in detail.partitions.iter().enumerate() {
            assert_eq!(partition.partition, index as i32, "partitions come sorted");
            assert_eq!(partition.replicas.len(), 1, "the dev cluster is RF 1");
            assert_eq!(partition.isr, partition.replicas, "one broker, all in sync");
            assert!(partition.replicas.contains(&partition.leader));
            assert!(partition.earliest_offset >= 0);
            assert!(partition.latest_offset >= partition.earliest_offset);
        }
        // The seed produces 50 records across the six partitions.
        let total: i64 = detail
            .partitions
            .iter()
            .map(|p| p.latest_offset - p.earliest_offset)
            .sum();
        assert!(total >= 50, "orders should hold the 50 seeded messages");

        let cleanup = detail
            .configs
            .iter()
            .find(|c| c.name == "cleanup.policy")
            .expect("every topic has a cleanup.policy");
        assert_eq!(cleanup.value.as_deref(), Some("delete"));
        assert!(
            cleanup.is_default,
            "orders never overrides cleanup.policy, so it is inherited: {cleanup:?}"
        );
        assert!(!cleanup.is_sensitive);
        assert!(!cleanup.is_read_only);
        assert_ne!(cleanup.source, "DYNAMIC_TOPIC_CONFIG");

        // Sorted, and a sensitive entry never carries a value (D5).
        let names: Vec<&str> = detail.configs.iter().map(|c| c.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "configs come sorted by name");
        for config in &detail.configs {
            assert!(
                !config.is_sensitive || config.value.is_none(),
                "sensitive config carried a value: {config:?}"
            );
        }
    }

    #[test]
    fn topic_detail_names_a_topic_that_is_not_there() {
        if !enabled() {
            return;
        }
        let conn = connect(true);
        let missing = unique("kavka-it-absent");
        let err = topic_detail(&conn, &missing)
            .expect_err("a topic that does not exist has no detail")
            .to_string();
        assert!(err.contains(&missing), "got {err}");
        assert!(err.contains("no topic named"), "got {err}");
    }

    #[test]
    fn a_topic_can_be_created_with_configs_and_deleted_again() {
        if !enabled() {
            return;
        }
        let conn = connect(false);
        let name = unique("kavka-it-topic");

        create_topic(
            &conn,
            &name,
            3,
            1,
            &[TopicConfig {
                name: "cleanup.policy".into(),
                value: "compact".into(),
            }],
        )
        .expect("create_topic");

        let detail = eventually(Duration::from_secs(20), "the new topic to appear", || {
            topic_detail(&conn, &name).ok()
        });
        assert_eq!(detail.partitions.len(), 3);
        assert_eq!(detail.partitions[0].replicas.len(), 1);
        let cleanup = detail
            .configs
            .iter()
            .find(|c| c.name == "cleanup.policy")
            .expect("cleanup.policy");
        assert_eq!(cleanup.value.as_deref(), Some("compact"));
        assert_eq!(
            cleanup.source, "DYNAMIC_TOPIC_CONFIG",
            "a value set at creation is the topic's own"
        );
        assert!(
            !cleanup.is_default,
            "an override is not a default: {cleanup:?}"
        );
        assert!(
            conn.list_topics()
                .expect("list_topics")
                .iter()
                .any(|t| t.name == name),
            "the new topic should be listed"
        );

        delete_topic(&conn, &name).expect("delete_topic");
        eventually(Duration::from_secs(20), "the topic to disappear", || {
            topic_detail(&conn, &name).err().map(|_| ())
        });
        assert!(
            !conn
                .list_topics()
                .expect("list_topics")
                .iter()
                .any(|t| t.name == name),
            "the deleted topic should be gone from the listing"
        );
    }

    #[test]
    fn read_only_connections_refuse_every_mutating_admin_call() {
        if !enabled() {
            return;
        }
        let conn = connect(true);
        let name = unique("kavka-it-readonly");

        for err in [
            create_topic(&conn, &name, 1, 1, &[]).expect_err("create must be refused"),
            delete_topic(&conn, SEEDED_TOPIC).expect_err("delete must be refused"),
            offsets_reset(
                &conn,
                &OffsetResetSpec {
                    group_id: unique("kavka-it-group"),
                    topic: SEEDED_TOPIC.into(),
                    partitions: None,
                    target: ResetTarget::Earliest,
                    force: true,
                },
            )
            .expect_err("reset must be refused"),
        ] {
            assert!(
                matches!(err, Error::ReadOnly(_)),
                "expected a read-only refusal, got {err}"
            );
        }
        // The topic the refused delete named is still there.
        assert!(topic_detail(&conn, SEEDED_TOPIC).is_ok());
    }

    #[test]
    fn groups_are_listed_described_and_reset() {
        if !enabled() {
            return;
        }
        let conn = connect(false);
        let group = unique("kavka-it-group");
        let consumer = joined_consumer(&group);
        let committed = consume_and_commit(&consumer, 20);
        assert!(
            !committed.is_empty(),
            "the probe consumer committed nothing"
        );

        // --- listed -----------------------------------------------------
        let listed = eventually(Duration::from_secs(20), "the group to be listed", || {
            groups_list(&conn)
                .expect("groups_list")
                .into_iter()
                .find(|g| g.group_id == group)
        });
        assert_eq!(listed.protocol_type, "consumer");
        assert!(listed.member_count >= 1, "got {listed:?}");

        // --- described, with lag ---------------------------------------
        let detail = eventually(Duration::from_secs(20), "the group to go Stable", || {
            let detail = group_detail(&conn, &group).expect("group_detail");
            (detail.state == "Stable").then_some(detail)
        });
        assert_eq!(detail.members.len(), 1);
        let member = &detail.members[0];
        assert_eq!(member.client_id, "kavka-it-probe");
        assert!(!member.member_id.is_empty());
        assert!(!member.client_host.is_empty());
        assert_eq!(
            member
                .assignments
                .iter()
                .filter(|tp| tp.topic == SEEDED_TOPIC)
                .count(),
            SEEDED_PARTITIONS,
            "the only member owns every partition: {:?}",
            member.assignments
        );

        // Every committed partition reports the offset we committed, and lag is
        // exactly what is left after it.
        for (partition, offset) in &committed {
            let row = detail
                .offsets
                .iter()
                .find(|o| o.topic == SEEDED_TOPIC && o.partition == *partition)
                .unwrap_or_else(|| panic!("no offset row for partition {partition}"));
            assert_eq!(row.committed, Some(*offset), "row {row:?}");
            assert_eq!(
                row.lag,
                Some(row.end_offset - offset),
                "lag must be end - committed: {row:?}"
            );
            assert!(row.lag.unwrap() >= 0);
        }
        // A partition the group never committed on reports no position and no
        // lag rather than an invented one.
        for row in detail.offsets.iter().filter(|o| o.committed.is_none()) {
            assert_eq!(row.lag, None, "lag without a position: {row:?}");
        }

        // --- the active-group guard ------------------------------------
        let state = state_of(&conn, &group);
        let spec = |force| OffsetResetSpec {
            group_id: group.clone(),
            topic: SEEDED_TOPIC.into(),
            partitions: None,
            target: ResetTarget::Earliest,
            force,
        };
        let guarded = offsets_reset(&conn, &spec(false))
            .expect_err("a live group must not be reset")
            .to_string();
        assert!(guarded.contains(&group), "got {guarded}");
        assert!(guarded.contains(&state), "state {state} unnamed: {guarded}");
        assert!(guarded.contains("Stop the application"), "got {guarded}");

        // force gets past *our* guard; Kafka then rejects the commit itself,
        // which is the whole point of the guard existing.
        let forced = offsets_reset(&conn, &spec(true));
        if let Err(err) = &forced {
            let message = err.to_string();
            assert!(
                !message.contains("Stop the application"),
                "force did not bypass the guard: {message}"
            );
        }

        // --- reset, once the group is really Empty ---------------------
        drop(consumer);
        eventually(Duration::from_secs(30), "the group to go Empty", || {
            (state_of(&conn, &group) == "Empty").then_some(())
        });

        let earliest = offsets_reset(&conn, &spec(false)).expect("reset to earliest");
        assert_eq!(earliest.len(), SEEDED_PARTITIONS);
        let detail = topic_detail(&conn, SEEDED_TOPIC).expect("topic_detail");
        for row in &earliest {
            let partition = detail
                .partitions
                .iter()
                .find(|p| p.partition == row.partition)
                .expect("partition");
            assert_eq!(
                row.committed,
                Some(partition.earliest_offset),
                "reset to earliest must commit the low watermark: {row:?}"
            );
            assert_eq!(row.end_offset, partition.latest_offset);
            assert_eq!(
                row.lag,
                Some(partition.latest_offset - partition.earliest_offset)
            );
        }

        // --- shift_by, clamped -----------------------------------------
        let shifted = offsets_reset(
            &conn,
            &OffsetResetSpec {
                group_id: group.clone(),
                topic: SEEDED_TOPIC.into(),
                partitions: None,
                target: ResetTarget::ShiftBy { shift_by: 10 },
                force: false,
            },
        )
        .expect("shift_by reset");
        for row in &shifted {
            let partition = detail
                .partitions
                .iter()
                .find(|p| p.partition == row.partition)
                .expect("partition");
            // The seed puts ~8 records in each partition, so +10 from the low
            // watermark runs off the end and has to stop at it.
            let expected = (partition.earliest_offset + 10).min(partition.latest_offset);
            assert_eq!(
                row.committed,
                Some(expected),
                "shift_by must clamp to the high watermark: {row:?} against {partition:?}"
            );
        }

        // --- a single partition, by explicit offset --------------------
        let one = offsets_reset(
            &conn,
            &OffsetResetSpec {
                group_id: group.clone(),
                topic: SEEDED_TOPIC.into(),
                partitions: Some(vec![0]),
                target: ResetTarget::Latest,
                force: false,
            },
        )
        .expect("single-partition reset");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].partition, 0);
        assert_eq!(one[0].committed, Some(one[0].end_offset));
        assert_eq!(one[0].lag, Some(0));

        // An index the topic doesn't have is named rather than sent.
        let err = offsets_reset(
            &conn,
            &OffsetResetSpec {
                group_id: group.clone(),
                topic: SEEDED_TOPIC.into(),
                partitions: Some(vec![99]),
                target: ResetTarget::Earliest,
                force: false,
            },
        )
        .expect_err("partition 99 does not exist")
        .to_string();
        assert!(err.contains("no partition 99"), "got {err}");
    }

    #[test]
    fn group_detail_names_a_group_that_is_not_there() {
        if !enabled() {
            return;
        }
        let conn = connect(true);
        let missing = unique("kavka-it-nogroup");
        let err = group_detail(&conn, &missing)
            .expect_err("an unknown group has no detail")
            .to_string();
        assert!(err.contains(&missing), "got {err}");
        assert!(err.contains("no consumer group named"), "got {err}");
    }
}
