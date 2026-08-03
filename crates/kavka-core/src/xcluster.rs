//! Cross-cluster work (Phase 5b): replaying a topic's records into another
//! cluster, diffing two topics' configurations, and carrying a consumer group's
//! position across.
//!
//! Everything here BLOCKS, like the rest of the core (docs/ARCHITECTURE.md);
//! the Tauri shell wraps it in its `blocking()` helper, and [`CopySession`]
//! follows [`crate::search::SearchSession`]'s shape exactly — everything that
//! can fail deterministically fails in `start` on the calling thread, `stop` is
//! idempotent and safe from any thread, and `Drop` joins the worker so no
//! session can leak a client that keeps writing to a cluster nobody is watching.
//!
//! # This module composes; it does not reimplement
//!
//! - The filter is [`crate::search::CompiledQuery`] — the same substring
//!   prefilter over raw bytes and the same CEL activation the search bar
//!   teaches. A copy filter that had its own vocabulary would be a second
//!   language to learn for the same question, and the prefilter is exactly the
//!   cheap half a copy wants: it rejects a record without decoding it.
//! - The producer's properties are [`crate::produce::tune`], so `acks=all` and
//!   "delivered means acknowledged" mean the same thing here as they do on the
//!   produce form.
//! - The offset arithmetic is [`crate::admin::resolve_reset_offset`] and the
//!   active-group refusal is [`crate::admin::ensure_group_resettable`], so a
//!   migration clamps and refuses with the same rules — and the same sentence —
//!   as an offset reset.
//!
//! # Read-only
//!
//! **The destination is checked before anything else happens** (D5: enforced in
//! core, not the UI), in [`CopySession::start`] and in
//! [`offsets_migrate_apply`]. A read-only *source* is fine and deliberately so:
//! reading a cluster is what read-only mode is for, and refusing to copy *out*
//! of a protected cluster would be a guardrail pointing the wrong way. Copying
//! *into* a production profile is a confirmation the UI owns; core re-checks
//! only the one thing it can know for certain, which is the read-only flag.

use crate::admin::ConfigEntry;
use crate::connection::ClusterConnection;
use crate::consume::SeekSpec;
use crate::produce::ProduceHeader;
use crate::search::{PartitionProgress, SearchQuery};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[cfg(feature = "kafka")]
use crate::admin::{self, GroupOffset, PartitionBounds, ResetTarget};
#[cfg(feature = "kafka")]
use crate::cancel::CancelToken;
#[cfg(feature = "kafka")]
use crate::connection::auth::KavkaClientContext;
#[cfg(feature = "kafka")]
use crate::connection::METADATA_TIMEOUT;
#[cfg(feature = "kafka")]
use crate::produce::{self, DeliverySink};
#[cfg(feature = "kafka")]
use crate::search::{CelFilter, CompiledQuery};
#[cfg(feature = "kafka")]
use crate::serdes::{self, MessageRecord, DEFAULT_MAX_VALUE_BYTES};
#[cfg(feature = "kafka")]
use crate::sr::SchemaRegistry;
#[cfg(feature = "kafka")]
use rdkafka::consumer::{BaseConsumer, CommitMode, Consumer};
#[cfg(feature = "kafka")]
use rdkafka::error::{KafkaError, RDKafkaErrorCode};
#[cfg(feature = "kafka")]
use rdkafka::message::{BorrowedMessage, Header, Headers, Message, OwnedHeaders};
#[cfg(feature = "kafka")]
use rdkafka::producer::{BaseProducer, BaseRecord, Producer};
#[cfg(feature = "kafka")]
use rdkafka::util::Timeout;
#[cfg(feature = "kafka")]
use rdkafka::{Offset, TopicPartitionList};
#[cfg(feature = "kafka")]
use std::collections::HashMap;
#[cfg(feature = "kafka")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "kafka")]
use std::sync::{Arc, Mutex, MutexGuard};

// ---------------------------------------------------------------------------
// Wire types. Field names are the IPC contract — the TypeScript in
// apps/desktop/src mirrors them exactly, so renaming one is a breaking change
// on both sides of the bridge. Deliberately outside the `kafka` gate: the
// shapes have to compile (and be asserted) on the bare tier.
// ---------------------------------------------------------------------------

/// One copy/replay run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopySpec {
    pub source_topic: String,
    /// Which connection the records are written to. **Routing information for
    /// the shell**, which resolves it to a live connection before calling
    /// anything here — core is handed the two connections and never looks a
    /// profile up. It rides in the spec because the spec is what the UI stores
    /// and replays.
    pub dest_profile_id: String,
    pub dest_topic: String,
    pub seek: SeekSpec,
    /// `None` = every partition of the source topic.
    pub partitions: Option<Vec<i32>>,
    /// `None` copies everything in the scan window. A filter that is present
    /// but empty is not a filter (see [`crate::search::CompiledQuery`]).
    pub filter: Option<SearchQuery>,
    /// A ceiling on records **offered** to the destination. `None` = the whole
    /// scan window. Records the destination then rejects are counted in
    /// [`CopyProgress::failed`], so `copied + failed` is what this bounds.
    pub max_messages: Option<u32>,
    /// Records per second, or `None`/`0` for as fast as delivery allows. See
    /// [`RateLimiter`] for what the number means precisely — it is a sustained
    /// rate with one second of burst, not a gap between records.
    pub rate_per_sec: Option<u32>,
    /// Write each record to the partition it came from. Requires the
    /// destination to have at least as many partitions; see
    /// [`ensure_partition_mapping`].
    ///
    /// With this off, Kafka's own partitioner decides — which still keeps a
    /// key's records together (murmur2 on the key, the same rule the Java
    /// client uses), just not necessarily on the same index.
    pub preserve_partition: bool,
    /// Append the `kavka.replay.source.*` headers — see
    /// [`provenance_headers`].
    pub provenance_headers: bool,
}

/// A copy's progress. Serializes to exactly the `kavka://copy/{id}` event
/// payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyProgress {
    /// Records the destination broker **acknowledged**, not records handed to
    /// librdkafka. Same rule as [`crate::produce::BulkProgress::sent`], for the
    /// same reason: a bar that fills before anything reached the cluster is a
    /// lie.
    pub copied: u64,
    /// Records read from the source and considered, whether they matched or
    /// not.
    pub scanned: u64,
    /// Records the destination rejected.
    pub failed: u64,
    pub done: bool,
    /// Why the copy stopped early — or, when it did not stop early, why
    /// `failed` is not zero, or why some records could not be judged by the
    /// filter. One field, in that order of importance, because a run that
    /// quietly moved fewer records than it read owes the user the reason.
    pub error: Option<String>,
    /// Partitions the copy finished because **nothing more arrived from the
    /// source**, rather than because their cursor reached the end watermark
    /// captured when the copy started. Ascending, and empty on a copy that read
    /// every window to its end.
    ///
    /// Exactly [`crate::search::SearchProgress::assumed_complete`]'s honesty,
    /// for exactly its reason. Transaction markers and aborted records occupy
    /// offsets a consumer never receives, so the watermark alone cannot always
    /// be reached and a quiet deadline is what ends the run — those partitions
    /// are complete as far as anything readable goes. But "as far as anything
    /// readable goes" is a claim the user is entitled to see, because the other
    /// reason a source partition goes quiet is a broker that stopped answering,
    /// and a copy that moved 900 of 1 000 records and said `done` with no
    /// further comment is the same lie as a silently truncated result list.
    ///
    /// **Source silence only.** Time the copy spends waiting on the
    /// *destination* — pacing, and the in-flight drain — never counts towards
    /// the deadline (see [`SourceSilence`]), so a slow destination can make a
    /// copy take longer but can never make it stop short.
    pub assumed_complete: Vec<i32>,
}

/// What a copy would move, worked out from watermarks alone — no records are
/// read and nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyEstimate {
    /// The size of the **scan window**: how many offsets the copy would walk,
    /// capped by [`CopySpec::max_messages`].
    ///
    /// **This is an upper bound, and `estimate_only` says when it is only
    /// that.** With a filter set, Kavka cannot know how many records match
    /// without reading them, so it reports what it will *look at* rather than
    /// inventing a match count. Even unfiltered it can read high on a topic
    /// written by a transactional producer: commit markers and aborted records
    /// occupy offsets a consumer never receives, and watermark arithmetic
    /// counts offsets.
    pub would_copy_estimate: u64,
    /// A filter is set, so the number above is the scan size and not a match
    /// count. Never silently: docs/DESIGN.md §7 rule 5.
    pub estimate_only: bool,
    /// One entry per partition the copy would cover, ascending —
    /// `current_offset` is where it would start, `end_offset` where it would
    /// stop.
    pub per_partition: Vec<PartitionProgress>,
}

/// One config entry, as the two sides have it.
///
/// `a`/`b` are `None` when the side does not have the entry at all **or** when
/// Kafka refused to send the value because it is sensitive — the two are
/// indistinguishable in this row, deliberately, because the row is a diff and
/// neither case has a value to show. `differs` tells them apart: an entry only
/// one side has differs; a sensitive entry both sides have does not (see
/// [`config_diff`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDiffRow {
    pub name: String,
    pub a: Option<String>,
    pub b: Option<String>,
    pub a_is_default: bool,
    pub b_is_default: bool,
    pub differs: bool,
}

/// Where one partition of a consumer group would land on the destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffsetMigrationRow {
    pub partition: i32,
    /// What the source group had committed, or `None` when it never committed
    /// on this partition.
    pub source_committed: Option<i64>,
    /// The timestamp of the first record at or after `source_committed` — the
    /// record the source group would have read next, and the only thing about
    /// an offset that means anything on another cluster.
    pub source_ts_ms: Option<i64>,
    /// What to commit for the destination group, or `None` when there is
    /// nothing to migrate.
    pub dest_offset: Option<i64>,
    /// How `dest_offset` was arrived at — see [`METHOD_TIMESTAMP`] and friends.
    pub method: String,
}

/// The destination offset came from a timestamp lookup: the source group's next
/// record was written at time T, and this is where T falls on the destination.
pub const METHOD_TIMESTAMP: &str = "timestamp";
/// The source group had read nothing yet (its offset is the source partition's
/// log start), so the destination group starts at the destination's earliest.
pub const METHOD_EARLIEST: &str = "earliest";
/// The source group had read everything (its offset is at or past the source
/// partition's end, or nothing readable remains there), so the destination
/// group starts at the destination's end and waits for new records.
///
/// **The fourth method string.** The IPC contract's parenthetical lists three;
/// this is the one the "committed beyond the end" case needs, and it is a
/// distinct fact from all three — reporting it as `timestamp` would claim a
/// lookup that never happened, and as `none` would claim there was nothing to
/// migrate when in fact the group was fully caught up.
pub const METHOD_LATEST: &str = "latest";
/// There is nothing to migrate for this partition: the source group never
/// committed on it, or the source partition is empty, so no offset on the
/// destination can be justified and none is committed.
pub const METHOD_NONE: &str = "none";

// ---------------------------------------------------------------------------
// Provenance. Pure, so the header names are asserted on the bare tier and the
// UI can compose the same shapes for a DLQ re-produce.
// ---------------------------------------------------------------------------

/// The connection this record was read from, by its name in Kavka's sidebar.
pub const PROVENANCE_CLUSTER: &str = "kavka.replay.source.cluster";
pub const PROVENANCE_TOPIC: &str = "kavka.replay.source.topic";
pub const PROVENANCE_PARTITION: &str = "kavka.replay.source.partition";
pub const PROVENANCE_OFFSET: &str = "kavka.replay.source.offset";
pub const PROVENANCE_TIMESTAMP: &str = "kavka.replay.source.ts";

/// The headers a copied record carries when [`CopySpec::provenance_headers`] is
/// on: where this record was, exactly, before Kavka moved it.
///
/// Every value is UTF-8 text, and the numbers are decimal — the same spelling
/// Kafka Connect uses for its dead-letter headers, and the one a `kcat` user can
/// read without a decoder ring. A record with no timestamp (pre-KIP-32) gets no
/// timestamp header rather than an invented one.
///
/// **The cluster is named by its connection name**, which is a local label a
/// user can rename; the topic/partition/offset triple is the part that is
/// precise. That is the right trade for a header a human reads to answer
/// "where did this come from" — the id would be precise and meaningless.
///
/// **Appended, never merged.** The record's own headers are copied verbatim
/// (that is what makes a copy a copy), so replaying an already-replayed record
/// leaves the earlier provenance in place beside the new: Kafka permits
/// duplicate header keys, and the chain is worth more than the tidiness.
pub fn provenance_headers(
    cluster: &str,
    topic: &str,
    partition: i32,
    offset: i64,
    timestamp_ms: Option<i64>,
) -> Vec<ProduceHeader> {
    let header = |key: &str, value: String| ProduceHeader {
        key: key.to_string(),
        value,
    };
    let mut headers = vec![
        header(PROVENANCE_CLUSTER, cluster.to_string()),
        header(PROVENANCE_TOPIC, topic.to_string()),
        header(PROVENANCE_PARTITION, partition.to_string()),
        header(PROVENANCE_OFFSET, offset.to_string()),
    ];
    if let Some(timestamp) = timestamp_ms {
        headers.push(header(PROVENANCE_TIMESTAMP, timestamp.to_string()));
    }
    headers
}

/// Refuses a partition-preserving copy the destination cannot hold.
///
/// Producing to partition 7 of a three-partition topic is an
/// UNKNOWN_TOPIC_OR_PARTITION per record, twenty seconds apart, for the whole
/// run — so this is checked once, before the first record, and the message
/// names both counts and both ways out (docs/DESIGN.md §7: what happened, then
/// the next click).
///
/// **`needed_partitions` is what the copy will actually write to**, not what
/// the source topic has: the highest partition index in scope, plus one. For an
/// unfiltered copy those are the same number, but a copy of partition 0 alone
/// out of a six-partition topic needs exactly one partition on the destination,
/// and refusing it because the *source* is wide would be a guardrail about a
/// partition no record is going to.
pub fn ensure_partition_mapping(
    source_topic: &str,
    needed_partitions: usize,
    dest_topic: &str,
    dest_partitions: usize,
) -> Result<()> {
    if dest_partitions >= needed_partitions {
        return Ok(());
    }
    let highest = needed_partitions.saturating_sub(1);
    Err(Error::Other(format!(
        "this copy writes as high as partition {highest} of {source_topic}, and {dest_topic} has \
         {dest_partitions} partitions — keeping each record's partition needs at least \
         {needed_partitions} on the destination. Recreate {dest_topic} with \
         {needed_partitions}, or turn off \"keep the same partition\" and let Kafka place them \
         by key."
    )))
}

/// The highest partition index a copy will write to, plus one — what
/// [`ensure_partition_mapping`] compares the destination against.
///
/// Zero for a copy with nothing in scope, which no destination can be too
/// narrow for.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
fn partitions_needed(selected: &[i32]) -> usize {
    selected.iter().copied().max().map_or(0, |highest| {
        usize::try_from(highest.saturating_add(1)).unwrap_or(0)
    })
}

// ---------------------------------------------------------------------------
// Cluster identity. Pure, so "is this the same cluster twice" is a unit test
// rather than a pair of fixtures — and enforced in CORE rather than the UI,
// because the UI compares the only thing it has, which is a profile id.
// ---------------------------------------------------------------------------

/// The operation a self-copy is refused by name for.
const SELF_COPY_OP: &str = "copy a topic into itself";

/// Do these two connections point at the same Kafka cluster?
///
/// **The broker's own cluster id wins whenever both sides have one.** It is the
/// only answer here that is not an inference: two profiles can spell the same
/// cluster with different bootstrap lists (one broker each, a VIP, a hostname
/// and its IP) and two clusters can never share an id.
///
/// The fallback is the bootstrap addresses, normalised to `host:port` and
/// compared as sets: **any** address in common means the same cluster, because
/// a broker address belongs to exactly one. It is deliberately not an equality
/// test — a profile listing one broker of a three-broker cluster and a profile
/// listing the other two are the same cluster, and an equality test would call
/// them different and wave the copy through.
///
/// What it cannot see: two spellings of one address that share no text
/// (`localhost` and `127.0.0.1`, a CNAME and its target). Those come back
/// "different clusters" and the copy proceeds — which is why this is the
/// fallback and not the rule.
pub fn same_cluster(
    a_cluster_id: Option<&str>,
    a_bootstrap: &[String],
    b_cluster_id: Option<&str>,
    b_bootstrap: &[String],
) -> bool {
    if let (Some(a), Some(b)) = (a_cluster_id, b_cluster_id) {
        return a == b;
    }
    let a = normalized_bootstrap(a_bootstrap);
    let b = normalized_bootstrap(b_bootstrap);
    !a.is_disjoint(&b)
}

/// `[scheme://]host[:port]` → `host:port`, lower-cased, with Kafka's default
/// port supplied. A set, because order and duplicates say nothing.
fn normalized_bootstrap(servers: &[String]) -> std::collections::BTreeSet<String> {
    servers
        .iter()
        .filter_map(|server| {
            let server = server.trim();
            let without_scheme = server.split_once("://").map_or(server, |(_, rest)| rest);
            let without_scheme = without_scheme.trim();
            if without_scheme.is_empty() {
                return None;
            }
            Some(match without_scheme.rsplit_once(':') {
                // Not a port (an unbracketed IPv6 literal): keep it whole.
                Some((host, port))
                    if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    format!("{}:{port}", host.to_ascii_lowercase())
                }
                _ => format!("{}:9092", without_scheme.to_ascii_lowercase()),
            })
        })
        .collect()
}

/// Refuses a copy whose destination is the topic it is reading.
///
/// A topic copied onto itself is not a slow copy, it is a copy that feeds
/// itself: every record written lands back in the log being scanned, the topic
/// grows by as much as it has already produced, and — with a live producer or a
/// second pass — it never catches up. The `[start, end)` windows captured at the
/// start bound one run; nothing bounds the shape of what the user just did to
/// their topic.
///
/// **Cluster identity, not profile identity.** Two saved connections to the
/// same brokers have two different ids, and comparing ids waves that case
/// straight through — which is why this check moved out of the wizard and into
/// core (D5: the guardrail is enforced where it cannot be bypassed).
pub fn ensure_not_self_copy(
    source_topic: &str,
    dest_topic: &str,
    source_cluster_id: Option<&str>,
    source_bootstrap: &[String],
    dest_cluster_id: Option<&str>,
    dest_bootstrap: &[String],
) -> Result<()> {
    if source_topic != dest_topic
        || !same_cluster(
            source_cluster_id,
            source_bootstrap,
            dest_cluster_id,
            dest_bootstrap,
        )
    {
        return Ok(());
    }
    Err(Error::Other(format!(
        "Kavka won't {SELF_COPY_OP}: {source_topic} is both the source and the destination on \
         this cluster, so every record the copy writes lands back in the log it is reading and \
         the topic grows for as long as the copy runs. Copy into a different topic, or pick a \
         connection to a different cluster."
    )))
}

// ---------------------------------------------------------------------------
// The rate limiter. Pure over a clock it is handed, so the arithmetic is unit
// tested rather than slept through.
// ---------------------------------------------------------------------------

/// A token bucket: `rate` records per second sustained, with up to one second's
/// worth of burst.
///
/// The burst is what makes a small copy at a slow rate feel instant instead of
/// pointlessly paced — 8 records at 10/s finish immediately, because the whole
/// point of the limit is to protect the destination cluster from a *sustained*
/// flood, and eight records are not one. The floor it guarantees for `n`
/// records is therefore `(n - rate) / rate` seconds, not `n / rate`.
#[derive(Debug)]
pub struct RateLimiter {
    rate: f64,
    capacity: f64,
    tokens: f64,
    /// When `tokens` was last correct. A wait moves this *forward* past `now`,
    /// so consecutive waits queue up instead of each starting from scratch.
    at: Instant,
}

impl RateLimiter {
    /// `None` for "no limit" — a rate of zero is the absence of a rate, not a
    /// copy that never moves.
    pub fn new(rate_per_sec: u32, now: Instant) -> Option<Self> {
        if rate_per_sec == 0 {
            return None;
        }
        let rate = f64::from(rate_per_sec);
        Some(Self {
            rate,
            capacity: rate,
            tokens: rate,
            at: now,
        })
    }

    /// Takes one token and answers how long the caller must wait, from `now`,
    /// before using it. `Duration::ZERO` means "go now".
    pub fn take(&mut self, now: Instant) -> Duration {
        // Refill, but only forwards: `at` can be ahead of `now` when a previous
        // call already spent time that has not elapsed yet.
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity);
            self.at = now;
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Duration::ZERO;
        }
        // The token this call takes is minted at `at + deficit`, and `at` moves
        // there — so two calls in the same instant wait for two *different*
        // tokens instead of both waiting for the same one, and the rate holds
        // over a long run instead of drifting a poll interval at a time.
        let deficit = Duration::from_secs_f64((1.0 - self.tokens) / self.rate);
        self.tokens = 0.0;
        self.at += deficit;
        self.at.saturating_duration_since(now)
    }
}

// ---------------------------------------------------------------------------
// Config diff. Pure: two lists in, rows out. The shell fetches the two lists
// with `admin::topic_detail` — which is also why this takes lists rather than
// connections, since the same function then serves a diff of a topic against a
// saved snapshot without pretending the snapshot is a cluster.
// ---------------------------------------------------------------------------

/// Diffs two configurations, one row per name either side has, sorted by name.
///
/// # What counts as a difference
///
/// **`differs` is about the effective value and nothing else.** Two topics with
/// `retention.ms = 604800000` do not differ, even if one of them sets it
/// explicitly and the other inherits it from a broker default — the messages
/// live for a week on both, which is the question a diff is asked. The
/// provenance is not lost: `a_is_default`/`b_is_default` carry it, so the UI can
/// mark the row "set here, inherited there" without calling it a difference.
///
/// Three consequences, each deliberate:
///
/// - **Both inherited and equal is the noise case** — the great majority of a
///   topic's ~50 entries — and it comes out `differs: false`, so a UI that
///   shows only differing rows shows a short list.
/// - **Both inherited but *different* is a real difference.** A topic that
///   keeps messages for a week on one cluster and a day on the other differs,
///   however the two clusters arrived at their defaults, and hiding it because
///   "nobody set it" would hide the surprise the diff exists to surface.
/// - **An entry only one side has differs.** Broker versions have different
///   config vocabularies, and a name that exists on one cluster and not the
///   other is worth a row.
///
/// # The one thing it cannot judge
///
/// Kafka never sends a sensitive value, so both sides arrive `None` and the row
/// reads as equal. It might not be. Nothing can know: comparing two values the
/// broker refused to send is not something a client gets to do, and inventing a
/// "possibly differs" state for every sensitive entry would make the diff cry
/// wolf on every row.
pub fn config_diff(a: &[ConfigEntry], b: &[ConfigEntry]) -> Vec<ConfigDiffRow> {
    let index = |entries: &[ConfigEntry]| -> BTreeMap<String, ConfigEntry> {
        entries
            .iter()
            .map(|entry| (entry.name.clone(), entry.clone()))
            .collect()
    };
    let left = index(a);
    let right = index(b);

    let mut names: Vec<&String> = left.keys().chain(right.keys()).collect();
    names.sort_unstable();
    names.dedup();

    names
        .into_iter()
        .map(|name| {
            let a = left.get(name);
            let b = right.get(name);
            ConfigDiffRow {
                name: name.clone(),
                a: a.and_then(|entry| entry.value.clone()),
                b: b.and_then(|entry| entry.value.clone()),
                a_is_default: a.is_some_and(|entry| entry.is_default),
                b_is_default: b.is_some_and(|entry| entry.is_default),
                differs: match (a, b) {
                    (Some(a), Some(b)) => a.value != b.value,
                    // Present on one side only.
                    _ => true,
                },
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Offset migration. The decisions are pure functions over watermarks so every
// edge — an empty partition, a group that read nothing, a group that read
// everything — is a unit test rather than a cluster fixture.
// ---------------------------------------------------------------------------

/// What the source side of one partition says about where its group is.
///
/// The `allow(dead_code)` covers the bare tier: without `kafka` there is no
/// cluster to plan against, so only the tests reach these — and they are the
/// half of the migration worth keeping testable there.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// Nothing to migrate.
    Nothing,
    /// The group had read nothing.
    Earliest,
    /// The group had read everything.
    Latest,
    /// Look up the timestamp of the first record at or after this offset.
    At(i64),
}

/// Reads one source partition's situation off its watermarks.
///
/// The order of the checks is the specification:
///
/// 1. **No commit** — the group never read this partition, so there is nothing
///    to carry across.
/// 2. **An empty partition** (`earliest == end`) — nothing was ever readable
///    there, so no offset on the destination can be justified by evidence. Even
///    a committed 0 is only "the group has read the nothing that was there";
///    committing 0 on the destination would look like a decision and be a
///    guess. The destination group's own `auto.offset.reset` is the honest
///    place for that call.
/// 3. **Committed at or below the log start** — the group has read nothing that
///    still exists, which is what "start from the beginning" means on the
///    destination.
/// 4. **Committed at or past the end** — the group is caught up; on the
///    destination that is the end, not a timestamp.
/// 5. **Anything in between** — the only case with a record to read a timestamp
///    from, and therefore the only case a timestamp lookup can serve.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
fn anchor(committed: Option<i64>, earliest: i64, end: i64) -> Anchor {
    let Some(committed) = committed else {
        return Anchor::Nothing;
    };
    if earliest >= end {
        return Anchor::Nothing;
    }
    if committed <= earliest {
        return Anchor::Earliest;
    }
    if committed >= end {
        return Anchor::Latest;
    }
    Anchor::At(committed)
}

/// The destination partition's answers: its watermarks, and where the source
/// timestamp falls in it (`None` = nothing at or after that time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
struct DestPosition {
    earliest: i64,
    end: i64,
    at_timestamp: Option<i64>,
}

/// Turns one partition's anchor and the destination's answers into a plan row.
///
/// `source_ts_ms` is `None` when nothing could be read at or after the
/// committed offset — a partition whose remaining offsets hold only transaction
/// markers, or one that was compacted out from under the group. That is
/// "everything readable has been read", so it lands where a caught-up group
/// lands: the destination's end. Guessing an offset from a timestamp we do not
/// have would be the one outcome nobody could audit.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
fn migration_row(
    partition: i32,
    committed: Option<i64>,
    anchor: Anchor,
    source_ts_ms: Option<i64>,
    dest: DestPosition,
) -> OffsetMigrationRow {
    let (dest_offset, method) = match anchor {
        Anchor::Nothing => (None, METHOD_NONE),
        Anchor::Earliest => (Some(dest.earliest), METHOD_EARLIEST),
        Anchor::Latest => (Some(dest.end), METHOD_LATEST),
        Anchor::At(_) => match (source_ts_ms, dest.at_timestamp) {
            (Some(_), Some(offset)) => (Some(offset), METHOD_TIMESTAMP),
            // Either nothing readable remained on the source, or nothing on the
            // destination is that recent. Both mean "start at the end and wait".
            _ => (Some(dest.end), METHOD_LATEST),
        },
    };
    OffsetMigrationRow {
        partition,
        source_committed: committed,
        source_ts_ms,
        dest_offset,
        method: method.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The copy engine.
// ---------------------------------------------------------------------------

/// The read half's poll granularity, and therefore the worst-case latency of a
/// [`CopySession::stop`] while the source is quiet.
#[cfg(feature = "kafka")]
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long the copy waits for its *first* record before concluding there is
/// nothing to read. Generous: it covers connect, metadata and first-fetch
/// latency on a cold cluster.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
const QUIET_BEFORE_DATA: Duration = Duration::from_secs(10);

/// How long it waits between records once they are flowing. The end watermarks
/// captured at the start are the primary completion signal, but a transactional
/// topic's markers occupy offsets a consumer never receives, so a watermark-only
/// loop can never reach the end — this is the backstop, and it is long because a
/// slow broker mid-copy must not read as "finished".
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
const QUIET_AFTER_DATA: Duration = Duration::from_secs(5);

/// **How long the SOURCE has been quiet** — and nothing else.
///
/// The copy has one clock that may end it short of its watermarks — the source
/// going silent — and several waits running through the same loop that are not
/// the source at all: the rate limiter's pacing gap, the in-flight drain that
/// lets delivery reports come back, and a CEL filter's decode, which can sit on
/// a Schema Registry HTTP call for that call's whole timeout. None of those is
/// the source saying anything, and a single deadline spanning all of them means
/// a paced copy, a slow destination or a slow registry spends the source's
/// silence budget — at which point the copy stops early, marks every remaining
/// partition finished, and reports success.
///
/// So the deadline is re-armed on every edge: when a record arrives (the source
/// spoke), after every wait spent on the destination, and after every decode
/// (neither of those was the source's time). What is left measures exactly what
/// it claims to.
///
/// Pure over a clock it is handed, and outside the `kafka` gate, so the
/// arithmetic is a unit test rather than a five-second sleep.
#[derive(Debug)]
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
struct SourceSilence {
    deadline: Instant,
}

#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
impl SourceSilence {
    /// Before the first record: the long, cold-cluster budget.
    fn waiting_for_first(now: Instant) -> Self {
        Self {
            deadline: now + QUIET_BEFORE_DATA,
        }
    }

    /// The source spoke, or a wait on the destination has just ended. Either
    /// way the budget starts again from `now`.
    fn heard_from_source(&mut self, now: Instant) {
        self.deadline = now + QUIET_AFTER_DATA;
    }

    /// Time spent on the destination is not the source being quiet. Identical
    /// arithmetic to [`Self::heard_from_source`], and a separate name because
    /// the two are separate facts and a reader has to be able to tell which
    /// call site is which.
    fn waited_on_destination(&mut self, now: Instant) {
        self.deadline = now + QUIET_AFTER_DATA;
    }

    /// Time spent deciding whether to keep a record is not the source being
    /// quiet either. `keep` decodes only when a CEL filter needs it to, and
    /// that decode can reach the Schema Registry over HTTP — a lookup bounded
    /// by [`crate::sr`]'s 10s timeout, twice [`QUIET_AFTER_DATA`], for a single
    /// record. Identical arithmetic once more, and a third name for the third
    /// fact, so a reader can tell which one a call site is stating.
    fn waited_on_decode(&mut self, now: Instant) {
        self.deadline = now + QUIET_AFTER_DATA;
    }

    fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

/// A copy is expected to run for minutes, so its records get the bulk timeout
/// rather than the single-send one.
#[cfg(feature = "kafka")]
const COPY_MESSAGE_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(feature = "kafka")]
const COPY_FLUSH: Duration = Duration::from_secs(60);

/// How long a cancelled copy gives its already-accepted records to settle.
/// Bounded by [`MAX_IN_FLIGHT`], and `stop()` has to feel immediate.
#[cfg(feature = "kafka")]
const CANCEL_FLUSH: Duration = Duration::from_secs(3);

/// Records handed to librdkafka but not yet acknowledged — the bound that keeps
/// an unlimited copy from outrunning its own memory.
#[cfg(feature = "kafka")]
const MAX_IN_FLIGHT: i32 = 5_000;

/// One poll of the producer: how finely a pacing wait is sliced, and how
/// promptly a cancel is noticed while it waits.
#[cfg(feature = "kafka")]
const POLL_SLICE: Duration = Duration::from_millis(50);

/// The operation name a read-only destination refuses by.
#[cfg(feature = "kafka")]
const COPY_OP: &str = "copy messages into this cluster";

/// One partition's read window, `[start, end)`.
#[cfg(feature = "kafka")]
#[derive(Debug, Clone, Copy)]
struct Window {
    partition: i32,
    start: i64,
    end: i64,
}

#[cfg(feature = "kafka")]
impl Window {
    fn size(&self) -> u64 {
        u64::try_from(self.end.saturating_sub(self.start).max(0)).unwrap_or(0)
    }
}

/// A running copy: one reader thread that polls the source, filters, paces and
/// produces, and a progress snapshot that is always the truth.
///
/// Dropping the session stops it and joins the thread, so the consumer, the
/// producer and their broker connections are gone before `drop` returns.
#[cfg(feature = "kafka")]
pub struct CopySession {
    shared: Arc<CopyShared>,
    cancel: CancelToken,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "kafka")]
struct CopyShared {
    /// Where delivery reports land — the destination's own count of what it
    /// accepted.
    sink: Arc<DeliverySink>,
    scanned: AtomicU64,
    /// Records the filter could not be evaluated against. Not matched, not
    /// copied, and not an error (see [`crate::search::SearchProgress`]) — but
    /// counted, because a copy that quietly skipped a tenth of a topic is the
    /// same lie as a truncated result list.
    unevaluated: AtomicU64,
    filter_error: Mutex<Option<String>>,
    done: AtomicBool,
    fatal: Mutex<Option<String>>,
    /// Partitions the quiet deadline finished short of their captured end
    /// watermark, ascending. Written once, by the reader, on its way out.
    assumed_complete: Mutex<Vec<i32>>,
}

#[cfg(feature = "kafka")]
impl CopyShared {
    /// Poison-tolerant, like everywhere else in the crate: a panicking reader
    /// must not turn a running copy into a permanently locked one.
    fn guard<T>(slot: &Mutex<T>) -> MutexGuard<'_, T> {
        slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// First failure wins: the one that stopped the copy is the one worth
    /// showing, and the rest are usually its echoes.
    fn note_fatal(&self, message: String) {
        let mut slot = Self::guard(&self.fatal);
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    fn note_unevaluated(&self, message: String) {
        self.unevaluated.fetch_add(1, Ordering::Relaxed);
        let mut slot = Self::guard(&self.filter_error);
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    /// These partitions were finished by the quiet deadline rather than by
    /// reaching their end watermark. Ascending, and stored rather than logged:
    /// a copy that moved less than it planned to says so in the payload the
    /// panel is already reading.
    fn note_assumed_complete(&self, mut partitions: Vec<i32>) {
        partitions.sort_unstable();
        partitions.dedup();
        *Self::guard(&self.assumed_complete) = partitions;
    }

    fn snapshot(&self) -> CopyProgress {
        let unevaluated = self.unevaluated.load(Ordering::Relaxed);
        let unjudged = || {
            let first = Self::guard(&self.filter_error).clone()?;
            Some(format!(
                "{unevaluated} record(s) couldn't be judged by the filter, so they were not \
                 copied — {first}"
            ))
        };
        CopyProgress {
            copied: self.sink.delivered(),
            scanned: self.scanned.load(Ordering::Relaxed),
            failed: self.sink.failed(),
            done: self.done.load(Ordering::Relaxed),
            error: Self::guard(&self.fatal)
                .clone()
                .or_else(|| self.sink.first_error())
                .or_else(unjudged),
            assumed_complete: Self::guard(&self.assumed_complete).clone(),
        }
    }
}

/// Everything the reader thread owns, bundled so the thread body takes one
/// argument instead of a dozen.
#[cfg(feature = "kafka")]
struct CopyRun {
    consumer: BaseConsumer<KavkaClientContext>,
    producer: BaseProducer<KavkaClientContext>,
    windows: Vec<Window>,
    source_topic: String,
    source_cluster: String,
    dest_topic: String,
    filter: CompiledQuery,
    registry: Option<SchemaRegistry>,
    preserve_partition: bool,
    provenance: bool,
    max_messages: Option<u64>,
    limiter: Option<RateLimiter>,
}

#[cfg(feature = "kafka")]
impl CopySession {
    /// Plans the copy and starts the reader.
    ///
    /// Order of operations is load-bearing:
    ///
    /// 1. **The destination's read-only flag**, before a filter is compiled or
    ///    a socket is opened (D5).
    /// 2. **The filter**, so a CEL typo costs no round trip.
    /// 3. **A topic copied onto itself**, which costs a cluster-id round trip
    ///    only when the two topic names are the same — see
    ///    [`ensure_not_self_copy`], and note that it is CLUSTER identity, not
    ///    profile identity, because two saved connections to one cluster have
    ///    two ids and one log.
    /// 4. **Both topics' metadata**, so a partition mapping that cannot work is
    ///    a sentence rather than one UNKNOWN_TOPIC_OR_PARTITION per record.
    /// 5. The source consumer, assigned on this thread, so a bad profile or an
    ///    unreachable broker is an error from `start` rather than a session
    ///    that reports `done` a moment later having moved nothing.
    ///
    /// The Schema Registry used for decoding is the **source** profile's, and
    /// it is used only when the filter is a CEL expression: the destination
    /// never sees a decoded record, because a copy writes the bytes it read.
    pub fn start(
        source: &ClusterConnection,
        dest: &ClusterConnection,
        spec: &CopySpec,
    ) -> Result<Self> {
        dest.ensure_writable(COPY_OP)?;

        let query = spec.filter.clone().unwrap_or_default();
        let filter = CompiledQuery::compile(&query)?;

        // Only the same-name case pays for the cluster ids: on every other copy
        // the topic names settle it without a broker call.
        if spec.source_topic == spec.dest_topic {
            ensure_not_self_copy(
                &spec.source_topic,
                &spec.dest_topic,
                source.cluster_id().as_deref(),
                &source.profile().bootstrap_servers,
                dest.cluster_id().as_deref(),
                &dest.profile().bootstrap_servers,
            )?;
        }

        let dest_partitions = partition_count(dest, &spec.dest_topic)?;
        let consumer = source.new_session_consumer()?;
        let resolved =
            resolve_partitions(&consumer, &spec.source_topic, spec.partitions.as_deref())?;
        if spec.preserve_partition {
            // What the copy will WRITE to, not what the source has: a copy of
            // partition 0 alone needs one partition on the destination however
            // wide the source is.
            ensure_partition_mapping(
                &spec.source_topic,
                partitions_needed(&resolved.selected),
                &spec.dest_topic,
                dest_partitions,
            )?;
        }
        let windows = plan_windows(&consumer, &spec.source_topic, spec, &resolved)?;

        let mut assignment = TopicPartitionList::new();
        for window in windows.iter().filter(|w| w.size() > 0) {
            assignment
                .add_partition_offset(
                    &spec.source_topic,
                    window.partition,
                    Offset::Offset(window.start),
                )
                .map_err(|e| {
                    Error::Other(format!(
                        "seeking {}[{}] to offset {}: {e}",
                        spec.source_topic, window.partition, window.start
                    ))
                })?;
        }
        // An empty assignment is legal and is what a copy of an empty window
        // gets: the reader sees no pending partitions and finishes at once.
        consumer.assign(&assignment).map_err(|e| {
            Error::Other(format!(
                "assigning partitions of {}: {e}",
                spec.source_topic
            ))
        })?;

        let producer = dest.new_producer(|config| {
            // A little batching, exactly like a bulk run: a copy is throughput,
            // and 5ms is invisible beside any rate a human would set.
            produce::tune(config, COPY_MESSAGE_TIMEOUT, "5");
        })?;

        let shared = Arc::new(CopyShared {
            sink: Arc::new(DeliverySink::default()),
            scanned: AtomicU64::new(0),
            unevaluated: AtomicU64::new(0),
            filter_error: Mutex::new(None),
            done: AtomicBool::new(false),
            fatal: Mutex::new(None),
            assumed_complete: Mutex::new(Vec::new()),
        });
        let cancel = CancelToken::new();
        let run = CopyRun {
            consumer,
            producer,
            windows,
            source_topic: spec.source_topic.clone(),
            source_cluster: source.profile().name.clone(),
            dest_topic: spec.dest_topic.clone(),
            filter,
            registry: source
                .profile()
                .schema_registry
                .as_ref()
                .map(SchemaRegistry::new),
            preserve_partition: spec.preserve_partition,
            provenance: spec.provenance_headers,
            max_messages: spec.max_messages.map(u64::from),
            limiter: spec
                .rate_per_sec
                .and_then(|rate| RateLimiter::new(rate, Instant::now())),
        };

        tracing::debug!(
            source = %spec.source_topic,
            dest = %spec.dest_topic,
            partitions = run.windows.len(),
            "copy started"
        );

        let worker = {
            let shared = Arc::clone(&shared);
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("kavka-copy".into())
                .spawn(move || {
                    pump(run, &shared, &cancel);
                    shared.done.store(true, Ordering::Relaxed);
                })
                .map_err(|e| Error::Other(format!("starting the copy thread: {e}")))?
        };

        Ok(Self {
            shared,
            cancel,
            worker: Some(worker),
        })
    }

    /// A snapshot for the throttled progress event. Every count is cumulative
    /// and monotonic, and `copied` is what the destination acknowledged.
    pub fn progress(&self) -> CopyProgress {
        self.shared.snapshot()
    }

    /// The token the reader checks. Exposed so a caller that already keys
    /// cancellation by profile can stop a copy the way it stops a fetch.
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Asks the copy to finish. Idempotent, safe from any thread, and returns
    /// immediately — records librdkafka already accepted are flushed first, so
    /// the final counts describe the destination rather than our intentions.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

/// Deliberately not derived: a derived form would print the filter — user text —
/// into any log line that touches a session.
#[cfg(feature = "kafka")]
impl std::fmt::Debug for CopySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let progress = self.progress();
        f.debug_struct("CopySession")
            .field("copied", &progress.copied)
            .field("scanned", &progress.scanned)
            .field("failed", &progress.failed)
            .field("done", &progress.done)
            .finish()
    }
}

#[cfg(feature = "kafka")]
impl Drop for CopySession {
    /// Joins the reader, so the consumer and the producer — and their broker
    /// connections — are gone before `drop` returns. A leaked copy thread is
    /// the expensive kind: it is still writing to a cluster.
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The reader: poll, filter, pace, produce — until the windows are read, the
/// ceiling is reached, the copy is cancelled, or a broker stops answering.
#[cfg(feature = "kafka")]
fn pump(run: CopyRun, shared: &CopyShared, cancel: &CancelToken) {
    let CopyRun {
        consumer,
        producer,
        windows,
        source_topic,
        source_cluster,
        dest_topic,
        filter,
        registry,
        preserve_partition,
        provenance,
        max_messages,
        mut limiter,
    } = run;

    let evaluator = filter.evaluator();
    // partition -> the end watermark it must reach to be finished.
    let mut pending: HashMap<i32, i64> = windows
        .iter()
        .filter(|window| window.size() > 0)
        .map(|window| (window.partition, window.end))
        .collect();
    // partition -> the next offset the copy expects there, so the quiet
    // deadline can say whether a partition it finished was actually short.
    let mut positions: HashMap<i32, i64> = windows
        .iter()
        .filter(|window| window.size() > 0)
        .map(|window| (window.partition, window.start))
        .collect();
    let mut offered: u64 = 0;
    let mut silence = SourceSilence::waiting_for_first(Instant::now());
    let mut cancelled = false;

    while !pending.is_empty() {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        if max_messages.is_some_and(|max| offered >= max) {
            break;
        }
        if silence.expired(Instant::now()) {
            // Not an error: a transactional topic's offsets can be occupied by
            // markers a consumer never receives, so the watermark alone cannot
            // always be reached. But "cannot be reached" and "a broker stopped
            // answering" look identical from here, so any partition still short
            // of its captured end is NAMED in the progress rather than quietly
            // marked finished.
            let short: Vec<i32> = pending
                .iter()
                .filter(|(partition, end)| {
                    positions.get(partition).copied().unwrap_or(i64::MIN) < **end
                })
                .map(|(partition, _)| *partition)
                .collect();
            tracing::debug!(
                topic = %source_topic,
                partitions = pending.len(),
                short = short.len(),
                "no more records arrived from the source; treating these partitions as copied"
            );
            shared.note_assumed_complete(short);
            break;
        }
        let message = match consumer.poll(POLL_INTERVAL) {
            None => continue,
            Some(Err(e)) => {
                shared.note_fatal(format!("reading {source_topic}: {e}"));
                cancelled = true;
                break;
            }
            Some(Ok(message)) => message,
        };
        silence.heard_from_source(Instant::now());

        let partition = message.partition();
        let offset = message.offset();
        let Some(&end) = pending.get(&partition) else {
            continue;
        };
        positions.insert(partition, offset.saturating_add(1));
        // At or past the watermark captured at the start: a producer is still
        // writing. A copy answers about the topic as it was when it was asked.
        if offset >= end {
            pending.remove(&partition);
            continue;
        }
        if offset + 1 >= end {
            pending.remove(&partition);
        }
        shared.scanned.fetch_add(1, Ordering::Relaxed);

        let verdict = keep(&message, &filter, evaluator.as_ref(), registry.as_ref());
        // Re-armed BEFORE the verdict is acted on, because two of its three arms
        // go straight back to the expiry check at the top of the loop without
        // passing a destination wait: a CEL filter's decode can sit on a schema
        // registry HTTP call for its full timeout, and a registry slow enough to
        // do that on record after record would spend the source's silence budget
        // exactly as a slow destination would — the copy would stop early and
        // call every unread partition finished.
        silence.waited_on_decode(Instant::now());
        match verdict {
            Ok(false) => continue,
            Err(e) => {
                shared.note_unevaluated(e);
                continue;
            }
            Ok(true) => {}
        }

        // Pacing first, so the wait is spent before a record is handed over
        // rather than after — the destination sees the rate the user asked for.
        //
        // The deadline is re-armed AFTER the wait, every time: a pacing gap is
        // the copy holding back, not the source going quiet, and a deadline
        // that counted it would let `rate_per_sec` truncate the very copy it
        // was set to protect.
        if let Some(limiter) = limiter.as_mut() {
            let wait = limiter.take(Instant::now());
            if !wait.is_zero() {
                pace(&producer, wait, cancel);
                silence.waited_on_destination(Instant::now());
                if cancel.is_cancelled() {
                    cancelled = true;
                    break;
                }
            }
        }
        // The in-flight bound. Polling here is not a stall: it is what collects
        // the delivery reports that let the window drain — and, for the same
        // reason as pacing, a destination slow enough to sit in this loop must
        // not be able to spend the source's silence budget.
        let mut drained = false;
        while producer.in_flight_count() >= MAX_IN_FLIGHT && !cancel.is_cancelled() {
            producer.poll(Timeout::After(POLL_SLICE));
            drained = true;
        }
        if drained {
            silence.waited_on_destination(Instant::now());
        }
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }

        let headers = outgoing_headers(
            &message,
            provenance.then(|| {
                provenance_headers(
                    &source_cluster,
                    &source_topic,
                    partition,
                    offset,
                    message.timestamp().to_millis(),
                )
            }),
        );
        let mut outgoing: BaseRecord<'_, [u8], [u8], Arc<DeliverySink>> =
            BaseRecord::with_opaque_to(&dest_topic, Arc::clone(&shared.sink));
        if let Some(key) = message.key() {
            outgoing = outgoing.key(key);
        }
        // No `payload` call at all is a null payload — the tombstone, which is
        // a different fact from an empty value on a compacted topic.
        if let Some(payload) = message.payload() {
            outgoing = outgoing.payload(payload);
        }
        if preserve_partition {
            outgoing = outgoing.partition(partition);
        }
        // The source record's own time, not now: a replay whose records all
        // claim to have been written this afternoon breaks every time-based
        // reprocessing downstream of it. A destination topic configured with
        // `log.append.time` overrides this, and that is the broker's decision
        // to make — the provenance header keeps the truth either way.
        if let Some(timestamp) = message.timestamp().to_millis() {
            outgoing = outgoing.timestamp(timestamp);
        }
        if let Some(headers) = headers {
            outgoing = outgoing.headers(headers);
        }

        loop {
            match producer.send(outgoing) {
                Ok(()) => {
                    offered += 1;
                    break;
                }
                Err((e, returned)) if is_queue_full(&e) => {
                    // Backpressure, not a failure: drain and offer it again.
                    // Third and last place the copy waits on the destination,
                    // and re-armed for the third and last time.
                    producer.poll(Timeout::After(POLL_SLICE));
                    silence.waited_on_destination(Instant::now());
                    if cancel.is_cancelled() {
                        cancelled = true;
                        break;
                    }
                    outgoing = returned;
                }
                Err((e, _)) => {
                    shared.note_fatal(format!("copying into {dest_topic} failed: {e}"));
                    cancelled = true;
                    break;
                }
            }
        }
        if cancelled {
            break;
        }
    }

    // Whatever librdkafka already accepted is delivered and counted, so the
    // final numbers describe the destination rather than our intentions.
    let drain = if cancelled { CANCEL_FLUSH } else { COPY_FLUSH };
    if let Err(e) = producer.flush(Timeout::After(drain)) {
        shared.note_fatal(format!(
            "some records were still unsent after {}s: {e}",
            drain.as_secs()
        ));
    }
    if let Some(error) = registry.as_ref().and_then(SchemaRegistry::take_error) {
        tracing::warn!(
            topic = %source_topic,
            %error,
            "schema registry unavailable; a CEL filter could not read those payloads"
        );
    }
}

/// Whether this record is one the copy should move.
///
/// The order is the performance story, and it is [`crate::search`]'s: raw bytes
/// first (no allocation, no parsing), and a decode only when there is a CEL
/// expression that needs one. `Err` is a filter that could not be evaluated —
/// counted and explained, never fatal, and never a copy.
#[cfg(feature = "kafka")]
fn keep(
    message: &BorrowedMessage<'_>,
    filter: &CompiledQuery,
    evaluator: Option<&CelFilter>,
    registry: Option<&SchemaRegistry>,
) -> std::result::Result<bool, String> {
    if !filter.matches_raw(message.key(), message.payload(), raw_headers(message)) {
        return Ok(false);
    }
    let Some(evaluator) = evaluator else {
        return Ok(true);
    };
    evaluator.matches(&decoded(message, registry))
}

/// One record decoded far enough for a CEL expression to judge it. Only ever
/// built when there *is* an expression — the copy itself writes raw bytes.
#[cfg(feature = "kafka")]
fn decoded(message: &BorrowedMessage<'_>, registry: Option<&SchemaRegistry>) -> MessageRecord {
    let headers: Vec<_> = message.headers().map_or_else(Vec::new, |headers| {
        (0..headers.count())
            .map(|i| {
                let header = headers.get(i);
                serdes::decode_header(header.key, header.value)
            })
            .collect()
    });
    MessageRecord {
        partition: message.partition(),
        offset: message.offset(),
        timestamp_ms: message.timestamp().to_millis(),
        key: message
            .key()
            .map(|bytes| serdes::decode(bytes, registry, DEFAULT_MAX_VALUE_BYTES)),
        value: message
            .payload()
            .map(|bytes| serdes::decode(bytes, registry, DEFAULT_MAX_VALUE_BYTES)),
        dlq: serdes::dlq_inspect(&headers),
        headers,
    }
}

/// The record's headers as raw name/value bytes, lazily — the prefilter runs on
/// every record of the scan, so materialising a `Vec` here would put an
/// allocation per record into the loop the prefilter exists to keep cheap.
#[cfg(feature = "kafka")]
fn raw_headers<'m>(
    message: &'m BorrowedMessage<'m>,
) -> impl Iterator<Item = (&'m [u8], Option<&'m [u8]>)> {
    message.headers().into_iter().flat_map(|headers| {
        (0..headers.count()).map(move |i| {
            let header = headers.get(i);
            (header.key.as_bytes(), header.value)
        })
    })
}

/// The copied record's headers: the source record's own, verbatim and in order,
/// then the provenance ones if they were asked for.
///
/// `None` rather than an empty set when there is nothing to carry: an empty
/// `OwnedHeaders` still allocates a native header list and attaches it to every
/// record — and, more importantly here, it would make a copied record carry an
/// empty header list where the original carried none.
#[cfg(feature = "kafka")]
fn outgoing_headers(
    message: &BorrowedMessage<'_>,
    provenance: Option<Vec<ProduceHeader>>,
) -> Option<OwnedHeaders> {
    let source = message.headers();
    let source_count = source.map_or(0, |headers| headers.count());
    let extra = provenance.as_ref().map_or(0, Vec::len);
    if source_count + extra == 0 {
        return None;
    }

    let mut owned = OwnedHeaders::new_with_capacity(source_count + extra);
    if let Some(headers) = source {
        for i in 0..headers.count() {
            // Straight through, including a null value and a non-UTF-8 name's
            // bytes: a copy that normalised headers would not be a copy.
            owned = owned.insert(headers.get(i));
        }
    }
    for header in provenance.into_iter().flatten() {
        owned = owned.insert(Header {
            key: &header.key,
            value: Some(header.value.as_bytes()),
        });
    }
    Some(owned)
}

/// Waits out one pacing gap. `poll` doubles as the sleep: it serves delivery
/// reports while it waits, and it is what makes a cancel land within
/// [`POLL_SLICE`] rather than at the end of a long wait.
#[cfg(feature = "kafka")]
fn pace(producer: &BaseProducer<KavkaClientContext>, wait: Duration, cancel: &CancelToken) {
    let deadline = Instant::now() + wait;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || cancel.is_cancelled() {
            return;
        }
        producer.poll(Timeout::After(remaining.min(POLL_SLICE)));
    }
}

#[cfg(feature = "kafka")]
fn is_queue_full(error: &KafkaError) -> bool {
    matches!(
        error,
        KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull)
    )
}

// ---------------------------------------------------------------------------
// Planning — shared by the copy and its dry run, which is the only way the two
// can be expected to agree.
// ---------------------------------------------------------------------------

/// What a copy would move, from watermarks alone: no records are read, nothing
/// is written, and no connection to the destination is opened.
///
/// The plan is computed by the same function [`CopySession::start`] uses, so
/// "the dry run said 4 218 and the copy moved 4 218" is a property of the code
/// rather than a coincidence — on a topic with no transactional writers and no
/// filter, which is exactly what [`CopyEstimate::would_copy_estimate`]'s docs
/// say.
#[cfg(feature = "kafka")]
pub fn copy_dry_run(source: &ClusterConnection, spec: &CopySpec) -> Result<CopyEstimate> {
    // Same order as `start`: a filter that does not compile fails here too,
    // before any broker call, rather than being discovered at copy time.
    let query = spec.filter.clone().unwrap_or_default();
    let filter = CompiledQuery::compile(&query)?;

    let consumer = source.new_session_consumer()?;
    let available = resolve_partitions(&consumer, &spec.source_topic, spec.partitions.as_deref())?;
    let windows = plan_windows(&consumer, &spec.source_topic, spec, &available)?;

    let scan: u64 = windows.iter().map(Window::size).sum();
    Ok(CopyEstimate {
        would_copy_estimate: match spec.max_messages {
            Some(max) => scan.min(u64::from(max)),
            None => scan,
        },
        estimate_only: !filter.is_match_all(),
        per_partition: windows
            .iter()
            .map(|window| PartitionProgress {
                partition: window.partition,
                current_offset: window.start,
                end_offset: window.end,
            })
            .collect(),
    })
}

/// What a topic has, and what this copy reads of it. Both halves are kept
/// because an error about one partition has to tell "this topic has no
/// partition 3" apart from "partition 3 exists, but your filter excluded it".
#[cfg(feature = "kafka")]
struct ResolvedPartitions {
    available: Vec<i32>,
    selected: Vec<i32>,
}

#[cfg(feature = "kafka")]
fn resolve_partitions(
    consumer: &BaseConsumer<KavkaClientContext>,
    topic: &str,
    requested: Option<&[i32]>,
) -> Result<ResolvedPartitions> {
    let metadata = consumer
        .fetch_metadata(Some(topic), METADATA_TIMEOUT)
        .map_err(|e| Error::Other(format!("asking the cluster about {topic}: {e}")))?;
    let mut available: Vec<i32> = metadata
        .topics()
        .iter()
        .filter(|found| found.name() == topic)
        .flat_map(|found| found.partitions().iter().map(|p| p.id()))
        .collect();
    available.sort_unstable();
    if available.is_empty() {
        return Err(Error::Other(format!(
            "this cluster has no topic called {topic:?}"
        )));
    }

    let Some(requested) = requested else {
        return Ok(ResolvedPartitions {
            selected: available.clone(),
            available,
        });
    };
    let unknown: Vec<String> = requested
        .iter()
        .filter(|p| !available.contains(p))
        .map(i32::to_string)
        .collect();
    if !unknown.is_empty() {
        return Err(Error::Other(format!(
            "{topic} has {} partitions, so there is no partition {}",
            available.len(),
            unknown.join(", ")
        )));
    }
    let mut selected: Vec<i32> = requested.to_vec();
    selected.sort_unstable();
    selected.dedup();
    Ok(ResolvedPartitions {
        available,
        selected,
    })
}

/// Turns a [`SeekSpec`] into per-partition read windows.
///
/// Empty windows are **kept**, exactly as in [`crate::search`]: the dry run
/// reports per partition, and a partition silently missing from that list is
/// indistinguishable, in the UI, from one that was never planned.
#[cfg(feature = "kafka")]
fn plan_windows(
    consumer: &BaseConsumer<KavkaClientContext>,
    topic: &str,
    spec: &CopySpec,
    resolved: &ResolvedPartitions,
) -> Result<Vec<Window>> {
    let partitions = &resolved.selected;
    let mut plan = Vec::with_capacity(partitions.len());

    match spec.seek {
        SeekSpec::Offset { partition, offset } => {
            if !resolved.available.contains(&partition) {
                return Err(Error::Other(format!(
                    "{topic} has no partition {partition} to seek in — it has {}, numbered 0 to {}",
                    resolved.available.len(),
                    resolved.available.len().saturating_sub(1)
                )));
            }
            if !partitions.contains(&partition) {
                return Err(Error::Other(format!(
                    "partition {partition} isn't in the partition filter — clear the filter, or \
                     seek within it"
                )));
            }
            let (low, high) = watermarks(consumer, topic, partition)?;
            plan.push(Window {
                partition,
                start: offset.clamp(low, high),
                end: high,
            });
        }
        SeekSpec::Timestamp { timestamp_ms } => {
            let mut request = TopicPartitionList::new();
            for partition in partitions {
                request
                    .add_partition_offset(topic, *partition, Offset::Offset(timestamp_ms))
                    .map_err(|e| Error::Other(format!("building the timestamp lookup: {e}")))?;
            }
            let resolved_times = consumer
                .offsets_for_times(request, METADATA_TIMEOUT)
                .map_err(|e| {
                    Error::Other(format!("asking {topic} for the offsets at that time: {e}"))
                })?;
            for partition in partitions {
                let (low, high) = watermarks(consumer, topic, *partition)?;
                // Nothing at or after the timestamp resolves to the end — an
                // empty window, which stays in the plan as a partition that is
                // already complete.
                let start = resolved_times
                    .find_partition(topic, *partition)
                    .and_then(|found| match found.offset() {
                        Offset::Offset(offset) => Some(offset),
                        _ => None,
                    })
                    .unwrap_or(high)
                    .clamp(low, high);
                plan.push(Window {
                    partition: *partition,
                    start,
                    end: high,
                });
            }
        }
        SeekSpec::Earliest | SeekSpec::Latest { .. } => {
            let last_n = match spec.seek {
                SeekSpec::Latest { last_n } => Some(i64::from(last_n)),
                _ => None,
            };
            for partition in partitions {
                let (low, high) = watermarks(consumer, topic, *partition)?;
                let start = match last_n {
                    // `low` guards a compacted or retention-trimmed partition
                    // whose earliest offset is far above zero.
                    Some(n) => high.saturating_sub(n).max(low),
                    None => low,
                };
                plan.push(Window {
                    partition: *partition,
                    start,
                    end: high,
                });
            }
        }
    }

    plan.sort_by_key(|window| window.partition);
    Ok(plan)
}

#[cfg(feature = "kafka")]
fn watermarks(
    consumer: &BaseConsumer<KavkaClientContext>,
    topic: &str,
    partition: i32,
) -> Result<(i64, i64)> {
    consumer
        .fetch_watermarks(topic, partition, METADATA_TIMEOUT)
        .map_err(|e| Error::Other(format!("reading the offsets of {topic}[{partition}]: {e}")))
}

/// How many partitions a topic has, or the same sentence every other module
/// uses when it has none.
#[cfg(feature = "kafka")]
fn partition_count(conn: &ClusterConnection, topic: &str) -> Result<usize> {
    let metadata = conn.with_consumer("reading topic metadata", |consumer| {
        consumer.fetch_metadata(Some(topic), METADATA_TIMEOUT)
    })?;
    let count = metadata
        .topics()
        .iter()
        .find(|found| found.name() == topic)
        .map_or(0, |found| found.partitions().len());
    if count == 0 {
        return Err(Error::Other(format!(
            "this cluster has no topic called {topic:?}"
        )));
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Offset migration, against clusters.
// ---------------------------------------------------------------------------

/// How long the plan waits for the one record it needs from each partition.
/// A bounded fetch: a plan is interactive, and a source partition that will not
/// answer resolves to "caught up" rather than hanging the dialog.
#[cfg(feature = "kafka")]
const ANCHOR_FETCH_DEADLINE: Duration = Duration::from_secs(15);

#[cfg(feature = "kafka")]
const MIGRATE_OP: &str = "migrate consumer group offsets";

/// Works out where a source group's position lands on another cluster, one row
/// per partition of the source topic.
///
/// # Why a timestamp and not the offset
///
/// An offset means nothing on another cluster: the same records can sit at
/// entirely different offsets after a mirror, a compaction or a partial
/// retention window. What survives the crossing is *time* — so the plan reads
/// the timestamp of the first record at or after the source group's committed
/// offset (the record it would have read next) and asks the destination where
/// that moment falls. Every edge that has no such record is named rather than
/// guessed; see [`anchor`] for the five cases and [`migration_row`] for what
/// each produces.
///
/// # `dest_group_id`
///
/// Used to refuse early. The apply step cannot commit to a group with live
/// members, and finding that out *after* reviewing a plan is a wasted trip — so
/// a destination group that already exists and is not `Empty` is refused here
/// too, with [`crate::admin::ensure_group_resettable`]'s own sentence. A
/// destination group that does not exist yet is the normal case for a
/// migration and is not an error.
#[cfg(feature = "kafka")]
pub fn offsets_migrate_plan(
    source: &ClusterConnection,
    group_id: &str,
    topic: &str,
    dest: &ClusterConnection,
    dest_group_id: &str,
    dest_topic: &str,
) -> Result<Vec<OffsetMigrationRow>> {
    if let Some(group) = admin::groups_list(dest)?
        .into_iter()
        .find(|group| group.group_id == dest_group_id)
    {
        admin::ensure_group_resettable(
            &group.group_id,
            &group.state,
            group.member_count as usize,
            false,
        )?;
    }

    let source_consumer = source.new_session_consumer()?;
    let partitions = resolve_partitions(&source_consumer, topic, None)?.available;
    let dest_partitions = partition_count(dest, dest_topic)?;
    // The same rule as a partition-preserving copy, and for the same reason: a
    // migration maps index to index, so a narrower destination has nowhere to
    // put the tail of the plan.
    ensure_partition_mapping(topic, partitions.len(), dest_topic, dest_partitions)?;

    let group_consumer = source.new_group_consumer(group_id)?;
    let committed = committed_offsets(&group_consumer, topic, &partitions)?;

    // Which partitions need a record read, and from where.
    let mut anchors = Vec::with_capacity(partitions.len());
    let mut wanted = Vec::new();
    for partition in &partitions {
        let (earliest, end) = watermarks(&source_consumer, topic, *partition)?;
        let at = committed.get(partition).copied().flatten();
        let anchor = anchor(at, earliest, end);
        if let Anchor::At(offset) = anchor {
            wanted.push((*partition, offset));
        }
        anchors.push((*partition, at, anchor));
    }
    let timestamps = timestamps_at(&source_consumer, topic, &wanted)?;

    // The destination side: one timestamp lookup for every partition that has a
    // timestamp to look up, plus watermarks for the rest.
    let dest_consumer = dest.new_session_consumer()?;
    let mut lookup = TopicPartitionList::new();
    for (partition, _, _) in &anchors {
        if let Some(Some(timestamp)) = timestamps.get(partition) {
            lookup
                .add_partition_offset(dest_topic, *partition, Offset::Offset(*timestamp))
                .map_err(|e| Error::Other(format!("building the timestamp lookup: {e}")))?;
        }
    }
    let resolved = if lookup.count() == 0 {
        TopicPartitionList::new()
    } else {
        dest_consumer
            .offsets_for_times(lookup, METADATA_TIMEOUT)
            .map_err(|e| {
                Error::Other(format!(
                    "asking {dest_topic} for the offsets at those times: {e}"
                ))
            })?
    };

    let mut plan = Vec::with_capacity(anchors.len());
    for (partition, at, anchor) in anchors {
        let (earliest, end) = watermarks(&dest_consumer, dest_topic, partition)?;
        let at_timestamp = resolved
            .find_partition(dest_topic, partition)
            .and_then(|found| match found.offset() {
                Offset::Offset(offset) if offset >= 0 => Some(offset),
                _ => None,
            });
        plan.push(migration_row(
            partition,
            at,
            anchor,
            timestamps.get(&partition).copied().flatten(),
            DestPosition {
                earliest,
                end,
                at_timestamp,
            },
        ));
    }
    Ok(plan)
}

/// Commits a plan to the destination group and reads back what Kafka stored.
///
/// Rows with no `dest_offset` are skipped — [`METHOD_NONE`] means there was
/// nothing to migrate, and committing anything for those partitions would be
/// the guess the plan refused to make. An all-`none` plan is a no-op with an
/// empty answer, not an error.
///
/// # Why this does not call [`crate::admin::offsets_reset`]
///
/// It would be the obvious reuse, and it cannot work: a reset describes the
/// group first and refuses a group the cluster has never heard of — which is
/// precisely the destination group of a migration, nearly every time. So the
/// *rules* are reused instead and only the plumbing is here: the read-only
/// check, [`crate::admin::ensure_group_resettable`]'s refusal word for word,
/// [`crate::admin::resolve_reset_offset`]'s clamping into `[earliest, end]`,
/// and the same static assignment that never makes Kavka a member of the group.
#[cfg(feature = "kafka")]
pub fn offsets_migrate_apply(
    dest: &ClusterConnection,
    dest_group_id: &str,
    dest_topic: &str,
    plan: &[OffsetMigrationRow],
) -> Result<Vec<GroupOffset>> {
    dest.ensure_writable(MIGRATE_OP)?;

    if let Some(group) = admin::groups_list(dest)?
        .into_iter()
        .find(|group| group.group_id == dest_group_id)
    {
        admin::ensure_group_resettable(
            &group.group_id,
            &group.state,
            group.member_count as usize,
            false,
        )?;
    }

    let mut wanted: Vec<(i32, i64)> = plan
        .iter()
        .filter_map(|row| row.dest_offset.map(|offset| (row.partition, offset)))
        .collect();
    wanted.sort_unstable();
    wanted.dedup_by_key(|(partition, _)| *partition);
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let available = partition_count(dest, dest_topic)?;
    if let Some((partition, _)) = wanted
        .iter()
        .find(|(partition, _)| usize::try_from(*partition).unwrap_or(usize::MAX) >= available)
    {
        return Err(Error::Other(format!(
            "{dest_topic} has {available} partitions, so there is no partition {partition} to \
             commit to"
        )));
    }

    let consumer = dest.new_group_consumer(dest_group_id)?;
    let partitions: Vec<i32> = wanted.iter().map(|(partition, _)| *partition).collect();
    // A static assignment, not a subscription: no JoinGroup and no heartbeat,
    // so this never makes Kavka a member of the destination group. It exists
    // because librdkafka will only commit for a group handle that has a
    // coordinator and a partition set.
    consumer
        .assign(&partition_list(dest_topic, &partitions, Offset::Invalid)?)
        .map_err(|e| Error::Other(format!("preparing the offset migration failed: {e}")))?;

    let mut target = TopicPartitionList::with_capacity(wanted.len());
    for (partition, offset) in &wanted {
        let (earliest, end) = watermarks(&consumer, dest_topic, *partition)?;
        // The same clamp a reset applies, and for the same reason: Kafka would
        // accept an out-of-range commit and the group would then fail its next
        // fetch with OFFSET_OUT_OF_RANGE, at which point `auto.offset.reset`
        // silently decides where the application really starts.
        let landed = admin::resolve_reset_offset(
            &ResetTarget::Offset { offset: *offset },
            PartitionBounds {
                earliest,
                end,
                committed: None,
                at_timestamp: None,
            },
        );
        target
            .add_partition_offset(dest_topic, *partition, Offset::Offset(landed))
            .map_err(|e| Error::Other(format!("preparing the offset migration failed: {e}")))?;
    }

    consumer
        .commit(&target, CommitMode::Sync)
        .map_err(|e| Error::Other(format!("committing the migrated offsets failed: {e}")))?;

    // Read back from the coordinator rather than echoing the request: the
    // answer is what Kafka stored, not what Kavka asked for.
    let committed = committed_offsets(&consumer, dest_topic, &partitions)?;
    let mut out = Vec::with_capacity(partitions.len());
    for partition in &partitions {
        let (_, end_offset) = watermarks(&consumer, dest_topic, *partition)?;
        let at = committed.get(partition).copied().flatten();
        out.push(GroupOffset {
            topic: dest_topic.to_string(),
            partition: *partition,
            committed: at,
            end_offset,
            // Negative lag is never a true statement about outstanding work.
            lag: at.map(|at| (end_offset - at).max(0)),
        });
    }
    Ok(out)
}

/// One group's committed offsets for these partitions. `None` means the group
/// has never committed there — librdkafka spells that `Offset::Invalid`, and a
/// negative raw offset means the same thing.
#[cfg(feature = "kafka")]
fn committed_offsets(
    consumer: &BaseConsumer<KavkaClientContext>,
    topic: &str,
    partitions: &[i32],
) -> Result<HashMap<i32, Option<i64>>> {
    if partitions.is_empty() {
        return Ok(HashMap::new());
    }
    let query = partition_list(topic, partitions, Offset::Invalid)?;
    let committed = consumer
        .committed_offsets(query, METADATA_TIMEOUT)
        .map_err(|e| Error::Other(format!("reading the group's committed offsets failed: {e}")))?;
    Ok(committed
        .to_topic_map()
        .into_iter()
        .map(|((_, partition), offset)| {
            let offset = match offset {
                Offset::Offset(at) if at >= 0 => Some(at),
                _ => None,
            };
            (partition, offset)
        })
        .collect())
}

/// The timestamp of the first record at or after each wanted offset.
///
/// One consumer for every partition at once, one deadline for the lot: a plan
/// is a dialog the user is waiting on, and a partition that answers nothing
/// resolves to `None` — which [`migration_row`] reads as "everything readable
/// has been read" rather than as a failure.
#[cfg(feature = "kafka")]
fn timestamps_at(
    consumer: &BaseConsumer<KavkaClientContext>,
    topic: &str,
    wanted: &[(i32, i64)],
) -> Result<HashMap<i32, Option<i64>>> {
    let mut found: HashMap<i32, Option<i64>> = wanted
        .iter()
        .map(|(partition, _)| (*partition, None))
        .collect();
    if wanted.is_empty() {
        return Ok(found);
    }

    let mut assignment = TopicPartitionList::with_capacity(wanted.len());
    for (partition, offset) in wanted {
        assignment
            .add_partition_offset(topic, *partition, Offset::Offset(*offset))
            .map_err(|e| Error::Other(format!("seeking {topic}[{partition}]: {e}")))?;
    }
    consumer
        .assign(&assignment)
        .map_err(|e| Error::Other(format!("assigning partitions of {topic}: {e}")))?;

    let deadline = Instant::now() + ANCHOR_FETCH_DEADLINE;
    let mut missing = wanted.len();
    while missing > 0 && Instant::now() < deadline {
        match consumer.poll(POLL_INTERVAL) {
            None => continue,
            // A partition that errors is a partition with no anchor, which is a
            // documented outcome rather than a failed plan.
            Some(Err(e)) => {
                tracing::warn!(topic, error = %e, "reading the group's next record");
                break;
            }
            Some(Ok(message)) => {
                if let Some(slot) = found.get_mut(&message.partition()) {
                    if slot.is_none() {
                        *slot = message.timestamp().to_millis();
                        missing -= 1;
                    }
                }
            }
        }
    }
    // Leave nothing assigned: this consumer is reused for watermarks.
    let _ = consumer.unassign();
    Ok(found)
}

#[cfg(feature = "kafka")]
fn partition_list(topic: &str, partitions: &[i32], offset: Offset) -> Result<TopicPartitionList> {
    let mut list = TopicPartitionList::with_capacity(partitions.len());
    for partition in partitions {
        list.add_partition_offset(topic, *partition, offset)
            .map_err(|e| Error::Other(format!("building the partition list failed: {e}")))?;
    }
    Ok(list)
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
pub fn copy_dry_run(_source: &ClusterConnection, _spec: &CopySpec) -> Result<CopyEstimate> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn offsets_migrate_plan(
    _source: &ClusterConnection,
    _group_id: &str,
    _topic: &str,
    _dest: &ClusterConnection,
    _dest_group_id: &str,
    _dest_topic: &str,
) -> Result<Vec<OffsetMigrationRow>> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn offsets_migrate_apply(
    _dest: &ClusterConnection,
    _dest_group_id: &str,
    _dest_topic: &str,
    _plan: &[OffsetMigrationRow],
) -> Result<Vec<crate::admin::GroupOffset>> {
    unsupported()
}

// ---------------------------------------------------------------------------
// Tests that need no cluster. Outside the `kafka` gate on purpose: the rules
// with the subtle edges — the diff, the bucket, the plan's five cases — are all
// pure, and they must stay testable on the bare toolchain tier.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod wire {
    use super::*;

    fn spec() -> CopySpec {
        CopySpec {
            source_topic: "orders".into(),
            dest_profile_id: "prof-2".into(),
            dest_topic: "orders-replay".into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            filter: None,
            max_messages: None,
            rate_per_sec: None,
            preserve_partition: true,
            provenance_headers: true,
        }
    }

    #[test]
    fn a_copy_spec_round_trips_through_the_ipc_shape() {
        let raw = serde_json::json!({
            "source_topic": "orders",
            "dest_profile_id": "prof-2",
            "dest_topic": "orders-replay",
            "seek": {"kind": "latest", "last_n": 100},
            "partitions": [0, 1],
            "filter": {"substring": "failed", "cel": null},
            "max_messages": 500,
            "rate_per_sec": 100,
            "preserve_partition": false,
            "provenance_headers": true,
        });
        let parsed: CopySpec = serde_json::from_value(raw.clone()).expect("parses");

        assert_eq!(parsed.source_topic, "orders");
        assert_eq!(parsed.seek, SeekSpec::Latest { last_n: 100 });
        assert_eq!(parsed.partitions, Some(vec![0, 1]));
        assert_eq!(
            parsed.filter.as_ref().unwrap().substring.as_deref(),
            Some("failed")
        );
        assert_eq!(parsed.max_messages, Some(500));
        assert_eq!(parsed.rate_per_sec, Some(100));
        assert!(!parsed.preserve_partition);
        assert_eq!(serde_json::to_value(&parsed).unwrap(), raw);
    }

    #[test]
    fn null_optionals_mean_everything_unfiltered_and_unpaced() {
        let parsed: CopySpec = serde_json::from_value(serde_json::json!({
            "source_topic": "orders",
            "dest_profile_id": "prof-2",
            "dest_topic": "orders-replay",
            "seek": {"kind": "earliest"},
            "partitions": null,
            "filter": null,
            "max_messages": null,
            "rate_per_sec": null,
            "preserve_partition": true,
            "provenance_headers": false,
        }))
        .expect("parses");
        assert_eq!(parsed, spec_with(|s| s.provenance_headers = false));
        assert!(parsed.filter.is_none());
    }

    fn spec_with(edit: impl FnOnce(&mut CopySpec)) -> CopySpec {
        let mut spec = spec();
        edit(&mut spec);
        spec
    }

    #[test]
    fn the_progress_payload_is_the_event_payload() {
        assert_eq!(
            serde_json::to_value(CopyProgress {
                copied: 4218,
                scanned: 4300,
                failed: 0,
                done: false,
                error: None,
                assumed_complete: Vec::new(),
            })
            .unwrap(),
            serde_json::json!({
                "copied": 4218,
                "scanned": 4300,
                "failed": 0,
                "done": false,
                "error": null,
                "assumed_complete": [],
            })
        );
    }

    /// The honesty field carries partition ids, so it has to survive the wire
    /// as a list rather than as a count — "partition 3 went quiet" is the fact,
    /// and "one partition went quiet" is not the same sentence.
    #[test]
    fn a_copy_that_finished_on_silence_names_the_partitions() {
        assert_eq!(
            serde_json::to_value(CopyProgress {
                copied: 900,
                scanned: 900,
                failed: 0,
                done: true,
                error: None,
                assumed_complete: vec![3, 5],
            })
            .unwrap(),
            serde_json::json!({
                "copied": 900,
                "scanned": 900,
                "failed": 0,
                "done": true,
                "error": null,
                "assumed_complete": [3, 5],
            })
        );
    }

    #[test]
    fn a_dry_run_says_whether_its_number_is_only_an_estimate() {
        assert_eq!(
            serde_json::to_value(CopyEstimate {
                would_copy_estimate: 4218,
                estimate_only: true,
                per_partition: vec![PartitionProgress {
                    partition: 0,
                    current_offset: 12,
                    end_offset: 1_000,
                }],
            })
            .unwrap(),
            serde_json::json!({
                "would_copy_estimate": 4218,
                "estimate_only": true,
                "per_partition": [
                    {"partition": 0, "current_offset": 12, "end_offset": 1000}
                ],
            })
        );
    }

    #[test]
    fn the_diff_and_migration_rows_match_the_contract() {
        assert_eq!(
            serde_json::to_value(ConfigDiffRow {
                name: "retention.ms".into(),
                a: Some("604800000".into()),
                b: None,
                a_is_default: false,
                b_is_default: false,
                differs: true,
            })
            .unwrap(),
            serde_json::json!({
                "name": "retention.ms",
                "a": "604800000",
                "b": null,
                "a_is_default": false,
                "b_is_default": false,
                "differs": true,
            })
        );
        assert_eq!(
            serde_json::to_value(OffsetMigrationRow {
                partition: 3,
                source_committed: Some(8412),
                source_ts_ms: Some(1_700_000_000_000),
                dest_offset: Some(77),
                method: METHOD_TIMESTAMP.into(),
            })
            .unwrap(),
            serde_json::json!({
                "partition": 3,
                "source_committed": 8412,
                "source_ts_ms": 1_700_000_000_000i64,
                "dest_offset": 77,
                "method": "timestamp",
            })
        );
    }
}

#[cfg(test)]
mod provenance {
    use super::*;

    #[test]
    fn the_header_names_are_the_contract() {
        let headers = provenance_headers("orders-prod", "orders", 3, 8412, Some(1_700_000_000_000));
        let pairs: Vec<(&str, &str)> = headers
            .iter()
            .map(|header| (header.key.as_str(), header.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("kavka.replay.source.cluster", "orders-prod"),
                ("kavka.replay.source.topic", "orders"),
                ("kavka.replay.source.partition", "3"),
                ("kavka.replay.source.offset", "8412"),
                ("kavka.replay.source.ts", "1700000000000"),
            ]
        );
    }

    /// A record from before KIP-32 has no timestamp, and an invented one would
    /// be indistinguishable from a real one for anybody reading it later.
    #[test]
    fn a_record_with_no_timestamp_gets_no_timestamp_header() {
        let headers = provenance_headers("dev", "orders", 0, 0, None);
        assert_eq!(headers.len(), 4);
        assert!(headers.iter().all(|h| h.key != PROVENANCE_TIMESTAMP));
        // Offset 0 and partition 0 are values, not absences.
        assert_eq!(headers[2].value, "0");
        assert_eq!(headers[3].value, "0");
    }
}

#[cfg(test)]
mod mapping {
    use super::*;

    #[test]
    fn a_wider_or_equal_destination_is_fine() {
        assert!(ensure_partition_mapping("orders", 6, "orders-replay", 6).is_ok());
        assert!(ensure_partition_mapping("orders", 6, "orders-replay", 12).is_ok());
        assert!(ensure_partition_mapping("orders", 1, "orders-replay", 1).is_ok());
    }

    #[test]
    fn a_narrower_destination_names_both_counts_and_both_ways_out() {
        let error = ensure_partition_mapping("orders", 6, "orders-replay", 3)
            .expect_err("3 < 6")
            .to_string();
        // Both numbers: what the copy needs, and what the destination has.
        assert!(
            error.contains("as high as partition 5 of orders"),
            "got {error}"
        );
        assert!(
            error.contains("orders-replay has 3 partitions"),
            "got {error}"
        );
        assert!(
            error.contains("at least 6 on the destination"),
            "got {error}"
        );
        // The two fixes, both stated.
        assert!(
            error.contains("Recreate orders-replay with 6"),
            "got {error}"
        );
        assert!(error.contains("keep the same partition"), "got {error}");
    }

    /// THE POINT OF `partitions_needed`: a copy's reach is the highest index it
    /// will write to, not how wide the source happens to be. Copying partition
    /// 0 of a six-partition topic into a one-partition destination is a
    /// perfectly good copy, and refusing it is a guardrail about a partition no
    /// record is going to.
    #[test]
    fn what_the_copy_reaches_is_the_highest_partition_in_scope() {
        assert_eq!(partitions_needed(&[0]), 1);
        assert_eq!(partitions_needed(&[0, 1, 2]), 3);
        // Not the count — the highest index plus one, so a gap still needs the
        // destination to be wide enough for the top of it.
        assert_eq!(partitions_needed(&[5]), 6);
        assert_eq!(partitions_needed(&[0, 5]), 6);
        // Nothing in scope is a copy no destination can be too narrow for.
        assert_eq!(partitions_needed(&[]), 0);
        assert!(ensure_partition_mapping("orders", partitions_needed(&[0]), "one", 1).is_ok());
        assert!(ensure_partition_mapping("orders", partitions_needed(&[5]), "one", 1).is_err());
    }
}

/// The deadline that decides when a copy has read everything readable. Its
/// whole correctness is "which waits count", so it is tested against a clock it
/// is handed rather than by sleeping through five seconds of it.
#[cfg(test)]
mod silence {
    use super::*;

    #[test]
    fn the_first_record_gets_the_long_cold_start_budget() {
        let start = Instant::now();
        let silence = SourceSilence::waiting_for_first(start);
        assert!(!silence.expired(start + QUIET_BEFORE_DATA - Duration::from_millis(1)));
        assert!(silence.expired(start + QUIET_BEFORE_DATA));
    }

    #[test]
    fn a_record_restarts_the_budget_from_when_it_arrived() {
        let start = Instant::now();
        let mut silence = SourceSilence::waiting_for_first(start);
        let arrived = start + Duration::from_secs(9);
        silence.heard_from_source(arrived);
        // The cold-start deadline is gone; the after-data one runs from here.
        assert!(!silence.expired(start + QUIET_BEFORE_DATA));
        assert!(!silence.expired(arrived + QUIET_AFTER_DATA - Duration::from_millis(1)));
        assert!(silence.expired(arrived + QUIET_AFTER_DATA));
    }

    /// THE BUG THIS TYPE EXISTS FOR. A copy paced at one record per second, or
    /// one whose destination sits in the in-flight drain, spends real time
    /// between polls — and none of it is the source going quiet. Without the
    /// re-arm the budget runs out mid-copy and every unread partition is
    /// silently marked finished.
    #[test]
    fn waiting_on_the_destination_never_spends_the_sources_budget() {
        let start = Instant::now();
        let mut silence = SourceSilence::waiting_for_first(start);
        let mut now = start;

        // Twenty records, each of them a full QUIET_AFTER_DATA + change spent
        // pacing and draining — four times the whole budget, twenty times over.
        for _ in 0..20 {
            silence.heard_from_source(now);
            now += QUIET_AFTER_DATA * 4;
            silence.waited_on_destination(now);
            assert!(
                !silence.expired(now),
                "a wait on the destination expired the source's deadline"
            );
        }

        // And the source genuinely going quiet still ends it, on schedule.
        assert!(!silence.expired(now + QUIET_AFTER_DATA - Duration::from_millis(1)));
        assert!(silence.expired(now + QUIET_AFTER_DATA));
    }

    /// THE SAME BUG, ON THE THIRD EDGE. A CEL filter decodes through the schema
    /// registry, and one lookup there is bounded by an HTTP timeout of 10s —
    /// twice the whole budget for a single record. Worse than the destination
    /// case: a record the filter rejects continues straight back to the expiry
    /// check with no pacing gap and no drain in between, so nothing else on the
    /// loop would ever re-arm the deadline.
    #[test]
    fn a_slow_decode_never_spends_the_sources_budget() {
        let start = Instant::now();
        let mut silence = SourceSilence::waiting_for_first(start);
        let mut now = start;

        // Ten records in a row that the filter throws away, each one having sat
        // on a registry lookup for its full timeout.
        for _ in 0..10 {
            silence.heard_from_source(now);
            now += Duration::from_secs(10);
            silence.waited_on_decode(now);
            assert!(
                !silence.expired(now),
                "a decode expired the source's deadline"
            );
        }

        // And the two kinds of wait compose: a record that decoded slowly and
        // was then paced slowly is still not the source going quiet.
        silence.heard_from_source(now);
        now += Duration::from_secs(10);
        silence.waited_on_decode(now);
        now += QUIET_AFTER_DATA * 4;
        silence.waited_on_destination(now);
        assert!(!silence.expired(now));

        // The source genuinely going quiet still ends it, on schedule.
        assert!(!silence.expired(now + QUIET_AFTER_DATA - Duration::from_millis(1)));
        assert!(silence.expired(now + QUIET_AFTER_DATA));
    }
}

/// Is the destination the same cluster as the source? Pure, because the answer
/// decides whether a copy is refused and "it depends on a live broker" is not
/// something a refusal rule should have to be.
#[cfg(test)]
mod identity {
    use super::*;

    fn servers(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_brokers_own_id_settles_it_whenever_both_sides_have_one() {
        // Different addresses, same cluster — a VIP and a broker of the same
        // cluster, which the addresses alone would call two clusters.
        assert!(same_cluster(
            Some("MkU3OEVBNTcwNTJENDM2Qk"),
            &servers(&["kafka-vip:9092"]),
            Some("MkU3OEVBNTcwNTJENDM2Qk"),
            &servers(&["broker-2.internal:9092"]),
        ));
        // Same addresses spelled identically, different ids — two clusters that
        // have been behind that name at different times.
        assert!(!same_cluster(
            Some("cluster-a"),
            &servers(&["kafka:9092"]),
            Some("cluster-b"),
            &servers(&["kafka:9092"]),
        ));
    }

    #[test]
    fn without_an_id_one_shared_broker_address_is_enough() {
        // A profile naming one broker and a profile naming two others of the
        // same cluster overlap on nothing — this is the case the fallback
        // cannot see, and it is documented rather than pretended away.
        assert!(!same_cluster(
            None,
            &servers(&["broker-1:9092"]),
            None,
            &servers(&["broker-2:9092", "broker-3:9092"]),
        ));
        // Overlapping at all is conclusive: a broker address belongs to exactly
        // one cluster.
        assert!(same_cluster(
            None,
            &servers(&["broker-1:9092", "broker-2:9092"]),
            None,
            &servers(&["broker-2:9092", "broker-3:9092"]),
        ));
    }

    #[test]
    fn addresses_are_compared_after_a_scheme_a_case_and_a_default_port() {
        assert!(same_cluster(
            None,
            &servers(&["PLAINTEXT://Kafka.Internal:9092"]),
            None,
            &servers(&["kafka.internal"]),
        ));
        assert!(same_cluster(
            None,
            &servers(&[" kafka.internal:9092 "]),
            None,
            &servers(&["SSL://kafka.internal:9092"]),
        ));
        // A different port is a different broker.
        assert!(!same_cluster(
            None,
            &servers(&["kafka.internal:9092"]),
            None,
            &servers(&["kafka.internal:9093"]),
        ));
    }

    /// One id and one absence cannot be compared as ids, so the addresses
    /// decide — which is the case a read-only destination profile with a slow
    /// broker actually lands in.
    #[test]
    fn one_missing_id_falls_back_to_the_addresses() {
        assert!(same_cluster(
            Some("cluster-a"),
            &servers(&["kafka:9092"]),
            None,
            &servers(&["kafka:9092"]),
        ));
        assert!(!same_cluster(
            Some("cluster-a"),
            &servers(&["kafka:9092"]),
            None,
            &servers(&["elsewhere:9092"]),
        ));
    }

    #[test]
    fn a_topic_copied_onto_itself_on_one_cluster_is_refused_by_the_growth_it_causes() {
        let error = ensure_not_self_copy(
            "orders",
            "orders",
            Some("cluster-a"),
            &servers(&["kafka:9092"]),
            Some("cluster-a"),
            // A SECOND PROFILE at the same cluster — the case a profile-id
            // comparison waves straight through.
            &servers(&["kafka:9092"]),
        )
        .expect_err("orders into orders on one cluster")
        .to_string();
        assert!(
            error.contains("both the source and the destination"),
            "got {error}"
        );
        assert!(
            error.contains("lands back in the log it is reading"),
            "got {error}"
        );
        assert!(
            error.contains("grows for as long as the copy runs"),
            "got {error}"
        );
        // The two ways out, as every §7 refusal owes.
        assert!(error.contains("Copy into a different topic"), "got {error}");
        assert!(error.contains("different cluster"), "got {error}");
    }

    #[test]
    fn the_same_topic_name_on_another_cluster_is_the_normal_case() {
        assert!(ensure_not_self_copy(
            "orders",
            "orders",
            Some("staging"),
            &servers(&["staging:9092"]),
            Some("prod"),
            &servers(&["prod:9092"]),
        )
        .is_ok());
    }

    #[test]
    fn a_replay_into_another_topic_on_the_same_cluster_is_allowed() {
        assert!(ensure_not_self_copy(
            "orders",
            "orders-replay",
            Some("cluster-a"),
            &servers(&["kafka:9092"]),
            Some("cluster-a"),
            &servers(&["kafka:9092"]),
        )
        .is_ok());
    }
}

#[cfg(test)]
mod rate {
    use super::*;

    #[test]
    fn no_rate_is_no_limiter() {
        assert!(RateLimiter::new(0, Instant::now()).is_none());
    }

    /// One second of burst, then the sustained rate — so a small copy is
    /// instant and a long one is paced.
    #[test]
    fn the_first_second_of_records_is_free_and_the_rest_is_paced() {
        let start = Instant::now();
        let mut bucket = RateLimiter::new(10, start).expect("a rate");

        for i in 0..10 {
            assert_eq!(bucket.take(start), Duration::ZERO, "burst record {i}");
        }
        // The eleventh has to wait for a token to be minted: 1/10th of a second.
        let wait = bucket.take(start);
        assert!(
            (wait.as_secs_f64() - 0.1).abs() < 1e-6,
            "expected 100ms, got {wait:?}"
        );
        // ...and the twelfth waits for the one after that, rather than starting
        // its own 100ms from `now` all over again.
        let next = bucket.take(start);
        assert!(
            (next.as_secs_f64() - 0.2).abs() < 1e-6,
            "waits queue up: got {next:?}"
        );
    }

    /// THE FLOOR THE INTEGRATION TEST ASSERTS: `n` records at `rate` per second
    /// cannot finish sooner than `(n - rate) / rate` seconds.
    ///
    /// Modelled the way the copy actually runs it — take, wait, take again —
    /// with a caller that spends no time on anything else, which is the fastest
    /// a paced run can possibly be.
    #[test]
    fn a_long_run_holds_the_sustained_rate() {
        let start = Instant::now();
        let mut bucket = RateLimiter::new(10, start).expect("a rate");
        let mut clock = start;
        for _ in 0..30 {
            clock += bucket.take(clock);
        }
        let total = clock.saturating_duration_since(start);
        assert!(
            total.as_secs_f64() >= 2.0 - 1e-9,
            "30 records at 10/s floor at 2s: got {total:?}"
        );
        assert!(
            total.as_secs_f64() < 2.001,
            "and the burst is not paid for twice: got {total:?}"
        );
    }

    /// Time passing between records is time the bucket refills in — a copy
    /// whose source is slow must not then be punished by the limiter.
    #[test]
    fn tokens_refill_while_the_source_is_slow() {
        let start = Instant::now();
        let mut bucket = RateLimiter::new(10, start).expect("a rate");
        for _ in 0..10 {
            bucket.take(start);
        }
        // A whole second later the bucket is full again.
        let later = start + Duration::from_secs(1);
        for i in 0..10 {
            assert_eq!(bucket.take(later), Duration::ZERO, "refilled record {i}");
        }
    }
}

#[cfg(test)]
mod configs {
    use super::*;

    fn entry(name: &str, value: Option<&str>, is_default: bool) -> ConfigEntry {
        ConfigEntry {
            name: name.into(),
            value: value.map(str::to_string),
            is_default,
            is_read_only: false,
            is_sensitive: value.is_none(),
            source: if is_default {
                "DEFAULT_CONFIG".into()
            } else {
                "DYNAMIC_TOPIC_CONFIG".into()
            },
        }
    }

    fn row<'r>(rows: &'r [ConfigDiffRow], name: &str) -> &'r ConfigDiffRow {
        rows.iter().find(|row| row.name == name).expect(name)
    }

    #[test]
    fn the_noise_case_is_not_a_difference() {
        let a = vec![entry("retention.ms", Some("604800000"), true)];
        let b = vec![entry("retention.ms", Some("604800000"), true)];
        let rows = config_diff(&a, &b);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].differs, "both inherited, same value");
        assert!(rows[0].a_is_default && rows[0].b_is_default);
    }

    /// The provenance differs and the value does not. The row carries both
    /// facts, and only the value decides `differs`.
    #[test]
    fn set_on_one_side_and_inherited_on_the_other_is_not_a_difference() {
        let a = vec![entry("retention.ms", Some("604800000"), false)];
        let b = vec![entry("retention.ms", Some("604800000"), true)];
        let rows = config_diff(&a, &b);
        assert!(!rows[0].differs);
        assert!(!rows[0].a_is_default, "A set it deliberately");
        assert!(rows[0].b_is_default, "B inherits it");
    }

    /// Two clusters with different defaults is exactly the surprise a diff
    /// exists to surface, so "nobody set it" must not hide it.
    #[test]
    fn two_different_defaults_still_differ() {
        let a = vec![entry("retention.ms", Some("604800000"), true)];
        let b = vec![entry("retention.ms", Some("86400000"), true)];
        assert!(config_diff(&a, &b)[0].differs);
    }

    #[test]
    fn an_entry_only_one_side_has_is_a_difference() {
        let a = vec![entry("remote.storage.enable", Some("false"), true)];
        let rows = config_diff(&a, &[]);
        let row = row(&rows, "remote.storage.enable");
        assert!(row.differs);
        assert_eq!(row.a.as_deref(), Some("false"));
        assert_eq!(row.b, None);
        assert!(!row.b_is_default, "an absent entry inherits nothing");
    }

    /// Kafka sends neither value, so neither can be compared — and a diff that
    /// flagged every sensitive entry as "possibly different" would cry wolf on
    /// every row of every comparison.
    #[test]
    fn a_sensitive_entry_on_both_sides_reads_as_equal() {
        let a = vec![entry("ssl.keystore.password", None, false)];
        let b = vec![entry("ssl.keystore.password", None, false)];
        let rows = config_diff(&a, &b);
        assert!(!rows[0].differs);
        assert_eq!(rows[0].a, None);
        assert_eq!(rows[0].b, None);
    }

    #[test]
    fn rows_are_the_union_of_both_sides_sorted_by_name() {
        let a = vec![
            entry("retention.ms", Some("1"), false),
            entry("cleanup.policy", Some("delete"), true),
        ];
        let b = vec![
            entry("segment.bytes", Some("2"), false),
            entry("cleanup.policy", Some("compact"), false),
        ];
        let rows = config_diff(&a, &b);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["cleanup.policy", "retention.ms", "segment.bytes"]);
        assert!(rows.iter().all(|row| row.differs));
    }

    #[test]
    fn two_empty_configurations_diff_to_nothing() {
        assert!(config_diff(&[], &[]).is_empty());
    }
}

#[cfg(test)]
mod migration {
    use super::*;

    const DEST: DestPosition = DestPosition {
        earliest: 100,
        end: 900,
        at_timestamp: Some(500),
    };

    fn row(committed: Option<i64>, earliest: i64, end: i64, ts: Option<i64>) -> OffsetMigrationRow {
        let anchor = anchor(committed, earliest, end);
        migration_row(3, committed, anchor, ts, DEST)
    }

    #[test]
    fn a_group_that_never_committed_here_has_nothing_to_migrate() {
        let row = row(None, 0, 1_000, None);
        assert_eq!(row.method, METHOD_NONE);
        assert_eq!(row.dest_offset, None);
        assert_eq!(row.source_committed, None);
    }

    /// An empty source partition carries no evidence — not even a committed 0,
    /// which only says the group read the nothing that was there.
    #[test]
    fn an_empty_source_partition_is_none_whatever_was_committed() {
        assert_eq!(row(Some(0), 0, 0, None).method, METHOD_NONE);
        assert_eq!(row(Some(0), 0, 0, None).dest_offset, None);
        // A retention-trimmed partition whose log start caught up with its end.
        assert_eq!(row(Some(700), 700, 700, None).method, METHOD_NONE);
    }

    #[test]
    fn committed_at_the_log_start_means_start_from_the_beginning() {
        let start = row(Some(0), 0, 1_000, None);
        assert_eq!(start.method, METHOD_EARLIEST);
        assert_eq!(start.dest_offset, Some(DEST.earliest));

        // A compacted partition whose log start is far above zero, with a
        // commit that is now below it: the group has read nothing that exists.
        assert_eq!(row(Some(40), 500, 1_000, None).method, METHOD_EARLIEST);
    }

    #[test]
    fn committed_at_or_past_the_end_means_caught_up() {
        for committed in [1_000, 1_001, i64::MAX] {
            let row = row(Some(committed), 0, 1_000, None);
            assert_eq!(row.method, METHOD_LATEST, "committed {committed}");
            assert_eq!(row.dest_offset, Some(DEST.end));
        }
    }

    #[test]
    fn a_commit_in_the_middle_crosses_by_timestamp() {
        let row = row(Some(400), 0, 1_000, Some(1_700_000_000_000));
        assert_eq!(row.method, METHOD_TIMESTAMP);
        assert_eq!(row.dest_offset, DEST.at_timestamp);
        assert_eq!(row.source_ts_ms, Some(1_700_000_000_000));
        assert_eq!(row.source_committed, Some(400));
    }

    /// Nothing readable at or after the committed offset — transaction markers,
    /// or a compaction that removed it. "Everything readable has been read" is
    /// the honest reading, and it lands where a caught-up group lands.
    #[test]
    fn no_readable_record_at_the_commit_falls_back_to_the_end() {
        let row = row(Some(400), 0, 1_000, None);
        assert_eq!(row.method, METHOD_LATEST);
        assert_eq!(row.dest_offset, Some(DEST.end));
    }

    /// The destination has nothing that recent — every record it holds is older
    /// than the source group's position — so the group starts at the end and
    /// waits, rather than being handed the whole destination topic to reread.
    #[test]
    fn a_destination_with_nothing_that_recent_starts_at_its_end() {
        let dest = DestPosition {
            at_timestamp: None,
            ..DEST
        };
        let row = migration_row(
            3,
            Some(400),
            anchor(Some(400), 0, 1_000),
            Some(1_700_000_000_000),
            dest,
        );
        assert_eq!(row.method, METHOD_LATEST);
        assert_eq!(row.dest_offset, Some(dest.end));
    }

    /// The five cases, as one table — because the order of the checks *is* the
    /// specification, and a reordering that broke one of them would otherwise
    /// still pass four tests.
    #[test]
    fn the_anchor_rules_in_order() {
        assert_eq!(anchor(None, 0, 100), Anchor::Nothing);
        assert_eq!(anchor(Some(50), 100, 100), Anchor::Nothing);
        assert_eq!(anchor(Some(0), 0, 100), Anchor::Earliest);
        assert_eq!(anchor(Some(100), 0, 100), Anchor::Latest);
        assert_eq!(anchor(Some(50), 0, 100), Anchor::At(50));
    }
}

/// The shell keeps copies on the same books as tails, searches and bulk runs
/// (`SessionMap` in apps/desktop/src-tauri/src/lib.rs), which requires exactly
/// this: a session that can be held in shared state and stopped from a thread
/// other than the one draining it. Asserted at compile time, because the
/// alternative is discovering it in the shell as a trait-bound error a phase
/// later.
#[cfg(all(test, feature = "kafka"))]
mod session_shape {
    use super::*;

    #[test]
    fn a_copy_session_can_live_in_the_shells_session_map() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<CopySession>();
        assert_send_sync::<CopyProgress>();
        assert_send_sync::<CopyEstimate>();
    }
}

// ---------------------------------------------------------------------------
// Tests that need the local dev cluster (dev/docker-compose.yml).
//
//   docker compose -f dev/docker-compose.yml up -d --wait
//   KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl
//
// They live INSIDE the crate rather than in `tests/` because they seed their
// own fixtures with a producer, and rdkafka is an optional dependency of this
// crate — a dev-dependency on it would make plain `cargo test -p kavka-core`
// require CMake and break the bare-toolchain tier the feature split exists to
// protect (the same reason src/consume.rs keeps its tail test inline).
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "kafka"))]
mod it {
    use super::*;
    use crate::profiles::{AuthConfig, ConnectionProfile, Environment};
    use rdkafka::config::ClientConfig;
    use rdkafka::producer::BaseProducer;

    fn bootstrap() -> String {
        std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
    }

    fn integration() -> bool {
        if std::env::var("KAVKA_IT").is_err() {
            eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
            return false;
        }
        true
    }

    fn connection(id: &str, read_only: bool) -> ClusterConnection {
        ClusterConnection::connect(ConnectionProfile {
            id: id.into(),
            name: "local docker".into(),
            environment: Environment::Dev,
            bootstrap_servers: vec![bootstrap()],
            auth: AuthConfig::Plaintext,
            read_only,
            schema_registry: None,
            connect_clusters: Vec::new(),
            metrics_endpoint: None,
            sampler_interval_ms: None,
        })
        .expect("connect")
    }

    /// A topic name no other run can collide with. Deliberately not the `uuid`
    /// crate — see the same helper in src/consume.rs.
    fn unique_topic(what: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("kavka-it-{what}-{}-{nanos:x}", std::process::id())
    }

    /// `create_topic` returns when the controller accepts it; a producer and a
    /// consumer each want metadata that has actually propagated.
    fn await_topic(conn: &ClusterConnection, topic: &str, partitions: usize) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(detail) = crate::admin::topic_detail(conn, topic) {
                if detail.partitions.len() == partitions {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "{topic} never reported {partitions} partitions"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn make_topic(conn: &ClusterConnection, what: &str, partitions: u32) -> String {
        let topic = unique_topic(what);
        crate::admin::create_topic(conn, &topic, partitions, 1, &[]).expect("create topic");
        await_topic(conn, &topic, partitions as usize);
        topic
    }

    /// One record to seed, spelled in bytes so a test can assert byte identity
    /// against exactly what it produced.
    struct Seed {
        partition: i32,
        key: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        headers: Vec<(String, Option<Vec<u8>>)>,
        timestamp: i64,
    }

    /// One record as it came back off the wire — the raw form, because "the
    /// copy is identical" is a claim about bytes and not about renderings.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Raw {
        partition: i32,
        offset: i64,
        key: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        headers: Vec<(String, Option<Vec<u8>>)>,
        timestamp: Option<i64>,
    }

    impl Raw {
        fn read(message: &BorrowedMessage<'_>) -> Self {
            Self {
                partition: message.partition(),
                offset: message.offset(),
                key: message.key().map(<[u8]>::to_vec),
                value: message.payload().map(<[u8]>::to_vec),
                headers: message.headers().map_or_else(Vec::new, |headers| {
                    (0..headers.count())
                        .map(|i| {
                            let header = headers.get(i);
                            (header.key.to_string(), header.value.map(<[u8]>::to_vec))
                        })
                        .collect()
                }),
                timestamp: message.timestamp().to_millis(),
            }
        }

        /// Everything except where it landed — what an identity copy has to
        /// reproduce exactly.
        fn payload_identity(&self) -> (&Option<Vec<u8>>, &Option<Vec<u8>>, Option<i64>) {
            (&self.key, &self.value, self.timestamp)
        }

        fn header(&self, name: &str) -> Option<&[u8]> {
            self.headers
                .iter()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.as_deref())
        }

        fn header_text(&self, name: &str) -> Option<String> {
            self.header(name)
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        }
    }

    fn seed(topic: &str, records: &[Seed]) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .set("acks", "all")
            .create()
            .expect("producer");
        for record in records {
            let mut headers = OwnedHeaders::new_with_capacity(record.headers.len().max(1));
            for (key, value) in &record.headers {
                headers = headers.insert(Header {
                    key,
                    value: value.as_deref(),
                });
            }
            let mut outgoing: BaseRecord<'_, [u8], [u8]> = BaseRecord::to(topic)
                .partition(record.partition)
                .timestamp(record.timestamp);
            if let Some(key) = record.key.as_deref() {
                outgoing = outgoing.key(key);
            }
            if let Some(value) = record.value.as_deref() {
                outgoing = outgoing.payload(value);
            }
            if !record.headers.is_empty() {
                outgoing = outgoing.headers(headers);
            }
            producer
                .send(outgoing)
                .map_err(|(e, _)| e)
                .expect("seeding send");
        }
        producer.flush(Duration::from_secs(20)).expect("flush");
    }

    /// Seeds inside a COMMITTED TRANSACTION, which leaves a control record on
    /// the last offset of the partition.
    ///
    /// That is the one fixture that produces a topic whose end watermark no
    /// consumer can ever reach: the marker occupies an offset and is never
    /// delivered, so a watermark-driven loop waits forever and the quiet
    /// deadline is what finishes it. Exactly the case
    /// [`CopyProgress::assumed_complete`] exists to report.
    fn seed_transactional(topic: &str, records: &[Seed]) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .set("acks", "all")
            .set("transactional.id", unique_topic("tx"))
            .create()
            .expect("producer");
        producer
            .init_transactions(Duration::from_secs(30))
            .expect("init transactions");
        producer.begin_transaction().expect("begin");
        for record in records {
            let mut outgoing: BaseRecord<'_, [u8], [u8]> = BaseRecord::to(topic)
                .partition(record.partition)
                .timestamp(record.timestamp);
            if let Some(key) = record.key.as_deref() {
                outgoing = outgoing.key(key);
            }
            if let Some(value) = record.value.as_deref() {
                outgoing = outgoing.payload(value);
            }
            producer
                .send(outgoing)
                .map_err(|(e, _)| e)
                .expect("seeding send");
        }
        producer
            .commit_transaction(Duration::from_secs(30))
            .expect("commit the transaction");
    }

    /// How long a read-back waits for a destination to settle before it answers
    /// with whatever it could see. Generous, because it is only ever spent on a
    /// topic that is genuinely short.
    const SETTLE: Duration = Duration::from_secs(15);

    /// Everything a topic holds right now, raw — once it holds at least
    /// `expected` records, or once [`SETTLE`] runs out.
    ///
    /// The retry is the point. A read-back starts the instant the copy reported
    /// done, and "done" is the *producer's* acks=all view: the records are on
    /// the leader, but this is a brand-new consumer whose metadata and
    /// watermarks come from a fresh fetch that can still be a beat behind —
    /// more so under a full suite hammering one broker. A single
    /// `fetch_watermarks` treated as final is how a destination that holds
    /// eight records answers zero (low == high on every partition, so the
    /// helper returned empty without reading anything) or five of six, and the
    /// test then blames a copy whose own delivery counts were right.
    ///
    /// So: re-fetch, re-read, mirroring how [`await_topic`] settles metadata.
    /// On expiry it answers what it has rather than panicking, so the caller's
    /// own assertion fails with the real numbers on both sides.
    fn read_all(conn: &ClusterConnection, topic: &str, expected: usize) -> Vec<Raw> {
        let settle = Instant::now() + SETTLE;
        loop {
            let out = read_once(conn, topic, settle);
            if out.len() >= expected || Instant::now() >= settle {
                return out;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// One pass: fetch the watermarks as they stand and read every partition to
    /// them. `deadline` bounds the drain, so a watermark that reads high — an
    /// offset no consumer can be given — costs the settle budget rather than
    /// wedging the suite.
    fn read_once(conn: &ClusterConnection, topic: &str, deadline: Instant) -> Vec<Raw> {
        let consumer = conn.new_session_consumer().expect("consumer");
        let available = resolve_partitions(&consumer, topic, None)
            .expect("partitions")
            .available;
        let mut assignment = TopicPartitionList::new();
        let mut pending: HashMap<i32, i64> = HashMap::new();
        for partition in &available {
            let (low, high) = watermarks(&consumer, topic, *partition).expect("watermarks");
            if low < high {
                assignment
                    .add_partition_offset(topic, *partition, Offset::Offset(low))
                    .expect("assignment");
                pending.insert(*partition, high);
            }
        }
        if pending.is_empty() {
            return Vec::new();
        }
        consumer.assign(&assignment).expect("assign");

        let mut out = Vec::new();
        while !pending.is_empty() && Instant::now() < deadline {
            match consumer.poll(Duration::from_millis(250)) {
                None => continue,
                Some(Err(e)) => panic!("reading {topic}: {e}"),
                Some(Ok(message)) => {
                    let Some(&end) = pending.get(&message.partition()) else {
                        continue;
                    };
                    if message.offset() >= end {
                        pending.remove(&message.partition());
                        continue;
                    }
                    out.push(Raw::read(&message));
                    if message.offset() + 1 >= end {
                        pending.remove(&message.partition());
                    }
                }
            }
        }
        // Not an assertion: a partition still short here is exactly the case the
        // caller is retrying for, and on the last pass the caller's own
        // `assert_eq!` on the count says more than a panic in a helper would.
        out.sort_by_key(|record| (record.partition, record.offset));
        out
    }

    fn spec(source: &str, dest: &str) -> CopySpec {
        CopySpec {
            source_topic: source.into(),
            dest_profile_id: "it-copy-dest".into(),
            dest_topic: dest.into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            filter: None,
            max_messages: None,
            rate_per_sec: None,
            preserve_partition: true,
            provenance_headers: false,
        }
    }

    /// Runs a copy to completion (or to `deadline`) and answers its final
    /// progress. The shell's emitter loop, in three lines.
    fn run(session: &CopySession, deadline: Duration) -> CopyProgress {
        let stop_at = Instant::now() + deadline;
        loop {
            let progress = session.progress();
            if progress.done {
                return progress;
            }
            assert!(
                Instant::now() < stop_at,
                "the copy did not finish within {deadline:?}: {progress:?}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// 50 records over 3 partitions, with every shape a copy has to carry
    /// through untouched: text and binary keys, a JSON value, a value that is
    /// not UTF-8 at all, a tombstone, a text header, a binary header and a
    /// null-valued header.
    fn awkward_fixture() -> Vec<Seed> {
        let base = 1_700_000_000_000_i64;
        (0..50)
            .map(|i| {
                let partition = i % 3;
                let tombstone = i == 7;
                Seed {
                    partition,
                    key: Some(format!("order-{i}").into_bytes()),
                    value: if tombstone {
                        None
                    } else if i == 11 {
                        Some(vec![0xff, 0xfe, 0x00, 0x80, 0x41])
                    } else {
                        Some(format!(r#"{{"orderId":{i},"status":"{}"}}"#, status(i)).into_bytes())
                    },
                    headers: vec![
                        ("trace-id".to_string(), Some(format!("t-{i}").into_bytes())),
                        ("signature".to_string(), Some(vec![0x00, 0xff, i as u8])),
                        ("retry".to_string(), None),
                    ],
                    timestamp: base + i64::from(i) * 1_000,
                }
            })
            .collect()
    }

    fn status(i: i32) -> &'static str {
        if i % 5 == 0 {
            "failed"
        } else {
            "created"
        }
    }

    /// THE IDENTITY COPY. Byte-for-byte on key, value, headers (in order,
    /// including a null-valued one) and timestamp — and, with
    /// `preserve_partition`, on the partition index too.
    #[test]
    fn an_identity_copy_carries_the_bytes_the_headers_and_the_timestamps() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-src", 3);
        let dest = make_topic(&conn, "copy-dst", 3);
        seed(&source, &awkward_fixture());

        let outcome = {
            let session =
                CopySession::start(&conn, &conn, &spec(&source, &dest)).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let before = read_all(&conn, &source, 50);
        let after = read_all(&conn, &dest, 50);
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert_eq!(outcome.copied, 50, "every record: {outcome:?}");
        assert_eq!(outcome.scanned, 50);
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.error, None);
        assert_eq!(before.len(), 50);
        assert_eq!(after.len(), 50);

        for (source_record, copy) in before.iter().zip(&after) {
            assert_eq!(
                copy.payload_identity(),
                source_record.payload_identity(),
                "record {} of {}",
                source_record.offset,
                source_record.partition
            );
            assert_eq!(
                copy.headers, source_record.headers,
                "headers are carried in order, values and nulls included"
            );
            assert_eq!(
                copy.partition, source_record.partition,
                "preserve_partition keeps the index"
            );
        }
        // A tombstone stayed a tombstone rather than becoming an empty value.
        assert!(
            after.iter().any(|record| record.value.is_none()),
            "the tombstone survived"
        );
        assert!(
            after
                .iter()
                .all(|record| record.header(PROVENANCE_TOPIC).is_none()),
            "provenance was off, so nothing was added"
        );
    }

    /// The seeded fixture every other integration test in the repo is written
    /// against, and the dry run's promise about it.
    #[test]
    fn the_dry_run_matches_what_an_unfiltered_copy_moves() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let dest = make_topic(&conn, "copy-orders", 6);

        let spec = spec("orders", &dest);
        let estimate = copy_dry_run(&conn, &spec).expect("dry run");
        let outcome = {
            let session = CopySession::start(&conn, &conn, &spec).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let copied = read_all(&conn, &dest, 50);
        let _ = crate::admin::delete_topic(&conn, &dest);

        // 50 messages over 6 partitions, seeded by dev/docker-compose.yml.
        assert_eq!(estimate.would_copy_estimate, 50);
        assert!(
            !estimate.estimate_only,
            "with no filter the number is exact"
        );
        assert_eq!(estimate.per_partition.len(), 6);
        let window: i64 = estimate
            .per_partition
            .iter()
            .map(|p| p.end_offset - p.current_offset)
            .sum();
        assert_eq!(window, 50, "the per-partition windows add up to the whole");

        assert_eq!(
            outcome.copied, estimate.would_copy_estimate,
            "the dry run is the copy's own planner: {outcome:?}"
        );
        assert_eq!(copied.len(), 50);
    }

    #[test]
    fn provenance_headers_name_the_source_record() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let dest = make_topic(&conn, "copy-prov", 6);

        let mut spec = spec("orders", &dest);
        spec.provenance_headers = true;
        let outcome = {
            let session = CopySession::start(&conn, &conn, &spec).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let copied = read_all(&conn, &dest, 50);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert_eq!(outcome.copied, 50);
        assert_eq!(copied.len(), 50);
        for record in &copied {
            assert_eq!(
                record.header_text(PROVENANCE_CLUSTER).as_deref(),
                Some("local docker"),
                "the connection's name, as the sidebar shows it"
            );
            assert_eq!(
                record.header_text(PROVENANCE_TOPIC).as_deref(),
                Some("orders")
            );
            // The record kept its own partition, so the provenance has to agree
            // with where the copy landed.
            assert_eq!(
                record.header_text(PROVENANCE_PARTITION).as_deref(),
                Some(record.partition.to_string().as_str())
            );
            let offset: i64 = record
                .header_text(PROVENANCE_OFFSET)
                .expect("an offset header")
                .parse()
                .expect("a decimal offset");
            assert!(offset >= 0);
            assert!(
                record.header_text(PROVENANCE_TIMESTAMP).is_some(),
                "a record with a timestamp gets one"
            );
        }
    }

    #[test]
    fn a_filtered_copy_moves_only_what_matches_and_says_its_estimate_is_one() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-filter-src", 3);
        let dest = make_topic(&conn, "copy-filter-dst", 3);
        let fixture = awkward_fixture();
        seed(&source, &fixture);
        // 0, 5, 10 … 45 — every fifth record, minus the one that is a
        // tombstone or binary. Counted from the fixture rather than assumed.
        let expected = fixture
            .iter()
            .filter(|record| {
                record
                    .value
                    .as_deref()
                    .is_some_and(|bytes| String::from_utf8_lossy(bytes).contains("failed"))
            })
            .count() as u64;
        assert!(expected > 0 && expected < 50, "a real subset: {expected}");

        let mut spec = spec(&source, &dest);
        spec.filter = Some(SearchQuery {
            substring: Some("\"status\":\"failed\"".into()),
            cel: None,
        });
        let estimate = copy_dry_run(&conn, &spec).expect("dry run");
        let outcome = {
            let session = CopySession::start(&conn, &conn, &spec).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let copied = read_all(&conn, &dest, expected as usize);
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert!(
            estimate.estimate_only,
            "with a filter, the number is what will be scanned"
        );
        assert_eq!(
            estimate.would_copy_estimate, 50,
            "the scan window, not the matches"
        );
        assert_eq!(outcome.scanned, 50, "every record was read and judged");
        assert_eq!(outcome.copied, expected, "only the matches were written");
        assert_eq!(copied.len() as u64, expected);
        assert!(copied.iter().all(|record| record
            .value
            .as_deref()
            .is_some_and(|bytes| String::from_utf8_lossy(bytes).contains("failed"))));
    }

    /// The rate is a promise about the destination, so the run cannot beat its
    /// own floor: `(n - rate) / rate` seconds, with one second of burst.
    #[test]
    fn a_rate_limited_copy_cannot_finish_before_its_floor() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-rate-src", 1);
        let dest = make_topic(&conn, "copy-rate-dst", 1);
        let base = 1_700_000_000_000_i64;
        seed(
            &source,
            &(0..30)
                .map(|i| Seed {
                    partition: 0,
                    key: Some(format!("k-{i}").into_bytes()),
                    value: Some(format!("{i}").into_bytes()),
                    headers: Vec::new(),
                    timestamp: base + i64::from(i),
                })
                .collect::<Vec<_>>(),
        );

        let mut spec = spec(&source, &dest);
        spec.rate_per_sec = Some(10);
        let started = Instant::now();
        let outcome = {
            let session = CopySession::start(&conn, &conn, &spec).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let elapsed = started.elapsed();
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert_eq!(outcome.copied, 30, "paced, not dropped: {outcome:?}");
        // 30 records at 10/s with a 10-record burst: 20 records of pacing = 2s.
        assert!(
            elapsed >= Duration::from_millis(1_900),
            "a 10/s copy of 30 records cannot take {elapsed:?}"
        );
        assert!(
            outcome.assumed_complete.is_empty(),
            "pacing is not the source going quiet: {outcome:?}"
        );
    }

    /// THE RATE LIMIT MUST NOT TRUNCATE THE COPY IT PACES. Every record is a
    /// full second apart, so the run spends far longer waiting on the
    /// destination than the source is ever allowed to be quiet for — and the
    /// copy still reads its window to the end and reports nothing assumed.
    #[test]
    fn a_slowly_paced_copy_reads_its_whole_window_and_assumes_nothing() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-paced-src", 1);
        let dest = make_topic(&conn, "copy-paced-dst", 1);
        const COUNT: i32 = 8;
        let base = 1_700_000_000_000_i64;
        seed(
            &source,
            &(0..COUNT)
                .map(|i| Seed {
                    partition: 0,
                    key: Some(format!("k-{i}").into_bytes()),
                    value: Some(format!("{i}").into_bytes()),
                    headers: Vec::new(),
                    timestamp: base + i64::from(i),
                })
                .collect::<Vec<_>>(),
        );

        let mut spec = spec(&source, &dest);
        // One per second, one second of burst: seven paced records, seven
        // seconds — longer than QUIET_AFTER_DATA, spent entirely on pacing.
        spec.rate_per_sec = Some(1);
        let outcome = {
            let session = CopySession::start(&conn, &conn, &spec).expect("start the copy");
            run(&session, Duration::from_secs(90))
        };
        let copied = read_all(&conn, &dest, COUNT as usize);
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert_eq!(
            outcome.copied,
            u64::try_from(COUNT).unwrap(),
            "the pacing gap must not be spent out of the source's silence budget: {outcome:?}"
        );
        assert_eq!(outcome.scanned, u64::try_from(COUNT).unwrap());
        assert_eq!(copied.len(), COUNT as usize);
        assert!(
            outcome.assumed_complete.is_empty(),
            "every partition reached its watermark: {outcome:?}"
        );
        assert_eq!(outcome.error, None);
    }

    /// SOURCE SILENCE, SIMULATED, and the honest flag it must raise.
    ///
    /// A committed transaction leaves a control record on the last offset: the
    /// end watermark is one past the last record a consumer can ever be given,
    /// so the copy finishes on its quiet deadline with that partition short.
    /// The records that exist are all copied — and the progress says which
    /// partition it stopped guessing about, because the other explanation for
    /// the same silence is a broker that stopped answering.
    #[test]
    fn a_copy_that_ends_on_source_silence_names_the_partitions_it_assumed() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-quiet-src", 1);
        let dest = make_topic(&conn, "copy-quiet-dst", 1);
        const COUNT: i32 = 6;
        let base = 1_700_000_000_000_i64;
        seed_transactional(
            &source,
            &(0..COUNT)
                .map(|i| Seed {
                    partition: 0,
                    key: Some(format!("k-{i}").into_bytes()),
                    value: Some(format!("{i}").into_bytes()),
                    headers: Vec::new(),
                    timestamp: base + i64::from(i),
                })
                .collect::<Vec<_>>(),
        );

        let outcome = {
            let session =
                CopySession::start(&conn, &conn, &spec(&source, &dest)).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let copied = read_all(&conn, &dest, COUNT as usize);
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert_eq!(
            outcome.copied,
            u64::try_from(COUNT).unwrap(),
            "everything readable was moved: {outcome:?}"
        );
        assert_eq!(copied.len(), COUNT as usize);
        // The marker's offset is inside the captured window and can never be
        // read, so the partition finished on silence and has to say so.
        assert_eq!(
            outcome.assumed_complete,
            vec![0],
            "a partition short of its watermark is named: {outcome:?}"
        );
        // Not an error, and not a failure count: it is a caveat on a copy that
        // otherwise did exactly what it said.
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.failed, 0);
    }

    /// Stopping is not a failure: whatever reached the destination is counted,
    /// the session ends promptly, and nothing is left writing.
    #[test]
    fn a_cancelled_copy_stops_promptly_and_reports_what_it_moved() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-cancel-src", 1);
        let dest = make_topic(&conn, "copy-cancel-dst", 1);
        let base = 1_700_000_000_000_i64;
        seed(
            &source,
            &(0..200)
                .map(|i| Seed {
                    partition: 0,
                    key: Some(format!("k-{i}").into_bytes()),
                    value: Some(format!("{i}").into_bytes()),
                    headers: Vec::new(),
                    timestamp: base + i64::from(i),
                })
                .collect::<Vec<_>>(),
        );

        let mut spec = spec(&source, &dest);
        // Slow enough that a copy of 200 records takes ~19s, so stopping after
        // a moment provably lands in the middle of it.
        spec.rate_per_sec = Some(10);
        let outcome = {
            let session = CopySession::start(&conn, &conn, &spec).expect("start the copy");
            std::thread::sleep(Duration::from_millis(700));
            let stopped = Instant::now();
            session.stop();
            // Idempotent, and from this thread rather than the reader's.
            session.stop();
            let outcome = run(&session, Duration::from_secs(15));
            assert!(
                stopped.elapsed() < Duration::from_secs(10),
                "a stop has to land within a poll and a flush, not at the end"
            );
            outcome
        };
        // The copy stopped mid-flight, so what the destination must hold is what
        // the copy said it delivered — the read-back settles until it sees that
        // many, and the assertion below is what judges the two against
        // each other.
        let copied = read_all(&conn, &dest, outcome.copied as usize);
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert!(outcome.done);
        assert!(
            outcome.copied < 200,
            "a cancelled copy did not move everything: {outcome:?}"
        );
        assert_eq!(
            copied.len() as u64,
            outcome.copied,
            "the count is what the destination actually holds"
        );
    }

    /// D5, from the writing side: the check is in core and it happens before a
    /// producer exists.
    #[test]
    fn a_read_only_destination_is_refused_before_anything_is_produced() {
        if !integration() {
            return;
        }
        let writable = connection("it-copy", false);
        // As wide as `orders`, so the only thing that can refuse this copy is
        // the read-only flag.
        let dest = make_topic(&writable, "copy-readonly", 6);
        let read_only = connection("it-copy-ro", true);

        let error = CopySession::start(&read_only, &read_only, &spec("orders", &dest))
            .expect_err("a read-only destination")
            .to_string();
        // …and the same source is fine when the destination is writable, which
        // is the half that must NOT be refused.
        let ok = CopySession::start(&read_only, &writable, &spec("orders", &dest));
        drop(ok.expect("reading a read-only cluster is what read-only is for"));
        let _ = crate::admin::delete_topic(&writable, &dest);

        assert!(error.contains("read-only"), "got {error}");
        assert!(
            error.contains("copy messages into this cluster"),
            "got {error}"
        );
    }

    #[test]
    fn preserving_partitions_into_a_narrower_topic_is_refused_up_front() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let dest = make_topic(&conn, "copy-narrow", 2);

        // `orders` has 6 partitions; 2 cannot hold them.
        let mut narrow = spec("orders", &dest);
        narrow.preserve_partition = true;
        let error = CopySession::start(&conn, &conn, &narrow)
            .expect_err("2 < 6")
            .to_string();

        // Without the flag the same copy is fine — Kafka places by key.
        let mut placed = spec("orders", &dest);
        placed.preserve_partition = false;
        let outcome = {
            let session = CopySession::start(&conn, &conn, &placed).expect("start the copy");
            run(&session, Duration::from_secs(60))
        };
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert!(
            error.contains("as high as partition 5 of orders"),
            "got {error}"
        );
        assert!(error.contains("has 2 partitions"), "got {error}");
        assert_eq!(outcome.copied, 50, "{outcome:?}");
    }

    /// The mapping guard is about the partitions the copy WRITES TO, so a
    /// partition filter narrows what the destination has to be able to hold.
    #[test]
    fn a_partition_filter_narrows_what_the_destination_has_to_hold() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let source = make_topic(&conn, "copy-reach-src", 3);
        let dest = make_topic(&conn, "copy-reach-dst", 1);
        let base = 1_700_000_000_000_i64;
        seed(
            &source,
            &(0..6)
                .map(|i| Seed {
                    partition: i % 3,
                    key: Some(format!("k-{i}").into_bytes()),
                    value: Some(format!("{i}").into_bytes()),
                    headers: Vec::new(),
                    timestamp: base + i64::from(i),
                })
                .collect::<Vec<_>>(),
        );

        // Partition 0 only: the copy reaches partition 0, and one partition on
        // the destination is exactly enough for that.
        let mut only_zero = spec(&source, &dest);
        only_zero.preserve_partition = true;
        only_zero.partitions = Some(vec![0]);
        let outcome = {
            let session = CopySession::start(&conn, &conn, &only_zero).expect("0 fits in 1");
            run(&session, Duration::from_secs(60))
        };

        // Unfiltered, the same copy reaches partition 2 and cannot land.
        let mut unfiltered = spec(&source, &dest);
        unfiltered.preserve_partition = true;
        let error = CopySession::start(&conn, &conn, &unfiltered)
            .expect_err("1 < 3")
            .to_string();
        let _ = crate::admin::delete_topic(&conn, &source);
        let _ = crate::admin::delete_topic(&conn, &dest);

        assert_eq!(
            outcome.copied, 2,
            "the two records on partition 0: {outcome:?}"
        );
        assert_eq!(outcome.error, None);
        assert!(
            error.contains("as high as partition 2"),
            "the highest partition the copy would touch: got {error}"
        );
        assert!(
            error.contains("at least 3 on the destination"),
            "got {error}"
        );
    }

    /// M2, from the cluster: TWO PROFILES, ONE CLUSTER. The wizard's old guard
    /// compared profile ids and would have started this copy — which is a topic
    /// feeding itself.
    #[test]
    fn a_topic_copied_onto_itself_is_refused_across_two_profiles_at_one_cluster() {
        if !integration() {
            return;
        }
        let source = connection("it-copy-self-a", false);
        let dest = connection("it-copy-self-b", false);
        assert_ne!(
            source.profile().id,
            dest.profile().id,
            "two saved connections, one cluster — the case a profile-id check misses"
        );

        let error = CopySession::start(&source, &dest, &spec("orders", "orders"))
            .expect_err("orders into orders on one cluster")
            .to_string();
        assert!(
            error.contains("both the source and the destination"),
            "got {error}"
        );
        assert!(
            error.contains("lands back in the log it is reading"),
            "got {error}"
        );

        // A replay into ANOTHER topic across the same two profiles is the whole
        // point of the feature and must still start.
        let elsewhere = make_topic(&source, "copy-self-ok", 6);
        drop(
            CopySession::start(&source, &dest, &spec("orders", &elsewhere))
                .expect("a replay into another topic is not a self-copy"),
        );
        let _ = crate::admin::delete_topic(&source, &elsewhere);
    }

    #[test]
    fn an_unknown_destination_topic_is_named_rather_than_created() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let error = CopySession::start(&conn, &conn, &spec("orders", "kavka-no-such-topic"))
            .expect_err("no such destination")
            .to_string();
        assert!(error.contains("no topic called"), "got {error}");
    }

    /// Commits offsets for a group without joining it — the same static
    /// assignment a reset uses, which is how a test can create the "stopped
    /// application" state a migration is for.
    fn commit(conn: &ClusterConnection, group: &str, topic: &str, offsets: &[(i32, i64)]) {
        let consumer = conn.new_group_consumer(group).expect("group consumer");
        let partitions: Vec<i32> = offsets.iter().map(|(partition, _)| *partition).collect();
        consumer
            .assign(&partition_list(topic, &partitions, Offset::Invalid).expect("list"))
            .expect("assign");
        let mut target = TopicPartitionList::new();
        for (partition, offset) in offsets {
            target
                .add_partition_offset(topic, *partition, Offset::Offset(*offset))
                .expect("target");
        }
        consumer
            .commit(&target, CommitMode::Sync)
            .expect("seed the group's offsets");
    }

    /// THE MIGRATION, end to end: plan, apply, read back — between two groups
    /// on one cluster, which is the case whose answer is knowable exactly.
    /// Same topic on both sides means a timestamp lookup must land on the very
    /// offset the source group had, so the plan is checkable rather than merely
    /// plausible.
    #[test]
    fn a_group_migrates_to_another_group_by_timestamp() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let topic = make_topic(&conn, "migrate", 3);
        let base = 1_700_000_000_000_i64;
        // Ten records per partition, one second apart, so no two records share
        // a timestamp and a lookup has exactly one right answer.
        seed(
            &topic,
            &(0..3)
                .flat_map(|partition| {
                    (0..10).map(move |i| Seed {
                        partition,
                        key: Some(format!("p{partition}-{i}").into_bytes()),
                        value: Some(format!(r#"{{"n":{i}}}"#).into_bytes()),
                        headers: Vec::new(),
                        timestamp: base + i64::from(partition) * 60_000 + i64::from(i) * 1_000,
                    })
                })
                .collect::<Vec<_>>(),
        );

        let source_group = unique_topic("group-src");
        let dest_group = unique_topic("group-dst");
        // One partition mid-stream, one that read nothing, one caught up.
        commit(&conn, &source_group, &topic, &[(0, 5), (1, 0), (2, 10)]);

        let plan = offsets_migrate_plan(&conn, &source_group, &topic, &conn, &dest_group, &topic)
            .expect("plan");
        let applied =
            offsets_migrate_apply(&conn, &dest_group, &topic, &plan).expect("apply the plan");
        let read_back = {
            let consumer = conn
                .new_group_consumer(&dest_group)
                .expect("group consumer");
            committed_offsets(&consumer, &topic, &[0, 1, 2]).expect("read back")
        };
        let _ = crate::admin::delete_topic(&conn, &topic);

        assert_eq!(plan.len(), 3, "one row per partition: {plan:?}");
        let row = |partition: i32| {
            plan.iter()
                .find(|row| row.partition == partition)
                .unwrap_or_else(|| panic!("a row for partition {partition}"))
        };

        // Mid-stream: the timestamp of the record at offset 5 resolves to
        // offset 5 on the same topic, and nothing else could be right.
        assert_eq!(row(0).method, METHOD_TIMESTAMP);
        assert_eq!(row(0).source_committed, Some(5));
        assert_eq!(row(0).source_ts_ms, Some(base + 5_000));
        assert_eq!(row(0).dest_offset, Some(5));

        // Read nothing: the earliest, with no lookup at all.
        assert_eq!(row(1).method, METHOD_EARLIEST);
        assert_eq!(row(1).source_ts_ms, None);
        assert_eq!(row(1).dest_offset, Some(0));

        // Caught up: the end, and no timestamp exists to look up.
        assert_eq!(row(2).method, METHOD_LATEST);
        assert_eq!(row(2).source_committed, Some(10));
        assert_eq!(row(2).dest_offset, Some(10));

        // What the apply answered is what the coordinator has.
        let landed: BTreeMap<i32, Option<i64>> = applied
            .iter()
            .map(|offset| (offset.partition, offset.committed))
            .collect();
        assert_eq!(landed.get(&0), Some(&Some(5)));
        assert_eq!(landed.get(&1), Some(&Some(0)));
        assert_eq!(landed.get(&2), Some(&Some(10)));
        assert_eq!(read_back.get(&0), Some(&Some(5)));
        assert_eq!(read_back.get(&1), Some(&Some(0)));
        assert_eq!(read_back.get(&2), Some(&Some(10)));

        // Lag is computed from the same watermarks: partition 0 is five behind,
        // partition 2 is caught up.
        let lag: BTreeMap<i32, Option<i64>> = applied
            .iter()
            .map(|offset| (offset.partition, offset.lag))
            .collect();
        assert_eq!(lag.get(&0), Some(&Some(5)));
        assert_eq!(lag.get(&2), Some(&Some(0)));
    }

    /// A plan with nothing in it commits nothing rather than erroring — and a
    /// read-only destination is refused before it gets that far.
    #[test]
    fn an_empty_plan_is_a_no_op_and_read_only_still_refuses() {
        if !integration() {
            return;
        }
        let conn = connection("it-copy", false);
        let rows = vec![OffsetMigrationRow {
            partition: 0,
            source_committed: None,
            source_ts_ms: None,
            dest_offset: None,
            method: METHOD_NONE.into(),
        }];
        assert!(
            offsets_migrate_apply(&conn, "kavka-it-empty-plan", "orders", &rows)
                .expect("a no-op plan")
                .is_empty()
        );

        let read_only = connection("it-copy-ro", true);
        let error = offsets_migrate_apply(&read_only, "kavka-it-empty-plan", "orders", &rows)
            .expect_err("a read-only destination")
            .to_string();
        assert!(error.contains("read-only"), "got {error}");
    }
}
