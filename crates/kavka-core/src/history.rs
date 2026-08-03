//! Lag history: the sampler, and the durable store it writes to
//! (docs/ARCHITECTURE.md D6).
//!
//! # Why this needs no broker cooperation
//!
//! A consumer group's lag is `end_offset - committed`, and [`crate::admin`]
//! already computes it for the groups screen. Sampling is therefore nothing
//! more than "call [`crate::admin::group_detail`] on a timer and keep the
//! answers" — no JMX, no exporter, no broker configuration, and it works on a
//! managed cluster where you have Describe and nothing else. That is what makes
//! lag charts the one Phase 4 feature that is *always* available; throughput
//! charts ([`crate::metrics`]) need an endpoint somebody set up.
//!
//! # Why the data is on disk and the metrics are not
//!
//! "24 h lag history survives app restarts" is a Phase 4 acceptance criterion,
//! and the answer to "was this group already behind before I went home" is
//! worth nothing if it starts empty every morning. So lag samples go to redb —
//! one file per profile, at `app-data-dir/history/<profile>.redb` (see
//! [`store_path`]). [`crate::metrics`] deliberately does the opposite, for
//! reasons spelled out there.
//!
//! # Shape of the store
//!
//! Two tables:
//!
//! - `samples_v1`: `(group, topic, partition, ts_ms)` -> `(has_committed,
//!   committed, end_offset)`. The key is ordered exactly the way every read
//!   wants to walk it — one partition's history over a time window is a single
//!   contiguous range.
//! - `series_v1`: `(group, topic, partition)` -> `(first_ts, last_ts)`. The
//!   index that makes retention affordable and answers "which groups do we have
//!   history for" without touching the samples at all.
//!
//! **Every operation walks `series_v1` in full rather than doing prefix
//! arithmetic on the sample key.** There is no cheap "all keys starting with
//! this group" bound in a lexicographic tuple keyspace — the successor of a
//! string prefix is a class of off-by-one this store does not need — and the
//! index is small: one entry per group×topic×partition, which is thousands even
//! on a large cluster, against millions of samples.
//!
//! `_v1` in the table names is the migration hatch. A future shape opens
//! `samples_v2` and leaves `v1` alone rather than reinterpreting bytes.
//!
//! # Blocking
//!
//! Every call here blocks (redb is synchronous, and so is the admin API it
//! samples). The Tauri shell wraps each in `spawn_blocking`, like the rest of
//! the core.

use crate::admin::{self, GroupDetail};
use crate::connection::ClusterConnection;
use crate::{Error, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How long samples are kept. Pruned on write, so a store that is never written
/// to again keeps whatever it had — deleting a stopped connection's history on
/// a timer would be a background thread doing damage nobody asked for.
pub const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Default sampling interval. Fifteen seconds is Kafka's own
/// `offsets.commit.interval.ms` order of magnitude: sampling much faster mostly
/// records the same committed offset twice, and sampling much slower turns a
/// short backlog into a chart with no evidence it happened.
pub const DEFAULT_INTERVAL_MS: u32 = 15_000;

/// Floor on the sampling interval. Each tick is a ListGroups plus, per group,
/// an OffsetFetch and one ListOffsets per partition — on a cluster with a
/// thousand partitions that is real load on the coordinator, and it is load
/// Kavka is adding to somebody's production broker without being asked twice.
pub const MIN_INTERVAL_MS: u32 = 5_000;

/// The sampling interval, held to [`MIN_INTERVAL_MS`]. A stored value from an
/// older build (or a hand-edited settings file) is clamped rather than
/// rejected: refusing to sample at all is a worse answer than sampling a little
/// less often than asked.
pub fn clamp_interval_ms(requested: u32) -> u32 {
    requested.max(MIN_INTERVAL_MS)
}

/// Wall-clock milliseconds since the Unix epoch — the only clock this module
/// uses, and the one every `ts_ms` in the IPC contract is in.
pub fn now_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        // Pre-epoch clock. Nothing here can do anything useful with it, and a
        // panic in the sampler would take the loop down.
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------------------
// The IPC contract. Field names are mirrored by the TypeScript in
// apps/desktop/src, so renaming one is a breaking change on both sides.
// ---------------------------------------------------------------------------

/// One partition's lag at one instant.
///
/// `committed` and `lag` are both `None` for a group that has never committed
/// on this partition — the same rule, and the same reason, as
/// [`crate::admin::GroupOffset`]: a group with no position has no measurable
/// backlog, and reporting `end_offset` as the lag would invent one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LagSample {
    pub ts_ms: i64,
    pub group_id: String,
    pub topic: String,
    pub partition: i32,
    pub committed: Option<i64>,
    pub end_offset: i64,
    pub lag: Option<i64>,
}

/// The window of history held for one group, for the picker that offers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupWindow {
    pub group_id: String,
    pub first_ts_ms: i64,
    pub last_ts_ms: i64,
}

/// What the sampler loop is doing, as the UI shows it.
///
/// `last_error` is the last tick's failure and is cleared by the next
/// successful one: a sampler that failed once an hour ago and has worked ever
/// since is not in an error state, and saying it is trains people to ignore the
/// field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplerStatus {
    pub running: bool,
    pub interval_ms: u32,
    pub last_sample_ms: Option<i64>,
    pub last_error: Option<String>,
}

impl Default for SamplerStatus {
    fn default() -> Self {
        Self {
            running: false,
            interval_ms: DEFAULT_INTERVAL_MS,
            last_sample_ms: None,
            last_error: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// A sample's key: group, topic, partition, timestamp — in that order, which is
/// the order every read walks.
type SampleKey = (&'static str, &'static str, i32, i64);

/// A sample's value: `(has_committed, committed, end_offset)`.
///
/// A fixed-width triple rather than an `Option<i64>` and a redundant lag:
/// `has_committed` is 0 or 1, `committed` is meaningless when it is 0, and the
/// lag is recomputed on read by [`lag_of`] — the *same* floor-at-zero arithmetic
/// [`crate::admin`] uses, tied to it by a test, so a stored sample and a live
/// one can never disagree about what lag means.
type SampleValue = (u8, i64, i64);

/// The series index's key and value: `(group, topic, partition)` ->
/// `(first_ts, last_ts)`.
type SeriesKey = (&'static str, &'static str, i32);
type SeriesValue = (i64, i64);

/// One row of the series index, owned: group, topic, partition, first, last.
type SeriesRow = (String, String, i32, i64, i64);

const SAMPLES: TableDefinition<SampleKey, SampleValue> = TableDefinition::new("samples_v1");

/// See the module docs for why every read walks this in full.
const SERIES: TableDefinition<SeriesKey, SeriesValue> = TableDefinition::new("series_v1");

/// The lag history of one connection.
///
/// One redb file per profile — rather than one database with a profile column —
/// because deleting a connection then deletes a file, a corrupt store costs one
/// cluster's charts instead of all of them, and two windows on two clusters
/// never contend for the same write lock.
pub struct HistoryStore {
    db: Database,
    path: PathBuf,
}

impl std::fmt::Debug for HistoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryStore")
            .field("path", &self.path)
            .finish()
    }
}

impl HistoryStore {
    /// Opens (or creates) the store at `path`, creating the parent directory.
    ///
    /// The error names the file, because the only useful things a user can do
    /// with a store that will not open are look at it and delete it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Other(format!("creating {}: {e}", parent.display())))?;
        }
        let db = Database::create(&path).map_err(|e| {
            Error::Other(format!(
                "couldn't open the lag history at {}: {e} — delete that file to start a fresh \
                 history for this connection",
                path.display()
            ))
        })?;
        Ok(Self { db, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes a batch of samples and prunes anything past [`RETENTION_MS`], in
    /// one transaction. Returns the number of samples written.
    ///
    /// One transaction for the batch, not one per sample: a tick on a large
    /// cluster is thousands of rows, and redb's durability cost is per commit.
    ///
    /// Retention runs on every write, deliberately. It is bounded by the size
    /// of the series index rather than by the size of the store (see the module
    /// docs), and the alternative — a sweep on a timer — is a second schedule
    /// to get wrong plus a store that grows without limit whenever that timer
    /// does not fire.
    pub fn append_samples(&self, batch: &[LagSample], now_ms: i64) -> Result<usize> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| self.failed("starting a write", e))?;
        let written = {
            let mut samples = txn
                .open_table(SAMPLES)
                .map_err(|e| self.failed("opening the samples table", e))?;
            let mut series = txn
                .open_table(SERIES)
                .map_err(|e| self.failed("opening the series table", e))?;

            for sample in batch {
                let key = (
                    sample.group_id.as_str(),
                    sample.topic.as_str(),
                    sample.partition,
                    sample.ts_ms,
                );
                let has_committed = u8::from(sample.committed.is_some());
                samples
                    .insert(
                        key,
                        (
                            has_committed,
                            sample.committed.unwrap_or(0),
                            sample.end_offset,
                        ),
                    )
                    .map_err(|e| self.failed("writing a sample", e))?;

                let index_key = (
                    sample.group_id.as_str(),
                    sample.topic.as_str(),
                    sample.partition,
                );
                let window = series
                    .get(index_key)
                    .map_err(|e| self.failed("reading the series index", e))?
                    .map(|guard| guard.value());
                let (first, last) = match window {
                    Some((first, last)) => (first.min(sample.ts_ms), last.max(sample.ts_ms)),
                    None => (sample.ts_ms, sample.ts_ms),
                };
                series
                    .insert(index_key, (first, last))
                    .map_err(|e| self.failed("updating the series index", e))?;
            }

            self.prune(&mut samples, &mut series, now_ms - RETENTION_MS)?;
            batch.len()
        };
        txn.commit()
            .map_err(|e| self.failed("committing the samples", e))?;
        Ok(written)
    }

    /// Drops everything older than `cutoff`, and forgets a series whose last
    /// sample has expired entirely.
    ///
    /// Takes the tables rather than opening its own: redb allows one handle per
    /// table per transaction, and this runs inside [`append_samples`]'s.
    fn prune(
        &self,
        samples: &mut redb::Table<SampleKey, SampleValue>,
        series: &mut redb::Table<SeriesKey, SeriesValue>,
        cutoff: i64,
    ) -> Result<()> {
        // Collected before mutating: the iterator borrows the table it is
        // about to be deleted from.
        let mut stale: Vec<SeriesRow> = Vec::new();
        {
            let index = series
                .iter()
                .map_err(|e| self.failed("scanning the series index", e))?;
            for entry in index {
                let (key, window) = entry.map_err(|e| self.failed("reading a series", e))?;
                let (group, topic, partition) = key.value();
                let (first, last) = window.value();
                if first < cutoff {
                    stale.push((group.to_string(), topic.to_string(), partition, first, last));
                }
            }
        }

        for (group, topic, partition, _first, last) in stale {
            let key = (group.as_str(), topic.as_str(), partition);
            samples
                .retain_in(
                    (group.as_str(), topic.as_str(), partition, i64::MIN)
                        ..(group.as_str(), topic.as_str(), partition, cutoff),
                    |_, _| false,
                )
                .map_err(|e| self.failed("pruning expired samples", e))?;

            if last < cutoff {
                // Nothing left at all — the group stopped being sampled long
                // enough ago that its whole window expired.
                series
                    .remove(key)
                    .map_err(|e| self.failed("forgetting an expired series", e))?;
                continue;
            }
            let surviving_first = samples
                .range(
                    (group.as_str(), topic.as_str(), partition, cutoff)
                        ..=(group.as_str(), topic.as_str(), partition, i64::MAX),
                )
                .map_err(|e| self.failed("re-reading a pruned series", e))?
                .next()
                .transpose()
                .map_err(|e| self.failed("re-reading a pruned series", e))?
                .map(|(key, _)| key.value().3);
            match surviving_first {
                Some(first) => series
                    .insert(key, (first, last))
                    .map_err(|e| self.failed("updating the series index", e))?
                    .map(|_| ()),
                // `last >= cutoff` with nothing surviving is impossible unless
                // the index disagreed with the samples; trusting the samples is
                // the repair.
                None => series
                    .remove(key)
                    .map_err(|e| self.failed("forgetting an empty series", e))?
                    .map(|_| ()),
            };
        }
        Ok(())
    }

    /// One group's history over `[from_ms, to_ms]`, downsampled to at most
    /// `max_points` per partition.
    ///
    /// `topic` narrows to one topic; `None` is every topic the group has
    /// history for. Results are ordered by topic, then partition, then time —
    /// which is chart order, one contiguous run per series.
    pub fn query(
        &self,
        group_id: &str,
        topic: Option<&str>,
        from_ms: i64,
        to_ms: i64,
        max_points: u32,
    ) -> Result<Vec<LagSample>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| self.failed("starting a read", e))?;
        let samples = match txn.open_table(SAMPLES) {
            Ok(table) => table,
            // A store that has never been written to has no tables yet, and
            // "no history" is the honest answer rather than an error.
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(self.failed("opening the samples table", e)),
        };

        let mut out = Vec::new();
        for (topic_name, partition) in self.series_of(group_id, topic)? {
            let range = samples
                .range(
                    (group_id, topic_name.as_str(), partition, from_ms)
                        ..=(group_id, topic_name.as_str(), partition, to_ms),
                )
                .map_err(|e| self.failed("reading a partition's history", e))?;
            let mut series = Vec::new();
            for entry in range {
                let (key, value) = entry.map_err(|e| self.failed("reading a sample", e))?;
                let (_, _, _, ts_ms) = key.value();
                let (has_committed, committed, end_offset) = value.value();
                let committed = (has_committed != 0).then_some(committed);
                series.push(LagSample {
                    ts_ms,
                    group_id: group_id.to_string(),
                    topic: topic_name.clone(),
                    partition,
                    committed,
                    end_offset,
                    lag: lag_of(committed, end_offset),
                });
            }
            out.extend(downsample_keeping_peaks(series, from_ms, to_ms, max_points));
        }
        Ok(out)
    }

    /// Every group with history in this store, and the window it covers.
    pub fn groups(&self) -> Result<Vec<GroupWindow>> {
        let mut windows: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for (group, _, _, first, last) in self.series_index()? {
            windows
                .entry(group)
                .and_modify(|window| {
                    window.0 = window.0.min(first);
                    window.1 = window.1.max(last);
                })
                .or_insert((first, last));
        }
        Ok(windows
            .into_iter()
            .map(|(group_id, (first_ts_ms, last_ts_ms))| GroupWindow {
                group_id,
                first_ts_ms,
                last_ts_ms,
            })
            .collect())
    }

    /// The `(topic, partition)` series this group has history for, in key
    /// order.
    fn series_of(&self, group_id: &str, topic: Option<&str>) -> Result<Vec<(String, i32)>> {
        Ok(self
            .series_index()?
            .into_iter()
            .filter(|(group, series_topic, _, _, _)| {
                group == group_id && topic.is_none_or(|wanted| series_topic == wanted)
            })
            .map(|(_, series_topic, partition, _, _)| (series_topic, partition))
            .collect())
    }

    /// The whole series index, materialized. See the module docs for why every
    /// read walks it instead of ranging over a key prefix.
    fn series_index(&self) -> Result<Vec<SeriesRow>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| self.failed("starting a read", e))?;
        let series = match txn.open_table(SERIES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(self.failed("opening the series table", e)),
        };
        let mut out = Vec::new();
        for entry in series
            .iter()
            .map_err(|e| self.failed("scanning the series index", e))?
        {
            let (key, window) = entry.map_err(|e| self.failed("reading a series", e))?;
            let (group, topic, partition) = key.value();
            let (first, last) = window.value();
            out.push((group.to_string(), topic.to_string(), partition, first, last));
        }
        Ok(out)
    }

    fn failed(&self, what: &str, e: impl std::fmt::Display) -> Error {
        Error::Other(format!(
            "lag history ({}): {what} failed: {e}",
            self.path.display()
        ))
    }
}

/// The lag arithmetic, in one place.
///
/// A commit can sit past the end — a reset to latest that raced a truncation,
/// or a stale watermark — and negative lag is never a true statement about
/// outstanding work, so it floors at zero. This is
/// [`crate::admin::group_detail`]'s rule, and a test pins the two together.
fn lag_of(committed: Option<i64>, end_offset: i64) -> Option<i64> {
    committed.map(|at| (end_offset - at).max(0))
}

/// Reduces one partition's samples to at most `max_points`, **keeping the
/// worst point in each bucket**.
///
/// This is the whole reason charts are worth drawing. A mean or a last-value
/// downsample turns a two-minute backlog spike into nothing at all once the
/// window is a day wide — which is precisely the moment somebody is looking for
/// it. Keeping the maximum means the chart can overstate how *long* a spike
/// lasted, and never that it happened; that is the right direction to be wrong
/// in for a lag chart.
///
/// A sample with no committed offset (`lag: None`) ranks below every real
/// measurement but still wins an otherwise empty bucket, so a stretch where the
/// group had no position stays visible instead of leaving a hole. Ties go to
/// the earliest sample, so a plateau is reported from where it started.
fn downsample_keeping_peaks(
    mut series: Vec<LagSample>,
    from_ms: i64,
    to_ms: i64,
    max_points: u32,
) -> Vec<LagSample> {
    let buckets = max_points.max(1) as i128;
    if (series.len() as i128) <= buckets {
        return series;
    }
    // A zero-or-negative span would put everything in one bucket; `max(1)`
    // makes that arithmetic honest rather than a division by zero.
    let span = (to_ms - from_ms).max(1) as i128;

    let mut kept: BTreeMap<i128, LagSample> = BTreeMap::new();
    for sample in series.drain(..) {
        let offset = (sample.ts_ms as i128 - from_ms as i128).max(0);
        let bucket = (offset * buckets / span).min(buckets - 1);
        match kept.get(&bucket) {
            // `-1` for "no position": below any real lag, above nothing.
            Some(best) if best.lag.unwrap_or(-1) >= sample.lag.unwrap_or(-1) => {}
            _ => {
                kept.insert(bucket, sample);
            }
        }
    }
    kept.into_values().collect()
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

/// What one tick of the sampler did.
///
/// Errors are collected rather than returned: one group the account cannot
/// describe must not stop the other forty being recorded, and a group that
/// disappears mid-tick (its last offset expired) is a normal event, not a
/// failure of the sampler.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRun {
    pub ts_ms: i64,
    pub groups: usize,
    pub written: usize,
    pub errors: Vec<String>,
}

impl SampleRun {
    /// One line for [`SamplerStatus::last_error`]: the first failure in full,
    /// then a count. Forty identical authorization errors would otherwise fill
    /// the status bar with the same sentence forty times.
    pub fn error_summary(&self) -> Option<String> {
        let first = self.errors.first()?;
        Some(match self.errors.len() {
            1 => first.clone(),
            n => format!("{first} (and {} more failed the same tick)", n - 1),
        })
    }
}

/// Everything one tick produced: the counters the status bar shows, and the
/// samples themselves.
///
/// The samples come back rather than only going to disk because the alert
/// evaluator wants them *fresh* — [`crate::alerts::Observation`] is built from
/// this, not from a store query. Re-reading what we just wrote would be a
/// second round trip to answer a question we already had the answer to, and
/// (worse) it would put the downsampling path between a measurement and the
/// alert that fires on it.
#[derive(Debug, Clone, Default)]
pub struct SampleTick {
    pub run: SampleRun,
    pub samples: Vec<LagSample>,
}

/// One tick: read every group's committed offsets and write the lag they imply.
///
/// The loop that calls this lives in the Tauri shell — it owns the timer, the
/// "is this profile still connected" question and the [`SamplerStatus`] it
/// publishes. This function is the part with the cluster and the disk in it.
pub fn sample_groups(conn: &ClusterConnection, store: &HistoryStore) -> SampleTick {
    let ts_ms = now_ms();
    let mut run = SampleRun {
        ts_ms,
        ..SampleRun::default()
    };

    let groups = match admin::groups_list(conn) {
        Ok(groups) => groups,
        Err(e) => {
            run.errors
                .push(format!("couldn't list consumer groups: {e}"));
            return SampleTick {
                run,
                samples: Vec::new(),
            };
        }
    };

    let mut samples = Vec::new();
    for group in &groups {
        match admin::group_detail(conn, &group.group_id) {
            Ok(detail) => {
                run.groups += 1;
                samples.extend(samples_from_detail(&detail, ts_ms));
            }
            // One group the account cannot describe — or one that expired
            // between the list and the describe — must not cost the other
            // forty. The message keeps the group's name, and loses the
            // newlines a classified broker error may carry so the status line
            // stays one line.
            Err(e) => run
                .errors
                .push(format!("{}: {e}", group.group_id).replace('\n', " ")),
        }
    }

    if !samples.is_empty() {
        match store.append_samples(&samples, ts_ms) {
            Ok(written) => run.written = written,
            Err(e) => run.errors.push(e.to_string()),
        }
    }
    SampleTick { run, samples }
}

/// The pure half of a tick: one group's detail becomes one sample per
/// partition, stamped with the tick's time rather than each partition's own —
/// so every series in a tick lines up on the chart's x-axis instead of
/// staircasing by however long the OffsetFetch took.
pub fn samples_from_detail(detail: &GroupDetail, ts_ms: i64) -> Vec<LagSample> {
    detail
        .offsets
        .iter()
        .map(|offset| LagSample {
            ts_ms,
            group_id: detail.group_id.clone(),
            topic: offset.topic.clone(),
            partition: offset.partition,
            committed: offset.committed,
            end_offset: offset.end_offset,
            lag: offset.lag,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Store paths
// ---------------------------------------------------------------------------

/// The store file for one profile, under the app's history directory.
///
/// **A profile id is not trusted as a path component.** Ids are generated by
/// the app, but a profile can arrive through
/// [`crate::profiles::import_json`] carrying whatever id its file said —
/// including `../../something` — and a store path is one of the few places a
/// string becomes a filesystem effect. Anything outside `[A-Za-z0-9._-]` is
/// replaced, and a replaced id additionally carries a hash of the original so
/// two ids that sanitize alike cannot share a history file.
pub fn store_path(history_dir: &Path, profile_id: &str) -> PathBuf {
    let sanitized: String = profile_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // `.` and `..` sanitize to themselves, and both are directories.
    let safe = !sanitized.is_empty()
        && sanitized != "."
        && sanitized != ".."
        && sanitized == profile_id
        && !sanitized.starts_with('.');
    let name = if safe {
        format!("{sanitized}.redb")
    } else {
        format!("{sanitized}-{:016x}.redb", fingerprint(profile_id))
    };
    history_dir.join(name)
}

/// FNV-1a, 64-bit. A hash, not a checksum: it exists so two sanitized-alike ids
/// get different files, and nothing depends on it being cryptographic.
fn fingerprint(value: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Scratch store dir, removed on drop — same shape as the profile store's,
    /// and for the same reason: no dev-dependency for a directory.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kavka-history-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn store(&self) -> HistoryStore {
            HistoryStore::open(store_path(&self.0, "p1")).expect("open store")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const HOUR: i64 = 60 * 60 * 1000;

    fn sample(ts_ms: i64, partition: i32, lag: i64) -> LagSample {
        LagSample {
            ts_ms,
            group_id: "checkout-service".into(),
            topic: "orders.v2".into(),
            partition,
            committed: Some(1_000),
            end_offset: 1_000 + lag,
            lag: Some(lag),
        }
    }

    #[test]
    fn samples_survive_a_write_and_a_reopen() {
        let dir = TempDir::new();
        let batch = vec![sample(1_000, 0, 5), sample(2_000, 0, 7)];
        {
            let store = dir.store();
            assert_eq!(store.append_samples(&batch, 2_000).unwrap(), 2);
        }
        // The acceptance criterion is literally "survives app restarts", so the
        // store is dropped and reopened rather than merely re-read.
        let store = dir.store();
        assert_eq!(
            store
                .query("checkout-service", None, 0, 9_999, 100)
                .unwrap(),
            batch
        );
    }

    #[test]
    fn a_sample_with_no_committed_offset_roundtrips_as_absent() {
        let dir = TempDir::new();
        let store = dir.store();
        let never_committed = LagSample {
            committed: None,
            lag: None,
            ..sample(1_000, 3, 0)
        };
        let batch = vec![never_committed];
        store.append_samples(&batch, 1_000).unwrap();

        let back = store
            .query("checkout-service", None, 0, 9_999, 100)
            .unwrap();
        assert_eq!(back, batch);
    }

    /// The stored lag is not stored at all — it is recomputed — so it has to
    /// agree with the live number on the groups screen. This is the tie.
    #[test]
    fn recomputed_lag_matches_the_admin_rule() {
        use crate::admin::{GroupDetail, GroupOffset};

        let detail = GroupDetail {
            group_id: "checkout-service".into(),
            state: "Stable".into(),
            members: Vec::new(),
            offsets: vec![
                GroupOffset {
                    topic: "orders.v2".into(),
                    partition: 0,
                    committed: Some(10),
                    end_offset: 42,
                    lag: Some(32),
                },
                // Committed past the end: a reset that raced a truncation.
                GroupOffset {
                    topic: "orders.v2".into(),
                    partition: 1,
                    committed: Some(99),
                    end_offset: 42,
                    lag: Some(0),
                },
                GroupOffset {
                    topic: "orders.v2".into(),
                    partition: 2,
                    committed: None,
                    end_offset: 42,
                    lag: None,
                },
            ],
        };

        let dir = TempDir::new();
        let store = dir.store();
        let batch = samples_from_detail(&detail, 5_000);
        store.append_samples(&batch, 5_000).unwrap();

        let back = store
            .query("checkout-service", None, 0, 9_999, 100)
            .unwrap();
        assert_eq!(back, batch);
        for (stored, live) in back.iter().zip(detail.offsets.iter()) {
            assert_eq!(stored.lag, live.lag, "partition {}", stored.partition);
        }
    }

    #[test]
    fn a_tick_stamps_every_partition_with_the_same_time() {
        use crate::admin::{GroupDetail, GroupOffset};

        let detail = GroupDetail {
            group_id: "g".into(),
            state: "Stable".into(),
            members: Vec::new(),
            offsets: (0..4)
                .map(|partition| GroupOffset {
                    topic: "t".into(),
                    partition,
                    committed: Some(0),
                    end_offset: 1,
                    lag: Some(1),
                })
                .collect(),
        };
        let batch = samples_from_detail(&detail, 7_777);
        assert_eq!(batch.len(), 4);
        assert!(batch.iter().all(|sample| sample.ts_ms == 7_777));
    }

    #[test]
    fn samples_past_the_retention_window_are_pruned_on_write() {
        let dir = TempDir::new();
        let store = dir.store();
        let now = 30 * 24 * HOUR;
        let old = now - RETENTION_MS - HOUR;

        store.append_samples(&[sample(old, 0, 1)], old).unwrap();
        assert_eq!(
            store
                .query("checkout-service", None, 0, i64::MAX, 100)
                .unwrap()
                .len(),
            1
        );

        // The write that crosses the boundary is the one that prunes.
        store.append_samples(&[sample(now, 0, 2)], now).unwrap();
        let back = store
            .query("checkout-service", None, 0, i64::MAX, 100)
            .unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].ts_ms, now);

        // ...and the index moved with it, so the picker doesn't offer a window
        // whose left half no longer exists.
        assert_eq!(
            store.groups().unwrap(),
            vec![GroupWindow {
                group_id: "checkout-service".into(),
                first_ts_ms: now,
                last_ts_ms: now,
            }]
        );
    }

    #[test]
    fn a_series_whose_whole_window_expired_is_forgotten() {
        let dir = TempDir::new();
        let store = dir.store();
        let now = 30 * 24 * HOUR;
        let old = now - RETENTION_MS - HOUR;

        store
            .append_samples(
                &[
                    LagSample {
                        group_id: "retired".into(),
                        ..sample(old, 0, 1)
                    },
                    sample(old, 0, 1),
                ],
                old,
            )
            .unwrap();
        assert_eq!(store.groups().unwrap().len(), 2);

        // A later write for one group prunes both — retention is a property of
        // the store, not of the series being written.
        store.append_samples(&[sample(now, 0, 2)], now).unwrap();
        assert_eq!(
            store
                .groups()
                .unwrap()
                .iter()
                .map(|g| g.group_id.as_str())
                .collect::<Vec<_>>(),
            vec!["checkout-service"]
        );
        assert!(store
            .query("retired", None, 0, i64::MAX, 100)
            .unwrap()
            .is_empty());
    }

    /// The property the whole feature rests on: a short spike inside a wide
    /// window must survive downsampling. A mean or a last-value rule loses it.
    #[test]
    fn downsampling_keeps_the_spike() {
        let dir = TempDir::new();
        let store = dir.store();
        let batch: Vec<LagSample> = (0..1_000)
            .map(|i| sample(i * 1_000, 0, if i == 617 { 4_200_000 } else { 12 }))
            .collect();
        store.append_samples(&batch, 1_000_000).unwrap();

        let back = store
            .query("checkout-service", None, 0, 999_000, 10)
            .unwrap();
        assert!(back.len() <= 10, "{} points for max_points=10", back.len());
        let peak = back.iter().max_by_key(|s| s.lag.unwrap_or(-1)).unwrap();
        assert_eq!(peak.lag, Some(4_200_000));
        assert_eq!(peak.ts_ms, 617_000, "the spike kept its own timestamp");
        // Ordered by time, so a chart can draw it without sorting.
        assert!(back.windows(2).all(|pair| pair[0].ts_ms < pair[1].ts_ms));
    }

    #[test]
    fn the_point_cap_is_per_partition() {
        let dir = TempDir::new();
        let store = dir.store();
        let batch: Vec<LagSample> = (0..300)
            .flat_map(|i| (0..3).map(move |partition| sample(i * 1_000, partition, i)))
            .collect();
        store.append_samples(&batch, 300_000).unwrap();

        let back = store
            .query("checkout-service", None, 0, 299_000, 10)
            .unwrap();
        for partition in 0..3 {
            let points = back.iter().filter(|s| s.partition == partition).count();
            assert!(points <= 10, "partition {partition} got {points} points");
            assert!(points > 0, "partition {partition} vanished");
        }
    }

    #[test]
    fn a_series_shorter_than_the_cap_is_returned_whole() {
        let dir = TempDir::new();
        let store = dir.store();
        let batch: Vec<LagSample> = (0..5).map(|i| sample(i * 1_000, 0, i)).collect();
        store.append_samples(&batch, 5_000).unwrap();

        assert_eq!(
            store
                .query("checkout-service", None, 0, 4_000, 100)
                .unwrap(),
            batch
        );
    }

    /// A bucket with only never-committed samples keeps one, so the gap is
    /// visible on the chart rather than absent from it.
    #[test]
    fn a_bucket_of_absent_positions_still_reports_a_point() {
        let series: Vec<LagSample> = (0..10)
            .map(|i| LagSample {
                committed: None,
                lag: None,
                ..sample(i * 100, 0, 0)
            })
            .collect();
        let kept = downsample_keeping_peaks(series, 0, 900, 2);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|sample| sample.lag.is_none()));
    }

    #[test]
    fn a_real_measurement_outranks_an_absent_one_in_the_same_bucket() {
        let series = vec![
            LagSample {
                committed: None,
                lag: None,
                ..sample(0, 0, 0)
            },
            sample(10, 0, 0),
            LagSample {
                committed: None,
                lag: None,
                ..sample(20, 0, 0)
            },
        ];
        let kept = downsample_keeping_peaks(series, 0, 100, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].lag, Some(0));
    }

    #[test]
    fn the_query_window_and_topic_both_narrow_the_answer() {
        let dir = TempDir::new();
        let store = dir.store();
        let batch = vec![
            sample(1_000, 0, 1),
            sample(2_000, 0, 2),
            sample(3_000, 0, 3),
            LagSample {
                topic: "payments.v1".into(),
                ..sample(2_000, 0, 9)
            },
        ];
        store.append_samples(&batch, 3_000).unwrap();

        let window = store
            .query("checkout-service", None, 2_000, 3_000, 100)
            .unwrap();
        assert_eq!(window.len(), 3);
        assert!(window.iter().all(|s| (2_000..=3_000).contains(&s.ts_ms)));

        let one_topic = store
            .query("checkout-service", Some("orders.v2"), 0, 9_999, 100)
            .unwrap();
        assert_eq!(one_topic.len(), 3);
        assert!(one_topic.iter().all(|s| s.topic == "orders.v2"));

        assert!(store
            .query("nobody", None, 0, 9_999, 100)
            .unwrap()
            .is_empty());
        assert!(store
            .query("checkout-service", Some("nope"), 0, 9_999, 100)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn an_empty_store_answers_rather_than_failing() {
        let dir = TempDir::new();
        let store = dir.store();
        assert!(store
            .query("anything", None, 0, i64::MAX, 100)
            .unwrap()
            .is_empty());
        assert!(store.groups().unwrap().is_empty());
    }

    #[test]
    fn groups_report_the_window_they_actually_cover() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .append_samples(
                &[
                    sample(1_000, 0, 1),
                    sample(9_000, 1, 1),
                    LagSample {
                        group_id: "billing".into(),
                        ..sample(4_000, 0, 1)
                    },
                ],
                9_000,
            )
            .unwrap();

        assert_eq!(
            store.groups().unwrap(),
            vec![
                GroupWindow {
                    group_id: "billing".into(),
                    first_ts_ms: 4_000,
                    last_ts_ms: 4_000,
                },
                GroupWindow {
                    group_id: "checkout-service".into(),
                    first_ts_ms: 1_000,
                    last_ts_ms: 9_000,
                },
            ]
        );
    }

    #[test]
    fn re_writing_the_same_instant_replaces_rather_than_duplicates() {
        let dir = TempDir::new();
        let store = dir.store();
        store.append_samples(&[sample(1_000, 0, 1)], 1_000).unwrap();
        store.append_samples(&[sample(1_000, 0, 5)], 1_000).unwrap();

        let back = store
            .query("checkout-service", None, 0, 9_999, 100)
            .unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].lag, Some(5));
    }

    #[test]
    fn a_store_path_stays_inside_the_history_directory() {
        let dir = Path::new("/app/history");
        assert_eq!(store_path(dir, "p1"), dir.join("p1.redb"));

        for hostile in ["../../etc/passwd", "..", ".", "", "a/b", "C:\\evil"] {
            let path = store_path(dir, hostile);
            assert_eq!(
                path.parent(),
                Some(dir),
                "{hostile:?} escaped to {}",
                path.display()
            );
        }
    }

    #[test]
    fn ids_that_sanitize_alike_get_different_files() {
        let dir = Path::new("/app/history");
        assert_ne!(store_path(dir, "a/b"), store_path(dir, "a:b"));
        assert_ne!(store_path(dir, ".."), store_path(dir, "."));
    }

    #[test]
    fn the_sampling_interval_has_a_floor() {
        assert_eq!(clamp_interval_ms(DEFAULT_INTERVAL_MS), DEFAULT_INTERVAL_MS);
        assert_eq!(clamp_interval_ms(0), MIN_INTERVAL_MS);
        assert_eq!(clamp_interval_ms(1_000), MIN_INTERVAL_MS);
        assert_eq!(clamp_interval_ms(60_000), 60_000);
    }

    #[test]
    fn a_ticks_errors_collapse_to_one_line() {
        let mut run = SampleRun::default();
        assert_eq!(run.error_summary(), None);
        run.errors.push("checkout-service: not authorized".into());
        assert_eq!(
            run.error_summary().as_deref(),
            Some("checkout-service: not authorized")
        );
        run.errors.push("billing: not authorized".into());
        assert_eq!(
            run.error_summary().as_deref(),
            Some("checkout-service: not authorized (and 1 more failed the same tick)")
        );
    }
}
