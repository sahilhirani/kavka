//! Broker metrics: a Prometheus text-format scraper and a rolling in-memory
//! window over what it finds (docs/ARCHITECTURE.md D6).
//!
//! # This is the optional half of monitoring
//!
//! [`crate::history`] needs nothing from anybody: lag is arithmetic on offsets
//! Kavka can already read. Throughput, storage growth and under-replicated
//! partitions live in JMX, and JMX is only reachable if somebody put an
//! exporter in front of it. So every path here degrades to "not configured" or
//! to "configured, unreachable, and here is why" while nothing else in the app
//! stops working — the D6 promise, stated as code in [`MetricsStatus`].
//!
//! # Why nothing here is persisted, and lag history is
//!
//! Two reasons, and the second is the real one:
//!
//! 1. **Write amplification.** A scrape of a ten-broker cluster with a thousand
//!    topics is tens of thousands of samples; at the sampler's cadence that is
//!    a redb commit per tick an order of magnitude larger than the lag
//!    sampler's, for data that is already durable somewhere else.
//! 2. **It is already somebody's job.** Anyone with a JMX exporter has a
//!    Prometheus behind it, with retention, rollups and alerting Kavka is not
//!    going to beat. Kavka's contribution is the live view and the correlation
//!    with lag — the 24 hours a person can hold in their head.
//!
//! So this module owns a ring buffer per series ([`WINDOW_MS`]) and nothing on
//! disk. A restart starts the throughput charts empty; the lag charts — the
//! ones nobody else has — survive it.
//!
//! # The exporter-name problem
//!
//! There is no standard for what a Kafka JMX metric is called in Prometheus:
//! the name is whatever the exporter's rules file makes it. The two shapes that
//! cover almost everything in the wild are the `kafka-2_0_0.yml` sample rules
//! shipped with `jmx_exporter` (which turn `BytesInPerSec`'s `Count` into
//! `kafka_server_brokertopicmetrics_bytesin_total`) and the generic
//! lowercase-everything fallback (which leaves `..._bytesinpersec_count`). Both
//! are in [`MAPPINGS`], along with the pre-computed rate attributes; an endpoint
//! that exposes neither reports the series as unavailable rather than as zero.
//!
//! # Blocking
//!
//! [`MetricsCollector::scrape`] blocks on a socket, like the rest of the core.
//! The Tauri shell wraps it in `spawn_blocking`.

use crate::profiles::MetricsEndpointConfig;
use crate::secrets;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};

/// A scrape is a background tick, not something a user is waiting on — but it
/// holds a blocking-pool slot, and an endpoint behind a black-holing firewall
/// would hold it for the OS connect timeout. Five seconds is well past a
/// healthy exporter (single-digit milliseconds) and well short of the sampling
/// interval it shares a schedule with.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// How much of each series is kept in memory.
pub const WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// Hard cap per series, so a fast scrape cadence — or a caller in a loop —
/// cannot turn the window into unbounded memory. 24h at the sampler's 5s floor
/// is 17 280 points; this leaves headroom and then stops.
const MAX_POINTS: usize = 20_000;

/// How stale a value may be and still drive an alert. Two things must not
/// happen when an exporter goes dark: a throughput-floor rule firing because
/// the last number it saw was low, and a firing rule *staying* fired on an
/// hour-old number. Both are answered by treating a stale series as absent —
/// see [`MetricsCollector::latest`].
pub const FRESH_MS: i64 = 5 * 60 * 1000;

/// How old a counter's previous reading may be and still produce a rate.
///
/// A `Count` difference divided by the time between two scrapes is only "the
/// rate right now" while those two scrapes are close together. After a laptop
/// suspend, a network partition or an exporter restart, the first scrape back
/// would otherwise draw the average of the entire outage as the current figure
/// — a four-hour mean landing on the chart as a live reading, with an alert
/// rule evaluating it.
///
/// [`FRESH_MS`] is reused rather than a second number invented. It is already
/// the line this module draws between a reading and history, and it is a small
/// multiple of every cadence the app allows: twenty scrapes at the sampler's
/// 15-second default, sixty at its 5-second floor. A baseline older than that
/// spans a gap [`MetricsCollector::latest`] would already refuse to serve
/// across, so honouring it here keeps one definition of stale instead of two.
const MAX_BASELINE_AGE_MS: i64 = FRESH_MS;

/// The series names the IPC contract fixes. Anything else is not a series this
/// build knows how to produce, and asking for one is answered with no points
/// rather than an error — a saved chart naming a series a rebuilt exporter no
/// longer exposes should go empty, not break the screen.
pub const SERIES_VOCABULARY: [&str; 6] = [
    "bytes_in_per_sec",
    "bytes_out_per_sec",
    "messages_in_per_sec",
    "under_replicated_partitions",
    "offline_partitions",
    "log_size_bytes",
];

/// The per-topic form of a series key: `"<series>:topic:<name>"`.
pub fn series_for_topic(series: &str, topic: &str) -> String {
    format!("{series}:topic:{topic}")
}

/// Splits a series key back into `(series, topic)`. A key with no `:topic:`
/// segment is cluster-level.
pub fn split_series_key(key: &str) -> (&str, Option<&str>) {
    match key.split_once(":topic:") {
        Some((series, topic)) => (series, Some(topic)),
        None => (key, None),
    }
}

// ---------------------------------------------------------------------------
// The IPC contract. Field names are mirrored by the TypeScript in
// apps/desktop/src, so renaming one is a breaking change on both sides.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    pub ts_ms: i64,
    pub value: f64,
}

/// What the metrics panel says about the endpoint, in the four facts a user
/// needs: whether one is configured, whether the last scrape worked, when it
/// was, and why it didn't (docs/DESIGN.md §7 — an error states what happened,
/// then the next click).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsStatus {
    pub configured: bool,
    pub reachable: bool,
    pub last_scrape_ms: Option<i64>,
    pub last_error: Option<String>,
    /// The series this endpoint actually produced, per-topic keys included.
    /// Empty on an endpoint that answered but exposed nothing Kavka recognises
    /// — a different problem from "unreachable", and one that has to read
    /// differently on screen.
    pub series_available: Vec<String>,
}

impl MetricsStatus {
    /// The answer for a profile with no `metrics_endpoint`. Not an error state:
    /// lag charts work without one, and the panel says so.
    pub fn unconfigured() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// The Prometheus text format
// ---------------------------------------------------------------------------

/// One parsed line: `name{label="value",…} 1.23 [timestamp]`.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricSample {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

impl MetricSample {
    pub fn label(&self, name: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Parses the Prometheus text exposition format, tolerantly.
///
/// Hand-rolled rather than taken from a crate: this is a line format with one
/// optional brace group, the whole parser is the next forty lines, and a
/// dependency for it would be larger than the thing it replaces (see this
/// crate's Cargo.toml, where every entry argues for itself).
///
/// **Tolerant is a requirement, not a shortcut.** A scrape is a few thousand
/// lines from a JVM agent whose output Kavka does not control; a line it cannot
/// read must cost that line and nothing else. So `# HELP`/`# TYPE` are skipped,
/// a malformed line is skipped, a trailing timestamp is ignored, and the
/// trailing comma `jmx_exporter` writes inside its brace group
/// (`{topic="orders.v2",}`) is accepted.
///
/// Values keep their `NaN`/`+Inf`: parsing is not the layer that decides what a
/// non-finite reading means — [`fold_samples`] is, and it drops them.
pub fn parse_prometheus(text: &str) -> Vec<MetricSample> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(sample) = parse_line(line) {
            out.push(sample);
        }
    }
    out
}

fn parse_line(line: &str) -> Option<MetricSample> {
    let name_end = line
        .find(|c: char| c == '{' || c.is_whitespace())
        .unwrap_or(line.len());
    let name = &line[..name_end];
    if name.is_empty() {
        return None;
    }
    let rest = &line[name_end..];

    let (labels, rest) = if rest.starts_with('{') {
        let close = find_closing_brace(rest)?;
        (parse_labels(&rest[1..close]), &rest[close + 1..])
    } else {
        (Vec::new(), rest)
    };

    // The remainder is `value [timestamp]`; the timestamp is deliberately
    // dropped. The scrape's own clock is the one every series here shares, and
    // an exporter-supplied timestamp would put two clocks on one chart.
    let value = parse_value(rest.split_whitespace().next()?)?;
    Some(MetricSample {
        name: name.to_string(),
        labels,
        value,
    })
}

/// The index of the `}` closing the group that starts at byte 0, honouring
/// quoted label values — `{reason="}"}` is legal, and a naive `find` gets it
/// wrong.
fn find_closing_brace(rest: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, c) in rest.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            '}' if !in_quotes => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_labels(body: &str) -> Vec<(String, String)> {
    let mut labels = Vec::new();
    let mut rest = body.trim();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else { break };
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(quoted) = after.strip_prefix('"') else {
            break;
        };
        let (value, tail) = take_quoted(quoted);
        if !key.is_empty() {
            labels.push((key, value));
        }
        rest = tail.trim_start().trim_start_matches(',').trim_start();
    }
    labels
}

/// Reads a label value up to its closing quote, unescaping `\\`, `\"` and `\n`
/// — the three escapes the exposition format defines.
fn take_quoted(input: &str) -> (String, &str) {
    let mut value = String::new();
    let mut chars = input.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => return (value, &input[index + 1..]),
            '\\' => match chars.next() {
                Some((_, 'n')) => value.push('\n'),
                Some((_, escaped)) => value.push(escaped),
                None => break,
            },
            _ => value.push(c),
        }
    }
    // Unterminated quote: keep what was read and let the caller drop the line
    // when the value turns out to be missing too.
    (value, "")
}

/// `NaN`, `+Inf`, `-Inf` and Java's `1.2345678E7` all reach here, and Rust's
/// own float parser accepts every one of them — so this is a `parse` plus the
/// decision to keep non-finite readings rather than invent zeros for them.
fn parse_value(raw: &str) -> Option<f64> {
    raw.parse::<f64>().ok()
}

// ---------------------------------------------------------------------------
// Exporter names -> the contract's vocabulary
// ---------------------------------------------------------------------------

/// Where a number came from, which decides what has to happen to it before it
/// is a rate. Ordered by how much this module trusts it — see [`MAPPINGS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    /// A level. Used as-is, and implies no rate.
    Gauge,
    /// A rate the JVM already computed (`OneMinuteRate`). Used as-is.
    Rate,
    /// A monotonic `Count`. The rate is computed here, from consecutive
    /// scrapes.
    Counter,
}

/// How several samples of one metric become one cluster number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aggregate {
    Sum,
    Max,
}

struct Mapping {
    exporter: &'static str,
    series: &'static str,
    source: Source,
    aggregate: Aggregate,
    /// Whether a `topic` label on this metric produces a per-topic series.
    per_topic: bool,
}

/// The mapping table — the only place an exporter's vocabulary is known.
///
/// Every entry names the MBean it comes from, because that is the thing a user
/// can actually check with `jconsole` when a series is missing.
///
/// **The counter/rate ambiguity, stated plainly.** A JMX exporter usually
/// publishes *both* halves of a Yammer meter: the monotonic `Count` and the
/// pre-computed `OneMinuteRate`. They disagree, legitimately — the first is
/// exact over whatever interval you sample it on, the second is an
/// exponentially weighted average that lags a step change by design and keeps
/// decaying after traffic stops. Kavka **prefers the counter** whenever one is
/// exposed (that is what [`Source`]'s ordering means) and falls back to the
/// rate gauge only when it is the sole option. The cost of that choice is one
/// blank tick at startup, because a counter needs two scrapes before it means
/// anything; the benefit is that a chart drawn on a 15-second cadence shows
/// what happened in those 15 seconds instead of a smeared minute of it.
///
/// **Order is meaningful, and it is the tie-break.** Several entries here are
/// the *same bean under a different rule file* — `..._bytesin_total` and
/// `..._bytesinpersec_count` are one number spelled two ways — and an exporter
/// configured with both rule sets publishes both. Exactly one spelling may
/// contribute to a series (see [`Rank`]); the one that appears first in this
/// table is the one that does, so the preferred spelling of a bean goes above
/// its alternates.
const MAPPINGS: &[Mapping] = &[
    // kafka.server<type=BrokerTopicMetrics, name=BytesInPerSec><>Count
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_bytesin_total",
        series: "bytes_in_per_sec",
        source: Source::Counter,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    // ...the same bean under the generic lowercase-everything rule.
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_bytesinpersec_count",
        series: "bytes_in_per_sec",
        source: Source::Counter,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    // kafka.server<type=BrokerTopicMetrics, name=BytesInPerSec><>OneMinuteRate
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_bytesinpersec_oneminuterate",
        series: "bytes_in_per_sec",
        source: Source::Rate,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    // kafka.server<type=BrokerTopicMetrics, name=BytesOutPerSec><>Count
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_bytesout_total",
        series: "bytes_out_per_sec",
        source: Source::Counter,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_bytesoutpersec_count",
        series: "bytes_out_per_sec",
        source: Source::Counter,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_bytesoutpersec_oneminuterate",
        series: "bytes_out_per_sec",
        source: Source::Rate,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    // kafka.server<type=BrokerTopicMetrics, name=MessagesInPerSec><>Count
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_messagesin_total",
        series: "messages_in_per_sec",
        source: Source::Counter,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_messagesinpersec_count",
        series: "messages_in_per_sec",
        source: Source::Counter,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    Mapping {
        exporter: "kafka_server_brokertopicmetrics_messagesinpersec_oneminuterate",
        series: "messages_in_per_sec",
        source: Source::Rate,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    // kafka.server<type=ReplicaManager, name=UnderReplicatedPartitions><>Value
    // — one per broker, so the cluster answer is the sum.
    Mapping {
        exporter: "kafka_server_replicamanager_underreplicatedpartitions",
        series: "under_replicated_partitions",
        source: Source::Gauge,
        aggregate: Aggregate::Sum,
        per_topic: false,
    },
    // kafka.cluster<type=Partition, name=UnderReplicated, topic=…, partition=…>
    // — a 0/1 per partition, which sums to the same number and carries a topic.
    Mapping {
        exporter: "kafka_cluster_partition_underreplicated",
        series: "under_replicated_partitions",
        source: Source::Gauge,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    // kafka.controller<type=KafkaController, name=OfflinePartitionsCount><>Value
    // — MAX, not sum: every broker exposes the bean and only the active
    // controller's answer means anything; the rest report 0. Summing would be
    // right by accident today and wrong the moment a failover leaves two
    // brokers briefly claiming the role.
    Mapping {
        exporter: "kafka_controller_kafkacontroller_offlinepartitionscount",
        series: "offline_partitions",
        source: Source::Gauge,
        aggregate: Aggregate::Max,
        per_topic: false,
    },
    // kafka.log<type=Log, name=Size, topic=…, partition=…><>Value — per
    // partition replica, summed. The cluster figure therefore counts every
    // replica, which is what "how much disk is this costing" means.
    Mapping {
        exporter: "kafka_log_size",
        series: "log_size_bytes",
        source: Source::Gauge,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
    Mapping {
        exporter: "kafka_log_log_size",
        series: "log_size_bytes",
        source: Source::Gauge,
        aggregate: Aggregate::Sum,
        per_topic: true,
    },
];

/// One series key's reading in one scrape, before rates are worked out.
#[derive(Debug, Clone, PartialEq)]
struct Reading {
    key: String,
    value: f64,
    source: Source,
}

/// A candidate reading's standing — the whole of the preference between the
/// several exporter metrics that can produce one series.
///
/// [`Source`] first: a counter beats a pre-computed rate beats a plain gauge,
/// for the reasons in [`MAPPINGS`]. Then position in that table, first match
/// winning, which is what keeps two *spellings of the same bean* from both
/// contributing. An exporter running both the `kafka-2_0_0.yml` sample rules
/// and the generic lowercase fallback publishes `..._bytesin_total` and
/// `..._bytesinpersec_count` side by side: they are one measurement, so summing
/// them doubles every throughput figure in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    source: Source,
    /// Reversed, so "earlier in [`MAPPINGS`]" sorts as "better" and one derived
    /// `Ord` covers both halves of the preference.
    table_position: Reverse<usize>,
}

/// Running aggregate of one series key's samples in one scrape.
#[derive(Debug, Clone, Copy, Default)]
struct Acc {
    sum: f64,
    max: f64,
    any: bool,
}

impl Acc {
    fn push(&mut self, value: f64) {
        self.sum += value;
        self.max = if self.any { self.max.max(value) } else { value };
        self.any = true;
    }

    fn value(self, aggregate: Aggregate) -> f64 {
        match aggregate {
            Aggregate::Sum => self.sum,
            Aggregate::Max => self.max,
        }
    }
}

/// Turns a scrape into one reading per series key.
///
/// Three decisions live here:
///
/// - **Non-finite readings are dropped.** A `NaN` from a bean that was never
///   initialised is not a measurement: charting it draws a hole, and comparing
///   it in an alert is silently false in both directions.
/// - **A topic-less sample wins over the per-topic ones for the cluster
///   figure.** Exporters publish `BytesInPerSec` both per topic *and* as an
///   all-topics aggregate; adding both would double every throughput number in
///   the app. Per-topic samples are only summed into the cluster figure when no
///   aggregate line exists at all.
/// - **A counter beats a rate gauge** for the same series, and between two
///   spellings of one bean the first in [`MAPPINGS`] wins — see [`Rank`].
fn fold_samples(samples: &[MetricSample]) -> Vec<Reading> {
    // Cluster figures, kept apart by where they came from so the double count
    // above is impossible rather than merely corrected — and, within that, by
    // RANK rather than by source, so two spellings of one bean stay two
    // candidates to choose between instead of becoming one sum.
    let mut from_aggregate: BTreeMap<(&str, Rank), (Acc, Aggregate)> = BTreeMap::new();
    let mut from_topics: BTreeMap<(&str, Rank), (Acc, Aggregate)> = BTreeMap::new();
    let mut per_topic: BTreeMap<(String, Rank), (Acc, Aggregate)> = BTreeMap::new();

    for sample in samples {
        if !sample.value.is_finite() {
            continue;
        }
        let Some((position, mapping)) = MAPPINGS
            .iter()
            .enumerate()
            .find(|(_, mapping)| mapping.exporter == sample.name)
        else {
            continue;
        };
        let rank = Rank {
            source: mapping.source,
            table_position: Reverse(position),
        };
        let topic = mapping
            .per_topic
            .then(|| sample.label("topic"))
            .flatten()
            .filter(|topic| !topic.is_empty());

        match topic {
            None => from_aggregate
                .entry((mapping.series, rank))
                .or_insert((Acc::default(), mapping.aggregate))
                .0
                .push(sample.value),
            Some(topic) => {
                from_topics
                    .entry((mapping.series, rank))
                    .or_insert((Acc::default(), mapping.aggregate))
                    .0
                    .push(sample.value);
                per_topic
                    .entry((series_for_topic(mapping.series, topic), rank))
                    .or_insert((Acc::default(), mapping.aggregate))
                    .0
                    .push(sample.value);
            }
        }
    }

    // Deliberately by source and not by rank: "the exporter published its own
    // all-topics line for this series" is a fact about the measurement, not
    // about which rule file spelled it, and the guard against double counting
    // has to be the broader of the two.
    let aggregated: BTreeSet<(&str, Source)> = from_aggregate
        .keys()
        .map(|(series, rank)| (*series, rank.source))
        .collect();
    let mut best: BTreeMap<String, (Rank, Reading)> = BTreeMap::new();
    for ((series, rank), (acc, aggregate)) in &from_aggregate {
        offer(&mut best, series.to_string(), *rank, *acc, *aggregate);
    }
    for ((series, rank), (acc, aggregate)) in &from_topics {
        if !aggregated.contains(&(series, rank.source)) {
            offer(&mut best, series.to_string(), *rank, *acc, *aggregate);
        }
    }
    for ((key, rank), (acc, aggregate)) in per_topic {
        offer(&mut best, key, rank, acc, aggregate);
    }
    best.into_values().map(|(_, reading)| reading).collect()
}

/// Keeps the best candidate for a series key, and only that one — [`Rank`] is
/// the whole of the decision, and a candidate that loses contributes nothing
/// rather than being added to the winner.
fn offer(
    best: &mut BTreeMap<String, (Rank, Reading)>,
    key: String,
    rank: Rank,
    acc: Acc,
    aggregate: Aggregate,
) {
    if best.get(&key).is_some_and(|(kept, _)| *kept >= rank) {
        return;
    }
    let value = acc.value(aggregate);
    best.insert(
        key.clone(),
        (
            rank,
            Reading {
                key,
                value,
                source: rank.source,
            },
        ),
    );
}

/// Turns this scrape's readings into points, computing counter rates against
/// `counters` (which it updates).
///
/// **A counter that went backwards produces no point.** The broker restarted,
/// or a broker dropped out of the scrape and took its share of the sum with it;
/// either way the difference is not a rate, and the honest output is a gap in
/// the chart rather than a negative spike or a fabricated zero. The new value
/// is remembered, so the next scrape measures from the new baseline.
///
/// **A baseline older than [`MAX_BASELINE_AGE_MS`] is treated exactly the same
/// way**, and for the same reason. The counter is intact; the *interval* is
/// not. Dividing a whole outage's worth of bytes by a whole outage draws its
/// average as the reading for right now — a number that is neither current nor
/// wrong enough to look wrong, sitting on the chart where the gap should be and
/// in front of any rule watching that series. So the first scrape back
/// re-baselines and emits nothing, and the one after it — a normal interval
/// later — is the first honest rate.
fn to_points(
    readings: &[Reading],
    now_ms: i64,
    counters: &mut HashMap<String, (i64, f64)>,
) -> Vec<(String, f64)> {
    let mut points = Vec::new();
    for reading in readings {
        match reading.source {
            Source::Rate | Source::Gauge => points.push((reading.key.clone(), reading.value)),
            Source::Counter => {
                let previous = counters.insert(reading.key.clone(), (now_ms, reading.value));
                // First sight of this counter: a total, not a rate.
                let Some((then, was)) = previous else {
                    continue;
                };
                let elapsed_ms = now_ms - then;
                if elapsed_ms <= 0 || elapsed_ms > MAX_BASELINE_AGE_MS || reading.value < was {
                    continue;
                }
                let per_second = (reading.value - was) / (elapsed_ms as f64 / 1000.0);
                points.push((reading.key.clone(), per_second));
            }
        }
    }
    points
}

// ---------------------------------------------------------------------------
// The collector
// ---------------------------------------------------------------------------

/// One profile's metrics endpoint, its rolling window, and its status.
///
/// Secret discipline (D5), the same as [`crate::sr::SchemaRegistry`]: the
/// password is resolved from the keychain once, folded straight into an
/// `Authorization` header value, redacted from `Debug`, and never put in the
/// URL — where ureq would copy it into every error string.
pub struct MetricsCollector {
    url: String,
    authorization: Option<String>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// series key -> (timestamp, raw counter) from the previous scrape.
    counters: HashMap<String, (i64, f64)>,
    series: BTreeMap<String, VecDeque<MetricPoint>>,
    last_scrape_ms: Option<i64>,
    last_error: Option<String>,
    reachable: bool,
}

impl fmt::Debug for MetricsCollector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state();
        f.debug_struct("MetricsCollector")
            .field("url", &self.url)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .field("series", &state.series.len())
            .field("last_scrape_ms", &state.last_scrape_ms)
            .finish()
    }
}

impl MetricsCollector {
    /// Builds a collector for a profile's endpoint.
    ///
    /// Never fails, for the same reason [`crate::sr::SchemaRegistry::new`] does
    /// not: a keychain that cannot produce the password becomes the collector's
    /// first error — which the user reads as "the endpoint rejected these
    /// credentials", the true and actionable statement — rather than a
    /// constructor that refuses to exist and takes the panel with it.
    pub fn new(config: &MetricsEndpointConfig) -> Self {
        let collector = Self {
            url: config.url.trim().to_string(),
            authorization: None,
            state: Mutex::new(State::default()),
        };
        let Some(username) = config.username.as_deref() else {
            return collector;
        };
        let password = match &config.password {
            Some(secret) => match secrets::resolve(secret) {
                Ok(password) => password,
                Err(e) => {
                    collector.state().last_error = Some(format!("metrics credentials: {e}"));
                    return collector;
                }
            },
            // A bearer-style setup where the key is the username alone is
            // legal, like the Schema Registry's.
            None => String::new(),
        };
        Self {
            authorization: Some(basic_auth(username, &password)),
            ..collector
        }
    }

    /// Fetches the endpoint and folds the result into the window. Returns how
    /// many series the scrape produced points for.
    ///
    /// The error is returned *and* recorded: the caller logs it, the panel
    /// reads it back out of [`MetricsStatus`], and neither has to hold it.
    pub fn scrape(&self, now_ms: i64) -> Result<usize, String> {
        match self.fetch() {
            Ok(body) => {
                let series = self.ingest(&body, now_ms);
                let mut state = self.state();
                state.reachable = true;
                state.last_scrape_ms = Some(now_ms);
                state.last_error = None;
                Ok(series)
            }
            Err(e) => {
                let mut state = self.state();
                state.reachable = false;
                state.last_scrape_ms = Some(now_ms);
                state.last_error = Some(e.clone());
                Err(e)
            }
        }
    }

    /// Parse, fold and append — everything a scrape does except the socket.
    /// Public so the rate arithmetic can be driven through a timeline in a test
    /// without a server in the way.
    pub fn ingest(&self, body: &str, now_ms: i64) -> usize {
        let readings = fold_samples(&parse_prometheus(body));
        let mut state = self.state();
        let State {
            counters, series, ..
        } = &mut *state;
        let points = to_points(&readings, now_ms, counters);
        let count = points.len();
        for (key, value) in points {
            let ring = series.entry(key).or_default();
            // Out-of-order arrivals cannot happen (one caller, one clock), but
            // two scrapes inside one millisecond can; the later reading
            // replaces the earlier one rather than sitting beside it.
            if ring.back().is_some_and(|last| last.ts_ms == now_ms) {
                ring.pop_back();
            }
            ring.push_back(MetricPoint {
                ts_ms: now_ms,
                value,
            });
        }

        // The sweep is over the WHOLE map, not just the keys this scrape
        // touched. A topic that is deleted, or a broker that leaves the
        // exporter's output, stops appearing in every later scrape — and a ring
        // only trimmed when it is written to is a ring that is never trimmed
        // again. That is a day-old reading sitting in `series_available` as a
        // series the endpoint still publishes, and the memory under it, for as
        // long as the connection stays up. An emptied ring is dropped entirely
        // rather than left as a key with nothing behind it, so "the exporter
        // publishes this" and "Kavka has a reading for it" stay the same
        // statement. (A counter's baseline is left alone: it is one f64 per
        // series, and `to_points` can no longer build a rate on a stale one.)
        series.retain(|_, ring| {
            while ring.len() > MAX_POINTS
                || ring
                    .front()
                    .is_some_and(|first| now_ms - first.ts_ms > WINDOW_MS)
            {
                ring.pop_front();
            }
            !ring.is_empty()
        });
        count
    }

    /// One series over a window, downsampled to at most `max_points`.
    ///
    /// An unknown series name is empty rather than an error — see
    /// [`SERIES_VOCABULARY`].
    pub fn query(
        &self,
        series: &str,
        from_ms: i64,
        to_ms: i64,
        max_points: u32,
    ) -> Vec<MetricPoint> {
        let state = self.state();
        let Some(ring) = state.series.get(series) else {
            return Vec::new();
        };
        let window: Vec<MetricPoint> = ring
            .iter()
            .filter(|point| point.ts_ms >= from_ms && point.ts_ms <= to_ms)
            .copied()
            .collect();
        downsample_keeping_excursions(window, from_ms, to_ms, max_points)
    }

    /// The freshest value of every series, for the alert evaluator.
    ///
    /// Anything older than [`FRESH_MS`] is left out: an alert must not fire —
    /// or stay fired — on a number from an endpoint that stopped answering an
    /// hour ago.
    pub fn latest(&self, now_ms: i64) -> Vec<(String, f64)> {
        let state = self.state();
        state
            .series
            .iter()
            .filter_map(|(key, ring)| {
                let last = ring.back()?;
                (now_ms - last.ts_ms <= FRESH_MS).then(|| (key.clone(), last.value))
            })
            .collect()
    }

    pub fn status(&self) -> MetricsStatus {
        let state = self.state();
        MetricsStatus {
            configured: true,
            reachable: state.reachable,
            last_scrape_ms: state.last_scrape_ms,
            last_error: state.last_error.clone(),
            series_available: state.series.keys().cloned().collect(),
        }
    }

    fn fetch(&self) -> Result<String, String> {
        let mut request = ureq::get(&self.url)
            .config()
            // Same reasoning as src/sr.rs: an internal exporter is routinely
            // fronted by a private CA that lives in the OS trust store and
            // nowhere else.
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            // Read the body ourselves: a 401 from a proxy usually explains
            // itself in the first line, and that line is the diagnosis.
            .http_status_as_error(false)
            .timeout_global(Some(HTTP_TIMEOUT))
            .build()
            .header("Accept", "text/plain");
        if let Some(authorization) = &self.authorization {
            request = request.header("Authorization", authorization);
        }
        let mut response = request
            .call()
            .map_err(|e| format!("couldn't reach the metrics endpoint at {}: {e}", self.url))?;

        let status = response.status();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("reading the metrics endpoint at {}: {e}", self.url))?;
        if !status.is_success() {
            return Err(format!(
                "the metrics endpoint at {} answered {}{}",
                self.url,
                status.as_u16(),
                first_line(&body)
            ));
        }
        Ok(body)
    }

    /// Poison-tolerant, like the rest of the crate: the guarded state is a
    /// cache of numbers, and turning one panic into a permanently dead metrics
    /// panel would be the worse failure.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The first line of an error body, quoted after a colon — enough to tell an
/// HTML login page from a plain 404 without pasting a page into a banner.
fn first_line(body: &str) -> String {
    let Some(line) = body.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return String::new();
    };
    match line.char_indices().nth(117) {
        Some((cut, _)) => format!(": {}…", &line[..cut]),
        None => format!(": {line}"),
    }
}

/// RFC 7617 basic credentials. Built here rather than by putting the
/// credentials in the URL, which would leak them into every ureq error string.
fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

/// Reduces a series to at most `max_points`, keeping **the point in each bucket
/// that deviates most from that bucket's mean**.
///
/// [`crate::history`] keeps the maximum, because a lag chart only ever has to
/// survive a spike. A metrics chart has to survive both directions: an
/// under-replicated spike matters, *and* so does the throughput dip a
/// `throughput_floor` rule just fired on — and a chart that draws a flat line
/// through the dip its own alert named is worse than no chart at all. Keeping
/// the largest excursion in either direction is one rule that preserves both.
///
/// (Alert evaluation never reads this. It reads the raw ring through
/// [`MetricsCollector::latest`], so nothing ever fires — or fails to — because
/// of how a chart was compressed.)
fn downsample_keeping_excursions(
    points: Vec<MetricPoint>,
    from_ms: i64,
    to_ms: i64,
    max_points: u32,
) -> Vec<MetricPoint> {
    let buckets = i128::from(max_points.max(1));
    if i128::try_from(points.len()).unwrap_or(i128::MAX) <= buckets {
        return points;
    }
    // A zero-or-negative span would put everything in one bucket; `max(1)`
    // makes that arithmetic honest rather than a division by zero.
    let span = i128::from((to_ms - from_ms).max(1));

    let mut grouped: BTreeMap<i128, Vec<MetricPoint>> = BTreeMap::new();
    for point in points {
        let offset = (i128::from(point.ts_ms) - i128::from(from_ms)).max(0);
        let bucket = (offset * buckets / span).min(buckets - 1);
        grouped.entry(bucket).or_default().push(point);
    }
    grouped
        .into_values()
        .filter_map(|bucket| {
            let mean = bucket.iter().map(|point| point.value).sum::<f64>() / bucket.len() as f64;
            bucket
                .into_iter()
                // `total_cmp` rather than `partial_cmp`: every value in the
                // ring is finite (see `fold_samples`), and a comparator that
                // can return `None` is a panic waiting for the day that stops
                // being true.
                .max_by(|a, b| (a.value - mean).abs().total_cmp(&(b.value - mean).abs()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real `jmx_exporter` dump, trimmed to the beans this module maps — and
    /// keeping its quirks: the `# HELP`/`# TYPE` preamble, Java scientific
    /// notation, the trailing comma inside the brace group, per-topic *and*
    /// aggregate lines for the same bean, a bean that reads `NaN` because it
    /// was never initialised, and metrics Kavka has no use for.
    const JMX_FIXTURE: &str = r#"
# HELP kafka_server_brokertopicmetrics_bytesin_total Attribute exposed for management (kafka.server<type=BrokerTopicMetrics, name=BytesInPerSec><>Count)
# TYPE kafka_server_brokertopicmetrics_bytesin_total untyped
kafka_server_brokertopicmetrics_bytesin_total{topic="orders.v2",} 8.0E6
kafka_server_brokertopicmetrics_bytesin_total{topic="payments.v1",} 2.0E6
kafka_server_brokertopicmetrics_bytesin_total 1.0E7
# HELP kafka_server_brokertopicmetrics_bytesinpersec_oneminuterate Attribute exposed for management (kafka.server<type=BrokerTopicMetrics, name=BytesInPerSec><>OneMinuteRate)
# TYPE kafka_server_brokertopicmetrics_bytesinpersec_oneminuterate untyped
kafka_server_brokertopicmetrics_bytesinpersec_oneminuterate 41234.5
# HELP kafka_server_brokertopicmetrics_bytesout_total Attribute exposed for management (kafka.server<type=BrokerTopicMetrics, name=BytesOutPerSec><>Count)
# TYPE kafka_server_brokertopicmetrics_bytesout_total untyped
kafka_server_brokertopicmetrics_bytesout_total 4.0E7
# HELP kafka_server_brokertopicmetrics_messagesin_total Attribute exposed for management (kafka.server<type=BrokerTopicMetrics, name=MessagesInPerSec><>Count)
# TYPE kafka_server_brokertopicmetrics_messagesin_total untyped
kafka_server_brokertopicmetrics_messagesin_total{topic="orders.v2",} 120000.0
kafka_server_brokertopicmetrics_messagesin_total 150000.0
# HELP kafka_server_replicamanager_underreplicatedpartitions Attribute exposed for management (kafka.server<type=ReplicaManager, name=UnderReplicatedPartitions><>Value)
# TYPE kafka_server_replicamanager_underreplicatedpartitions untyped
kafka_server_replicamanager_underreplicatedpartitions{instance="broker-1",} 2.0
kafka_server_replicamanager_underreplicatedpartitions{instance="broker-2",} 1.0
kafka_server_replicamanager_underreplicatedpartitions{instance="broker-3",} 0.0
# HELP kafka_controller_kafkacontroller_offlinepartitionscount Attribute exposed for management (kafka.controller<type=KafkaController, name=OfflinePartitionsCount><>Value)
# TYPE kafka_controller_kafkacontroller_offlinepartitionscount untyped
kafka_controller_kafkacontroller_offlinepartitionscount{instance="broker-1",} 3.0
kafka_controller_kafkacontroller_offlinepartitionscount{instance="broker-2",} 0.0
# HELP kafka_log_size Attribute exposed for management (kafka.log<type=Log, name=Size, topic=…, partition=…><>Value)
# TYPE kafka_log_size untyped
kafka_log_size{topic="orders.v2",partition="0",} 1048576.0
kafka_log_size{topic="orders.v2",partition="1",} 2097152.0
kafka_log_size{topic="payments.v1",partition="0",} 524288.0
# HELP kafka_server_kafkaserver_linux_disk_read_bytes Attribute exposed for management
# TYPE kafka_server_kafkaserver_linux_disk_read_bytes untyped
kafka_server_kafkaserver_linux_disk_read_bytes NaN
jvm_threads_current 42.0
"#;

    fn named<'a>(samples: &'a [MetricSample], name: &str) -> Vec<&'a MetricSample> {
        samples.iter().filter(|s| s.name == name).collect()
    }

    fn reading(readings: &[Reading], key: &str) -> Option<f64> {
        readings.iter().find(|r| r.key == key).map(|r| r.value)
    }

    #[test]
    fn the_jmx_exporter_fixture_parses() {
        let samples = parse_prometheus(JMX_FIXTURE);
        // The HELP/TYPE lines are gone and nothing else is.
        assert_eq!(samples.len(), 17, "{samples:#?}");

        let bytes_in = named(&samples, "kafka_server_brokertopicmetrics_bytesin_total");
        assert_eq!(bytes_in.len(), 3);
        assert_eq!(bytes_in[0].label("topic"), Some("orders.v2"));
        assert_eq!(bytes_in[0].value, 8.0e6);
        // The trailing comma inside the braces is jmx_exporter's own output,
        // and it produces one label rather than an empty second one.
        assert_eq!(bytes_in[0].labels.len(), 1);
        // The aggregate line carries no labels at all.
        assert!(bytes_in[2].labels.is_empty());
        assert_eq!(bytes_in[2].value, 1.0e7);

        let sizes = named(&samples, "kafka_log_size");
        assert_eq!(sizes[0].label("topic"), Some("orders.v2"));
        assert_eq!(sizes[0].label("partition"), Some("0"));

        // A metric with no mapping still parses — dropping it is the folding
        // step's job, not the parser's.
        assert_eq!(named(&samples, "jvm_threads_current").len(), 1);
    }

    #[test]
    fn the_parser_keeps_non_finite_values_and_the_fold_drops_them() {
        let samples = parse_prometheus(JMX_FIXTURE);
        let nan = named(&samples, "kafka_server_kafkaserver_linux_disk_read_bytes");
        assert!(nan[0].value.is_nan());

        let samples = parse_prometheus(
            "kafka_server_replicamanager_underreplicatedpartitions NaN\n\
             kafka_controller_kafkacontroller_offlinepartitionscount +Inf\n\
             kafka_log_size{topic=\"t\",} -Inf\n",
        );
        assert_eq!(samples.len(), 3);
        assert!(samples[1].value.is_infinite());
        assert!(
            fold_samples(&samples).is_empty(),
            "a non-finite reading is not a measurement"
        );
    }

    #[test]
    fn a_malformed_line_costs_that_line_and_nothing_else() {
        let samples = parse_prometheus(
            "kafka_log_size{topic=\"a\",} 1.0\n\
             this line has no value\n\
             kafka_log_size{unterminated=\"b 2.0\n\
             = 5\n\
             \n\
             kafka_log_size{topic=\"c\",} 3.0\n",
        );
        assert_eq!(
            samples
                .iter()
                .filter(|s| s.name == "kafka_log_size" && s.label("topic").is_some())
                .count(),
            2
        );
        assert_eq!(
            reading(&fold_samples(&samples), "log_size_bytes"),
            Some(4.0)
        );
    }

    #[test]
    fn label_values_are_unescaped_and_may_contain_braces() {
        let samples = parse_prometheus(
            r#"kafka_log_size{topic="a\"b",reason="}",note="line\nbreak\\here",partition="0"} 7"#,
        );
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].label("topic"), Some("a\"b"));
        assert_eq!(samples[0].label("reason"), Some("}"));
        assert_eq!(samples[0].label("note"), Some("line\nbreak\\here"));
        assert_eq!(samples[0].value, 7.0);
    }

    #[test]
    fn an_exporter_supplied_timestamp_is_ignored() {
        let samples = parse_prometheus("kafka_log_size{topic=\"a\"} 12.5 1719000000000");
        assert_eq!(samples[0].value, 12.5);
    }

    #[test]
    fn a_metric_with_no_labels_parses() {
        let samples = parse_prometheus("kafka_server_replicamanager_underreplicatedpartitions 4");
        assert!(samples[0].labels.is_empty());
        assert_eq!(samples[0].value, 4.0);
    }

    #[test]
    fn gauges_aggregate_per_the_mapping_table() {
        let readings = fold_samples(&parse_prometheus(JMX_FIXTURE));

        // Summed across brokers: 2 + 1 + 0.
        assert_eq!(reading(&readings, "under_replicated_partitions"), Some(3.0));
        // Max, not sum: only the active controller's answer means anything.
        assert_eq!(reading(&readings, "offline_partitions"), Some(3.0));
        // Summed across partition replicas, and split per topic.
        assert_eq!(reading(&readings, "log_size_bytes"), Some(3_670_016.0));
        assert_eq!(
            reading(&readings, "log_size_bytes:topic:orders.v2"),
            Some(3_145_728.0)
        );
        assert_eq!(
            reading(&readings, "log_size_bytes:topic:payments.v1"),
            Some(524_288.0)
        );
    }

    /// The double-count trap: an exporter publishes `BytesInPerSec` per topic
    /// *and* as an all-topics aggregate. Adding both reports 20 MB/s on a
    /// cluster doing 10.
    #[test]
    fn the_cluster_figure_prefers_the_exporters_own_aggregate() {
        let readings = fold_samples(&parse_prometheus(JMX_FIXTURE));
        assert_eq!(reading(&readings, "bytes_in_per_sec"), Some(1.0e7));
        assert_eq!(
            reading(&readings, "bytes_in_per_sec:topic:orders.v2"),
            Some(8.0e6)
        );

        // With no aggregate line, the per-topic ones are summed instead.
        let no_aggregate = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total{topic=\"a\",} 3\n\
             kafka_server_brokertopicmetrics_bytesin_total{topic=\"b\",} 4\n",
        ));
        assert_eq!(reading(&no_aggregate, "bytes_in_per_sec"), Some(7.0));
    }

    #[test]
    fn a_counter_beats_a_rate_gauge_for_the_same_series() {
        let readings = fold_samples(&parse_prometheus(JMX_FIXTURE));
        let bytes_in = readings
            .iter()
            .find(|r| r.key == "bytes_in_per_sec")
            .expect("series");
        assert_eq!(bytes_in.source, Source::Counter);
        assert_eq!(
            bytes_in.value, 1.0e7,
            "the counter's total, not the gauge's rate"
        );
    }

    /// The other double-count trap, and the one a single exporter springs on
    /// its own: `jmx_exporter` running both the sample Kafka rules and the
    /// generic lowercase fallback publishes one bean under two names. Adding
    /// them reports 20 MB/s on a cluster doing 10, exactly like the
    /// aggregate-versus-per-topic case above.
    #[test]
    fn two_spellings_of_one_bean_contribute_once() {
        // Deliberately different values: whichever number comes out is the name
        // of the spelling that won.
        let readings = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total 1000\n\
             kafka_server_brokertopicmetrics_bytesinpersec_count 999\n\
             kafka_server_brokertopicmetrics_bytesinpersec_oneminuterate 41234.5\n",
        ));
        let bytes_in: Vec<&Reading> = readings
            .iter()
            .filter(|r| r.key == "bytes_in_per_sec")
            .collect();
        assert_eq!(bytes_in.len(), 1, "one series, one reading: {readings:#?}");
        assert_eq!(
            bytes_in[0].value, 1000.0,
            "not 1999, and not the rate gauge either"
        );
        assert_eq!(
            bytes_in[0].source,
            Source::Counter,
            "the preferred spelling is the counter first in MAPPINGS"
        );

        // Per-topic keys follow the same rule, and so does the cluster figure
        // summed out of them when no aggregate line exists.
        let per_topic = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total{topic=\"orders.v2\",} 1000\n\
             kafka_server_brokertopicmetrics_bytesinpersec_count{topic=\"orders.v2\",} 999\n",
        ));
        assert_eq!(
            reading(&per_topic, "bytes_in_per_sec:topic:orders.v2"),
            Some(1000.0)
        );
        assert_eq!(reading(&per_topic, "bytes_in_per_sec"), Some(1000.0));
    }

    #[test]
    fn a_rate_gauge_is_used_when_it_is_the_only_source() {
        let readings = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesinpersec_oneminuterate 41234.5\n",
        ));
        assert_eq!(readings[0].source, Source::Rate);

        // ...and it needs no second scrape to become a point.
        let mut counters = HashMap::new();
        assert_eq!(
            to_points(&readings, 1_000, &mut counters),
            vec![("bytes_in_per_sec".to_string(), 41234.5)]
        );
    }

    #[test]
    fn a_counter_becomes_a_per_second_rate_on_the_second_scrape() {
        let mut counters = HashMap::new();
        let readings = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total 1000\n",
        ));
        // First scrape: a total is not a rate.
        assert!(to_points(&readings, 10_000, &mut counters).is_empty());

        let later = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total 4000\n",
        ));
        // 3000 bytes over 15 seconds.
        assert_eq!(
            to_points(&later, 25_000, &mut counters),
            vec![("bytes_in_per_sec".to_string(), 200.0)]
        );
    }

    #[test]
    fn a_counter_reset_produces_a_gap_and_then_recovers() {
        let mut counters = HashMap::new();
        let scrape = |total: &str| {
            fold_samples(&parse_prometheus(&format!(
                "kafka_server_brokertopicmetrics_bytesin_total {total}\n"
            )))
        };

        assert!(to_points(&scrape("1000"), 0, &mut counters).is_empty());
        assert_eq!(
            to_points(&scrape("2000"), 1_000, &mut counters),
            vec![("bytes_in_per_sec".to_string(), 1000.0)]
        );
        // The broker restarted: the counter is back near zero, and the
        // difference is not a rate.
        assert!(
            to_points(&scrape("50"), 2_000, &mut counters).is_empty(),
            "a reset must not draw a negative spike"
        );
        // The next scrape measures from the new baseline, not the old one.
        assert_eq!(
            to_points(&scrape("150"), 3_000, &mut counters),
            vec![("bytes_in_per_sec".to_string(), 100.0)]
        );
    }

    /// The gap trap: an endpoint that goes away for hours and comes back has a
    /// counter that is perfectly intact and an interval that is not. Dividing
    /// by the whole outage draws its average as the reading for *right now* —
    /// a plausible number where the gap belongs, and in front of any rule
    /// watching that series.
    #[test]
    fn a_baseline_older_than_the_bound_re_baselines_instead_of_averaging_the_gap() {
        let scrape = |total: &str| {
            fold_samples(&parse_prometheus(&format!(
                "kafka_server_brokertopicmetrics_bytesin_total {total}\n"
            )))
        };

        let mut counters = HashMap::new();
        assert!(to_points(&scrape("1000"), 0, &mut counters).is_empty());
        // Dark for longer than a baseline may live, then back with hours of
        // accumulated traffic behind it.
        let back = MAX_BASELINE_AGE_MS + 1;
        assert!(
            to_points(&scrape("100000000"), back, &mut counters).is_empty(),
            "an outage's average is not the rate right now"
        );
        // The next scrape measures a real interval, from the new baseline —
        // 15 000 bytes over 15 seconds, not 100 MB over five minutes.
        assert_eq!(
            to_points(&scrape("100015000"), back + 15_000, &mut counters),
            vec![("bytes_in_per_sec".to_string(), 1000.0)]
        );

        // A baseline exactly at the bound still counts: the rule is "older
        // than", so no ordinary cadence can trip it.
        let mut counters = HashMap::new();
        assert!(to_points(&scrape("0"), 0, &mut counters).is_empty());
        assert_eq!(
            to_points(&scrape("1000"), MAX_BASELINE_AGE_MS, &mut counters),
            vec![(
                "bytes_in_per_sec".to_string(),
                1000.0 / (MAX_BASELINE_AGE_MS as f64 / 1000.0)
            )]
        );
    }

    #[test]
    fn two_scrapes_in_the_same_millisecond_produce_no_rate() {
        let mut counters = HashMap::new();
        let readings = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total 1000\n",
        ));
        to_points(&readings, 5_000, &mut counters);
        let readings = fold_samples(&parse_prometheus(
            "kafka_server_brokertopicmetrics_bytesin_total 2000\n",
        ));
        assert!(to_points(&readings, 5_000, &mut counters).is_empty());
    }

    fn collector_at(url: &str, authorization: Option<String>) -> MetricsCollector {
        MetricsCollector {
            url: url.to_string(),
            authorization,
            state: Mutex::new(State::default()),
        }
    }

    fn collector() -> MetricsCollector {
        collector_at("http://127.0.0.1:1/metrics", None)
    }

    #[test]
    fn the_window_drops_points_older_than_a_day() {
        let collector = collector();
        let text = "kafka_server_replicamanager_underreplicatedpartitions 1\n";
        collector.ingest(text, 0);
        collector.ingest(text, WINDOW_MS / 2);
        collector.ingest(text, WINDOW_MS + 1);

        let points = collector.query("under_replicated_partitions", i64::MIN, i64::MAX, 100);
        assert_eq!(points.len(), 2, "the point at t=0 aged out");
        assert_eq!(points[0].ts_ms, WINDOW_MS / 2);
    }

    /// A ring is only trimmed when it is written to, so a series that stops
    /// arriving is a series that is never trimmed again — a day-old number
    /// listed as something the endpoint publishes, and its memory held for as
    /// long as the connection is up. Deleting a topic is enough to cause it.
    #[test]
    fn a_series_the_exporter_stopped_publishing_ages_out() {
        let collector = collector();
        let both = "kafka_log_size{topic=\"orders.v2\",partition=\"0\",} 1024\n\
                    kafka_log_size{topic=\"retired.v1\",partition=\"0\",} 512\n";
        let one = "kafka_log_size{topic=\"orders.v2\",partition=\"0\",} 2048\n";
        let retired = "log_size_bytes:topic:retired.v1".to_string();

        collector.ingest(both, 0);
        // The topic is deleted; every scrape from here on is missing its line.
        collector.ingest(one, WINDOW_MS / 2);
        assert!(
            collector.status().series_available.contains(&retired),
            "still inside the window, so still a reading someone can look at"
        );

        collector.ingest(one, WINDOW_MS + 1);
        let available = collector.status().series_available;
        assert!(
            !available.contains(&retired),
            "a series nothing has published for a day is not one this endpoint \
             offers: {available:?}"
        );
        assert!(available.contains(&"log_size_bytes:topic:orders.v2".to_string()));
        assert!(collector
            .query(&retired, i64::MIN, i64::MAX, 100)
            .is_empty());
    }

    #[test]
    fn a_stale_series_is_not_offered_to_the_alert_evaluator() {
        let collector = collector();
        collector.ingest(
            "kafka_server_replicamanager_underreplicatedpartitions 7\n",
            1_000,
        );
        assert_eq!(
            collector.latest(1_000),
            vec![("under_replicated_partitions".to_string(), 7.0)]
        );
        assert!(
            collector.latest(1_000 + FRESH_MS + 1).is_empty(),
            "an alert must not fire on a number from a dead endpoint"
        );
    }

    #[test]
    fn an_unknown_series_is_empty_rather_than_an_error() {
        let collector = collector();
        collector.ingest("kafka_log_size{topic=\"a\"} 1\n", 0);
        assert!(collector.query("no_such_series", 0, 1, 10).is_empty());
        assert_eq!(collector.query("log_size_bytes", 0, 1, 10).len(), 1);
    }

    /// The mirror of the lag store's spike test, in the other direction: a
    /// throughput dip is what a `throughput_floor` rule fires on, and the chart
    /// beside the alert has to show it.
    #[test]
    fn metric_downsampling_keeps_a_dip_as_well_as_a_spike() {
        let mut points: Vec<MetricPoint> = (0..1_000)
            .map(|i| MetricPoint {
                ts_ms: i * 1_000,
                value: 500.0,
            })
            .collect();
        points[137].value = 0.0;
        points[820].value = 9_000.0;

        let kept = downsample_keeping_excursions(points, 0, 999_000, 10);
        assert!(kept.len() <= 10);
        assert!(
            kept.iter().any(|p| p.ts_ms == 137_000 && p.value == 0.0),
            "the dip vanished: {kept:?}"
        );
        assert!(
            kept.iter()
                .any(|p| p.ts_ms == 820_000 && p.value == 9_000.0),
            "the spike vanished: {kept:?}"
        );
        assert!(kept.windows(2).all(|pair| pair[0].ts_ms < pair[1].ts_ms));
    }

    #[test]
    fn a_short_series_is_returned_whole() {
        let points: Vec<MetricPoint> = (0..5)
            .map(|i| MetricPoint {
                ts_ms: i,
                value: i as f64,
            })
            .collect();
        assert_eq!(
            downsample_keeping_excursions(points.clone(), 0, 4, 100),
            points
        );
    }

    #[test]
    fn status_reports_an_endpoint_that_answered_and_one_that_did_not() {
        let collector = collector();
        assert_eq!(
            collector.status(),
            MetricsStatus {
                configured: true,
                reachable: false,
                last_scrape_ms: None,
                last_error: None,
                series_available: Vec::new(),
            }
        );

        // Nothing is listening on port 1.
        let error = collector.scrape(1_000).unwrap_err();
        let status = collector.status();
        assert!(!status.reachable);
        assert_eq!(status.last_scrape_ms, Some(1_000));
        assert_eq!(status.last_error.as_deref(), Some(error.as_str()));
        assert!(
            error.contains("127.0.0.1:1"),
            "the error has to name the endpoint: {error}"
        );
    }

    #[test]
    fn a_scrape_reads_the_endpoint_and_sends_its_credentials() {
        use crate::sr::canned::CannedRegistry;

        let server = CannedRegistry::start(vec![("/metrics", 200, JMX_FIXTURE)]);
        let collector = collector_at(
            &format!("{}/metrics", server.url()),
            Some(basic_auth("scrape", "s3cr3t")),
        );

        assert!(collector.scrape(1_000).expect("scrape") > 0);

        let seen = server.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "GET");
        assert_eq!(seen[0].path, "/metrics");
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some("Basic c2NyYXBlOnMzY3IzdA==")
        );

        let status = collector.status();
        assert!(status.reachable);
        assert_eq!(status.last_error, None);
        assert!(status
            .series_available
            .contains(&"under_replicated_partitions".to_string()));
        // The counter-backed series is absent after one scrape, by design.
        assert!(!status
            .series_available
            .contains(&"bytes_in_per_sec".to_string()));

        assert_eq!(
            collector.query("under_replicated_partitions", 0, 2_000, 10),
            vec![MetricPoint {
                ts_ms: 1_000,
                value: 3.0
            }]
        );
    }

    #[test]
    fn a_non_200_names_the_status_and_what_the_endpoint_said() {
        use crate::sr::canned::CannedRegistry;

        let server = CannedRegistry::start(vec![("/metrics", 401, "Unauthorized\nnope")]);
        let collector = collector_at(&format!("{}/metrics", server.url()), None);

        let error = collector.scrape(1_000).unwrap_err();
        assert!(error.contains("401"), "{error}");
        assert!(error.contains("Unauthorized"), "{error}");
        assert!(!collector.status().reachable);
    }

    #[test]
    fn a_series_key_carries_its_topic() {
        assert_eq!(
            series_for_topic("bytes_in_per_sec", "orders.v2"),
            "bytes_in_per_sec:topic:orders.v2"
        );
        assert_eq!(
            split_series_key("bytes_in_per_sec:topic:orders.v2"),
            ("bytes_in_per_sec", Some("orders.v2"))
        );
        assert_eq!(
            split_series_key("bytes_in_per_sec"),
            ("bytes_in_per_sec", None)
        );
    }

    /// Every series the mapping table can produce is one the contract names,
    /// and every name in the contract is reachable from some exporter metric.
    #[test]
    fn the_mapping_table_and_the_contract_agree() {
        for mapping in MAPPINGS {
            assert!(
                SERIES_VOCABULARY.contains(&mapping.series),
                "{} maps to {}, which is not in the contract",
                mapping.exporter,
                mapping.series
            );
        }
        for series in SERIES_VOCABULARY {
            assert!(
                MAPPINGS.iter().any(|m| m.series == series),
                "nothing produces {series}"
            );
        }
    }

    #[test]
    fn the_debug_impl_never_prints_the_credentials() {
        let collector = collector_at(
            "https://prometheus.example/federate",
            Some(basic_auth("scrape", "s3cr3t")),
        );
        let debug = format!("{collector:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("s3cr3t"));
        assert!(!debug.contains(&basic_auth("scrape", "s3cr3t")));
    }
}
