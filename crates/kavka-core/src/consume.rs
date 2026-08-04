//! Bounded fetch and live tail — the read half of the message browser.
//!
//! Both sit on their own consumer from
//! [`ClusterConnection::new_session_consumer`], assign partitions by hand and
//! never join a consumer group: browsing a topic must not move anybody's
//! committed offsets, and a group member appearing and vanishing every time a
//! user clicks a topic is the kind of side effect that gets a tool banned from
//! production.
//!
//! Everything here BLOCKS, like the rest of the core (docs/ARCHITECTURE.md).
//! The Tauri shell wraps these in its `blocking()` helper.
//!
//! The unbounded streaming search (D3) is Phase 2 and lands in
//! [`crate::search`]; this module is the bounded, interactive half it will
//! reuse.

use crate::cancel::CancelToken;
use crate::connection::ClusterConnection;
use crate::profiles::SchemaRegistryConfig;
use crate::serdes::MessageRecord;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[cfg(feature = "kafka")]
use crate::connection::auth::KavkaClientContext;
#[cfg(feature = "kafka")]
use crate::serdes::{self, SharedDecoder, DEFAULT_MAX_VALUE_BYTES};
#[cfg(feature = "kafka")]
use crate::sr::SchemaRegistry;
#[cfg(feature = "kafka")]
use rdkafka::consumer::{BaseConsumer, Consumer};
#[cfg(feature = "kafka")]
use rdkafka::message::{BorrowedMessage, Headers, Message};
#[cfg(feature = "kafka")]
use rdkafka::{Offset, TopicPartitionList};
#[cfg(feature = "kafka")]
use std::collections::{HashMap, VecDeque};
#[cfg(feature = "kafka")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "kafka")]
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(feature = "kafka")]
use std::time::{Duration, Instant};

/// Hard cap on one fetch, applied in core rather than trusted from the caller.
/// The message table virtualizes, but the IPC hop does not: 2000 decoded
/// records is already a multi-megabyte JSON payload, and a UI bug asking for
/// 10 million must cost a truncated answer, not the app.
pub const MAX_MESSAGES: u32 = 2000;

/// Where a fetch starts.
///
/// `Latest { last_n }` names a window **per partition**, not overall — there is
/// no single global tail to take, so "the last 20" is read as the last 20 of
/// each partition. What comes *back* is bounded by
/// [`FetchSpec::max_messages`], and for this seek that bound keeps the newest
/// records of the topic overall; see [`fetch_messages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeekSpec {
    Earliest,
    Latest { last_n: u32 },
    Offset { partition: i32, offset: i64 },
    Timestamp { timestamp_ms: i64 },
}

/// One bounded read of a topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchSpec {
    pub topic: String,
    pub seek: SeekSpec,
    /// `None` = every partition.
    pub partitions: Option<Vec<i32>>,
    /// Capped at [`MAX_MESSAGES`].
    pub max_messages: u32,
    /// Display truncation only; `None` uses
    /// [`crate::serdes::DEFAULT_MAX_VALUE_BYTES`].
    pub max_value_bytes: Option<u32>,
}

/// The whole fetch gives up here regardless of progress. A browse is an
/// interactive action: an answer that arrives after 30 seconds is not an
/// answer, and the partial result is still useful.
#[cfg(feature = "kafka")]
const FETCH_DEADLINE: Duration = Duration::from_secs(30);

#[cfg(feature = "kafka")]
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long a fetch waits for its *first* record before concluding there is
/// nothing to read. Generous, because this covers the broker's own connect,
/// metadata and first-fetch latency on a cold cluster.
#[cfg(feature = "kafka")]
const QUIET_BEFORE_DATA: Duration = Duration::from_secs(5);

/// How long it waits between records once data is flowing.
///
/// The end watermarks captured at the start are the primary completion signal,
/// but they are not sufficient on their own: transaction markers and aborted
/// messages occupy offsets the consumer never receives, so on a transactional
/// topic `next_offset` can never reach `high` and a watermark-only loop waits
/// out the full deadline on a topic it has already read to the end. This is
/// the backstop that makes "never hang on a quiet topic" true.
#[cfg(feature = "kafka")]
const QUIET_AFTER_DATA: Duration = Duration::from_millis(1_500);

#[cfg(feature = "kafka")]
const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

/// Floor on the working buffer a `Latest` fetch holds while it decides which
/// records are actually the newest. Below `2 × max_messages` the buffer would
/// be re-sorted on nearly every record; below a floor, a small `max_messages`
/// would do it on every record of a wide topic.
#[cfg(feature = "kafka")]
const NEWEST_BUFFER_FLOOR: usize = 1_024;

/// Records a tail holds for a consumer that is not draining fast enough. At
/// the 30px row height of the message table this is roughly 150 screens of
/// scrollback — past that, the newest records matter and the oldest do not.
#[cfg(feature = "kafka")]
const TAIL_CAPACITY: usize = 5_000;

/// Most a single [`TailSession::next_batch`] hands back. Bounds the size of one
/// IPC event so a burst cannot produce a 5000-record payload the webview has to
/// parse in one frame.
#[cfg(feature = "kafka")]
const TAIL_BATCH: usize = 500;

/// Sets how quickly [`TailSession::stop`] takes effect: the pump notices the
/// flag between polls.
#[cfg(feature = "kafka")]
const TAIL_POLL: Duration = Duration::from_millis(200);

/// Reads a bounded window of a topic.
///
/// Returns records sorted by `(timestamp, partition, offset)` — the
/// cross-partition chronological order the message table shows. Partitions are
/// read concurrently by librdkafka and arrive interleaved, so without this the
/// same fetch renders in a different order every time.
///
/// # Which end of the window `max_messages` keeps
///
/// The result is chronological, so truncating it has to know which end of time
/// the caller asked about — and the two answers are opposite. **The rule:**
///
/// - **`Latest { last_n }` truncates from the FRONT and keeps the NEWEST
///   `max_messages`.** Each partition contributes a window of its own last
///   `last_n`, and the union of those windows provably contains the newest
///   `last_n` records of the topic overall: any record outside the union is
///   older than `last_n` records of its own partition, so at least `last_n`
///   records in the union are newer than it. The whole union is therefore read
///   before anything is dropped. Stopping the poll loop at `max_messages`
///   instead — the old behaviour — kept whichever partitions librdkafka
///   happened to deliver first, which is a partition-biased sample and not
///   "the newest N" under any reading.
/// - **`Earliest`, `Offset` and `Timestamp` truncate from the BACK and keep the
///   OLDEST `max_messages`.** Those seeks name a starting point and read
///   forward, so their contract is "the first N from there" and their read
///   window is open-ended — the whole rest of the topic — which is exactly why
///   the loop may stop as soon as it has enough.
///
/// The fetch stops at whichever comes first: enough records (see above), every
/// assigned partition reaching the end watermark captured when the fetch
/// began, a quiet period (see [`QUIET_AFTER_DATA`]), [`FETCH_DEADLINE`], or
/// `cancel`. It never waits for messages that did not exist when it started.
///
/// **A cancelled fetch returns `Ok` with what it had already read**, not an
/// error: cancellation is Kavka's own doing — the user asked a newer question —
/// so there is nothing to report, and the caller discards the stale answer
/// anyway.
#[cfg(feature = "kafka")]
pub fn fetch_messages(
    conn: &ClusterConnection,
    profile_sr: Option<&SchemaRegistryConfig>,
    spec: &FetchSpec,
    cancel: Option<&CancelToken>,
) -> Result<Vec<MessageRecord>> {
    let max_messages = spec.max_messages.min(MAX_MESSAGES) as usize;
    let max_display = spec
        .max_value_bytes
        .map_or(DEFAULT_MAX_VALUE_BYTES, |bytes| bytes as usize);
    // See the doc comment: only this seek keeps the tail of the sorted result.
    let keep_newest = matches!(spec.seek, SeekSpec::Latest { .. });
    let prune_at = max_messages.saturating_mul(2).max(NEWEST_BUFFER_FLOOR);

    let consumer = conn.new_session_consumer()?;
    let plan = seek_plan(&consumer, spec)?;
    if max_messages == 0 || plan.is_empty() {
        return Ok(Vec::new());
    }

    let mut assignment = TopicPartitionList::new();
    for slot in &plan {
        assignment
            .add_partition_offset(&spec.topic, slot.partition, Offset::Offset(slot.start))
            .map_err(|e| {
                Error::Other(format!(
                    "seeking {}[{}] to offset {}: {e}",
                    spec.topic, slot.partition, slot.start
                ))
            })?;
    }
    consumer
        .assign(&assignment)
        .map_err(|e| Error::Other(format!("assigning partitions of {}: {e}", spec.topic)))?;

    let registry = profile_sr.map(SchemaRegistry::new);
    // Asked once, before the first poll — the plugin set belongs to the
    // connection and was compiled when it opened
    // ([`ClusterConnection::decoder_for`]). `None` is the common case and the
    // whole cost of not having a plugin.
    let decoder = conn.decoder_for(&spec.topic);
    // partition -> the end watermark it must reach to be finished.
    let mut pending: HashMap<i32, i64> =
        plan.iter().map(|slot| (slot.partition, slot.end)).collect();
    let mut records: Vec<MessageRecord> = Vec::new();
    let deadline = Instant::now() + FETCH_DEADLINE;
    let mut quiet_deadline = Instant::now() + QUIET_BEFORE_DATA;

    // `keep_newest` reads the whole union of the per-partition windows — it
    // cannot know which records are the newest until it has seen all of them —
    // and prunes the buffer instead of stopping early.
    while !pending.is_empty() && (keep_newest || records.len() < max_messages) {
        let now = Instant::now();
        if now >= deadline || now >= quiet_deadline {
            break;
        }
        // Checked once per poll, which is also the worst-case latency of a
        // cancel. Whatever has been read so far is still returned.
        if cancel.is_some_and(CancelToken::is_cancelled) {
            tracing::debug!(topic = %spec.topic, "fetch cancelled; returning what it has");
            break;
        }
        match consumer.poll(POLL_INTERVAL) {
            None => continue,
            Some(Err(e)) => {
                return Err(Error::Other(format!("reading {}: {e}", spec.topic)));
            }
            Some(Ok(message)) => {
                quiet_deadline = Instant::now() + QUIET_AFTER_DATA;
                let partition = message.partition();
                let offset = message.offset();
                let Some(&end) = pending.get(&partition) else {
                    continue;
                };
                // Past the watermark captured at the start: a producer is
                // still writing. A bounded fetch answers about the topic as it
                // was when the user asked, so this partition is done.
                if offset >= end {
                    pending.remove(&partition);
                    continue;
                }
                records.push(record(
                    &message,
                    registry.as_ref(),
                    max_display,
                    decoder.as_ref(),
                ));
                if offset + 1 >= end {
                    pending.remove(&partition);
                }
                if keep_newest && records.len() >= prune_at {
                    // Safe to drop early: everything cut here is older than
                    // `max_messages` records already held, so it could not
                    // have survived the final truncation either. This is what
                    // bounds the buffer of a `Latest` fetch on a topic with
                    // many partitions, where the union is `last_n` × them.
                    sort_chronologically(&mut records);
                    let cut = records.len() - max_messages;
                    records.drain(..cut);
                }
            }
        }
    }

    if let Some(error) = registry.as_ref().and_then(SchemaRegistry::take_error) {
        // Once per fetch, not once per message (see src/sr.rs).
        tracing::warn!(
            topic = %spec.topic,
            %error,
            "schema registry unavailable; affected payloads are shown as hex"
        );
    }

    sort_chronologically(&mut records);
    if records.len() > max_messages {
        if keep_newest {
            // Truncate the FRONT: the tail of a chronological list is its
            // newest records.
            let cut = records.len() - max_messages;
            records.drain(..cut);
        } else {
            records.truncate(max_messages);
        }
    }
    Ok(records)
}

/// The cross-partition order every fetch answers in.
#[cfg(feature = "kafka")]
fn sort_chronologically(records: &mut [MessageRecord]) {
    records.sort_by_key(|r| {
        // A record with no timestamp predates KIP-32 (or the broker set
        // `log.message.timestamp.type` to something it then lost); sorting it
        // first keeps the order total and stable.
        (r.timestamp_ms.unwrap_or(i64::MIN), r.partition, r.offset)
    });
}

#[cfg(not(feature = "kafka"))]
pub fn fetch_messages(
    _conn: &ClusterConnection,
    _profile_sr: Option<&SchemaRegistryConfig>,
    _spec: &FetchSpec,
    _cancel: Option<&CancelToken>,
) -> Result<Vec<MessageRecord>> {
    Err(Error::Other("built without the `kafka` feature".into()))
}

/// One partition's read window: `[start, end)`, where `end` is the high
/// watermark captured before the first poll.
#[cfg(feature = "kafka")]
struct PartitionSlot {
    partition: i32,
    start: i64,
    end: i64,
}

/// Turns a [`SeekSpec`] into per-partition start offsets and the end
/// watermarks that bound the read. Partitions with nothing to read are dropped
/// here, so an empty plan means "this fetch is already complete" and the loop
/// never runs.
#[cfg(feature = "kafka")]
fn seek_plan(
    consumer: &BaseConsumer<KavkaClientContext>,
    spec: &FetchSpec,
) -> Result<Vec<PartitionSlot>> {
    let ResolvedPartitions {
        available,
        selected: partitions,
    } = resolve_partitions(consumer, &spec.topic, spec.partitions.as_deref())?;
    let mut plan = Vec::with_capacity(partitions.len());

    match spec.seek {
        // An explicit offset names its own partition; the partition filter, if
        // any, only has to agree with it.
        SeekSpec::Offset { partition, offset } => {
            // Existence is checked against what the TOPIC has, never against
            // the filter. Told "orders has no partition 3" while partition 3
            // is sitting there in the topic list, a user goes looking for a
            // cluster problem that does not exist — the filter is the thing in
            // their way, and it is the thing they can clear.
            if !available.contains(&partition) {
                return Err(Error::Other(format!(
                    "{} has no partition {partition} to seek in — it has {}, numbered 0 to {}",
                    spec.topic,
                    available.len(),
                    available.len().saturating_sub(1)
                )));
            }
            if !partitions.contains(&partition) {
                return Err(Error::Other(format!(
                    "partition {partition} isn't in the partition filter — clear the filter, or \
                     seek within it"
                )));
            }
            let (low, high) = watermarks(consumer, &spec.topic, partition)?;
            plan.push(PartitionSlot {
                partition,
                start: offset.clamp(low, high),
                end: high,
            });
        }
        SeekSpec::Timestamp { timestamp_ms } => {
            let mut request = TopicPartitionList::new();
            for partition in &partitions {
                request
                    .add_partition_offset(&spec.topic, *partition, Offset::Offset(timestamp_ms))
                    .map_err(|e| Error::Other(format!("building the timestamp lookup: {e}")))?;
            }
            let resolved = consumer
                .offsets_for_times(request, METADATA_TIMEOUT)
                .map_err(|e| {
                    Error::Other(format!(
                        "asking {} for the offsets at that time: {e}",
                        spec.topic
                    ))
                })?;
            for partition in &partitions {
                let (low, high) = watermarks(consumer, &spec.topic, *partition)?;
                // No offset at or after the timestamp resolves to the end —
                // i.e. nothing to read, which the retain below drops.
                let start = resolved
                    .find_partition(&spec.topic, *partition)
                    .and_then(|found| match found.offset() {
                        Offset::Offset(offset) => Some(offset),
                        _ => None,
                    })
                    .unwrap_or(high)
                    .clamp(low, high);
                plan.push(PartitionSlot {
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
            for partition in &partitions {
                let (low, high) = watermarks(consumer, &spec.topic, *partition)?;
                let start = match last_n {
                    // `low` guards a compacted or retention-trimmed partition
                    // whose earliest offset is far above zero.
                    Some(n) => high.saturating_sub(n).max(low),
                    None => low,
                };
                plan.push(PartitionSlot {
                    partition: *partition,
                    start,
                    end: high,
                });
            }
        }
    }

    plan.retain(|slot| slot.start < slot.end);
    Ok(plan)
}

/// What a topic has, and what this fetch will read of it.
///
/// Both halves are kept because an error about one partition has to be able to
/// tell "this topic has no partition 3" apart from "partition 3 exists, but
/// your filter excluded it" — two different mistakes with two different fixes.
#[cfg(feature = "kafka")]
struct ResolvedPartitions {
    /// Every partition the topic has, ascending.
    available: Vec<i32>,
    /// The ones this fetch reads: all of `available`, or the caller's filter.
    selected: Vec<i32>,
}

/// The partitions a fetch or tail will read, validated against the cluster so
/// a stale UI cannot silently read nothing.
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

#[cfg(feature = "kafka")]
fn record(
    message: &BorrowedMessage<'_>,
    registry: Option<&SchemaRegistry>,
    max_display: usize,
    decoder: Option<&SharedDecoder>,
) -> MessageRecord {
    // Resolved once per record into the borrow `decode_with` takes; the plugin
    // itself was compiled when the connection opened
    // ([`ClusterConnection::decoder_for`]).
    let custom = decoder.map(|plugin| plugin.as_ref());
    let headers = message.headers().map_or_else(Vec::new, |headers| {
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
            .map(|bytes| serdes::decode_with(bytes, registry, max_display, custom)),
        // `None` here is a tombstone, not an empty value — the two are
        // different facts about a compacted topic.
        value: message
            .payload()
            .map(|bytes| serdes::decode_with(bytes, registry, max_display, custom)),
        // Before `headers` moves. `None` for every record that is not a dead
        // letter, which costs one pass over a short list and nothing on the
        // wire (see `MessageRecord::dlq`).
        dlq: serdes::dlq_inspect(&headers),
        masked: false,
        headers,
    }
}

/// A live tail of a topic: a dedicated consumer on a dedicated thread, feeding
/// a bounded ring the UI drains at its own pace.
///
/// Overflow drops the **oldest** records and counts them ([`Self::dropped`]).
/// A live tail is a window on now: when a topic outruns the reader, the
/// interesting records are the new ones, and blocking the pump to preserve old
/// ones would apply backpressure to a Kafka broker on behalf of a webview.
/// The drop count is reported so the UI can say so out loud rather than
/// silently showing a gappy stream.
#[cfg(feature = "kafka")]
pub struct TailSession {
    shared: Arc<TailShared>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "kafka")]
struct TailShared {
    queue: Mutex<TailQueue>,
    ready: Condvar,
    stopping: AtomicBool,
    dropped: AtomicU64,
}

#[cfg(feature = "kafka")]
#[derive(Default)]
struct TailQueue {
    records: VecDeque<MessageRecord>,
    /// The pump has exited; once `records` drains, this session is over.
    ended: bool,
}

#[cfg(feature = "kafka")]
impl TailSession {
    /// Starts tailing from the end of each partition.
    ///
    /// The consumer is created and assigned on the calling thread so a bad
    /// profile, an unreachable broker or an unknown topic is an error from
    /// `start`, not a session that reports "ended" a moment later with no
    /// explanation.
    pub fn start(
        conn: &ClusterConnection,
        profile_sr: Option<&SchemaRegistryConfig>,
        topic: &str,
        partitions: Option<&[i32]>,
    ) -> Result<Self> {
        let consumer = conn.new_session_consumer()?;
        let assigned = resolve_partitions(&consumer, topic, partitions)?.selected;

        let mut assignment = TopicPartitionList::new();
        for partition in &assigned {
            assignment
                .add_partition_offset(topic, *partition, Offset::End)
                .map_err(|e| {
                    Error::Other(format!("tailing {topic}[{partition}] from the end: {e}"))
                })?;
        }
        consumer
            .assign(&assignment)
            .map_err(|e| Error::Other(format!("assigning partitions of {topic}: {e}")))?;

        let registry = profile_sr.map(SchemaRegistry::new);
        // Resolved on this thread and MOVED into the pump: the tail outlives
        // this call, so it cannot borrow the connection.
        let decoder = conn.decoder_for(topic);
        let shared = Arc::new(TailShared {
            queue: Mutex::new(TailQueue::default()),
            ready: Condvar::new(),
            stopping: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
        });

        let worker = {
            let shared = Arc::clone(&shared);
            let topic = topic.to_string();
            std::thread::Builder::new()
                .name("kavka-tail".into())
                .spawn(move || {
                    pump(&consumer, registry.as_ref(), decoder, &shared, &topic);
                    shared.finish();
                })
                .map_err(|e| Error::Other(format!("starting the live tail thread: {e}")))?
        };

        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// Waits up to `timeout` for records.
    ///
    /// `None` means the session has ended and will produce nothing further.
    /// `Some(empty)` means the topic was simply quiet — a state the UI has to
    /// be able to say out loud (docs/DESIGN.md §7, "Live tail, quiet topic"),
    /// which is why it is not folded into `None`.
    pub fn next_batch(&self, timeout: Duration) -> Option<Vec<MessageRecord>> {
        let deadline = Instant::now() + timeout;
        let mut queue = self.shared.lock();
        loop {
            if !queue.records.is_empty() {
                let take = queue.records.len().min(TAIL_BATCH);
                return Some(queue.records.drain(..take).collect());
            }
            // Ordering matters: records buffered before the pump exited are
            // still delivered, and only a drained-and-ended queue is `None`.
            if queue.ended {
                return None;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Some(Vec::new());
            }
            let (guard, wait) = self
                .shared
                .ready
                .wait_timeout(queue, remaining)
                .unwrap_or_else(|e| e.into_inner());
            queue = guard;
            if wait.timed_out() && queue.records.is_empty() && !queue.ended {
                return Some(Vec::new());
            }
        }
    }

    /// Records dropped to overflow since the session started.
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Asks the pump to finish. Idempotent, and safe to call from any thread
    /// (the shell's `tail_stop` command runs on a different one from the
    /// reader). Returns immediately; the session ends within one poll interval.
    pub fn stop(&self) {
        self.shared.stopping.store(true, Ordering::Relaxed);
        self.shared.ready.notify_all();
    }
}

/// Deliberately not derived: the derived form would dump every buffered record
/// into any log line or panic message that touches a session.
#[cfg(feature = "kafka")]
impl std::fmt::Debug for TailSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let queue = self.shared.lock();
        f.debug_struct("TailSession")
            .field("buffered", &queue.records.len())
            .field("dropped", &self.shared.dropped.load(Ordering::Relaxed))
            .field("ended", &queue.ended)
            .finish()
    }
}

#[cfg(feature = "kafka")]
impl Drop for TailSession {
    /// Joins the pump so the consumer — and its broker connections — are gone
    /// before `drop` returns. A leaked tail thread on a dropped session is a
    /// consumer that keeps fetching from a cluster nobody is looking at.
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(feature = "kafka")]
impl TailShared {
    /// Poison-tolerant for the same reason as everywhere else in the crate: a
    /// panicking reader must not turn a live tail into a permanently locked
    /// one.
    fn lock(&self) -> MutexGuard<'_, TailQueue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, record: MessageRecord) {
        let mut queue = self.lock();
        if queue.records.len() >= TAIL_CAPACITY {
            queue.records.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        queue.records.push_back(record);
        drop(queue);
        self.ready.notify_all();
    }

    fn finish(&self) {
        self.lock().ended = true;
        self.ready.notify_all();
    }
}

#[cfg(feature = "kafka")]
fn pump(
    consumer: &BaseConsumer<KavkaClientContext>,
    registry: Option<&SchemaRegistry>,
    decoder: Option<SharedDecoder>,
    shared: &TailShared,
    topic: &str,
) {
    while !shared.stopping.load(Ordering::Relaxed) {
        match consumer.poll(TAIL_POLL) {
            None => {}
            Some(Ok(message)) => {
                shared.push(record(
                    &message,
                    registry,
                    DEFAULT_MAX_VALUE_BYTES,
                    decoder.as_ref(),
                ));
            }
            Some(Err(e)) => {
                // The session ends rather than spinning on a broker that is
                // not going to answer; the shell's final event tells the user,
                // and starting a new tail is one click.
                tracing::warn!(topic, error = %e, "live tail stopped");
                break;
            }
        }
    }
    if let Some(error) = registry.and_then(SchemaRegistry::take_error) {
        tracing::warn!(
            topic,
            %error,
            "schema registry unavailable; affected payloads are shown as hex"
        );
    }
}

/// The request shapes cross IPC, so their wire form is part of the contract
/// the TypeScript side is written against. Asserted here, and outside the
/// `kafka` gate, because a rename would otherwise compile and ship.
#[cfg(test)]
mod wire {
    use super::*;

    #[test]
    fn seek_specs_are_tagged_by_kind() {
        for (seek, expected) in [
            (SeekSpec::Earliest, serde_json::json!({"kind": "earliest"})),
            (
                SeekSpec::Latest { last_n: 20 },
                serde_json::json!({"kind": "latest", "last_n": 20}),
            ),
            (
                SeekSpec::Offset {
                    partition: 3,
                    offset: 8412,
                },
                serde_json::json!({"kind": "offset", "partition": 3, "offset": 8412}),
            ),
            (
                SeekSpec::Timestamp {
                    timestamp_ms: 1_700_000_000_000,
                },
                serde_json::json!({"kind": "timestamp", "timestamp_ms": 1_700_000_000_000i64}),
            ),
        ] {
            assert_eq!(serde_json::to_value(seek).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<SeekSpec>(expected).unwrap(),
                seek,
                "round trip"
            );
        }
    }

    #[test]
    fn a_fetch_spec_round_trips_through_the_ipc_shape() {
        let raw = serde_json::json!({
            "topic": "orders",
            "seek": {"kind": "latest", "last_n": 5},
            "partitions": [0, 1],
            "max_messages": 500,
            "max_value_bytes": 262144,
        });
        let spec: FetchSpec = serde_json::from_value(raw.clone()).expect("parses");

        assert_eq!(spec.topic, "orders");
        assert_eq!(spec.seek, SeekSpec::Latest { last_n: 5 });
        assert_eq!(spec.partitions, Some(vec![0, 1]));
        assert_eq!(spec.max_value_bytes, Some(262_144));
        assert_eq!(serde_json::to_value(&spec).unwrap(), raw);
    }

    #[test]
    fn null_partitions_and_value_cap_mean_all_and_default() {
        let spec: FetchSpec = serde_json::from_value(serde_json::json!({
            "topic": "orders",
            "seek": {"kind": "earliest"},
            "partitions": null,
            "max_messages": 100,
            "max_value_bytes": null,
        }))
        .expect("parses");
        assert_eq!(spec.partitions, None);
        assert_eq!(spec.max_value_bytes, None);
    }
}

/// The tail's produce-and-read roundtrip lives here rather than in
/// `tests/consume_local.rs` because it needs rdkafka's producer, and rdkafka is
/// a feature-gated dependency of this crate — a `dev-dependency` on it would
/// make plain `cargo test -p kavka-core` require CMake and break the
/// bare-toolchain tier the feature split exists to protect (see Cargo.toml).
#[cfg(all(test, feature = "kafka"))]
mod tests {
    use super::*;
    use crate::profiles::{AuthConfig, ConnectionProfile};
    use rdkafka::config::ClientConfig;
    use rdkafka::producer::{BaseProducer, BaseRecord, Producer};

    /// The tail test produces, so it uses `dead-letter` rather than `orders` —
    /// `tests/consume_local.rs` asserts the exact seeded contents of `orders`.
    const TAIL_TOPIC: &str = "dead-letter";

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

    fn connection(read_only: bool) -> ClusterConnection {
        ClusterConnection::connect(ConnectionProfile {
            id: "it-consume".into(),
            name: "local docker".into(),
            environment: "dev".into(),
            bootstrap_servers: vec![bootstrap()],
            auth: AuthConfig::Plaintext,
            read_only,
            schema_registry: None,
            connect_clusters: Vec::new(),
            metrics_endpoint: None,
            sampler_interval_ms: None,
            wasm_serdes: Vec::new(),
        })
        .expect("connect")
    }

    fn local_connection() -> ClusterConnection {
        connection(true)
    }

    /// A topic name no other run can collide with.
    ///
    /// Deliberately not the `uuid` crate: kavka-core has no uuid dependency and
    /// a test fixture is not worth adding one for. A pid plus the wall clock in
    /// nanoseconds is unique across concurrent runs on one machine and across
    /// machines sharing a cluster, which is the whole requirement.
    fn unique_topic(what: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("kavka-it-{what}-{}-{nanos:x}", std::process::id())
    }

    /// `create_topic` returns when the controller accepts it; the producer and
    /// the consumer each want metadata that has actually propagated.
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

    #[test]
    fn a_tail_delivers_records_produced_after_it_started() {
        if !integration() {
            return;
        }
        let conn = local_connection();
        let tail = TailSession::start(&conn, None, TAIL_TOPIC, None).expect("start tail");

        // Assigning at `Offset::End` resolves the end position when the fetcher
        // first asks the broker; producing before that lands *below* the
        // resolved position and would never be delivered. Two empty batches is
        // the settle, and it is not part of the 5s budget asserted below.
        for _ in 0..2 {
            let quiet = tail.next_batch(Duration::from_secs(1));
            assert!(
                quiet.is_some_and(|batch| batch.is_empty()),
                "the topic should be quiet, and the session live, before we produce"
            );
        }

        let marker = format!("tail-{}", std::process::id());
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .create()
            .expect("producer");
        for i in 1..=3 {
            let key = format!("{marker}-{i}");
            let payload = format!(r#"{{"n":{i},"marker":"{marker}"}}"#);
            producer
                .send(
                    BaseRecord::to(TAIL_TOPIC)
                        .key(&key)
                        .payload(&payload)
                        .partition(0),
                )
                .map_err(|(e, _)| e)
                .expect("send");
        }
        producer.flush(Duration::from_secs(10)).expect("flush");

        let mut seen: Vec<String> = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while seen.len() < 3 && Instant::now() < deadline {
            let Some(batch) = tail.next_batch(Duration::from_millis(250)) else {
                panic!("the tail ended before the produced records arrived");
            };
            for record in batch {
                let key = record.key.expect("keyed").text;
                if key.starts_with(&marker) {
                    let value = record.value.expect("not a tombstone");
                    assert_eq!(value.json.unwrap()["marker"], marker);
                    seen.push(key);
                }
            }
        }
        assert_eq!(seen.len(), 3, "delivered within 5s: {seen:?}");
        assert_eq!(tail.dropped(), 0);

        // Idempotent, and again after the session has already ended.
        tail.stop();
        tail.stop();
    }

    #[test]
    fn a_stopped_tail_ends_rather_than_hanging() {
        if !integration() {
            return;
        }
        let conn = local_connection();
        let tail = TailSession::start(&conn, None, TAIL_TOPIC, Some(&[0])).expect("start tail");
        tail.stop();

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "stop() never ended the session");
            if tail.next_batch(Duration::from_millis(250)).is_none() {
                break;
            }
        }
    }

    /// THE NEWEST-N INVARIANT.
    ///
    /// `Latest { last_n }` reads a window per partition, but `max_messages`
    /// bounds the answer overall — so on a topic whose partitions are skewed,
    /// the two must combine into "the newest `max_messages` records of the
    /// topic", never "whichever partitions answered first".
    ///
    /// The fixture is deliberately lopsided and therefore has to be its own
    /// topic: partitions 1 and 2 hold 30 old records each, partition 0 holds
    /// 20 recent ones, and partition 2 holds ONE record newer than all of
    /// them. That straggler is what makes the assertion order-independent —
    /// the correct answer spans two partitions, so no partition-biased sample
    /// can produce it by luck, whichever partition librdkafka happens to
    /// deliver first. A fetch that stops polling at `max_messages` returns
    /// whatever arrived; a fetch that keeps the HEAD of the sorted union
    /// returns the oldest. Only keeping the TAIL answers the question asked.
    #[test]
    fn latest_plus_max_messages_returns_the_newest_across_the_whole_topic() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let topic = unique_topic("newest");
        crate::admin::create_topic(&conn, &topic, 3, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 3);

        // One day back, so nothing is stamped in the future and no broker
        // timestamp policy has an opinion about it.
        let base = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_millis(),
        )
        .expect("millis fit i64")
            - 86_400_000;
        // The recent records sit an hour after every old one, so there is no
        // tie anywhere for the sort to break — and therefore no way for the
        // partition index to decide which records are "newest".
        let newest_base = base + 3_600_000;

        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .create()
            .expect("producer");
        let send = |partition: i32, key: &str, timestamp: i64, age: &str| {
            let payload = format!("{{\"age\":\"{age}\"}}");
            producer
                .send(
                    BaseRecord::to(&topic)
                        .key(key)
                        .payload(&payload)
                        .partition(partition)
                        .timestamp(timestamp),
                )
                .map_err(|(e, _)| e)
                .expect("send");
        };
        for partition in [1, 2] {
            for i in 0..30 {
                send(
                    partition,
                    &format!("old-{partition}-{i}"),
                    base + i64::from(i),
                    "old",
                );
            }
        }
        // 21 recent records over two partitions. The newest 20 of the topic
        // are therefore `new-1 … new-20`: `new-0` is the one that falls off.
        let mut recent: Vec<String> = Vec::new();
        for i in 0..20 {
            let key = format!("new-{i}");
            send(0, &key, newest_base + i64::from(i), "new");
            recent.push(key);
        }
        let straggler = "new-20".to_string();
        send(2, &straggler, newest_base + 20, "new");
        recent.push(straggler);
        producer.flush(Duration::from_secs(10)).expect("flush");

        let expected: Vec<String> = recent[1..].to_vec();

        let fetch = |max_messages: u32| {
            fetch_messages(
                &conn,
                None,
                &FetchSpec {
                    topic: topic.clone(),
                    seek: SeekSpec::Latest { last_n: 20 },
                    partitions: None,
                    max_messages,
                    max_value_bytes: None,
                },
                None,
            )
        };
        let twenty = fetch(20);
        let ten = fetch(10);
        let _ = crate::admin::delete_topic(&conn, &topic);

        let twenty = twenty.expect("fetch newest 20");
        let keys: Vec<String> = twenty
            .iter()
            .map(|r| r.key.as_ref().expect("keyed").text.clone())
            .collect();
        assert_eq!(
            keys, expected,
            "latest{{last_n:20}} + max_messages 20 must be the topic's newest 20, in order"
        );
        assert!(
            twenty.iter().all(|r| r.partition != 1),
            "partition 1 holds only old records: {:?}",
            twenty.iter().map(|r| r.partition).collect::<Vec<_>>()
        );
        assert_eq!(
            twenty.last().expect("20 records").partition,
            2,
            "the single newest record lives on partition 2 — the answer is not one partition's"
        );

        // The same rule one step tighter: a smaller cap cuts the OLD end.
        let ten = ten.expect("fetch newest 10");
        let ten_keys: Vec<String> = ten
            .iter()
            .map(|r| r.key.as_ref().expect("keyed").text.clone())
            .collect();
        assert_eq!(ten_keys, expected[10..], "max_messages truncates the front");
    }

    #[test]
    fn a_cancelled_fetch_returns_what_it_has_rather_than_an_error() {
        if !integration() {
            return;
        }
        let conn = local_connection();
        let cancel = CancelToken::new();
        cancel.cancel();
        let started = Instant::now();
        let records = fetch_messages(
            &conn,
            None,
            &FetchSpec {
                topic: "orders".into(),
                seek: SeekSpec::Earliest,
                partitions: None,
                max_messages: 500,
                max_value_bytes: None,
            },
            Some(&cancel),
        )
        .expect("a cancelled fetch is not an error");

        assert!(records.is_empty(), "cancelled before the first poll");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "cancellation is noticed within a poll, not at the deadline"
        );
    }

    #[test]
    fn tailing_an_unknown_topic_fails_at_start() {
        if !integration() {
            return;
        }
        let conn = local_connection();
        let err = TailSession::start(&conn, None, "kavka-no-such-topic", None)
            .expect_err("unknown topic")
            .to_string();
        assert!(err.contains("no topic called"), "got {err}");
    }
}
