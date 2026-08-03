//! Produce: one record, or a bulk generator — the write half of the message
//! browser (Phase 2).
//!
//! Everything here BLOCKS, like the rest of the core
//! (docs/ARCHITECTURE.md); the Tauri shell wraps each call in its `blocking()`
//! helper. Every entry point calls [`ClusterConnection::ensure_writable`]
//! **first** — before a template is parsed, before a schema is fetched and
//! before a socket is opened — so a read-only connection costs a rejected call
//! rather than a round trip (D5: read-only is enforced in core, not the UI).
//!
//! # Why `BaseProducer` rather than `FutureProducer`
//!
//! `FutureProducer::send` hands back a future that only resolves on a runtime,
//! so every caller here would end in a `block_on`: an async client driven from
//! a blocking core, which is the shape this crate exists to avoid.
//! `BaseProducer` is the blocking half of the same client — `send` enqueues,
//! `poll`/`flush` drive delivery on the calling thread, and delivery reports
//! arrive on [`rdkafka::producer::ProducerContext::delivery`]. It also carries
//! [`KavkaClientContext`], which is what mints OAUTHBEARER/MSK tokens; a
//! `FutureProducer` would need that same context *and* a runtime.
//!
//! ([`crate::admin`] does use `block_on`, because rdkafka's AdminClient offers
//! no blocking API at all. Producing does, so it uses it.)
//!
//! # Why the producer is per call rather than cached on the connection
//!
//! [`send`] builds a producer, uses it, flushes it and drops it. A producer
//! cached on the connection would have to be polled by *somebody* forever to
//! serve delivery reports and token refreshes — a permanent thread per
//! connection — and would hold a second set of broker connections open for a
//! feature most sessions use a handful of times. The per-call cost is one
//! connect and handshake on a human-initiated action, which is invisible next
//! to the click that caused it.
//!
//! Bulk is the case where that cost would matter, and it is exactly the case
//! that already amortizes it: [`BulkSession`] holds one producer for its whole
//! run.
//!
//! # Delivery is confirmed, never assumed
//!
//! `acks=all` plus a flush before answering, everywhere. `send` returns the
//! partition and offset **the broker assigned**, and [`BulkProgress::sent`]
//! counts acknowledged records rather than records handed to librdkafka — a
//! progress bar that fills before anything reaches the cluster is a lie, and
//! the one Kavka would be caught in.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "kafka")]
use crate::cancel::CancelToken;
#[cfg(feature = "kafka")]
use crate::connection::auth::KavkaClientContext;
#[cfg(feature = "kafka")]
use crate::connection::ClusterConnection;
#[cfg(feature = "kafka")]
use crate::profiles::SchemaRegistryConfig;
#[cfg(feature = "kafka")]
use rdkafka::config::ClientConfig;
#[cfg(feature = "kafka")]
use rdkafka::error::{KafkaError, RDKafkaErrorCode};
#[cfg(feature = "kafka")]
use rdkafka::message::{Header, Message, OwnedHeaders};
#[cfg(feature = "kafka")]
use rdkafka::producer::{BaseProducer, BaseRecord, DeliveryResult, Producer, ProducerContext};
#[cfg(feature = "kafka")]
use rdkafka::util::Timeout;
#[cfg(feature = "kafka")]
use std::sync::atomic::AtomicBool;
#[cfg(feature = "kafka")]
use std::sync::{Arc, Mutex, MutexGuard};
#[cfg(feature = "kafka")]
use std::time::{Duration, Instant};

#[cfg(not(feature = "kafka"))]
use crate::connection::ClusterConnection;
#[cfg(not(feature = "kafka"))]
use crate::profiles::SchemaRegistryConfig;

/// Hard cap on one bulk run, applied in core rather than trusted from the
/// caller — the same rule as [`crate::consume::MAX_MESSAGES`]. A UI bug asking
/// for ten million records must cost a truncated run, not a cluster.
pub const MAX_BULK: u32 = 100_000;

// ---------------------------------------------------------------------------
// Wire types. Field names are the IPC contract — the TypeScript in
// apps/desktop/src mirrors them exactly, so renaming one is a breaking change
// on both sides of the bridge. Deliberately outside the `kafka` gate: the
// shapes have to compile (and be asserted) on the bare tier.
// ---------------------------------------------------------------------------

/// What to put in a record's value.
///
/// `avro` encodes `json` against the **latest** schema registered for
/// `subject` and frames it the Confluent way — the exact bytes
/// [`crate::serdes::decode`] reads back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProduceValueSpec {
    Text {
        text: String,
    },
    Json {
        json: serde_json::Value,
    },
    Avro {
        subject: String,
        json: serde_json::Value,
    },
}

/// One header on a produced record. Kafka allows a null header value; the
/// produce form does not offer one, so this side of the bridge takes a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProduceHeader {
    pub key: String,
    pub value: String,
}

/// One record to produce.
///
/// A `value` of `None` is a **tombstone** — a null payload, which is a
/// different fact from an empty one on a compacted topic (docs/DESIGN.md
/// §5.2), and is carried in the type for exactly that reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProduceRecordSpec {
    pub key: Option<String>,
    pub value: Option<ProduceValueSpec>,
    #[serde(default)]
    pub headers: Vec<ProduceHeader>,
    /// `None` lets Kafka's partitioner choose — by key hash when there is a
    /// key, round-robin when there is not.
    pub partition: Option<i32>,
}

/// Where a produced record landed, as the broker reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub partition: i32,
    pub offset: i64,
}

/// One bulk generation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkSpec {
    /// Capped at [`MAX_BULK`].
    pub count: u32,
    /// Pause between records. `0` sends as fast as delivery allows, with the
    /// in-flight window bounded so the run cannot outrun its own memory.
    pub interval_ms: u32,
    /// `None` produces records with no key.
    pub key_template: Option<String>,
    pub value_template: String,
    pub partition: Option<i32>,
}

impl BulkSpec {
    /// How many records this run will actually produce.
    ///
    /// The cap is applied here rather than trusted from the caller, and it is
    /// public so the produce form can say *"Kavka will send 100 000 — the most
    /// one run does"* before the run starts, instead of the user discovering
    /// the truncation from the progress counter.
    pub fn capped_count(&self) -> u32 {
        self.count.min(MAX_BULK)
    }
}

/// A bulk run's progress. Serializes to exactly the `kavka://bulk/{id}` event
/// payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkProgress {
    /// Records the broker **acknowledged**, not records handed to librdkafka.
    pub sent: u32,
    pub failed: u32,
    pub done: bool,
    /// The reason the run ended early, or — when it did not end early but some
    /// records were rejected — the first rejection, so the UI can say why
    /// `failed` is not zero.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// The template engine (bulk generation).
// ---------------------------------------------------------------------------

/// The placeholder vocabulary, in one place: the parser's error messages, the
/// produce form's hint text and docs/FEATURES.md all quote this list, and three
/// copies of it would drift within a phase.
pub const PLACEHOLDERS: &str =
    "{{seq}}, {{uuid}}, {{now_iso}}, {{now_ms}}, {{rand_int A B}} and {{choice a|b|c}}";

/// A compiled bulk template.
///
/// Parsed once, at [`BulkSession::start`], so a typo is an error from `start`
/// rather than a run that cheerfully produces 100 000 records containing the
/// literal text `{{sq}}`.
///
/// # What is and is not a placeholder
///
/// A placeholder is `{{`, a lowercase identifier, optional arguments, `}}`.
/// Anything else between doubled braces is **literal text**, which is what
/// keeps a JSON template like `{{"nested": 1}}` working instead of being
/// rejected as an unknown placeholder. A doubled brace followed by an
/// identifier Kavka does not know is still an error — that is the typo case,
/// and silently passing it through is how `{{sq}}` ends up in a topic.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, PartialEq)]
enum Chunk {
    Literal(String),
    Seq,
    Uuid,
    NowIso,
    NowMs,
    /// Inclusive at both ends.
    RandInt {
        low: i64,
        high: i64,
    },
    Choice(Vec<String>),
}

impl Template {
    /// Compiles a template, or says where it went wrong.
    ///
    /// Every error names the **byte offset of the `{{` that caused it**, so the
    /// produce form can put the caret there rather than making the user hunt
    /// through a 400-character line (docs/DESIGN.md §7: state what happened,
    /// then the next click).
    pub fn parse(source: &str) -> Result<Self> {
        let mut chunks = Vec::new();
        let mut literal = String::new();
        let bytes = source.as_bytes();
        let mut at = 0;

        while at < bytes.len() {
            if !source[at..].starts_with("{{") {
                // By character, not by byte: `source[at..=at]` would slice a
                // multi-byte character in half and panic on the first template
                // anyone writes in a language other than English.
                let next = source[at..].chars().next().expect("at < len");
                literal.push(next);
                at += next.len_utf8();
                continue;
            }
            let Some(close) = source[at + 2..].find("}}") else {
                // Only a real placeholder deserves this error: a stray `{{` in
                // prose or JSON is literal text (see the type's docs).
                if !opens_a_placeholder(&source[at + 2..]) {
                    literal.push('{');
                    at += 1;
                    continue;
                }
                return Err(Error::Other(format!(
                    "unclosed {{{{ at position {at} — a placeholder ends with }}}}"
                )));
            };
            let inner = source[at + 2..at + 2 + close].trim();
            if !opens_a_placeholder(inner) {
                literal.push('{');
                at += 1;
                continue;
            }
            if !literal.is_empty() {
                chunks.push(Chunk::Literal(std::mem::take(&mut literal)));
            }
            chunks.push(placeholder(inner, at)?);
            at += close + 4;
        }
        if !literal.is_empty() {
            chunks.push(Chunk::Literal(literal));
        }
        Ok(Self { chunks })
    }

    /// Renders one record's worth of text.
    ///
    /// `seq` is 1-based (the first record of a run is `{{seq}}` = 1). The clock
    /// and the generator are arguments rather than ambient state so a run is
    /// reproducible from a seed — which is what makes every placeholder,
    /// including `{{uuid}}`, testable rather than merely plausible.
    pub fn render(&self, seq: u64, now_ms: i64, rng: &mut Rng) -> String {
        let mut out = String::new();
        for chunk in &self.chunks {
            match chunk {
                Chunk::Literal(text) => out.push_str(text),
                Chunk::Seq => out.push_str(&seq.to_string()),
                Chunk::Uuid => out.push_str(&uuid_v4(rng)),
                Chunk::NowIso => out.push_str(&iso8601(now_ms)),
                Chunk::NowMs => out.push_str(&now_ms.to_string()),
                Chunk::RandInt { low, high } => {
                    out.push_str(&rng.int_between(*low, *high).to_string())
                }
                Chunk::Choice(options) => {
                    let pick = rng.int_between(0, options.len() as i64 - 1) as usize;
                    out.push_str(&options[pick]);
                }
            }
        }
        out
    }

    /// One record's worth of text against the wall clock — what the produce
    /// form previews before a run is started, and what a caller with no
    /// opinion about the clock wants.
    pub fn render_now(&self, seq: u64, rng: &mut Rng) -> String {
        self.render(seq, now_ms(), rng)
    }

    /// Whether this template renders the same text every time — true only when
    /// it has no placeholder at all, or only `{{seq}}`.
    #[cfg(test)]
    fn is_deterministic(&self) -> bool {
        self.chunks
            .iter()
            .all(|c| matches!(c, Chunk::Literal(_) | Chunk::Seq))
    }
}

/// Whether text between doubled braces looks like a placeholder at all: a
/// name that starts with a lowercase letter, optionally followed by arguments.
///
/// The rule is what keeps `{{"nested": 1}}` and `{{1}}` as literal JSON while
/// still catching `{{sq}}` as a typo. Everything Kavka knows is lowercase, so
/// nothing legitimate is excluded by it.
fn opens_a_placeholder(inner: &str) -> bool {
    let name = inner.split_whitespace().next().unwrap_or("");
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
}

fn placeholder(inner: &str, at: usize) -> Result<Chunk> {
    let (name, rest) = match inner.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim()),
        None => (inner, ""),
    };
    let no_arguments = |chunk: Chunk| {
        if rest.is_empty() {
            Ok(chunk)
        } else {
            Err(Error::Other(format!(
                "{{{{{name}}}}} at position {at} takes no arguments"
            )))
        }
    };
    match name {
        "seq" => no_arguments(Chunk::Seq),
        "uuid" => no_arguments(Chunk::Uuid),
        "now_iso" => no_arguments(Chunk::NowIso),
        "now_ms" => no_arguments(Chunk::NowMs),
        "rand_int" => {
            let mut bounds = rest.split_whitespace();
            let parsed = (bounds.next(), bounds.next(), bounds.next());
            let (Some(low), Some(high), None) = parsed else {
                return Err(Error::Other(format!(
                    "{{{{rand_int}}}} at position {at} takes two whole numbers, low then high — \
                     {{{{rand_int 1 100}}}}"
                )));
            };
            let (Ok(low), Ok(high)) = (low.parse::<i64>(), high.parse::<i64>()) else {
                return Err(Error::Other(format!(
                    "{{{{rand_int {low} {high}}}}} at position {at} takes two whole numbers, low \
                     then high — {{{{rand_int 1 100}}}}"
                )));
            };
            if low > high {
                return Err(Error::Other(format!(
                    "{{{{rand_int {low} {high}}}}} at position {at} has its bounds the wrong way \
                     round — the low one comes first"
                )));
            }
            Ok(Chunk::RandInt { low, high })
        }
        "choice" => {
            if rest.is_empty() {
                return Err(Error::Other(format!(
                    "{{{{choice}}}} at position {at} needs at least one option, separated by | — \
                     {{{{choice a|b|c}}}}"
                )));
            }
            Ok(Chunk::Choice(
                rest.split('|').map(|o| o.trim().to_string()).collect(),
            ))
        }
        other => Err(Error::Other(format!(
            "unknown placeholder {{{{{other}}}}} at position {at} — Kavka knows {PLACEHOLDERS}"
        ))),
    }
}

/// A small, fast, **non-cryptographic** generator (SplitMix64).
///
/// It seeds bulk test data, and nothing else: no key material, no tokens, no
/// identifiers anything trusts. Kavka has no `rand` dependency and this is not
/// a reason to add one — `getrandom`/`rand` would pull a build-tool
/// requirement onto the bare tier (see Cargo.toml) to make sample orders
/// slightly less predictable.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator whose whole output is fixed by `seed` — the form the tests
    /// use, and the reason `{{uuid}}` is testable at all.
    pub fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    /// Seeded from the clock, the process id and a per-process counter, so two
    /// bulk runs started in the same millisecond do not produce identical data.
    pub fn from_entropy() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos() as u64);
        Self(
            nanos
                ^ (u64::from(std::process::id()) << 32)
                ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed),
        )
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish over `[low, high]`, both ends included.
    ///
    /// `i128` because the span of `i64::MIN..=i64::MAX` does not fit an `i64`.
    /// The modulo bias is at most one part in 2^64 divided by the span, which
    /// for sample data is not a bias anyone can observe.
    fn int_between(&mut self, low: i64, high: i64) -> i64 {
        let span = i128::from(high) - i128::from(low) + 1;
        let draw = i128::from(self.next_u64()) % span;
        (i128::from(low) + draw) as i64
    }
}

/// A version 4 UUID in the canonical 8-4-4-4-12 form.
///
/// Hand-rolled for the same reason as [`Rng`]: this names a sample record, and
/// a dependency that adds a build-tool requirement to the bare tier is too
/// much to pay for it. The version and variant nibbles are set per RFC 9562
/// §5.4 so the result is a *valid* v4, not merely 32 hex characters.
fn uuid_v4(rng: &mut Rng) -> String {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
    bytes[8..].copy_from_slice(&rng.next_u64().to_le_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut out = String::with_capacity(36);
    for (i, byte) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// RFC 3339 in UTC to millisecond precision — `2023-11-14T22:13:20.000Z`.
///
/// Hand-rolled civil-from-days (Howard Hinnant's algorithm) rather than a
/// `chrono`/`time` dependency: this is the only date formatting in the crate,
/// and the algorithm is shorter than the justification for adding a dependency
/// would be.
fn iso8601(now_ms: i64) -> String {
    let days = now_ms.div_euclid(86_400_000);
    let ms_of_day = now_ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second, milli) = (
        ms_of_day / 3_600_000,
        (ms_of_day / 60_000) % 60,
        (ms_of_day / 1_000) % 60,
        ms_of_day % 1_000,
    );
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
}

/// Days since 1970-01-01 -> (year, month, day). Valid across the whole range a
/// Kafka timestamp can hold.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis()),
    )
    .unwrap_or(i64::MAX)
}

// ---------------------------------------------------------------------------
// Encoding a value. Outside the `kafka` gate on purpose: it is the serde
// pipeline run backwards, has no librdkafka involvement, and is therefore
// testable — Schema Registry and all — on the bare tier.
// ---------------------------------------------------------------------------

/// Turns a [`ProduceValueSpec`] into the bytes that go on the wire.
///
/// `profile_sr` is the connection's registry, needed only by the `avro` kind.
/// Unlike the decode path — where an unreachable registry degrades to hex with
/// the schema id attached (src/sr.rs) — every failure here is an **error**:
/// there is no honest degradation for "encode this against a schema I could
/// not fetch", and producing bytes no consumer can read is worse than not
/// producing at all.
pub fn encode_value(
    value: &ProduceValueSpec,
    profile_sr: Option<&SchemaRegistryConfig>,
) -> Result<Vec<u8>> {
    match value {
        ProduceValueSpec::Text { text } => Ok(text.as_bytes().to_vec()),
        ProduceValueSpec::Json { json } => serde_json::to_vec(json)
            .map_err(|e| Error::Other(format!("this value is not serializable as JSON: {e}"))),
        ProduceValueSpec::Avro { subject, json } => {
            let Some(config) = profile_sr else {
                return Err(Error::Other(format!(
                    "this connection has no Schema Registry, so Kavka can't encode {subject} — add \
                     one in the connection's settings, or send the value as JSON"
                )));
            };
            let registry = crate::sr::SchemaRegistry::new(config);
            let schema = registry.latest_schema(subject)?;
            match &schema.kind {
                crate::sr::SchemaKind::Avro(avro) => {
                    crate::serdes::encode_avro(avro, schema.schema_id, json).map_err(|e| {
                        Error::Other(format!("{subject} version {}: {e}", schema.version))
                    })
                }
                crate::sr::SchemaKind::Json => Err(Error::Other(format!(
                    "{subject} is registered as JSON Schema, not Avro — send this value as JSON"
                ))),
                crate::sr::SchemaKind::Unsupported(kind) => Err(Error::Other(format!(
                    "{subject} is registered as {kind}, which this build cannot encode yet"
                ))),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The producer itself.
// ---------------------------------------------------------------------------

/// How long librdkafka keeps retrying one record before reporting it failed.
/// Deliberately shorter than [`SEND_FLUSH`], so a record that cannot be
/// delivered arrives as a *delivery report naming the reason* rather than as
/// our own flush timeout.
#[cfg(feature = "kafka")]
const SEND_MESSAGE_TIMEOUT: Duration = Duration::from_secs(20);

/// How long [`send`] waits for the delivery report.
#[cfg(feature = "kafka")]
const SEND_FLUSH: Duration = Duration::from_secs(30);

/// A bulk run's per-record timeout and its final drain. Longer than a single
/// send's: a paced run is expected to take minutes, and abandoning its tail
/// would make the counts lie.
#[cfg(feature = "kafka")]
const BULK_MESSAGE_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(feature = "kafka")]
const BULK_FLUSH: Duration = Duration::from_secs(60);

/// How long a cancelled run gives its already-accepted records to settle.
/// Bounded by [`MAX_IN_FLIGHT`], so this is a short wait in practice — and
/// `stop()` has to feel immediate (docs/DESIGN.md §7).
#[cfg(feature = "kafka")]
const CANCEL_FLUSH: Duration = Duration::from_secs(3);

/// Records handed to librdkafka but not yet acknowledged. This is the bound
/// that makes `interval_ms: 0` mean "as fast as delivery allows" rather than
/// "as fast as memory allows": past it the pump polls instead of sending.
#[cfg(feature = "kafka")]
const MAX_IN_FLIGHT: i32 = 5_000;

/// librdkafka's own queue, kept above [`MAX_IN_FLIGHT`] so `QueueFull` is the
/// backstop rather than the mechanism.
#[cfg(feature = "kafka")]
const QUEUE_MAX_MESSAGES: &str = "20000";

/// One poll of the producer: the granularity at which delivery reports are
/// collected, pacing sleeps are served and a cancel is noticed.
#[cfg(feature = "kafka")]
const POLL_SLICE: Duration = Duration::from_millis(50);

#[cfg(feature = "kafka")]
const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

/// Where delivery reports land.
///
/// One per [`send`] call or per [`BulkSession`], handed to librdkafka as the
/// record's delivery opaque and read back on the callback. Counting here rather
/// than at the send site is what makes "sent" mean *acknowledged*.
#[cfg(feature = "kafka")]
#[derive(Debug, Default)]
pub struct DeliverySink {
    delivered: AtomicU64,
    failed: AtomicU64,
    last: Mutex<Option<Delivery>>,
    error: Mutex<Option<String>>,
}

#[cfg(feature = "kafka")]
impl DeliverySink {
    /// Records the broker **acknowledged**, ever.
    ///
    /// The three readers exist because a sink is the only thing that knows
    /// whether a record reached a cluster, and [`crate::xcluster`] produces
    /// through a producer of its own rather than through [`send`] or
    /// [`BulkSession`] — a copy is one long stream of records that already
    /// exist, so neither a per-call producer nor a template engine fits it.
    /// They are reads and nothing else: the counting rule ("sent means
    /// acknowledged") stays here, where it is justified.
    pub fn delivered(&self) -> u64 {
        self.delivered.load(Ordering::Relaxed)
    }

    pub fn failed(&self) -> u64 {
        self.failed.load(Ordering::Relaxed)
    }

    /// The first rejection, verbatim — the broker's own wording, which is what
    /// an expert needs under `Show details` (docs/DESIGN.md §7).
    pub fn first_error(&self) -> Option<String> {
        guard(&self.error).clone()
    }

    fn record(&self, result: &DeliveryResult<'_>) {
        match result {
            Ok(message) => {
                *guard(&self.last) = Some(Delivery {
                    partition: message.partition(),
                    offset: message.offset(),
                });
                self.delivered.fetch_add(1, Ordering::Relaxed);
            }
            Err((cause, _)) => {
                let mut slot = guard(&self.error);
                if slot.is_none() {
                    // Verbatim: the broker's own wording is the thing an expert
                    // needs, and the shell puts it under `Show details`
                    // (docs/DESIGN.md §7).
                    *slot = Some(cause.to_string());
                }
                drop(slot);
                self.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Poison-tolerant, like the rest of the crate: every slot guarded here is an
/// `Option` a panicking holder cannot leave inconsistent, and a permanently
/// unusable session would be the worse failure.
#[cfg(feature = "kafka")]
fn guard<T>(slot: &Mutex<T>) -> MutexGuard<'_, T> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

/// Kavka's one client context grows a producer half here rather than in
/// `src/auth/`: producing is its only user, and the delivery opaque it names
/// ([`DeliverySink`]) is this module's type. Coherence is per crate, not per
/// module, so the impl is at home next to what it serves.
#[cfg(feature = "kafka")]
impl ProducerContext for KavkaClientContext {
    type DeliveryOpaque = Arc<DeliverySink>;

    fn delivery(&self, result: &DeliveryResult<'_>, sink: Arc<DeliverySink>) {
        sink.record(result);
    }
}

/// The producer properties every produce path shares.
///
/// `pub(crate)` for [`crate::xcluster`], which builds its own producer for a
/// cross-cluster copy: `acks=all` is a promise Kavka makes about every count it
/// shows, and a second copy of these four properties somewhere else is how a
/// build ends up with two answers to "is a delivered record durable".
#[cfg(feature = "kafka")]
pub(crate) fn tune(config: &mut ClientConfig, message_timeout: Duration, linger_ms: &str) {
    config
        // Delivery is confirmed by the whole ISR, so a count Kavka shows is a
        // count Kafka has durably.
        .set("acks", "all")
        .set(
            "message.timeout.ms",
            message_timeout.as_millis().to_string(),
        )
        .set("queue.buffering.max.messages", QUEUE_MAX_MESSAGES)
        .set("linger.ms", linger_ms);
}

/// Produces one record and returns where the broker put it.
///
/// Order of operations is load-bearing:
/// 1. [`ClusterConnection::ensure_writable`] — before anything, so a read-only
///    connection never reaches the network (D5).
/// 2. Encode the value, which is where a schema mismatch or an unreachable
///    registry is reported — cheaper to fail here than after a connect.
/// 3. Build the producer, check the topic (and the partition, if one was
///    named) exists.
/// 4. Send, flush, and answer with the delivery report.
#[cfg(feature = "kafka")]
pub fn send(
    conn: &ClusterConnection,
    profile_sr: Option<&SchemaRegistryConfig>,
    topic: &str,
    record: &ProduceRecordSpec,
) -> Result<Delivery> {
    conn.ensure_writable("produce a message")?;

    let payload = match &record.value {
        Some(value) => Some(encode_value(value, profile_sr)?),
        None => None,
    };

    let producer = conn.new_producer(|config| {
        // A single send is a human waiting on a click: batching would only add
        // latency to a batch of one.
        tune(config, SEND_MESSAGE_TIMEOUT, "0");
    })?;
    let partitions = topic_partitions(&producer, topic)?;
    ensure_partition_exists(topic, &partitions, record.partition)?;

    let sink = Arc::new(DeliverySink::default());
    let headers = owned_headers(&record.headers);
    let mut outgoing: BaseRecord<'_, [u8], [u8], Arc<DeliverySink>> =
        BaseRecord::with_opaque_to(topic, Arc::clone(&sink));
    if let Some(key) = record.key.as_deref() {
        outgoing = outgoing.key(key.as_bytes());
    }
    // No `payload` call at all is a null payload — the tombstone.
    if let Some(bytes) = payload.as_deref() {
        outgoing = outgoing.payload(bytes);
    }
    if let Some(partition) = record.partition {
        outgoing = outgoing.partition(partition);
    }
    if let Some(headers) = headers {
        outgoing = outgoing.headers(headers);
    }

    producer
        .send(outgoing)
        .map_err(|(e, _)| Error::Other(format!("producing to {topic} failed: {e}")))?;
    // `flush` polls, so the delivery callback has run by the time it returns.
    let flushed = producer.flush(Timeout::After(SEND_FLUSH));

    if let Some(cause) = guard(&sink.error).clone() {
        return Err(Error::Other(format!(
            "the broker did not accept the message: {cause}"
        )));
    }
    if let Some(delivery) = *guard(&sink.last) {
        return Ok(delivery);
    }
    Err(Error::Other(match flushed {
        Err(e) => format!(
            "producing to {topic} was not confirmed within {}s: {e}",
            SEND_FLUSH.as_secs()
        ),
        Ok(()) => format!(
            "producing to {topic} was not confirmed within {}s",
            SEND_FLUSH.as_secs()
        ),
    }))
}

#[cfg(not(feature = "kafka"))]
pub fn send(
    _conn: &ClusterConnection,
    _profile_sr: Option<&SchemaRegistryConfig>,
    _topic: &str,
    _record: &ProduceRecordSpec,
) -> Result<Delivery> {
    Err(Error::Other("built without the `kafka` feature".into()))
}

/// `None` rather than an empty header set: an empty `OwnedHeaders` still
/// allocates a native header list and attaches it to every record.
#[cfg(feature = "kafka")]
fn owned_headers(headers: &[ProduceHeader]) -> Option<OwnedHeaders> {
    if headers.is_empty() {
        return None;
    }
    let mut owned = OwnedHeaders::new_with_capacity(headers.len());
    for header in headers {
        owned = owned.insert(Header {
            key: &header.key,
            value: Some(&header.value),
        });
    }
    Some(owned)
}

/// The partitions a topic has, so producing to one that isn't there is a
/// sentence rather than a 20-second wait for `message.timeout.ms` to expire on
/// a topic librdkafka keeps asking about.
#[cfg(feature = "kafka")]
fn topic_partitions(producer: &BaseProducer<KavkaClientContext>, topic: &str) -> Result<Vec<i32>> {
    let metadata = producer
        .client()
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
        // The same sentence src/consume.rs uses, deliberately: one wording for
        // one fact, whichever direction the user was moving.
        return Err(Error::Other(format!(
            "this cluster has no topic called {topic:?}"
        )));
    }
    Ok(available)
}

#[cfg(feature = "kafka")]
fn ensure_partition_exists(topic: &str, available: &[i32], requested: Option<i32>) -> Result<()> {
    let Some(partition) = requested else {
        return Ok(());
    };
    if available.contains(&partition) {
        return Ok(());
    }
    Err(Error::Other(format!(
        "{topic} has no partition {partition} to produce to — it has {}, numbered 0 to {}",
        available.len(),
        available.len().saturating_sub(1)
    )))
}

/// A bulk generation run: one producer on one thread, feeding a topic from a
/// template until the count is reached or the caller stops it.
///
/// Same session shape as [`crate::consume::TailSession`] — everything that can
/// fail early fails in [`start`](Self::start) on the calling thread, progress
/// is read from a shared cell, [`stop`](Self::stop) is idempotent and safe from
/// any thread, and `Drop` joins the worker so a dropped session cannot leave a
/// producer writing to a cluster nobody is watching.
#[cfg(feature = "kafka")]
pub struct BulkSession {
    shared: Arc<BulkShared>,
    cancel: CancelToken,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "kafka")]
struct BulkShared {
    sink: Arc<DeliverySink>,
    done: AtomicBool,
    /// The reason the run stopped early, if it did.
    fatal: Mutex<Option<String>>,
}

#[cfg(feature = "kafka")]
impl BulkSession {
    /// Starts a run. `count` is capped at [`MAX_BULK`].
    ///
    /// Everything a user can get wrong is checked here, before a single record
    /// is produced: read-only first, then the templates, then the topic and
    /// partition. A malformed template that only surfaced on record 40 000
    /// would leave 39 999 records of garbage in a topic.
    pub fn start(conn: &ClusterConnection, topic: &str, spec: &BulkSpec) -> Result<Self> {
        conn.ensure_writable("produce messages in bulk")?;

        let count = spec.capped_count();
        let value = Template::parse(&spec.value_template)
            .map_err(|e| Error::Other(format!("the value template is not valid: {e}")))?;
        let key = match &spec.key_template {
            Some(template) => Some(
                Template::parse(template)
                    .map_err(|e| Error::Other(format!("the key template is not valid: {e}")))?,
            ),
            None => None,
        };

        let producer = conn.new_producer(|config| {
            // A little batching: a bulk run is throughput, and 5ms is invisible
            // beside any interval a human would set.
            tune(config, BULK_MESSAGE_TIMEOUT, "5");
        })?;
        let partitions = topic_partitions(&producer, topic)?;
        ensure_partition_exists(topic, &partitions, spec.partition)?;

        let shared = Arc::new(BulkShared {
            sink: Arc::new(DeliverySink::default()),
            done: AtomicBool::new(false),
            fatal: Mutex::new(None),
        });
        let cancel = CancelToken::new();
        let run = BulkRun {
            topic: topic.to_string(),
            count,
            interval: Duration::from_millis(u64::from(spec.interval_ms)),
            partition: spec.partition,
            key,
            value,
        };

        let worker = {
            let shared = Arc::clone(&shared);
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("kavka-bulk".into())
                .spawn(move || {
                    pump(&producer, &run, &shared, &cancel);
                    shared.done.store(true, Ordering::Relaxed);
                })
                .map_err(|e| Error::Other(format!("starting the bulk produce thread: {e}")))?
        };

        Ok(Self {
            shared,
            cancel,
            worker: Some(worker),
        })
    }

    /// A snapshot of the run, for the throttled progress event.
    pub fn progress(&self) -> BulkProgress {
        let sink = &self.shared.sink;
        let fatal = guard(&self.shared.fatal).clone();
        BulkProgress {
            sent: sink
                .delivered
                .load(Ordering::Relaxed)
                .min(u64::from(u32::MAX)) as u32,
            failed: sink.failed.load(Ordering::Relaxed).min(u64::from(u32::MAX)) as u32,
            done: self.shared.done.load(Ordering::Relaxed),
            // A run that finished with some records rejected has no fatal
            // error but still owes the user the reason `failed` is not zero.
            error: fatal.or_else(|| guard(&sink.error).clone()),
        }
    }

    /// Asks the run to stop. Idempotent, safe from any thread, and returns
    /// immediately — records already accepted by librdkafka are flushed first
    /// so the final counts are true rather than merely prompt.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

/// Deliberately not derived: a derived form would print the templates, which
/// are user text, into any log line that touches a session.
#[cfg(feature = "kafka")]
impl std::fmt::Debug for BulkSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let progress = self.progress();
        f.debug_struct("BulkSession")
            .field("sent", &progress.sent)
            .field("failed", &progress.failed)
            .field("done", &progress.done)
            .finish()
    }
}

#[cfg(feature = "kafka")]
impl Drop for BulkSession {
    /// Joins the pump, so the producer — and its broker connections — are gone
    /// before `drop` returns.
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Everything the pump needs, so it takes one argument rather than seven.
#[cfg(feature = "kafka")]
struct BulkRun {
    topic: String,
    count: u32,
    interval: Duration,
    partition: Option<i32>,
    key: Option<Template>,
    value: Template,
}

#[cfg(feature = "kafka")]
fn pump(
    producer: &BaseProducer<KavkaClientContext>,
    run: &BulkRun,
    shared: &BulkShared,
    cancel: &CancelToken,
) {
    let mut rng = Rng::from_entropy();
    let mut cancelled = false;

    for seq in 1..=u64::from(run.count) {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        let clock = now_ms();
        let value = run.value.render(seq, clock, &mut rng);
        let key = run.key.as_ref().map(|t| t.render(seq, clock, &mut rng));

        // The in-flight bound. Polling here is not a stall: it is what collects
        // the delivery reports that let the window drain.
        while producer.in_flight_count() >= MAX_IN_FLIGHT && !cancel.is_cancelled() {
            producer.poll(Timeout::After(POLL_SLICE));
        }
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }

        let mut outgoing: BaseRecord<'_, [u8], [u8], Arc<DeliverySink>> =
            BaseRecord::with_opaque_to(&run.topic, Arc::clone(&shared.sink));
        if let Some(key) = key.as_deref() {
            outgoing = outgoing.key(key.as_bytes());
        }
        outgoing = outgoing.payload(value.as_bytes());
        if let Some(partition) = run.partition {
            outgoing = outgoing.partition(partition);
        }

        loop {
            match producer.send(outgoing) {
                Ok(()) => break,
                Err((e, returned)) if is_queue_full(&e) => {
                    // Backpressure, not a failure: drain and offer it again.
                    producer.poll(Timeout::After(POLL_SLICE));
                    if cancel.is_cancelled() {
                        cancelled = true;
                        break;
                    }
                    outgoing = returned;
                }
                Err((e, _)) => {
                    *guard(&shared.fatal) = Some(format!("producing to {} failed: {e}", run.topic));
                    cancelled = true;
                    break;
                }
            }
        }
        if cancelled {
            break;
        }
        pace(producer, run.interval, cancel);
    }

    // Whatever librdkafka already accepted is delivered and counted, so the
    // final numbers describe the cluster rather than our intentions.
    let drain = if cancelled { CANCEL_FLUSH } else { BULK_FLUSH };
    if let Err(e) = producer.flush(Timeout::After(drain)) {
        let mut fatal = guard(&shared.fatal);
        if fatal.is_none() {
            *fatal = Some(format!(
                "some records were still unsent after {}s: {e}",
                drain.as_secs()
            ));
        }
    }
}

/// Waits out one `interval_ms` gap. `poll` doubles as the sleep: it serves
/// delivery reports while it waits, and it is what makes a cancel land within
/// [`POLL_SLICE`] rather than at the end of a long interval.
#[cfg(feature = "kafka")]
fn pace(producer: &BaseProducer<KavkaClientContext>, interval: Duration, cancel: &CancelToken) {
    if interval.is_zero() {
        // Still poll: with no pause at all, nothing else would collect delivery
        // reports until the in-flight bound was hit.
        producer.poll(Timeout::After(Duration::ZERO));
        return;
    }
    let deadline = Instant::now() + interval;
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
// Tests. The wire shapes and the template engine are asserted on the bare tier
// (no librdkafka, no cluster); the roundtrips need both and live below them.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod wire {
    use super::*;

    #[test]
    fn value_specs_are_tagged_by_kind() {
        for (value, expected) in [
            (
                ProduceValueSpec::Text {
                    text: "order-42".into(),
                },
                serde_json::json!({"kind": "text", "text": "order-42"}),
            ),
            (
                ProduceValueSpec::Json {
                    json: serde_json::json!({"orderId": 7}),
                },
                serde_json::json!({"kind": "json", "json": {"orderId": 7}}),
            ),
            (
                ProduceValueSpec::Avro {
                    subject: "orders-value".into(),
                    json: serde_json::json!({"orderId": 7}),
                },
                serde_json::json!({
                    "kind": "avro",
                    "subject": "orders-value",
                    "json": {"orderId": 7},
                }),
            ),
        ] {
            assert_eq!(serde_json::to_value(&value).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<ProduceValueSpec>(expected).unwrap(),
                value,
                "round trip"
            );
        }
    }

    #[test]
    fn a_record_spec_round_trips_through_the_ipc_shape() {
        let raw = serde_json::json!({
            "key": "order-42",
            "value": {"kind": "text", "text": "hello"},
            "headers": [{"key": "trace-id", "value": "abc-123"}],
            "partition": 3,
        });
        let spec: ProduceRecordSpec = serde_json::from_value(raw.clone()).expect("parses");

        assert_eq!(spec.key.as_deref(), Some("order-42"));
        assert_eq!(spec.headers.len(), 1);
        assert_eq!(spec.partition, Some(3));
        assert_eq!(serde_json::to_value(&spec).unwrap(), raw);
    }

    /// A tombstone is a null value, not an empty one — the distinction the
    /// whole compacted-topic story rests on (docs/DESIGN.md §5.2).
    #[test]
    fn a_null_value_parses_as_a_tombstone_and_headers_default_to_none() {
        let spec: ProduceRecordSpec = serde_json::from_value(serde_json::json!({
            "key": "order-42",
            "value": null,
            "partition": null,
        }))
        .expect("parses");
        assert!(spec.value.is_none());
        assert!(spec.headers.is_empty());
        assert_eq!(spec.partition, None);
    }

    #[test]
    fn bulk_shapes_match_the_ipc_contract() {
        let raw = serde_json::json!({
            "count": 500,
            "interval_ms": 0,
            "key_template": "order-{{seq}}",
            "value_template": "{\"id\": {{seq}}}",
            "partition": null,
        });
        let spec: BulkSpec = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(spec.count, 500);
        assert_eq!(spec.key_template.as_deref(), Some("order-{{seq}}"));
        assert_eq!(serde_json::to_value(&spec).unwrap(), raw);

        assert_eq!(
            serde_json::to_value(BulkProgress {
                sent: 120,
                failed: 0,
                done: false,
                error: None,
            })
            .unwrap(),
            serde_json::json!({"sent": 120, "failed": 0, "done": false, "error": null})
        );
        assert_eq!(
            serde_json::to_value(Delivery {
                partition: 3,
                offset: 8412,
            })
            .unwrap(),
            serde_json::json!({"partition": 3, "offset": 8412})
        );
    }

    /// The cap is applied in core, not trusted from the caller — the same rule
    /// as [`crate::consume::MAX_MESSAGES`].
    #[test]
    fn a_bulk_run_is_capped_in_core() {
        assert_eq!(MAX_BULK, 100_000);
        for (asked, produced) in [
            (0, 0),
            (1, 1),
            (MAX_BULK, MAX_BULK),
            (MAX_BULK + 1, MAX_BULK),
            (u32::MAX, MAX_BULK),
        ] {
            let spec = BulkSpec {
                count: asked,
                interval_ms: 0,
                key_template: None,
                value_template: "{{seq}}".into(),
                partition: None,
            };
            assert_eq!(spec.capped_count(), produced, "asked for {asked}");
        }
    }
}

#[cfg(test)]
mod templates {
    use super::*;

    /// A fixed clock, so `{{now_iso}}`/`{{now_ms}}` are assertable:
    /// 2023-11-14T22:13:20.000Z.
    const CLOCK: i64 = 1_700_000_000_000;

    fn render(source: &str, seq: u64) -> String {
        Template::parse(source)
            .expect(source)
            .render(seq, CLOCK, &mut Rng::seeded(42))
    }

    #[test]
    fn a_template_with_no_placeholders_is_its_own_output() {
        assert_eq!(render("plain text", 1), "plain text");
        assert_eq!(render("", 1), "");
    }

    #[test]
    fn seq_counts_from_one() {
        assert_eq!(render("order-{{seq}}", 1), "order-1");
        assert_eq!(render("order-{{seq}}", 500), "order-500");
        // Twice in one template is twice the same number, not a counter.
        assert_eq!(render("{{seq}}/{{seq}}", 7), "7/7");
    }

    #[test]
    fn the_clock_placeholders_read_the_clock_they_are_given() {
        assert_eq!(render("{{now_ms}}", 1), CLOCK.to_string());
        assert_eq!(render("{{now_iso}}", 1), "2023-11-14T22:13:20.000Z");
    }

    #[test]
    fn iso8601_covers_the_epoch_leap_years_and_millis() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
        // 29 February on a leap year, the day civil-from-days is written for.
        assert_eq!(iso8601(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
        // Before the epoch: the arithmetic is euclidean, not truncating.
        assert_eq!(iso8601(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn uuids_are_valid_version_4_uuids() {
        let mut rng = Rng::seeded(1);
        for _ in 0..64 {
            let uuid = uuid_v4(&mut rng);
            assert_eq!(uuid.len(), 36, "{uuid}");
            let groups: Vec<&str> = uuid.split('-').collect();
            assert_eq!(
                groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
                vec![8, 4, 4, 4, 12],
                "{uuid}"
            );
            assert!(
                uuid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()),
                "{uuid}"
            );
            // RFC 9562 §5.4: version nibble 4, variant nibble 8/9/a/b.
            assert_eq!(&groups[2][..1], "4", "{uuid}");
            assert!(
                ['8', '9', 'a', 'b'].contains(&groups[3].chars().next().unwrap()),
                "{uuid}"
            );
        }
    }

    #[test]
    fn uuids_do_not_repeat_within_a_run() {
        let mut rng = Rng::seeded(7);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..2_000 {
            assert!(seen.insert(uuid_v4(&mut rng)), "the generator repeated");
        }
    }

    #[test]
    fn rand_int_stays_inside_its_bounds_and_includes_both_ends() {
        let template = Template::parse("{{rand_int 1 3}}").expect("parses");
        let mut rng = Rng::seeded(3);
        let mut seen = std::collections::BTreeSet::new();
        for seq in 1..=400 {
            let drawn: i64 = template.render(seq, CLOCK, &mut rng).parse().expect("int");
            assert!((1..=3).contains(&drawn), "{drawn} escaped 1..=3");
            seen.insert(drawn);
        }
        assert_eq!(
            seen,
            [1, 2, 3].into_iter().collect(),
            "both ends are inclusive"
        );

        // A single-value range is a constant, not an error.
        assert_eq!(render("{{rand_int 5 5}}", 1), "5");
        // Negative bounds are whole numbers too.
        let negative: i64 = render("{{rand_int -3 -1}}", 1).parse().expect("int");
        assert!((-3..=-1).contains(&negative), "{negative}");
    }

    #[test]
    fn choice_only_ever_produces_one_of_its_options() {
        let template = Template::parse("{{choice created|shipped|paid}}").expect("parses");
        let mut rng = Rng::seeded(11);
        let mut seen = std::collections::BTreeSet::new();
        for seq in 1..=400 {
            let picked = template.render(seq, CLOCK, &mut rng);
            assert!(
                ["created", "shipped", "paid"].contains(&picked.as_str()),
                "{picked}"
            );
            seen.insert(picked);
        }
        assert_eq!(seen.len(), 3, "every option comes up: {seen:?}");

        // Options are trimmed, so `a | b` is not " b".
        assert_eq!(render("{{choice only}}", 1), "only");
        assert_eq!(render("{{choice  padded  }}", 1), "padded");
    }

    #[test]
    fn a_seeded_run_is_reproducible() {
        let template = Template::parse(
            r#"{"id":"{{uuid}}","n":{{rand_int 1 1000}},"s":"{{choice a|b}}","seq":{{seq}}}"#,
        )
        .expect("parses");
        let run = |seed| {
            let mut rng = Rng::seeded(seed);
            (1..=20)
                .map(|seq| template.render(seq, CLOCK, &mut rng))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(99), run(99), "same seed, same records");
        assert_ne!(run(99), run(100), "a different seed is different data");
        assert!(!template.is_deterministic());
    }

    #[test]
    fn a_template_of_literals_and_seq_alone_is_deterministic() {
        let template = Template::parse("order-{{seq}}").expect("parses");
        assert!(template.is_deterministic());
        assert_eq!(
            template.render(4, CLOCK, &mut Rng::seeded(1)),
            template.render(4, CLOCK, &mut Rng::seeded(2)),
            "no generator is consulted at all"
        );
    }

    #[test]
    fn every_placeholder_renders_something() {
        let rendered = render(
            "{{seq}}|{{uuid}}|{{now_iso}}|{{now_ms}}|{{rand_int 1 2}}|{{choice x}}",
            9,
        );
        let parts: Vec<&str> = rendered.split('|').collect();
        assert_eq!(parts.len(), 6, "{rendered}");
        assert_eq!(parts[0], "9");
        assert_eq!(parts[1].len(), 36);
        assert_eq!(parts[2], "2023-11-14T22:13:20.000Z");
        assert_eq!(parts[3], CLOCK.to_string());
        assert!(["1", "2"].contains(&parts[4]));
        assert_eq!(parts[5], "x");
    }

    /// The preview path the produce form uses: same rendering, wall clock.
    #[test]
    fn render_now_reads_the_wall_clock() {
        let before = now_ms();
        let rendered: i64 = Template::parse("{{now_ms}}")
            .expect("parses")
            .render_now(1, &mut Rng::seeded(1))
            .parse()
            .expect("a timestamp");
        assert!(
            (before..=now_ms()).contains(&rendered),
            "{rendered} is not now"
        );
        assert!(Template::parse("{{now_iso}}")
            .expect("parses")
            .render_now(1, &mut Rng::seeded(1))
            .ends_with('Z'));
    }

    #[test]
    fn whitespace_inside_a_placeholder_is_ignored() {
        assert_eq!(render("{{  seq  }}", 3), "3");
        assert_eq!(render("{{ rand_int  4   4 }}", 1), "4");
    }

    /// The parser must not steal a JSON object that happens to start with two
    /// braces — `{{"a": 1}}` is a nested object, not a broken placeholder.
    #[test]
    fn doubled_braces_that_are_not_placeholders_stay_literal() {
        for literal in [
            r#"{{"nested": 1}}"#,
            "{{ }}",
            "{{1}}",
            "{{UPPER}}",
            "a { b } c",
            "{{",
        ] {
            assert_eq!(render(literal, 1), literal, "{literal} was rewritten");
        }
    }

    #[test]
    fn malformed_templates_name_the_position_and_the_fix() {
        for (source, position, needle) in [
            ("order-{{seq", 6, "unclosed"),
            ("{{sq}}", 0, "unknown placeholder {{sq}}"),
            ("ok {{now}} ok", 3, "unknown placeholder {{now}}"),
            ("{{seq 3}}", 0, "takes no arguments"),
            ("{{uuid extra}}", 0, "takes no arguments"),
            ("{{rand_int}}", 0, "two whole numbers"),
            ("x{{rand_int 1}}", 1, "two whole numbers"),
            ("{{rand_int 1 2 3}}", 0, "two whole numbers"),
            ("{{rand_int a b}}", 0, "two whole numbers"),
            ("{{rand_int 5 1}}", 0, "wrong way round"),
            ("{{choice}}", 0, "at least one option"),
        ] {
            let err = Template::parse(source).expect_err(source).to_string();
            assert!(err.contains(needle), "{source} -> {err}");
            assert!(
                err.contains(&format!("position {position}")),
                "{source} -> {err}"
            );
        }
    }

    #[test]
    fn the_unknown_placeholder_error_lists_what_kavka_does_know() {
        let err = Template::parse("{{sq}}").unwrap_err().to_string();
        for known in [
            "{{seq}}",
            "{{uuid}}",
            "{{rand_int A B}}",
            "{{choice a|b|c}}",
        ] {
            assert!(err.contains(known), "{err}");
        }
    }
}

/// Encoding a value needs the Schema Registry but not a cluster, so the Avro
/// path is asserted here — canned registry and all — on the bare tier.
#[cfg(test)]
mod encoding {
    use super::*;
    use crate::profiles::SchemaRegistryConfig;
    use crate::sr::canned::CannedRegistry;

    /// `GET /subjects/orders-value/versions/latest` — the encode side's one
    /// lookup, answering with the same Order schema the decode tests use.
    const LATEST: &str = r#"{"subject":"orders-value","version":4,"id":217,"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#;
    /// `GET /schemas/ids/217` — the decode side of the same schema.
    const BY_ID: &str = r#"{"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#;
    const VERSIONS: &str = r#"[{"subject":"orders-value","version":4}]"#;

    fn config(url: &str) -> SchemaRegistryConfig {
        SchemaRegistryConfig {
            url: url.to_string(),
            username: None,
            password: None,
        }
    }

    #[test]
    fn text_is_its_own_bytes_and_json_is_compact() {
        assert_eq!(
            encode_value(
                &ProduceValueSpec::Text {
                    text: "order-42".into()
                },
                None
            )
            .unwrap(),
            b"order-42"
        );
        assert_eq!(
            encode_value(
                &ProduceValueSpec::Json {
                    json: serde_json::json!({"orderId": 7}),
                },
                None,
            )
            .unwrap(),
            br#"{"orderId":7}"#
        );
    }

    /// The point of the exercise: bytes this crate produced, read back by the
    /// decoder this crate ships, against one registry.
    #[test]
    fn avro_encodes_against_the_latest_schema_and_decodes_straight_back() {
        let server = CannedRegistry::start(vec![
            ("/subjects/orders-value/versions/latest", 200, LATEST),
            ("/schemas/ids/217", 200, BY_ID),
            ("/schemas/ids/217/versions", 200, VERSIONS),
        ]);
        let bytes = encode_value(
            &ProduceValueSpec::Avro {
                subject: "orders-value".into(),
                json: serde_json::json!({"orderId": 7, "status": "created"}),
            },
            Some(&config(server.url())),
        )
        .expect("encodes");

        // Confluent framing, then the datum the decoder's own fixture uses.
        assert_eq!(
            bytes,
            vec![
                0x00, 0x00, 0x00, 0x00, 0xd9, 0x0e, 0x0e, b'c', b'r', b'e', b'a', b't', b'e', b'd'
            ]
        );

        let registry = crate::sr::SchemaRegistry::new(&config(server.url()));
        let decoded = crate::serdes::decode(&bytes, Some(&registry), 262_144);
        assert_eq!(decoded.encoding, crate::serdes::Encoding::Avro);
        assert_eq!(
            decoded.json.unwrap(),
            serde_json::json!({"orderId": 7, "status": "created"})
        );
        assert_eq!(decoded.schema.unwrap().schema_id, 217);
    }

    #[test]
    fn a_schema_mismatch_names_the_field_and_the_subject() {
        let server = CannedRegistry::start(vec![(
            "/subjects/orders-value/versions/latest",
            200,
            LATEST,
        )]);
        let err = encode_value(
            &ProduceValueSpec::Avro {
                subject: "orders-value".into(),
                json: serde_json::json!({"orderId": "seven", "status": "created"}),
            },
            Some(&config(server.url())),
        )
        .expect_err("a string is not an int")
        .to_string();

        assert!(err.contains("orderId"), "got {err}");
        assert!(err.contains("orders-value"), "got {err}");
        assert!(err.contains("version 4"), "got {err}");
    }

    #[test]
    fn an_unreachable_registry_says_so_rather_than_producing_something_else() {
        // Port 1 is never listening; the connection is refused immediately.
        let err = encode_value(
            &ProduceValueSpec::Avro {
                subject: "orders-value".into(),
                json: serde_json::json!({"orderId": 7, "status": "created"}),
            },
            Some(&config("http://127.0.0.1:1")),
        )
        .expect_err("no registry")
        .to_string();

        assert!(err.contains("schema registry"), "got {err}");
        assert!(err.contains("orders-value"), "got {err}");
    }

    #[test]
    fn avro_without_a_configured_registry_says_which_setting_is_missing() {
        let err = encode_value(
            &ProduceValueSpec::Avro {
                subject: "orders-value".into(),
                json: serde_json::json!({}),
            },
            None,
        )
        .expect_err("no registry configured")
        .to_string();
        assert!(err.contains("no Schema Registry"), "got {err}");
        assert!(err.contains("connection's settings"), "got {err}");
    }

    #[test]
    fn a_non_avro_subject_is_named_rather_than_encoded_anyway() {
        let server = CannedRegistry::start(vec![(
            "/subjects/events-value/versions/latest",
            200,
            r#"{"subject":"events-value","version":2,"id":8,"schema":"{\"type\":\"object\"}","schemaType":"JSON"}"#,
        )]);
        let err = encode_value(
            &ProduceValueSpec::Avro {
                subject: "events-value".into(),
                json: serde_json::json!({"a": 1}),
            },
            Some(&config(server.url())),
        )
        .expect_err("JSON Schema is not Avro")
        .to_string();
        assert!(err.contains("JSON Schema"), "got {err}");
    }
}

/// The produce-and-read-back roundtrips. They live here rather than in
/// `tests/` because they need rdkafka, which is a feature-gated dependency of
/// this crate — a `dev-dependency` on it would make plain
/// `cargo test -p kavka-core` require CMake and break the bare-toolchain tier
/// the feature split exists to protect (see Cargo.toml, and the same note in
/// src/consume.rs).
#[cfg(all(test, feature = "kafka"))]
mod tests {
    use super::*;
    use crate::consume::{fetch_messages, FetchSpec, SeekSpec};
    use crate::profiles::{AuthConfig, ConnectionProfile, Environment};
    use crate::serdes::{Encoding, MessageRecord};
    use crate::sr::canned::CannedRegistry;

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
        connection_with(read_only, None)
    }

    fn connection_with(
        read_only: bool,
        schema_registry: Option<SchemaRegistryConfig>,
    ) -> ClusterConnection {
        ClusterConnection::connect(ConnectionProfile {
            id: "it-produce".into(),
            name: "local docker".into(),
            environment: Environment::Dev,
            bootstrap_servers: vec![bootstrap()],
            auth: AuthConfig::Plaintext,
            read_only,
            schema_registry,
            connect_clusters: Vec::new(),
            metrics_endpoint: None,
            sampler_interval_ms: None,
        })
        .expect("connect")
    }

    /// A topic name no other run can collide with — the same pid-plus-nanos
    /// scheme as src/consume.rs, and for the same reason (no uuid dependency).
    fn unique_topic(what: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("kavka-it-{what}-{}-{nanos:x}", std::process::id())
    }

    /// `create_topic` returns when the controller accepts it; the producer
    /// wants metadata that has actually propagated.
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

    fn read_back(
        conn: &ClusterConnection,
        sr: Option<&SchemaRegistryConfig>,
        topic: &str,
        max_messages: u32,
    ) -> Vec<MessageRecord> {
        fetch_messages(
            conn,
            sr,
            &FetchSpec {
                topic: topic.to_string(),
                seek: SeekSpec::Earliest,
                partitions: None,
                max_messages,
                max_value_bytes: None,
            },
            None,
        )
        .expect("fetch")
    }

    fn record(key: Option<&str>, value: Option<ProduceValueSpec>) -> ProduceRecordSpec {
        ProduceRecordSpec {
            key: key.map(str::to_string),
            value,
            headers: Vec::new(),
            partition: Some(0),
        }
    }

    #[test]
    fn text_json_and_tombstone_records_survive_a_roundtrip() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let topic = unique_topic("produce");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);

        let text = ProduceRecordSpec {
            headers: vec![
                ProduceHeader {
                    key: "trace-id".into(),
                    value: "abc-123".into(),
                },
                ProduceHeader {
                    key: "source".into(),
                    value: "kavka".into(),
                },
            ],
            ..record(
                Some("order-1"),
                Some(ProduceValueSpec::Text {
                    text: "plain text".into(),
                }),
            )
        };
        let json = record(
            Some("order-2"),
            Some(ProduceValueSpec::Json {
                json: serde_json::json!({"orderId": 2, "status": "created"}),
            }),
        );
        let tombstone = record(Some("order-1"), None);

        let first = send(&conn, None, &topic, &text).expect("send text");
        let second = send(&conn, None, &topic, &json).expect("send json");
        let third = send(&conn, None, &topic, &tombstone).expect("send tombstone");

        // The broker's own answer, not an echo of what we asked for.
        assert_eq!((first.partition, first.offset), (0, 0));
        assert_eq!((second.partition, second.offset), (0, 1));
        assert_eq!((third.partition, third.offset), (0, 2));

        let records = read_back(&conn, None, &topic, 10);
        let _ = crate::admin::delete_topic(&conn, &topic);
        assert_eq!(records.len(), 3, "{records:?}");

        let text_back = &records[0];
        assert_eq!(text_back.key.as_ref().expect("keyed").text, "order-1");
        let value = text_back.value.as_ref().expect("not a tombstone");
        assert_eq!(value.encoding, Encoding::Utf8);
        assert_eq!(value.text, "plain text");
        assert_eq!(value.raw_len, "plain text".len());
        let headers: Vec<(&str, Option<&str>)> = text_back
            .headers
            .iter()
            .map(|h| (h.key.as_str(), h.value.as_deref()))
            .collect();
        assert_eq!(
            headers,
            vec![("trace-id", Some("abc-123")), ("source", Some("kavka")),],
            "headers keep their order and their values"
        );

        let json_back = records[1].value.as_ref().expect("not a tombstone");
        assert_eq!(json_back.encoding, Encoding::Json);
        assert_eq!(
            json_back.json.as_ref().unwrap(),
            &serde_json::json!({"orderId": 2, "status": "created"})
        );

        // A tombstone is a null value, and it kept its key — which is the
        // entire point of one on a compacted topic.
        assert!(records[2].value.is_none(), "{:?}", records[2].value);
        assert_eq!(records[2].key.as_ref().expect("keyed").text, "order-1");
        assert!(records[2].headers.is_empty());
    }

    #[test]
    fn avro_produced_by_kavka_is_read_back_by_kavka() {
        if !integration() {
            return;
        }
        let server = CannedRegistry::start(vec![
            (
                "/subjects/orders-value/versions/latest",
                200,
                r#"{"subject":"orders-value","version":4,"id":217,"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#,
            ),
            (
                "/schemas/ids/217",
                200,
                r#"{"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#,
            ),
            (
                "/schemas/ids/217/versions",
                200,
                r#"[{"subject":"orders-value","version":4}]"#,
            ),
        ]);
        let sr = SchemaRegistryConfig {
            url: server.url().to_string(),
            username: None,
            password: None,
        };
        let conn = connection_with(false, Some(sr.clone()));
        let topic = unique_topic("avro");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);

        let delivery = send(
            &conn,
            Some(&sr),
            &topic,
            &record(
                Some("order-7"),
                Some(ProduceValueSpec::Avro {
                    subject: "orders-value".into(),
                    json: serde_json::json!({"orderId": 7, "status": "created"}),
                }),
            ),
        )
        .expect("send avro");
        assert_eq!(delivery.partition, 0);

        let records = read_back(&conn, Some(&sr), &topic, 10);
        let _ = crate::admin::delete_topic(&conn, &topic);

        assert_eq!(records.len(), 1);
        let value = records[0].value.as_ref().expect("not a tombstone");
        assert_eq!(value.encoding, Encoding::Avro);
        assert_eq!(
            value.json.as_ref().unwrap(),
            &serde_json::json!({"orderId": 7, "status": "created"})
        );
        let schema = value.schema.as_ref().expect("framed");
        assert_eq!(schema.schema_id, 217);
        assert_eq!(schema.subject.as_deref(), Some("orders-value"));
        assert_eq!(schema.version, Some(4));
    }

    #[test]
    fn a_bulk_run_delivers_every_record_and_counts_them_exactly() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let topic = unique_topic("bulk");
        crate::admin::create_topic(&conn, &topic, 3, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 3);

        let session = BulkSession::start(
            &conn,
            &topic,
            &BulkSpec {
                count: 500,
                interval_ms: 0,
                key_template: Some("order-{{seq}}".into()),
                value_template:
                    r#"{"seq":{{seq}},"id":"{{uuid}}","status":"{{choice created|paid}}"}"#.into(),
                partition: None,
            },
        )
        .expect("start bulk");

        let deadline = Instant::now() + Duration::from_secs(60);
        let progress = loop {
            let progress = session.progress();
            if progress.done {
                break progress;
            }
            assert!(Instant::now() < deadline, "the bulk run never finished");
            std::thread::sleep(Duration::from_millis(100));
        };

        assert_eq!(
            progress.sent, 500,
            "every record acknowledged: {progress:?}"
        );
        assert_eq!(progress.failed, 0, "{progress:?}");
        assert_eq!(progress.error, None, "{progress:?}");

        let records = read_back(&conn, None, &topic, 1000);
        let _ = crate::admin::delete_topic(&conn, &topic);
        assert_eq!(records.len(), 500, "500 records on the topic");

        // `{{seq}}` is 1-based and each record gets its own number, in both the
        // key and the value.
        let mut seqs: Vec<i64> = records
            .iter()
            .map(|r| {
                let value = r.value.as_ref().expect("not a tombstone");
                value.json.as_ref().expect("json")["seq"]
                    .as_i64()
                    .expect("seq is a number")
            })
            .collect();
        seqs.sort_unstable();
        assert_eq!(seqs, (1..=500).collect::<Vec<i64>>());

        let keys: std::collections::BTreeSet<String> = records
            .iter()
            .map(|r| r.key.as_ref().expect("keyed").text.clone())
            .collect();
        assert_eq!(keys.len(), 500, "keys render per record");
        assert!(keys.contains("order-1") && keys.contains("order-500"));

        let ids: std::collections::BTreeSet<String> = records
            .iter()
            .map(|r| {
                r.value.as_ref().unwrap().json.as_ref().unwrap()["id"]
                    .as_str()
                    .expect("uuid")
                    .to_string()
            })
            .collect();
        assert_eq!(ids.len(), 500, "{{uuid}} is drawn per record");
    }

    #[test]
    fn cancelling_a_bulk_run_stops_it_promptly() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let topic = unique_topic("bulk-cancel");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);

        let session = BulkSession::start(
            &conn,
            &topic,
            &BulkSpec {
                // At 5ms a piece this is 25 seconds of work, so a run that
                // finishes quickly finished because it was stopped.
                count: 5_000,
                interval_ms: 5,
                key_template: None,
                value_template: "{{seq}}".into(),
                partition: None,
            },
        )
        .expect("start bulk");

        std::thread::sleep(Duration::from_millis(400));
        let stopped_at = Instant::now();
        session.stop();
        session.stop(); // idempotent

        let deadline = stopped_at + Duration::from_secs(5);
        let progress = loop {
            let progress = session.progress();
            if progress.done {
                break progress;
            }
            assert!(
                Instant::now() < deadline,
                "stop() did not end the run: {progress:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        };

        assert!(
            stopped_at.elapsed() < Duration::from_secs(5),
            "a stop has to feel immediate"
        );
        assert!(
            progress.sent < 5_000,
            "the run should have stopped early: {progress:?}"
        );
        assert!(progress.sent > 0, "it should have produced something first");

        let records = read_back(&conn, None, &topic, 2000);
        let _ = crate::admin::delete_topic(&conn, &topic);
        // Nothing is lost between the count and the cluster: everything
        // librdkafka accepted before the stop was flushed.
        assert_eq!(records.len(), progress.sent as usize);
    }

    /// D5: read-only is enforced in core, and enforced *first* — before a
    /// schema fetch, before a producer, before a socket.
    #[test]
    fn a_read_only_connection_is_refused_before_any_network_io() {
        if !integration() {
            return;
        }
        let conn = connection(true);
        // A registry that would hang or fail loudly if it were ever consulted.
        let unreachable = SchemaRegistryConfig {
            url: "http://127.0.0.1:1".into(),
            username: None,
            password: None,
        };

        let started = Instant::now();
        let err = send(
            &conn,
            Some(&unreachable),
            "orders",
            &record(
                Some("order-1"),
                Some(ProduceValueSpec::Avro {
                    subject: "orders-value".into(),
                    json: serde_json::json!({"orderId": 1}),
                }),
            ),
        )
        .expect_err("read-only must refuse");
        assert!(matches!(err, Error::ReadOnly(_)), "got {err}");
        assert!(err.to_string().contains("read-only"), "got {err}");

        let bulk = BulkSession::start(
            &conn,
            "orders",
            &BulkSpec {
                count: 10,
                interval_ms: 0,
                // Malformed on purpose: read-only is checked before this is
                // even parsed, so the error must still be the read-only one.
                key_template: Some("{{sq}}".into()),
                value_template: "{{seq}}".into(),
                partition: None,
            },
        )
        .expect_err("read-only must refuse");
        assert!(matches!(bulk, Error::ReadOnly(_)), "got {bulk}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "nothing was attempted over the network"
        );
    }

    #[test]
    fn producing_to_a_topic_or_partition_that_is_not_there_says_which() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let missing = send(
            &conn,
            None,
            "kavka-no-such-topic",
            &record(None, Some(ProduceValueSpec::Text { text: "x".into() })),
        )
        .expect_err("unknown topic")
        .to_string();
        assert!(missing.contains("no topic called"), "got {missing}");

        let bad_partition = send(
            &conn,
            None,
            "orders",
            &ProduceRecordSpec {
                partition: Some(99),
                ..record(None, Some(ProduceValueSpec::Text { text: "x".into() }))
            },
        )
        .expect_err("unknown partition")
        .to_string();
        assert!(
            bad_partition.contains("no partition 99"),
            "got {bad_partition}"
        );
        assert!(
            bad_partition.contains("numbered 0 to 5"),
            "got {bad_partition}"
        );
    }
}
