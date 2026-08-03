//! Streaming unbounded search (docs/ARCHITECTURE.md D3) — the crown jewel.
//!
//! Parallel per-partition consumers, a raw-byte substring prefilter applied
//! *before* deserialization, a CEL expression applied *after* it, progressive
//! results over a bounded buffer, cooperative cancellation. No fetch-limit
//! pre-commit: a search runs to the end watermarks captured when it started,
//! and the result buffer filling up bounds what is *shown*, never what is
//! *counted*. That is the feature Offset Explorer's bounded search loses on,
//! and the Phase 2 acceptance gate ("search never silently truncates") is
//! precisely this distinction.
//!
//! # The shape of one search
//!
//! ```text
//!   start()  ─┬─ compile the filter          (a bad CEL expression fails HERE)
//!             ├─ resolve partitions + watermarks on one planning consumer
//!             └─ spawn min(partitions, 8) workers, each owning its consumer
//!                     │
//!   worker  ─────────►│ poll ─► raw bytes ─► substring? ─► decode ─► CEL? ─►
//!                     │            │             │                    │
//!                  scanned++    (skip)        (buffer if a slot is free)
//!                     │
//!   caller  ◄─ next_results(timeout) ◄─ bounded buffer ─► progress()
//! ```
//!
//! Everything here BLOCKS, like the rest of the core; the Tauri shell wraps it
//! in its `blocking()` helper. [`SearchSession`] follows
//! [`crate::consume::TailSession`]'s pattern exactly: consumers created and
//! assigned on the calling thread so a bad profile is an error from `start`,
//! `stop` idempotent and safe from any thread, and `Drop` joining the workers
//! so no session can leak a consumer that keeps fetching from a cluster nobody
//! is looking at.
//!
//! # What bounds a search
//!
//! **The end watermarks captured at start.** A search answers about the topic
//! as it was when the user asked: records produced *during* the scan sit at or
//! past the captured watermark and are skipped, and the partition is marked
//! complete instead. Without that a search of a busy topic never terminates,
//! and "scanned 412,000 of ~2.4M" — the progress line docs/DESIGN.md §7
//! specifies — would have no denominator. Live-tailing past the end is a
//! separate mode ([`crate::consume::TailSession`]), not a search.
//!
//! # What a filter can match
//!
//! **A filter can only match what it can read.** The substring prefilter reads
//! raw bytes — the key, the value **and every header's name and value** — so it
//! matches anything, including a payload no decoder can make sense of, which is
//! then shown as hex. That set is exactly what the search bar promises
//! ("Find in key, value or headers"), and the promise is kept here rather than
//! in the copy. A CEL expression reads the *decoded* record, so a record whose
//! value no decoder could read (it rendered as hex) is counted as scanned and
//! never matched while a CEL expression is in play: there is no honest answer
//! to `value.status == "failed"` about bytes we could not parse, and "no" is the
//! only safe one. The two halves of that rule are asserted in this module's
//! tests.
//!
//! **A CEL expression that cannot be evaluated is not a match and not a
//! failure.** A heterogeneous topic is the normal case, so a record the
//! expression could not judge is counted in [`SearchProgress::unevaluated`] and
//! the first message is kept verbatim in [`SearchProgress::filter_error`] —
//! visible, because a filter silently answering "no" for half a topic is the
//! same lie as a truncated result list.

use crate::consume::SeekSpec;
use crate::serdes::{Encoding, MessageRecord};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(feature = "kafka")]
use crate::cancel::CancelToken;
#[cfg(feature = "kafka")]
use crate::connection::auth::KavkaClientContext;
#[cfg(feature = "kafka")]
use crate::connection::ClusterConnection;
#[cfg(feature = "kafka")]
use crate::profiles::SchemaRegistryConfig;
#[cfg(feature = "kafka")]
use crate::serdes::{self, DEFAULT_MAX_VALUE_BYTES};
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
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};
#[cfg(feature = "kafka")]
use std::sync::{Condvar, Mutex, MutexGuard};
#[cfg(feature = "kafka")]
use std::time::{Duration, Instant};

/// Hard cap on the RESULT buffer, applied in core rather than trusted from the
/// caller — 10,000 decoded records is already a large IPC payload, and a UI bug
/// asking for ten million must cost a bounded answer, not the app.
///
/// **This bounds what is buffered, never what is matched.** Once the buffer is
/// full the scan keeps running and `matched` keeps climbing; the honest report
/// of that gap is what [`SearchProgress`] exists for.
pub const MAX_BUFFERED: u32 = 10_000;

/// Upper bound on worker threads, and therefore on the consumers (and broker
/// connections) one search opens. Past this, a wide topic would cost more in
/// fetch sessions and context switches than it buys in parallelism, and a
/// 1000-partition topic would open 1000 consumers.
pub const MAX_WORKERS: usize = 8;

/// What has to be true of a record for it to be a hit.
///
/// Both fields `None` matches everything (a plain "read the whole topic" scan);
/// both set is an **AND**, with the cheap half first — the substring runs on
/// raw bytes and can reject a record before anything is deserialized.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Case-insensitive (ASCII) substring, matched against the raw key, value
    /// and header **bytes** before any decoding.
    pub substring: Option<String>,
    /// CEL expression evaluated against the decoded record. See
    /// [`CelFilter`] for the activation it is given.
    pub cel: Option<String>,
}

/// One streaming search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSpec {
    pub topic: String,
    pub seek: SeekSpec,
    /// `None` = every partition.
    pub partitions: Option<Vec<i32>>,
    pub query: SearchQuery,
    /// Size of the result buffer, capped at [`MAX_BUFFERED`]. Matching does not
    /// stop here — see [`SearchProgress::buffered`].
    pub max_buffered: u32,
    /// Display truncation; `None` uses
    /// [`crate::serdes::DEFAULT_MAX_VALUE_BYTES`].
    ///
    /// **This also bounds what a CEL expression can see.** Over the cap the
    /// decoder previews the payload instead of parsing it (see
    /// [`crate::serdes::decode`]), so a value that would have decoded to JSON
    /// arrives as truncated text and `value.orderId` finds nothing. The
    /// substring prefilter is unaffected: it reads the bytes off the wire, not
    /// the rendering. Leave this `None` unless you know your payloads.
    pub max_value_bytes: Option<u32>,
}

/// How far one partition's scan has got, and where it stops.
///
/// `current_offset` is the **next** offset that partition's consumer will read,
/// so it starts at the seek position and reaches `end_offset` exactly when the
/// partition is finished — including a partition that was already empty, which
/// reports `current_offset == end_offset` from the first progress event rather
/// than looking stalled forever.
///
/// "Finished" is not "every offset yielded a record": on a transactional topic
/// some offsets hold markers a consumer never receives, so a partition can be
/// completely read with `scanned` well below `end_offset - start`. A partition
/// left short of its watermark on a `done` search is one the user **cancelled**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionProgress {
    pub partition: i32,
    pub current_offset: i64,
    pub end_offset: i64,
}

/// The truth about a running search. Every count here is cumulative and
/// monotonic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchProgress {
    /// Records read and considered.
    pub scanned: u64,
    /// Records that matched. **Keeps counting after the buffer is full** —
    /// this is the number the UI must show, and the reason a Kavka search can
    /// say "1,204 matches, showing the first 10,000" instead of quietly
    /// answering a different question.
    pub matched: u64,
    /// Records actually placed in the result buffer, ever — not the current
    /// queue depth, which drains as the caller reads. Stops at
    /// `min(spec.max_buffered, MAX_BUFFERED)`; `buffered < matched` is the
    /// signal that results were capped.
    pub buffered: u32,
    /// Records the CEL expression could not be evaluated against — the
    /// expression asked for a field this record lacks, or answered with
    /// something that is not true/false.
    ///
    /// **Not matched, and not an error.** A heterogeneous topic is the normal
    /// case. But a record nobody could judge is not a record judged "no", so
    /// the count is reported rather than logged: it is the difference between
    /// "nothing matched" and "this expression doesn't apply to these records".
    pub unevaluated: u64,
    /// One entry per partition this search covers, ascending by partition.
    pub per_partition: Vec<PartitionProgress>,
    /// Partitions that finished because **nothing more arrived**, rather than
    /// because their cursor reached the captured end watermark, ascending.
    ///
    /// Transaction markers and aborted records occupy offsets a consumer never
    /// receives, so on a transactional topic the watermark alone cannot always
    /// be reached and a quiet deadline is what ends the scan (see
    /// `QUIET_AFTER_DATA`). Those partitions are complete as far as anything
    /// readable goes — but "as far as anything readable goes" is a claim the
    /// user is entitled to see, because the other reason a partition goes quiet
    /// is a broker that stopped answering.
    pub assumed_complete: Vec<i32>,
    /// Scan rate over the whole search so far, from a monotonic clock. Frozen
    /// when the search ends, so a late poll reports the rate achieved rather
    /// than one that decays towards zero.
    pub msgs_per_sec: f64,
    /// Every worker has finished (completed, cancelled, or failed).
    pub done: bool,
    /// Set when the search **failed** — a broker error, not a filter that
    /// happened to match nothing. A CEL expression that cannot be evaluated
    /// against some records is *not* a failed search, so it never lands here;
    /// it is counted in `unevaluated` and explained in `filter_error`.
    pub error: Option<String>,
    /// The first evaluation failure, verbatim, or `None` when every record the
    /// expression saw could be judged. One example explains a whole `unevaluated`
    /// count; the rest are the same sentence with a different offset.
    pub filter_error: Option<String>,
}

// ---------------------------------------------------------------------------
// The compiled filter. Deliberately outside the `kafka` gate: compiling and
// evaluating a filter involves no librdkafka, so it has to build (and be
// testable) on the bare tier that tooling and clippy use, exactly like the
// serde pipeline it sits on top of.
// ---------------------------------------------------------------------------

/// A [`SearchQuery`] turned into something a hot loop can run: the substring
/// pre-lowercased into bytes, the CEL expression parsed once.
///
/// Cheap to share across the worker threads (`Send + Sync`, the program behind
/// an `Arc`); the per-thread half is [`CelFilter`], which carries the
/// interpreter's own context and is built once per worker by
/// [`Self::evaluator`].
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    /// ASCII-lowercased, so the hot loop lowercases only the haystack.
    needle: Option<Vec<u8>>,
    cel: Option<Arc<cel::Program>>,
    cel_source: Option<String>,
}

impl CompiledQuery {
    /// Compiles a query, or explains why it cannot be compiled.
    ///
    /// A CEL syntax error is returned **here** — synchronously, before a single
    /// broker call — so a typo surfaces as a message under the expression
    /// editor rather than as a search that starts, ends and finds nothing. The
    /// message keeps the interpreter's line/column and caret, which is the part
    /// that points at the mistake, and names the expression it came from.
    pub fn compile(query: &SearchQuery) -> Result<Self> {
        let needle = query
            .substring
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| s.as_bytes().to_ascii_lowercase());

        let source = query.cel.as_ref().filter(|s| !s.trim().is_empty());
        let cel = match source {
            None => None,
            Some(expression) => Some(Arc::new(cel::Program::compile(expression).map_err(
                |errors| {
                    Error::Other(format!(
                        "this filter expression didn't parse:\n{errors}\n  in: {expression}"
                    ))
                },
            )?)),
        };

        Ok(Self {
            needle,
            cel,
            cel_source: source.cloned(),
        })
    }

    /// Whether this query rejects nothing — the "read the whole topic" case,
    /// where every record is a hit and no decoding is needed to decide that.
    pub fn is_match_all(&self) -> bool {
        self.needle.is_none() && self.cel.is_none()
    }

    /// Whether a record has to be decoded before this query can judge it.
    pub fn needs_decode(&self) -> bool {
        self.cel.is_some()
    }

    /// The prefilter: does the substring occur in these raw bytes?
    ///
    /// Runs on the key, the value and **every header's name and value** as they
    /// came off the wire, before any decoding — that is what makes it cheap
    /// enough to be the first thing a search does with a record. `true` when the
    /// query has no substring, so this composes as the first half of an AND.
    ///
    /// **Headers are in the promise, so they are in the filter.** The search bar
    /// says "Find in key, value or headers"; a prefilter that read two of those
    /// three would make the copy a lie that no test could catch, because the
    /// records it silently skips look exactly like records that never matched.
    /// `headers` is taken as an iterator rather than a slice so the hot loop can
    /// walk librdkafka's own header list without allocating a `Vec` per record.
    ///
    /// Case folding is ASCII-only. The bytes have no declared encoding at this
    /// point (they may not be text at all), so Unicode case folding would be
    /// guessing; a non-ASCII needle is matched byte-for-byte, which is exactly
    /// right for UTF-8 input and honest about everything else.
    pub fn matches_raw<'h>(
        &self,
        key: Option<&[u8]>,
        value: Option<&[u8]>,
        headers: impl IntoIterator<Item = (&'h [u8], Option<&'h [u8]>)>,
    ) -> bool {
        let Some(needle) = self.needle.as_deref() else {
            return true;
        };
        if contains_ignoring_ascii_case(key.unwrap_or_default(), needle)
            || contains_ignoring_ascii_case(value.unwrap_or_default(), needle)
        {
            return true;
        }
        headers.into_iter().any(|(name, value)| {
            contains_ignoring_ascii_case(name, needle)
                || contains_ignoring_ascii_case(value.unwrap_or_default(), needle)
        })
    }

    /// The per-thread half of the filter, or `None` when there is no CEL
    /// expression to evaluate.
    ///
    /// Built once per worker: the interpreter's root context carries the whole
    /// CEL standard library, so building one per record would cost more than
    /// evaluating the expression.
    pub fn evaluator(&self) -> Option<CelFilter> {
        self.cel.as_ref().map(|program| CelFilter {
            program: Arc::clone(program),
            root: cel::Context::default(),
            source: self.cel_source.clone().unwrap_or_default(),
        })
    }
}

/// Substring search over bytes, folding ASCII case in the haystack only (the
/// needle arrives already lowercased).
///
/// Byte-oriented on purpose: keys and values are frequently not UTF-8, and a
/// prefilter that could only look at text would be unable to run before
/// decoding — which is the entire point of it.
pub fn contains_ignoring_ascii_case(haystack: &[u8], needle_lowercase: &[u8]) -> bool {
    let Some((&first, rest)) = needle_lowercase.split_first() else {
        return true;
    };
    if haystack.len() < needle_lowercase.len() {
        return false;
    }
    // `first` prunes most positions with one comparison; only a hit on it pays
    // for the tail.
    for start in 0..=(haystack.len() - needle_lowercase.len()) {
        if haystack[start].to_ascii_lowercase() != first {
            continue;
        }
        if haystack[start + 1..]
            .iter()
            .zip(rest)
            .all(|(byte, wanted)| byte.to_ascii_lowercase() == *wanted)
        {
            return true;
        }
    }
    false
}

/// One worker's CEL interpreter: the shared compiled program plus this
/// thread's root context.
///
/// # The activation
///
/// | name | type | value |
/// |------|------|-------|
/// | `key` | `string \| null` | the decoded key's text; `null` for a keyless record **or** a key that decoded to hex |
/// | `value` | `dynamic` | the parsed JSON tree when the value decoded to one, otherwise its text; `null` for a tombstone |
/// | `value_text` | `string` | the value's **display form** — the same rendering the message table shows — and `""` for a tombstone |
/// | `headers` | `map(string, string)` | header values as rendered by [`crate::serdes::decode_header`]; a null-valued header is present with an empty string, so `"x" in headers` still answers correctly |
/// | `partition` | `int` | |
/// | `offset` | `int` | |
/// | `timestamp_ms` | `int \| null` | `null` on a record from before KIP-32 |
///
/// Duplicate header keys — which Kafka permits — collapse to the last one seen;
/// a map is the shape an expression can actually use.
///
/// # Why `value_text` exists
///
/// `value` changes shape with the payload: a JSON body arrives as a map, a
/// plain-text body as a string. That is what makes `value.status == "failed"`
/// possible, and it is also why `string(value)` — the obvious way to write "look
/// anywhere in the body", and the thing the UI cheatsheet used to teach — is an
/// **error** on every JSON record: CEL has no conversion from a map to a string.
/// `value_text` is always bound and always a string, so
/// `value_text.contains("timeout")` works on every shape a topic can hold,
/// including a mixed one. It is the *display* text, so it is also exactly what
/// the user is reading in the table when they write the filter.
pub struct CelFilter {
    program: Arc<cel::Program>,
    root: cel::Context<'static>,
    source: String,
}

impl CelFilter {
    /// Evaluates the expression against one decoded record.
    ///
    /// `Ok(false)` for a record whose value no decoder could read (it rendered
    /// as hex): see the module docs — a filter can only match what it can read,
    /// so such a record is scanned and not matched rather than being judged on
    /// a hex rendering it never had. `Err` is a *filter* problem (the
    /// expression referenced a field this record lacks, or returned something
    /// other than true/false), which the caller reports once rather than once
    /// per message.
    pub fn matches(&self, record: &MessageRecord) -> std::result::Result<bool, String> {
        if record
            .value
            .as_ref()
            .is_some_and(|value| value.encoding == Encoding::Hex)
        {
            return Ok(false);
        }

        let mut context = self.root.new_inner_scope();
        let key = record
            .key
            .as_ref()
            .filter(|key| key.encoding != Encoding::Hex)
            .map(|key| key.text.as_str());
        let headers: std::collections::BTreeMap<&str, &str> = record
            .headers
            .iter()
            .map(|header| {
                (
                    header.key.as_str(),
                    header.value.as_deref().unwrap_or_default(),
                )
            })
            .collect();

        bind(&mut context, "key", key)?;
        bind(&mut context, "headers", &headers)?;
        bind(&mut context, "partition", i64::from(record.partition))?;
        bind(&mut context, "offset", record.offset)?;
        bind(&mut context, "timestamp_ms", record.timestamp_ms)?;
        match record.value.as_ref() {
            None => bind(&mut context, "value", Option::<&str>::None)?,
            Some(value) => match value.json.as_ref() {
                Some(json) => bind(&mut context, "value", json)?,
                None => bind(&mut context, "value", value.text.as_str())?,
            },
        }
        // ALWAYS bound, whatever the value turned out to be — that is the whole
        // point of it (see the type's docs). A tombstone has no text, and `""`
        // is the honest rendering of "no value" for a string binding: an
        // expression asking whether the body contains something gets `false`,
        // not an evaluation failure.
        bind(
            &mut context,
            "value_text",
            record
                .value
                .as_ref()
                .map_or("", |value| value.text.as_str()),
        )?;

        match self.program.execute(&context) {
            Ok(cel::Value::Bool(matched)) => Ok(matched),
            Ok(other) => Err(format!(
                "a filter has to answer true or false; `{}` answered {other:?}",
                self.source
            )),
            Err(e) => Err(format!("`{}` could not be evaluated: {e}", self.source)),
        }
    }
}

/// Deliberately not derived: `cel::Context` is not `Debug`, and the derived
/// form would be noise anyway — the expression is the interesting part.
impl std::fmt::Debug for CelFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelFilter")
            .field("source", &self.source)
            .finish()
    }
}

/// Binds one activation variable.
///
/// `cel` accepts anything `serde::Serialize` — which is how a decoded payload
/// crosses into the interpreter: `serde_json::Value` is already the canonical
/// form the serde pipeline produces (docs/ARCHITECTURE.md D4), so there is no
/// bespoke conversion between the two value trees to keep in sync.
fn bind<V: cel::objects::TryIntoValue>(
    context: &mut cel::Context<'_>,
    name: &'static str,
    value: V,
) -> std::result::Result<(), String> {
    context
        .add_variable(name, value)
        .map_err(|e| format!("binding `{name}` for the filter: {e}"))
}

// ---------------------------------------------------------------------------
// The engine.
// ---------------------------------------------------------------------------

/// How long a worker waits on one poll. Also the worst-case latency of a
/// [`SearchSession::stop`].
#[cfg(feature = "kafka")]
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long a worker waits for its *first* record before concluding there is
/// nothing to read. Generous: this covers the broker's connect, metadata and
/// first-fetch latency on a cold cluster.
#[cfg(feature = "kafka")]
const QUIET_BEFORE_DATA: Duration = Duration::from_secs(10);

/// How long a worker waits between records once data is flowing.
///
/// The end watermarks captured at the start are the primary completion signal,
/// but they are not sufficient on their own: transaction markers and aborted
/// messages occupy offsets the consumer never receives, so on a transactional
/// topic the next offset can never reach the watermark and a watermark-only
/// loop would run until the user gave up. Longer than
/// [`crate::consume`]'s equivalent because a search is not an interactive
/// fetch — it is expected to run for minutes, and a slow broker mid-scan must
/// not read as "finished".
#[cfg(feature = "kafka")]
const QUIET_AFTER_DATA: Duration = Duration::from_secs(5);

#[cfg(feature = "kafka")]
const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

/// Most a single [`SearchSession::next_results`] hands back. Bounds one IPC
/// event so a burst cannot produce a 10,000-record payload the webview has to
/// parse in one frame.
#[cfg(feature = "kafka")]
const RESULT_BATCH: usize = 500;

/// A running search: N worker threads, a bounded result buffer the caller
/// drains at its own pace, and a progress snapshot that is always the truth.
///
/// Dropping the session stops it and joins the workers, so the consumers — and
/// their broker connections — are gone before `drop` returns.
#[cfg(feature = "kafka")]
pub struct SearchSession {
    shared: Arc<SearchShared>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "kafka")]
struct SearchShared {
    results: Mutex<ResultQueue>,
    ready: Condvar,
    cancel: CancelToken,
    scanned: AtomicU64,
    matched: AtomicU64,
    /// Records a CEL expression could not judge. Counted rather than logged:
    /// see [`SearchProgress::unevaluated`].
    unevaluated: AtomicU64,
    /// The first evaluation failure across every worker, kept verbatim.
    filter_error: Mutex<Option<String>>,
    /// Slots taken in the result buffer, ever. Reserved with a compare-exchange
    /// *before* a record is decoded, so the cap is exact under N workers and no
    /// worker pays to decode a record there is no room for.
    buffered: AtomicU32,
    max_buffered: u32,
    cursors: Vec<PartitionCursor>,
    live_workers: AtomicUsize,
    started: Instant,
    /// Nanos the search took, written once when the last worker leaves; 0 while
    /// it is still running. Freezes [`SearchProgress::msgs_per_sec`].
    elapsed_nanos: AtomicU64,
    error: Mutex<Option<String>>,
}

#[cfg(feature = "kafka")]
struct ResultQueue {
    records: VecDeque<MessageRecord>,
    /// Every worker has left; once `records` drains, this session is over.
    ended: bool,
}

#[cfg(feature = "kafka")]
struct PartitionCursor {
    partition: i32,
    /// The next offset this partition's worker will read.
    current: AtomicI64,
    /// The end watermark captured at start — what bounds the scan.
    end: i64,
    /// This partition was completed by the quiet deadline rather than by
    /// reaching `end`. See [`SearchProgress::assumed_complete`].
    assumed: AtomicBool,
}

/// One partition's read window, `[start, end)`.
#[cfg(feature = "kafka")]
#[derive(Clone, Copy)]
struct PartitionSlot {
    partition: i32,
    start: i64,
    end: i64,
}

#[cfg(feature = "kafka")]
impl PartitionSlot {
    fn remaining(&self) -> i64 {
        self.end.saturating_sub(self.start).max(0)
    }
}

/// Everything one worker needs, bundled so the thread body takes a single
/// argument instead of eight.
#[cfg(feature = "kafka")]
struct Worker {
    consumer: BaseConsumer<KavkaClientContext>,
    slots: Vec<PartitionSlot>,
    shared: Arc<SearchShared>,
    filter: Arc<CompiledQuery>,
    registry: Option<Arc<SchemaRegistry>>,
    topic: String,
    max_display: usize,
    /// partition -> its index in `shared.cursors`. A worker owns its partitions
    /// exclusively, so this is all the indexing it needs.
    cursors: HashMap<i32, usize>,
}

#[cfg(feature = "kafka")]
impl SearchSession {
    /// Compiles the filter, plans the scan, and starts the workers.
    ///
    /// Everything that can fail deterministically fails here, on the calling
    /// thread: a CEL syntax error (first, before any network call), an unknown
    /// topic or partition, an unreachable broker, a consumer that cannot be
    /// created. A returned session is one that is genuinely running.
    pub fn start(
        conn: &ClusterConnection,
        profile_sr: Option<&SchemaRegistryConfig>,
        spec: &SearchSpec,
    ) -> Result<Self> {
        // First, and deliberately: a typo in an expression must not cost a
        // round trip to a broker to discover.
        let filter = Arc::new(CompiledQuery::compile(&spec.query)?);

        let planner = conn.new_session_consumer()?;
        let slots = seek_plan(&planner, spec)?;
        drop(planner);

        let cursors: Vec<PartitionCursor> = slots
            .iter()
            .map(|slot| PartitionCursor {
                partition: slot.partition,
                current: AtomicI64::new(slot.start),
                end: slot.end,
                assumed: AtomicBool::new(false),
            })
            .collect();
        let index: HashMap<i32, usize> = slots
            .iter()
            .enumerate()
            .map(|(i, slot)| (slot.partition, i))
            .collect();

        // An already-empty partition is planned (so progress can report it as
        // complete) but never assigned to a worker.
        let work: Vec<PartitionSlot> = slots
            .iter()
            .copied()
            .filter(|slot| slot.remaining() > 0)
            .collect();
        let shares = distribute(&work, work.len().min(MAX_WORKERS));
        let worker_count = shares.len();

        let shared = Arc::new(SearchShared {
            results: Mutex::new(ResultQueue {
                records: VecDeque::new(),
                ended: worker_count == 0,
            }),
            ready: Condvar::new(),
            cancel: CancelToken::new(),
            scanned: AtomicU64::new(0),
            matched: AtomicU64::new(0),
            unevaluated: AtomicU64::new(0),
            filter_error: Mutex::new(None),
            buffered: AtomicU32::new(0),
            max_buffered: spec.max_buffered.min(MAX_BUFFERED),
            cursors,
            live_workers: AtomicUsize::new(worker_count),
            started: Instant::now(),
            elapsed_nanos: AtomicU64::new(0),
            error: Mutex::new(None),
        });

        let registry = profile_sr.map(|config| Arc::new(SchemaRegistry::new(config)));
        let max_display = spec
            .max_value_bytes
            .map_or(DEFAULT_MAX_VALUE_BYTES, |bytes| bytes as usize);

        // Consumers are created and assigned HERE, not inside the threads, so
        // an auth failure or a bad assignment is an error from `start` rather
        // than a session that reports "done" a moment later with no records and
        // no explanation.
        let mut workers = Vec::with_capacity(worker_count);
        for share in shares {
            let consumer = conn.new_session_consumer()?;
            let mut assignment = TopicPartitionList::new();
            for slot in &share {
                assignment
                    .add_partition_offset(&spec.topic, slot.partition, Offset::Offset(slot.start))
                    .map_err(|e| {
                        Error::Other(format!(
                            "seeking {}[{}] to offset {}: {e}",
                            spec.topic, slot.partition, slot.start
                        ))
                    })?;
            }
            consumer.assign(&assignment).map_err(|e| {
                Error::Other(format!("assigning partitions of {}: {e}", spec.topic))
            })?;
            workers.push(Worker {
                consumer,
                cursors: share
                    .iter()
                    .map(|slot| (slot.partition, index[&slot.partition]))
                    .collect(),
                slots: share,
                shared: Arc::clone(&shared),
                filter: Arc::clone(&filter),
                registry: registry.clone(),
                topic: spec.topic.clone(),
                max_display,
            });
        }

        tracing::debug!(
            topic = %spec.topic,
            partitions = shared.cursors.len(),
            workers = workers.len(),
            "search started"
        );

        let mut handles = Vec::with_capacity(worker_count);
        let mut failure = None;
        for (i, worker) in workers.into_iter().enumerate() {
            match std::thread::Builder::new()
                .name(format!("kavka-search-{i}"))
                .spawn(move || {
                    let shared = Arc::clone(&worker.shared);
                    scan(worker);
                    shared.worker_finished();
                }) {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    failure = Some(Error::Other(format!("starting a search thread: {e}")));
                    break;
                }
            }
        }
        // Whatever did not start still holds a slot in `live_workers`, and a
        // session that can never reach zero is one whose readers wait forever.
        for _ in handles.len()..worker_count {
            shared.worker_finished();
        }

        // Built before the error is returned on purpose: the threads that DID
        // start are owned by the session, so returning `Err` drops it, which
        // stops and joins them. Anything else leaks a consumer that keeps
        // fetching for a search nobody will ever read.
        let session = Self {
            shared,
            workers: handles,
        };
        match failure {
            Some(e) => Err(e),
            None => Ok(session),
        }
    }

    /// Waits up to `timeout` for matched records.
    ///
    /// `None` means the search is over and will produce nothing further.
    /// `Some(empty)` means it is still scanning and has found nothing yet — a
    /// state the UI must be able to say out loud (docs/DESIGN.md §7: never say
    /// "no results" while a scan is running), which is why it is not folded
    /// into `None`.
    pub fn next_results(&self, timeout: Duration) -> Option<Vec<MessageRecord>> {
        let deadline = Instant::now() + timeout;
        let mut queue = self.shared.lock();
        loop {
            if !queue.records.is_empty() {
                let take = queue.records.len().min(RESULT_BATCH);
                return Some(queue.records.drain(..take).collect());
            }
            // Ordering matters: records buffered before the last worker left
            // are still delivered, and only a drained-and-ended queue is `None`.
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

    /// A consistent-enough snapshot for the UI: the counters are read
    /// independently, so `scanned` may be a record or two ahead of `matched` on
    /// a running search. Never the other way round in a way that matters, and
    /// never a lie about the cap — `buffered` is reserved before a record is
    /// decoded, so it is exact.
    pub fn progress(&self) -> SearchProgress {
        self.shared.snapshot()
    }

    /// The token the workers check. Exposed so a caller that already keys
    /// cancellation by profile can cancel this search the same way it cancels
    /// a fetch.
    pub fn cancel_token(&self) -> CancelToken {
        self.shared.cancel.clone()
    }

    /// Asks the workers to finish. Idempotent, and safe from any thread (the
    /// shell's `search_stop` command runs on a different one from the reader).
    /// Returns immediately; the workers notice within one poll interval.
    pub fn stop(&self) {
        self.shared.cancel.cancel();
        self.shared.ready.notify_all();
    }
}

/// Deliberately not derived: the derived form would dump every buffered record
/// into any log line or panic message that touches a session.
#[cfg(feature = "kafka")]
impl std::fmt::Debug for SearchSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let progress = self.shared.snapshot();
        f.debug_struct("SearchSession")
            .field("scanned", &progress.scanned)
            .field("matched", &progress.matched)
            .field("buffered", &progress.buffered)
            .field("done", &progress.done)
            .finish()
    }
}

#[cfg(feature = "kafka")]
impl Drop for SearchSession {
    /// Joins the workers so their consumers are gone before `drop` returns.
    /// Leaked search threads are the expensive kind of leak: each one holds a
    /// consumer that keeps fetching at full speed from a cluster nobody is
    /// looking at.
    fn drop(&mut self) {
        self.stop();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(feature = "kafka")]
impl SearchShared {
    /// Poison-tolerant, like everywhere else in the crate: a panicking reader
    /// must not turn a running search into a permanently locked one.
    fn lock(&self) -> MutexGuard<'_, ResultQueue> {
        self.results.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Takes a slot in the result buffer, or reports that the buffer is full.
    ///
    /// Reserving before decoding is what makes "matching never stops at the
    /// cap" cheap: past the cap a substring-only search never deserializes
    /// another record, it just keeps counting.
    fn reserve(&self) -> bool {
        self.buffered
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |taken| {
                (taken < self.max_buffered).then_some(taken + 1)
            })
            .is_ok()
    }

    fn push(&self, record: MessageRecord) {
        self.lock().records.push_back(record);
        self.ready.notify_all();
    }

    fn advance(&self, cursor: usize, next_offset: i64) {
        self.cursors[cursor]
            .current
            .store(next_offset, Ordering::Relaxed);
    }

    fn complete(&self, cursor: usize) {
        let cursor = &self.cursors[cursor];
        cursor.current.store(cursor.end, Ordering::Relaxed);
    }

    /// Completes a partition that went quiet before its watermark, and says so.
    /// The bar reaches the end either way; only this flag can tell the user
    /// which of the two ways it got there.
    fn complete_by_deadline(&self, cursor: usize) {
        self.complete(cursor);
        self.cursors[cursor].assumed.store(true, Ordering::Relaxed);
    }

    /// One record the filter could not judge. Counted always; the first message
    /// is kept because the rest are the same sentence about another offset.
    fn note_unevaluated(&self, message: String) {
        self.unevaluated.fetch_add(1, Ordering::Relaxed);
        let mut slot = self.filter_error.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    /// First failure wins: the one that stopped the search is the one worth
    /// showing, and the others are usually its echoes.
    fn note_error(&self, message: String) {
        let mut slot = self.error.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    fn worker_finished(&self) {
        if self.live_workers.fetch_sub(1, Ordering::AcqRel) == 1 {
            let elapsed = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            self.elapsed_nanos.store(elapsed.max(1), Ordering::Relaxed);
            self.lock().ended = true;
            self.ready.notify_all();
        }
    }

    fn snapshot(&self) -> SearchProgress {
        let frozen = self.elapsed_nanos.load(Ordering::Relaxed);
        let elapsed = if frozen == 0 {
            self.started.elapsed().as_secs_f64()
        } else {
            frozen as f64 / 1e9
        };
        let scanned = self.scanned.load(Ordering::Relaxed);
        SearchProgress {
            scanned,
            matched: self.matched.load(Ordering::Relaxed),
            buffered: self.buffered.load(Ordering::Relaxed),
            unevaluated: self.unevaluated.load(Ordering::Relaxed),
            per_partition: self
                .cursors
                .iter()
                .map(|cursor| PartitionProgress {
                    partition: cursor.partition,
                    current_offset: cursor.current.load(Ordering::Relaxed),
                    end_offset: cursor.end,
                })
                .collect(),
            // `cursors` is built from a partition-sorted plan, so this comes out
            // ascending without a sort.
            assumed_complete: self
                .cursors
                .iter()
                .filter(|cursor| cursor.assumed.load(Ordering::Relaxed))
                .map(|cursor| cursor.partition)
                .collect(),
            msgs_per_sec: if elapsed > 0.0 {
                scanned as f64 / elapsed
            } else {
                0.0
            },
            done: self.lock().ended,
            error: self.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            filter_error: self
                .filter_error
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }
}

/// Spreads partitions over `count` workers, largest first onto the
/// least-loaded worker.
///
/// Round-robin would be simpler and wrong for the common shape: a topic where
/// one partition holds most of the data (a hot key, or a producer that never
/// set a partitioner) would leave seven workers idle while the eighth did the
/// whole search.
#[cfg(feature = "kafka")]
fn distribute(slots: &[PartitionSlot], count: usize) -> Vec<Vec<PartitionSlot>> {
    if count == 0 {
        return Vec::new();
    }
    let mut ordered: Vec<PartitionSlot> = slots.to_vec();
    ordered.sort_by(|a, b| {
        b.remaining()
            .cmp(&a.remaining())
            .then(a.partition.cmp(&b.partition))
    });

    let mut shares: Vec<Vec<PartitionSlot>> = vec![Vec::new(); count];
    let mut loads = vec![0i64; count];
    for slot in ordered {
        let lightest = loads
            .iter()
            .enumerate()
            .min_by_key(|(i, load)| (**load, *i))
            .map_or(0, |(i, _)| i);
        loads[lightest] = loads[lightest].saturating_add(slot.remaining());
        shares[lightest].push(slot);
    }
    shares.retain(|share| !share.is_empty());
    shares
}

/// One worker: poll, filter, buffer, until its partitions are done, the search
/// is cancelled, or the broker stops answering.
#[cfg(feature = "kafka")]
fn scan(worker: Worker) {
    let Worker {
        consumer,
        slots,
        shared,
        filter,
        registry,
        topic,
        max_display,
        cursors,
    } = worker;
    let evaluator = filter.evaluator();
    let mut filter_error: Option<String> = None;
    // partition -> the end watermark it must reach to be finished.
    let mut pending: HashMap<i32, i64> = slots.iter().map(|s| (s.partition, s.end)).collect();
    let mut quiet_deadline = Instant::now() + QUIET_BEFORE_DATA;

    while !pending.is_empty() {
        if shared.cancel.is_cancelled() {
            tracing::debug!(%topic, "search cancelled; the worker is leaving");
            break;
        }
        if Instant::now() >= quiet_deadline {
            // Not an error: a transactional topic's offsets can be occupied by
            // markers the consumer never receives, so the watermark alone
            // cannot always be reached. See QUIET_AFTER_DATA.
            tracing::debug!(
                %topic,
                partitions = pending.len(),
                "no more records arrived; treating these partitions as read"
            );
            // These partitions ARE read as far as anything readable goes —
            // there is simply nothing at the offsets between here and the
            // watermark — so progress ends where a finished search's progress
            // belongs, and each one is recorded as completed BY DEADLINE so the
            // UI can say which partitions those were. Cancellation deliberately
            // does not do this: a search the user stopped is one that did not
            // finish, and its bar must not claim otherwise.
            for partition in pending.keys() {
                if let Some(&cursor) = cursors.get(partition) {
                    shared.complete_by_deadline(cursor);
                }
            }
            break;
        }
        match consumer.poll(POLL_INTERVAL) {
            None => continue,
            Some(Err(e)) => {
                shared.note_error(format!("searching {topic}: {e}"));
                break;
            }
            Some(Ok(message)) => {
                quiet_deadline = Instant::now() + QUIET_AFTER_DATA;
                let partition = message.partition();
                let offset = message.offset();
                let (Some(&end), Some(&cursor)) =
                    (pending.get(&partition), cursors.get(&partition))
                else {
                    continue;
                };
                // At or past the watermark captured at the start: a producer is
                // still writing. A search answers about the topic as it was
                // when it was asked, so this partition is done.
                if offset >= end {
                    pending.remove(&partition);
                    shared.complete(cursor);
                    continue;
                }
                shared.advance(cursor, offset + 1);
                shared.scanned.fetch_add(1, Ordering::Relaxed);
                if let Err(e) = consider(
                    &message,
                    &shared,
                    &filter,
                    evaluator.as_ref(),
                    registry.as_deref(),
                    max_display,
                ) {
                    if filter_error.is_none() {
                        filter_error = Some(e.clone());
                    }
                    // Every failure counts, not just the first: "17 records
                    // couldn't be judged" is the number the UI needs, and it is
                    // the difference between a filter that found nothing and a
                    // filter that could not look.
                    shared.note_unevaluated(e);
                }
                if offset + 1 >= end {
                    pending.remove(&partition);
                    shared.complete(cursor);
                }
            }
        }
    }

    if let Some(error) = filter_error {
        // Logged once per worker, not once per message: an expression that does
        // not apply to every record on a heterogeneous topic is normal, and the
        // first example is the one that explains it. The COUNT goes to the UI
        // (see `SearchProgress::unevaluated`) — a log line is not a report.
        tracing::warn!(%topic, %error, "a filter could not be evaluated on some records");
    }
    if let Some(error) = registry.as_deref().and_then(SchemaRegistry::take_error) {
        tracing::warn!(
            %topic,
            %error,
            "schema registry unavailable; affected payloads are shown as hex"
        );
    }
}

/// Judges one record and buffers it if it is a hit and there is room.
///
/// The order is the whole performance story: raw bytes first (no allocation, no
/// parsing), then a reservation in the result buffer, and only then a decode.
/// `Err` is a filter that could not be evaluated — counted and explained by the
/// caller, never fatal.
#[cfg(feature = "kafka")]
fn consider(
    message: &BorrowedMessage<'_>,
    shared: &SearchShared,
    filter: &CompiledQuery,
    evaluator: Option<&CelFilter>,
    registry: Option<&SchemaRegistry>,
    max_display: usize,
) -> std::result::Result<(), String> {
    if !filter.matches_raw(message.key(), message.payload(), raw_headers(message)) {
        return Ok(());
    }
    let Some(evaluator) = evaluator else {
        // The raw bytes already decided. Past the cap this never decodes
        // another record — it just keeps counting, which is the difference
        // between a bounded search and an honest one.
        shared.matched.fetch_add(1, Ordering::Relaxed);
        if shared.reserve() {
            shared.push(record(message, registry, max_display));
        }
        return Ok(());
    };

    let decoded = record(message, registry, max_display);
    match evaluator.matches(&decoded) {
        Ok(false) => Ok(()),
        Ok(true) => {
            shared.matched.fetch_add(1, Ordering::Relaxed);
            if shared.reserve() {
                shared.push(decoded);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// One record's headers as raw name/value bytes, lazily.
///
/// Deliberately an iterator over librdkafka's own list: the prefilter runs on
/// every record of the scan, and materialising a `Vec` here would put an
/// allocation per record into the hot loop the prefilter exists to keep cheap.
/// The name goes in as bytes so a needle can match a header *name* the same way
/// it matches everything else.
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

#[cfg(feature = "kafka")]
fn record(
    message: &BorrowedMessage<'_>,
    registry: Option<&SchemaRegistry>,
    max_display: usize,
) -> MessageRecord {
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
            .map(|bytes| serdes::decode(bytes, registry, max_display)),
        // `None` here is a tombstone, not an empty value.
        value: message
            .payload()
            .map(|bytes| serdes::decode(bytes, registry, max_display)),
        headers,
    }
}

/// Turns a [`SeekSpec`] into per-partition read windows and the end watermarks
/// that bound the scan.
///
/// Unlike [`crate::consume`]'s planner, empty windows are **kept**: a search
/// reports progress per partition, and a partition silently missing from that
/// list is indistinguishable, in the UI, from one that never started.
#[cfg(feature = "kafka")]
fn seek_plan(
    consumer: &BaseConsumer<KavkaClientContext>,
    spec: &SearchSpec,
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
            // the filter: told "orders has no partition 3" while partition 3 is
            // sitting there in the topic list, a user goes looking for a
            // cluster problem that does not exist.
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
                // i.e. an empty window, which stays in the plan as a partition
                // that is already complete.
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

    plan.sort_by_key(|slot| slot.partition);
    Ok(plan)
}

/// What a topic has, and what this search will read of it. Both halves are kept
/// so an error about one partition can tell "this topic has no partition 3"
/// apart from "partition 3 exists, but your filter excluded it".
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

// ---------------------------------------------------------------------------
// Tests that need no cluster. Outside the `kafka` gate on purpose: the filter
// is the half of the engine with the subtle rules, and it must stay testable on
// the bare toolchain tier.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod filters {
    use super::*;
    use crate::serdes::{decode, decode_header, DEFAULT_MAX_VALUE_BYTES};

    /// The compiled program is shared across worker threads by `Arc`; if that
    /// ever stops holding, this stops compiling rather than the engine
    /// silently re-parsing the expression once per thread.
    #[test]
    fn a_compiled_query_can_cross_thread_boundaries() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CompiledQuery>();
        assert_send_sync::<SearchQuery>();
        assert_send_sync::<SearchProgress>();
    }

    fn query(substring: Option<&str>, cel: Option<&str>) -> SearchQuery {
        SearchQuery {
            substring: substring.map(str::to_string),
            cel: cel.map(str::to_string),
        }
    }

    fn record(key: Option<&[u8]>, value: Option<&[u8]>) -> MessageRecord {
        MessageRecord {
            partition: 3,
            offset: 8412,
            timestamp_ms: Some(1_700_000_000_000),
            key: key.map(|bytes| decode(bytes, None, DEFAULT_MAX_VALUE_BYTES)),
            value: value.map(|bytes| decode(bytes, None, DEFAULT_MAX_VALUE_BYTES)),
            headers: vec![
                decode_header("trace-id", Some(b"abc-123")),
                decode_header("retry", None),
            ],
        }
    }

    fn matches(cel: &str, record: &MessageRecord) -> std::result::Result<bool, String> {
        CompiledQuery::compile(&query(None, Some(cel)))
            .expect("compiles")
            .evaluator()
            .expect("has an expression")
            .matches(record)
    }

    /// A record with no headers — most of these tests are about the key and the
    /// value, and spelling the empty list out every time reads worse than
    /// naming it once.
    const NO_HEADERS: [(&[u8], Option<&[u8]>); 0] = [];

    // --- the prefilter ----------------------------------------------------

    #[test]
    fn the_prefilter_folds_ascii_case_in_both_directions() {
        let filter = CompiledQuery::compile(&query(Some("Order-4"), None)).unwrap();
        assert!(filter.matches_raw(Some(b"order-4"), None, NO_HEADERS));
        assert!(filter.matches_raw(Some(b"ORDER-42"), None, NO_HEADERS));
        assert!(filter.matches_raw(None, Some(br#"{"id":"oRdEr-4"}"#), NO_HEADERS));
        assert!(!filter.matches_raw(Some(b"order-5"), Some(b"nothing here"), NO_HEADERS));
    }

    #[test]
    fn the_prefilter_reads_key_and_value_and_needs_only_one() {
        let filter = CompiledQuery::compile(&query(Some("failed"), None)).unwrap();
        assert!(filter.matches_raw(Some(b"failed-99"), Some(b"{}"), NO_HEADERS));
        assert!(filter.matches_raw(Some(b"k"), Some(br#"{"status":"failed"}"#), NO_HEADERS));
        assert!(!filter.matches_raw(Some(b"k"), Some(br#"{"status":"ok"}"#), NO_HEADERS));
        // A tombstone still has a key to match on, and a keyless record still
        // has a value.
        assert!(filter.matches_raw(Some(b"failed"), None, NO_HEADERS));
        assert!(filter.matches_raw(None, Some(b"failed"), NO_HEADERS));
        assert!(!filter.matches_raw(None, None, NO_HEADERS));
    }

    /// THE SEARCH BAR SAYS "key, value or headers", so all three are searched.
    /// The two halves of a header are separate hiding places: a trace id lives
    /// in the VALUE of `trace-id`, and "which messages carry a retry marker at
    /// all" is a question about the NAME.
    #[test]
    fn the_prefilter_reads_header_names_and_header_values() {
        let by_name = CompiledQuery::compile(&query(Some("retry-count"), None)).unwrap();
        assert!(
            by_name.matches_raw(
                Some(b"k"),
                Some(b"nothing in the body"),
                [(b"retry-count".as_slice(), Some(b"3".as_slice()))],
            ),
            "a needle that exists ONLY in a header name has to match"
        );

        let by_value = CompiledQuery::compile(&query(Some("7f3c9a"), None)).unwrap();
        assert!(
            by_value.matches_raw(
                Some(b"k"),
                Some(b"nothing in the body"),
                [(b"trace-id".as_slice(), Some(b"7F3C9A".as_slice()))],
            ),
            "a needle that exists ONLY in a header value has to match, folding case"
        );

        // ...and a record whose headers hold neither is still not a match.
        assert!(!by_value.matches_raw(
            Some(b"k"),
            Some(b"nothing in the body"),
            [(b"trace-id".as_slice(), Some(b"000000".as_slice()))],
        ));
        // A null-valued header (Kafka allows them) is a name with no value, and
        // neither half may panic on it.
        assert!(by_name.matches_raw(None, None, [(b"retry-count".as_slice(), None)]));
        assert!(!by_value.matches_raw(None, None, [(b"trace-id".as_slice(), None)]));
    }

    /// The prefilter's whole reason to exist is that it runs before anything
    /// knows what these bytes are — so it has to work on bytes that are not
    /// text at all, and never panic on a multi-byte boundary.
    #[test]
    fn the_prefilter_matches_inside_bytes_that_are_not_utf8() {
        let filter = CompiledQuery::compile(&query(Some("needle"), None)).unwrap();
        let mut payload = vec![0xff, 0xfe, 0x00, 0x80];
        payload.extend_from_slice(b"NeEdLe");
        payload.extend_from_slice(&[0xc3, 0x28]); // an invalid UTF-8 sequence
        assert!(std::str::from_utf8(&payload).is_err(), "not text");
        assert!(filter.matches_raw(None, Some(&payload), NO_HEADERS));

        // A needle that is itself not ASCII is matched byte for byte — the
        // honest answer when the haystack has no declared encoding.
        let unicode = CompiledQuery::compile(&query(Some("Ünicode"), None)).unwrap();
        assert!(unicode.matches_raw(None, Some("xÜnicodex".as_bytes()), NO_HEADERS));
        assert!(!unicode.matches_raw(None, Some("xünicodex".as_bytes()), NO_HEADERS));
    }

    #[test]
    fn a_needle_longer_than_the_payload_does_not_panic_or_match() {
        let filter = CompiledQuery::compile(&query(Some("a-very-long-needle"), None)).unwrap();
        assert!(!filter.matches_raw(Some(b"ab"), Some(b""), NO_HEADERS));
        assert!(!filter.matches_raw(None, None, NO_HEADERS));
        assert!(!filter.matches_raw(None, None, [(b"h".as_slice(), Some(b"v".as_slice()))]));
    }

    #[test]
    fn an_empty_query_matches_everything_and_needs_no_decode() {
        let filter = CompiledQuery::compile(&SearchQuery::default()).unwrap();
        assert!(filter.is_match_all());
        assert!(!filter.needs_decode());
        assert!(filter.matches_raw(None, None, NO_HEADERS));
        assert!(filter.evaluator().is_none());

        // An empty string is not a filter; it is the search box before anyone
        // typed in it.
        let blank = CompiledQuery::compile(&query(Some(""), Some("   "))).unwrap();
        assert!(blank.is_match_all());
    }

    #[test]
    fn contains_ignoring_ascii_case_handles_the_edges() {
        assert!(contains_ignoring_ascii_case(b"", b""));
        assert!(contains_ignoring_ascii_case(b"abc", b""));
        assert!(!contains_ignoring_ascii_case(b"", b"a"));
        assert!(contains_ignoring_ascii_case(b"abc", b"abc"));
        assert!(contains_ignoring_ascii_case(b"aab", b"ab"), "restarts");
        assert!(!contains_ignoring_ascii_case(b"ab", b"abc"));
    }

    // --- compile errors ---------------------------------------------------

    /// A typo has to fail where it was typed, with something that points at it.
    #[test]
    fn a_broken_expression_fails_to_compile_and_says_where() {
        let error = CompiledQuery::compile(&query(None, Some("value.orderId >=")))
            .expect_err("an incomplete expression")
            .to_string();

        assert!(error.contains("didn't parse"), "got {error}");
        assert!(
            error.contains("value.orderId >="),
            "the message has to carry the expression: got {error}"
        );
        // The interpreter's own line:column marker, which is what a caret in
        // the editor is drawn from.
        assert!(error.contains(":1:"), "got {error}");
    }

    #[test]
    fn an_unbalanced_expression_fails_rather_than_matching_nothing() {
        for broken in ["value.status == \"failed", "(partition == 1", "&& true"] {
            assert!(
                CompiledQuery::compile(&query(None, Some(broken))).is_err(),
                "{broken:?} should not compile"
            );
        }
    }

    // --- the activation ---------------------------------------------------

    #[test]
    fn a_json_value_is_reachable_field_by_field() {
        let record = record(
            Some(b"order-45"),
            Some(br#"{"orderId":45,"status":"created","amount":315.5,"tags":["a","b"]}"#),
        );

        assert_eq!(matches("value.orderId >= 45", &record), Ok(true));
        assert_eq!(matches("value.orderId >= 46", &record), Ok(false));
        assert_eq!(matches(r#"value.status == "created""#, &record), Ok(true));
        assert_eq!(matches("value.amount > 300.0", &record), Ok(true));
        assert_eq!(matches(r#""b" in value.tags"#, &record), Ok(true));
        assert_eq!(matches("size(value.tags) == 2", &record), Ok(true));
    }

    #[test]
    fn the_key_headers_and_coordinates_are_all_bindable() {
        let record = record(Some(b"order-45"), Some(br#"{"orderId":45}"#));

        assert_eq!(matches(r#"key.startsWith("order-")"#, &record), Ok(true));
        assert_eq!(
            matches(r#"headers["trace-id"] == "abc-123""#, &record),
            Ok(true)
        );
        assert_eq!(matches(r#""trace-id" in headers"#, &record), Ok(true));
        assert_eq!(matches("partition == 3", &record), Ok(true));
        assert_eq!(matches("offset == 8412", &record), Ok(true));
        assert_eq!(matches("timestamp_ms > 1600000000000", &record), Ok(true));
        // A header Kafka stored with a null value is present with an empty
        // string, so membership still answers correctly.
        assert_eq!(matches(r#""retry" in headers"#, &record), Ok(true));
        assert_eq!(matches(r#"headers["retry"] == """#, &record), Ok(true));
    }

    #[test]
    fn a_keyless_record_binds_key_to_null_rather_than_an_empty_string() {
        let record = record(None, Some(br#"{"orderId":1}"#));
        assert_eq!(matches("key == null", &record), Ok(true));
        assert_eq!(matches(r#"key == """#, &record), Ok(false));
        // ...and a keyed one is not null.
        let keyed = super::filters::record(Some(b"k"), Some(br#"{"orderId":1}"#));
        assert_eq!(matches("key == null", &keyed), Ok(false));
        assert_eq!(matches(r#"key == "k""#, &keyed), Ok(true));
    }

    #[test]
    fn a_tombstone_binds_value_to_null() {
        let record = record(Some(b"order-9"), None);
        assert_eq!(matches("value == null", &record), Ok(true));
        assert_eq!(matches(r#"key == "order-9""#, &record), Ok(true));
    }

    #[test]
    fn a_text_value_binds_as_a_string_not_as_a_tree() {
        let record = record(Some(b"k"), Some(b"plain text, not json"));
        assert_eq!(matches(r#"value.contains("not json")"#, &record), Ok(true));
        assert_eq!(matches(r#"value.startsWith("plain")"#, &record), Ok(true));
    }

    /// `value_text` IS ALWAYS BOUND AND ALWAYS A STRING.
    ///
    /// This is the binding the cheatsheet teaches, so it has to work on every
    /// shape a topic can hold — and the reason it exists is the first assertion
    /// here: `string(value)` is an ERROR on a JSON record, which made the
    /// previously-taught expression fail on exactly the payloads people search.
    #[test]
    fn value_text_is_bound_for_every_shape_of_value() {
        let json = record(Some(b"k"), Some(br#"{"status":"failed","n":1}"#));
        assert!(
            matches(r#"string(value).contains("failed")"#, &json).is_err(),
            "the expression this replaced has to be the broken one"
        );
        assert_eq!(
            matches(r#"value_text.contains("failed")"#, &json),
            Ok(true),
            "a JSON body is searchable as its display text"
        );
        assert_eq!(
            matches(r#"value_text.contains("shipped")"#, &json),
            Ok(false)
        );

        // Plain text: the same expression, the same answer.
        let text = record(Some(b"k"), Some(b"connect timeout after 30s"));
        assert_eq!(
            matches(r#"value_text.contains("timeout")"#, &text),
            Ok(true)
        );
        assert_eq!(
            matches(r#"value_text.startsWith("connect")"#, &text),
            Ok(true)
        );

        // A tombstone has no value at all, so its text is empty — `false`, not
        // an evaluation failure.
        let tombstone = record(Some(b"order-9"), None);
        assert_eq!(matches(r#"value_text == """#, &tombstone), Ok(true));
        assert_eq!(
            matches(r#"value_text.contains("anything")"#, &tombstone),
            Ok(false)
        );

        // A payload no decoder could read never reaches the expression at all —
        // THE READ RULE outranks `value_text`, which would otherwise offer up a
        // hex dump nobody wrote a filter against.
        let hex = record(Some(b"k"), Some(&[0xfe, 0xff, 0xfe, 0xff]));
        assert_eq!(hex.value.as_ref().unwrap().encoding, Encoding::Hex);
        assert_eq!(matches(r#"value_text.contains("fe")"#, &hex), Ok(false));
    }

    #[test]
    fn timestamps_can_be_absent() {
        let mut record = record(Some(b"k"), Some(b"{}"));
        record.timestamp_ms = None;
        assert_eq!(matches("timestamp_ms == null", &record), Ok(true));
    }

    /// THE READ RULE. A filter can only match what it can read.
    ///
    /// A payload no decoder could make sense of renders as hex. Asking a CEL
    /// expression about it would mean either judging the hex *rendering* (which
    /// no user wrote their filter against) or inventing a value; both are
    /// lies. It is counted as scanned and not matched — while the raw-byte
    /// prefilter, which reads the same bytes the payload is made of, still
    /// matches it.
    #[test]
    fn an_undecodable_value_never_matches_a_cel_filter_but_can_match_raw_bytes() {
        let binary: Vec<u8> = vec![0xfe, 0xff, 0xfe, 0xff, 0x01, 0x02];
        let record = record(Some(b"order-4"), Some(&binary));
        assert_eq!(
            record.value.as_ref().unwrap().encoding,
            Encoding::Hex,
            "the fixture has to be genuinely undecodable"
        );

        // Every one of these is false — including the ones that never mention
        // `value`, because the record as a whole could not be read.
        for expression in ["true", "partition == 3", r#"key == "order-4""#] {
            assert_eq!(
                matches(expression, &record),
                Ok(false),
                "{expression} on an unreadable record"
            );
        }

        // The prefilter reads bytes, so it still finds the key.
        let raw = CompiledQuery::compile(&query(Some("order-4"), None)).unwrap();
        assert!(raw.matches_raw(Some(b"order-4"), Some(&binary), NO_HEADERS));
    }

    /// A binary *key* is a different case: a record with an unreadable key and
    /// a perfectly good value is still worth filtering on, so the key binds as
    /// `null` — the same shape a keyless record has — rather than as hex text
    /// no expression was written against.
    #[test]
    fn an_undecodable_key_binds_as_null() {
        let record = record(Some(&[0xfe, 0xff, 0xfe, 0xff]), Some(br#"{"orderId":7}"#));
        assert_eq!(record.key.as_ref().unwrap().encoding, Encoding::Hex);
        assert_eq!(matches("key == null", &record), Ok(true));
        assert_eq!(matches("value.orderId == 7", &record), Ok(true));
    }

    // --- evaluation failures ---------------------------------------------

    #[test]
    fn a_filter_that_cannot_be_evaluated_says_so_instead_of_matching() {
        let record = record(Some(b"k"), Some(br#"{"orderId":1}"#));
        let error = matches("value.noSuchField == 1", &record).expect_err("no such key");
        assert!(error.contains("value.noSuchField"), "got {error}");
    }

    #[test]
    fn a_filter_that_is_not_a_predicate_is_a_filter_problem() {
        let record = record(Some(b"k"), Some(br#"{"orderId":1}"#));
        let error = matches("value.orderId", &record).expect_err("not a bool");
        assert!(error.contains("true or false"), "got {error}");
    }

    // --- the wire shape ---------------------------------------------------

    /// The Phase 2 IPC contract fixes these shapes and the TypeScript side is
    /// written against them literally, so a rename would compile and ship.
    #[test]
    fn the_ipc_wire_form_is_fixed() {
        let raw = serde_json::json!({
            "topic": "orders",
            "seek": {"kind": "earliest"},
            "partitions": [0, 1],
            "query": {"substring": "order-4", "cel": "value.orderId >= 45"},
            "max_buffered": 5000,
            "max_value_bytes": null,
        });
        let spec: SearchSpec = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(spec.topic, "orders");
        assert_eq!(spec.query.substring.as_deref(), Some("order-4"));
        assert_eq!(serde_json::to_value(&spec).unwrap(), raw);

        let progress = SearchProgress {
            scanned: 412_000,
            matched: 17,
            buffered: 17,
            unevaluated: 4,
            per_partition: vec![PartitionProgress {
                partition: 0,
                current_offset: 900,
                end_offset: 1000,
            }],
            assumed_complete: vec![3, 5],
            msgs_per_sec: 21_000.5,
            done: false,
            error: None,
            filter_error: Some("`value.status` could not be evaluated: no such key".into()),
        };
        assert_eq!(
            serde_json::to_value(&progress).unwrap(),
            serde_json::json!({
                "scanned": 412_000,
                "matched": 17,
                "buffered": 17,
                "unevaluated": 4,
                "per_partition": [{"partition": 0, "current_offset": 900, "end_offset": 1000}],
                "assumed_complete": [3, 5],
                "msgs_per_sec": 21_000.5,
                "done": false,
                "error": null,
                "filter_error": "`value.status` could not be evaluated: no such key",
            })
        );

        // Both halves of a query are optional, and absent means "no constraint".
        let empty: SearchQuery =
            serde_json::from_value(serde_json::json!({"substring": null, "cel": null}))
                .expect("parses");
        assert_eq!(empty, SearchQuery::default());
    }
}

/// The reporting half of the engine, with no cluster involved: the counters and
/// flags a worker sets, read back through the snapshot the UI actually receives.
///
/// Gated on `kafka` because [`SearchShared`] is — it holds the worker
/// bookkeeping — but nothing here talks to a broker. **`assumed_complete` is
/// covered here rather than by an integration test**: reproducing it live means
/// a partition whose captured watermark is unreachable, which needs a
/// transactional producer writing markers (or a broker that stops answering
/// mid-scan) plus a five-second quiet deadline per run. The branch that sets it
/// is two lines in `scan`; what is worth pinning down is that a partition
/// completed by deadline is reported and one completed normally is not.
#[cfg(all(test, feature = "kafka"))]
mod reporting {
    use super::*;

    fn shared(ends: &[i64]) -> SearchShared {
        SearchShared {
            results: Mutex::new(ResultQueue {
                records: VecDeque::new(),
                ended: false,
            }),
            ready: Condvar::new(),
            cancel: CancelToken::new(),
            scanned: AtomicU64::new(0),
            matched: AtomicU64::new(0),
            unevaluated: AtomicU64::new(0),
            filter_error: Mutex::new(None),
            buffered: AtomicU32::new(0),
            max_buffered: 10,
            cursors: ends
                .iter()
                .enumerate()
                .map(|(i, end)| PartitionCursor {
                    partition: i32::try_from(i).expect("a test fixture fits i32"),
                    current: AtomicI64::new(0),
                    end: *end,
                    assumed: AtomicBool::new(false),
                })
                .collect(),
            live_workers: AtomicUsize::new(1),
            started: Instant::now(),
            elapsed_nanos: AtomicU64::new(0),
            error: Mutex::new(None),
        }
    }

    /// A partition that ran out of records before its watermark is COMPLETE and
    /// SAID SO. Both halves matter: the bar has to reach the end (the records
    /// between here and the watermark do not exist), and the UI has to be able
    /// to name the partitions where that happened, because the other reason a
    /// partition goes quiet is a broker that stopped answering.
    #[test]
    fn a_partition_completed_by_the_quiet_deadline_is_reported_as_assumed() {
        let shared = shared(&[100, 100, 100]);
        shared.complete(0);
        shared.complete_by_deadline(1);
        // Partition 2 is left mid-scan, as a cancelled one would be.
        shared.advance(2, 40);

        let progress = shared.snapshot();
        assert_eq!(progress.assumed_complete, vec![1]);
        assert_eq!(progress.per_partition[0].current_offset, 100);
        assert_eq!(
            progress.per_partition[1].current_offset, 100,
            "a deadline still completes the partition — it is not a stall"
        );
        assert_eq!(progress.per_partition[2].current_offset, 40);
    }

    /// The default is the quiet one: a search that reached every watermark has
    /// nothing to disclose, and an empty list is what lets the UI say nothing.
    #[test]
    fn a_search_that_reached_every_watermark_lists_no_assumptions() {
        let shared = shared(&[10, 10]);
        shared.complete(0);
        shared.complete(1);
        assert!(shared.snapshot().assumed_complete.is_empty());
    }

    /// EVERY failure is counted, and the FIRST is the one kept — the count is
    /// what tells the user "this filter couldn't judge 17 records", and one
    /// example is what tells them why. Keeping the last would mean the message
    /// changed under them as the scan ran.
    #[test]
    fn evaluation_failures_are_counted_and_the_first_one_is_kept() {
        let shared = shared(&[10]);
        assert_eq!(shared.snapshot().unevaluated, 0);
        assert_eq!(shared.snapshot().filter_error, None);

        shared.note_unevaluated("`value.n` could not be evaluated: at offset 1".into());
        shared.note_unevaluated("`value.n` could not be evaluated: at offset 2".into());
        shared.note_unevaluated("`value.n` could not be evaluated: at offset 3".into());

        let progress = shared.snapshot();
        assert_eq!(progress.unevaluated, 3);
        assert_eq!(
            progress.filter_error.as_deref(),
            Some("`value.n` could not be evaluated: at offset 1"),
        );
        // A filter that could not judge some records is not a failed search.
        assert_eq!(progress.error, None);
    }
}

/// The half of the engine that needs a cluster *and* a producer. It lives here
/// rather than in `tests/search_local.rs` for the same reason
/// `src/consume.rs`'s tail test does: rdkafka is a feature-gated dependency of
/// this crate, and a `dev-dependency` on it would make plain
/// `cargo test -p kavka-core` require CMake, breaking the bare-toolchain tier
/// the feature split exists to protect (see Cargo.toml).
///
/// Run:  docker compose -f dev/docker-compose.yml up -d --wait
///       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl
#[cfg(all(test, feature = "kafka"))]
mod cluster {
    use super::*;
    use crate::profiles::{AuthConfig, ConnectionProfile, Environment};
    use rdkafka::config::ClientConfig;
    use rdkafka::producer::{BaseProducer, BaseRecord, Producer};

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

    /// The throughput gate is off by default: it seeds 200,000 messages, which
    /// is a minute of setup nobody wants on every `cargo test`.
    fn perf() -> bool {
        if std::env::var("KAVKA_PERF").is_err() {
            eprintln!("skipped: set KAVKA_PERF=1 (with KAVKA_IT=1) to run the throughput gate");
            return false;
        }
        true
    }

    fn connection(read_only: bool) -> ClusterConnection {
        ClusterConnection::connect(ConnectionProfile {
            id: "it-search".into(),
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
    /// crate: kavka-core has no uuid dependency and a test fixture is not worth
    /// adding one for.
    fn unique_topic(what: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("kavka-it-{what}-{}-{nanos:x}", std::process::id())
    }

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

    /// Seeds `count` small keyed JSON messages spread over the topic's
    /// partitions, and returns when the broker has acknowledged all of them.
    fn seed(topic: &str, count: usize, partitions: i32) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .set("queue.buffering.max.messages", "1000000")
            .set("batch.num.messages", "10000")
            .set("linger.ms", "20")
            .create()
            .expect("producer");
        for i in 0..count {
            let key = format!("k-{i}");
            let payload = format!(r#"{{"n":{i},"tag":"bulk","pad":"aaaaaaaaaaaaaaaa"}}"#);
            let partition = i32::try_from(i).expect("fixture fits i32") % partitions;
            if i % 1_000 == 0 {
                // Drains delivery events so the client's queue does not grow
                // for the whole seed.
                producer.poll(Duration::ZERO);
            }
            loop {
                let sent = producer.send(
                    BaseRecord::to(topic)
                        .key(&key)
                        .payload(&payload)
                        .partition(partition),
                );
                match sent {
                    Ok(()) => break,
                    // The local queue is full; let it drain rather than
                    // dropping the record and seeding a topic that is short.
                    Err((_, _)) => {
                        producer.poll(Duration::from_millis(50));
                    }
                }
            }
        }
        producer.flush(Duration::from_secs(120)).expect("flush");
    }

    /// Seeds exactly the records given, in order, on partition 0 — the fixture
    /// for questions about the *shape* of a topic rather than its size.
    fn seed_records(topic: &str, records: &[(&str, &str)]) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .create()
            .expect("producer");
        for (key, payload) in records {
            producer
                .send(
                    BaseRecord::to(topic)
                        .key(*key)
                        .payload(*payload)
                        .partition(0),
                )
                .expect("queue a record");
        }
        producer.flush(Duration::from_secs(30)).expect("flush");
    }

    fn spec(topic: &str, query: SearchQuery) -> SearchSpec {
        SearchSpec {
            topic: topic.into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            query,
            max_buffered: MAX_BUFFERED,
            max_value_bytes: None,
        }
    }

    /// Drains a session to completion, returning everything it buffered.
    fn drain(session: &SearchSession) -> Vec<MessageRecord> {
        let mut all = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(120);
        while let Some(batch) = session.next_results(Duration::from_millis(250)) {
            all.extend(batch);
            assert!(Instant::now() < deadline, "the search never finished");
        }
        all
    }

    /// CANCELLATION, mid-scan. Stopped at the first sign of life, a search has
    /// to stop *promptly*, keep what it already found, report `done` — and
    /// leave no thread behind when the session is dropped.
    ///
    /// The fixture is 50,000 messages rather than the few thousand that would
    /// prove the mechanism, because the assertion "this stopped part-way" is
    /// only meaningful while the scan is still running: on this cluster the
    /// engine reads a few thousand records faster than a test can notice it
    /// started, and a fixture that finishes first would make the test pass for
    /// the wrong reason. If this ever fails on `scanned`, the machine got
    /// faster — raise `COUNT`.
    #[test]
    fn a_cancelled_search_stops_promptly_and_leaves_no_threads_behind() {
        if !integration() {
            return;
        }
        const COUNT: usize = 50_000;
        const BUFFER: u32 = 2_000;

        let conn = connection(false);
        let topic = unique_topic("cancel");
        crate::admin::create_topic(&conn, &topic, 6, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 6);
        seed(&topic, COUNT, 6);

        let session = SearchSession::start(
            &conn,
            None,
            &SearchSpec {
                max_buffered: BUFFER,
                ..spec(&topic, SearchQuery::default())
            },
        )
        .expect("start search");

        // Cancelling a search that never started proves nothing, so wait for
        // the scan to actually be moving — and no longer than that.
        let deadline = Instant::now() + Duration::from_secs(30);
        while session.progress().scanned == 0 {
            assert!(Instant::now() < deadline, "the search never got going");
            std::thread::sleep(Duration::from_millis(1));
        }

        let stopped_at = Instant::now();
        session.stop();
        session.stop(); // idempotent

        let mut progress = session.progress();
        while !progress.done {
            assert!(
                stopped_at.elapsed() < Duration::from_secs(10),
                "stop() did not end the search: {progress:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
            progress = session.progress();
        }
        let took = stopped_at.elapsed();
        eprintln!(
            "cancelled after {} of {COUNT} scanned ({} matched, {} buffered) in {took:?}",
            progress.scanned, progress.matched, progress.buffered
        );
        assert!(
            took < Duration::from_secs(5),
            "cancellation is noticed within a poll, not at a deadline: {took:?}"
        );
        assert!(progress.error.is_none(), "cancelling is not a failure");
        // The watermarks are read from the cluster, so this is the fixture
        // checking itself: "it stopped part-way" means nothing if the topic
        // turned out to hold a hundred records.
        let seeded: i64 = progress
            .per_partition
            .iter()
            .map(|p| p.end_offset)
            .sum::<i64>();
        assert_eq!(
            seeded, COUNT as i64,
            "the fixture has to actually hold {COUNT} records"
        );
        assert!(
            progress.scanned > 0 && progress.scanned < COUNT as u64,
            "a cancelled scan is partial, got {} of {COUNT}",
            progress.scanned
        );
        assert!(
            progress
                .per_partition
                .iter()
                .any(|p| p.current_offset < p.end_offset),
            "a cancelled scan leaves at least one partition short of its end"
        );

        // Partial results survive: cancellation is Kavka's own doing, and what
        // was already found is still an answer.
        let partial = drain(&session);
        assert!(!partial.is_empty(), "a match-all search matches everything");
        assert_eq!(
            partial.len(),
            usize::try_from(progress.buffered).unwrap(),
            "everything that was buffered is still delivered"
        );

        // The thread-leak check. `Drop` joins every worker, so a worker that
        // outlived its session would block here — and 50,000 records is far
        // more than the scan had left to do when it was told to stop, so a
        // `drop` that takes seconds means a thread that never noticed.
        let dropped_at = Instant::now();
        drop(session);
        assert!(
            dropped_at.elapsed() < Duration::from_secs(5),
            "dropping the session had to join a worker that was still running"
        );

        let _ = crate::admin::delete_topic(&conn, &topic);
    }

    /// A FILTER THAT CANNOT JUDGE EVERY RECORD SAYS HOW MANY IT COULD NOT.
    ///
    /// The fixture is the case this is really about: a topic holding two shapes
    /// at once, which is what a real topic looks like a year in. `value.n` is a
    /// perfectly good question about half of it and an error about the other
    /// half, and the failure mode this test exists to prevent is the one where
    /// the UI reports "6 matches, 12 scanned" and never mentions that the other
    /// six were never judged at all.
    ///
    /// The search still *finishes* — an unjudgeable record is not a failed
    /// search, it is a heterogeneous topic.
    #[test]
    fn a_filter_that_cannot_judge_some_records_counts_them_and_still_finishes() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let topic = unique_topic("unevaluated");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);

        let mut fixture: Vec<(String, String)> = Vec::new();
        for i in 0..6 {
            fixture.push((format!("json-{i}"), format!(r#"{{"n":{i}}}"#)));
            fixture.push((format!("text-{i}"), format!("record {i}, not json at all")));
        }
        let borrowed: Vec<(&str, &str)> = fixture
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        seed_records(&topic, &borrowed);

        let session = SearchSession::start(
            &conn,
            None,
            &spec(
                &topic,
                SearchQuery {
                    substring: None,
                    cel: Some("value.n >= 0".into()),
                },
            ),
        )
        .expect("start search");
        let found = drain(&session);
        let progress = session.progress();

        assert!(progress.done, "the search ran to the end");
        assert_eq!(
            progress.error, None,
            "a filter that cannot judge a record is not a failed search"
        );
        assert_eq!(progress.scanned, 12, "every record was read");
        assert_eq!(progress.matched, 6, "the six JSON records answered true");
        assert_eq!(found.len(), 6);
        assert_eq!(
            progress.unevaluated, 6,
            "the six text records could not be judged, and that is reported"
        );
        let explanation = progress
            .filter_error
            .expect("an unevaluated count comes with an example");
        assert!(
            explanation.contains("value.n"),
            "the example has to name the expression: got {explanation}"
        );

        let _ = crate::admin::delete_topic(&conn, &topic);
    }

    /// THE THROUGHPUT GATE (docs/ROADMAP.md Phase 2). The 1M msgs/min number is
    /// a CI-hardware gate that lands with the perf topic; this is the local
    /// floor that catches an engine which has stopped streaming — an accidental
    /// per-record allocation, a lock in the hot loop, a decode that should have
    /// been skipped.
    #[test]
    fn the_engine_sustains_the_local_throughput_floor() {
        if !integration() || !perf() {
            return;
        }
        const COUNT: usize = 200_000;
        const FLOOR_PER_MIN: f64 = 200_000.0;

        let conn = connection(false);
        let topic = unique_topic("perf");
        crate::admin::create_topic(&conn, &topic, 6, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 6);
        let seeding = Instant::now();
        seed(&topic, COUNT, 6);
        eprintln!("seeded {COUNT} messages in {:?}", seeding.elapsed());

        // A substring that matches a handful of records: this measures the scan
        // path — poll, prefilter, count — which is what the gate is about.
        let wall = Instant::now();
        let session = SearchSession::start(
            &conn,
            None,
            &spec(
                &topic,
                SearchQuery {
                    substring: Some("\"n\":199999".into()),
                    cel: None,
                },
            ),
        )
        .expect("start search");
        let found = drain(&session);
        let wall = wall.elapsed();
        let progress = session.progress();

        assert_eq!(
            progress.scanned, COUNT as u64,
            "the whole topic has to be scanned"
        );
        assert!(!found.is_empty(), "the needle is in the topic");

        let per_min = progress.msgs_per_sec * 60.0;
        eprintln!(
            "PERF substring scan: {} messages in {:.2}s = {:.0} msgs/s = {:.0} msgs/min \
             (wall clock incl. start + drain: {wall:?})",
            progress.scanned,
            progress.scanned as f64 / progress.msgs_per_sec,
            progress.msgs_per_sec,
            per_min
        );

        // Informational second number: the same topic with a CEL filter, which
        // decodes every record. Not gated — it measures serde_json plus the
        // interpreter, not the search engine — but it is the number that tells
        // you what a filtered search costs.
        let filtered = SearchSession::start(
            &conn,
            None,
            &spec(
                &topic,
                SearchQuery {
                    substring: None,
                    cel: Some("value.n >= 199990".into()),
                },
            ),
        )
        .expect("start search");
        let hits = drain(&filtered);
        let filtered_progress = filtered.progress();
        eprintln!(
            "PERF cel scan:       {} messages in {:.2}s = {:.0} msgs/s = {:.0} msgs/min ({} matched)",
            filtered_progress.scanned,
            filtered_progress.scanned as f64 / filtered_progress.msgs_per_sec,
            filtered_progress.msgs_per_sec,
            filtered_progress.msgs_per_sec * 60.0,
            hits.len(),
        );

        let _ = crate::admin::delete_topic(&conn, &topic);

        assert!(
            per_min >= FLOOR_PER_MIN,
            "scanned {per_min:.0} msgs/min, below the {FLOOR_PER_MIN:.0} floor"
        );
    }
}
