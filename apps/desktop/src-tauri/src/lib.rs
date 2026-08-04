//! Tauri shell: thin IPC layer over kavka-core. Commands stay dumb — all
//! logic, validation, and read-only enforcement live in the core crate.
//! Anything that blocks (file I/O, keychain, librdkafka calls — including
//! client drop) runs on the blocking pool, never the event loop.

use kavka_core::acl::{AclBinding, AclFilter};
use kavka_core::admin::{
    self, ConfigEntry, GroupDetail, GroupInfo, GroupOffset, OffsetResetSpec, TopicConfig,
    TopicDetail, TopicInfo,
};
use kavka_core::alerts::{
    self, AlertChannels, AlertEvent, AlertRule, AlertState, AlertStore, Observation,
};
use kavka_core::cancel::CancelToken;
use kavka_core::connect::{ConfigValidation, ConnectorSummary};
use kavka_core::connection::{ClusterConnection, ClusterOverview};
use kavka_core::consume::{self, FetchSpec, TailSession};
use kavka_core::history::{self, GroupWindow, HistoryStore, LagSample, SamplerStatus};
use kavka_core::masking::{MaskRule, MaskSet, MaskStore, MASK_NOTICE_MARKER};
use kavka_core::metrics::{MetricPoint, MetricsCollector, MetricsStatus};
use kavka_core::nlq::{self, SchemaHint, Translation};
use kavka_core::produce::{self, BulkSession, BulkSpec, Delivery, ProduceRecordSpec};
use kavka_core::profiles::{
    export_json, import_json, AuthConfig, ConnectionProfile, Environment, ImportReport,
    ImportStrategy, ProfileStore, WasmSerdeConfig,
};
// `TopicPartition` here is the protocol module's — `admin` has an
// identically-shaped one for group assignments, which is why it is reached
// through `admin::` everywhere rather than imported alongside this.
use kavka_core::protocol::{
    PartitionResult, ProtocolClient, QuorumInfo, QuotaEntity, QuotaEntityPart, QuotaOp,
    ReassignmentSpec, ReassignmentState, ShareGroupDetail, ShareGroupInfo, TopicPartition,
};
use kavka_core::search::{SearchSession, SearchSpec};
use kavka_core::serdes::MessageRecord;
use kavka_core::sql::{SqlColumn, SqlSession, SqlSpec};
use kavka_core::sr::{CompatibilityCheck, CompatibilityInForce, RegisteredId, SubjectVersion};
use kavka_core::streams::StreamsTopology;
use kavka_core::xcluster::{
    self, ConfigDiffRow, CopyEstimate, CopySession, CopySpec, OffsetMigrationRow,
};
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

/// How long a tail's reader waits for records before looking at the world
/// again. Also the worst-case latency of `tail_stop` and of the `ended`
/// payload, which is why it is short rather than "as long as the topic is
/// quiet".
const TAIL_POLL: Duration = Duration::from_millis(500);

/// How often a search or a bulk run is allowed to tell the UI where it is.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// How long a session's emitter waits for the UI to say it is listening before
/// emitting anyway.
///
/// The UI can only register its listeners after `search_start` / `produce_bulk`
/// resolves, because the id is what the events are addressed to — so anything
/// emitted in that window is lost, and a lost `done` is a progress bar that
/// never finishes while a lost result batch is rows the UI counts but cannot
/// show. A sleep long enough to *probably* cover an IPC round trip is not a
/// fix, it is a bet: the emitter now WAITS for `session_ready`, which the UI
/// calls the moment its listeners are up, so delivery is deterministic rather
/// than probable.
///
/// The timeout is the fallback for the one case the handshake cannot cover — a
/// window that never calls `session_ready` at all (an old build, a UI that
/// threw between subscribing and confirming). It emits anyway rather than
/// leaving a session running with nobody watching it forever.
///
/// **Nothing is lost while it waits.** Matches sit in the core's result buffer
/// until the first read, and progress is a snapshot of counters, not a stream.
const READY_TIMEOUT: Duration = Duration::from_secs(3);

/// How long a search's reader waits for matches before looking at the world
/// again. Also the worst-case latency of `search_stop` and of the final
/// progress event.
const SEARCH_POLL: Duration = Duration::from_millis(250);

/// A bulk run reports by snapshot rather than by blocking read, so this is only
/// how promptly its `done` becomes an event; `PROGRESS_EVERY` still bounds how
/// often everything before it does.
const BULK_POLL: Duration = Duration::from_millis(50);

/// How long a query's reader waits for result rows before looking at the world
/// again. Also the worst-case latency of `sql_stop` and of the final progress
/// event — the same trade as [`SEARCH_POLL`], for the same shape of read.
const SQL_POLL: Duration = Duration::from_millis(250);

/// A copy reports by snapshot for the same reason a bulk run does — the counts
/// are written by delivery callbacks on librdkafka's thread — so this is only
/// how promptly its `done` becomes an event.
const COPY_POLL: Duration = Duration::from_millis(50);

struct AppState {
    store: Arc<ProfileStore>,
    connections: Mutex<HashMap<String, Arc<ClusterConnection>>>,
    /// Live tails, keyed by the id their events are addressed to.
    tails: SessionMap<TailSession>,
    /// Running searches and bulk produce runs, on the same books for the same
    /// reason as tails: each owns a librdkafka client, and a cluster the user
    /// disconnects — or deletes — must not leave one working.
    searches: SessionMap<SearchSession>,
    bulks: SessionMap<BulkSession>,
    /// Running SQL queries, on the same books and for the same reason: a query
    /// owns a consumer, and an in-memory Arrow scan besides.
    sqls: SessionMap<SqlSession>,
    /// Running cross-cluster copies. The one kind of session bound to TWO
    /// profiles — see [`SessionEntry::dest_profile_id`] — because it owns a
    /// consumer on one cluster and a producer on another.
    copies: SessionMap<CopySession>,
    id_seq: AtomicU64,
    id_epoch: u64,
    /// The subscribe handshake of every session that has not started emitting
    /// yet, keyed by the same id its events are addressed to (see
    /// [`READY_TIMEOUT`]). Searches, bulk runs, queries and copies share it
    /// because they share the id namespace and the race.
    ready: Mutex<HashMap<String, Arc<ReadyGate>>>,
    /// The in-flight `messages_fetch` of each profile, so a newer browse can
    /// stop the one it replaces. A fetch is interactive and can hold a
    /// blocking-pool slot for the core's full 30s deadline: the moment the
    /// user changes the range, that slot is being spent on an answer the UI
    /// has already decided to throw away (`fetchSeq` in MessagesView).
    fetches: Mutex<HashMap<String, CancelToken>>,
    /// One kept-alive wire-protocol connection per profile — see
    /// [`ProtocolSlot`] and [`AppState::protocol_slot`].
    protocol: Mutex<HashMap<String, ProtocolSlot>>,
    /// Alert rules, channels and incident history — `alerts.json`, beside
    /// `profiles.json`. Local throughout: nothing here is read from or written
    /// to a cluster, which is why the alert commands are the one surface a
    /// read-only connection is not blocked from.
    alerts: Arc<AlertStore>,
    /// Display-masking rules — `masking.json`, beside the two above and local
    /// for the same reason.
    masks: Arc<MaskStore>,
    /// Each profile's rules **compiled**, so a scan of 400,000 records compiles
    /// its regexes once rather than 400,000 times (see
    /// `kavka_core::masking`'s performance contract).
    ///
    /// Keyed by profile because that is the scope of a rule, and dropped by
    /// every command that edits one — which is what makes a rule the user just
    /// switched on apply to the next batch of a tail that is already running,
    /// without a per-record read of `masking.json`.
    mask_sets: Mutex<HashMap<String, Arc<MaskSet>>>,
    /// The lag-history files, one per profile — see [`HistoryStores`].
    histories: Arc<HistoryStores>,
    /// The background sampler/scraper/alert loop of each connected profile,
    /// keyed by profile id because there is exactly one per connection — see
    /// [`Monitor`].
    monitors: Mutex<HashMap<String, Arc<Monitor>>>,
}

/// One profile's cached [`ProtocolClient`], or `None` when there isn't a live
/// one yet.
///
/// **Why a `Mutex` per slot rather than one over the map.** A `ProtocolClient`
/// owns a single socket with a single correlation-id sequence and a single read
/// cursor; it is explicitly NOT concurrency-safe (see the core module's docs),
/// so two commands must never be inside one at the same time. Serializing them
/// is therefore mandatory — but serializing them *per profile* is enough, and a
/// lock over the whole map would instead make a two-second reassignment poll on
/// one cluster block a quota read on another.
///
/// **Why an `Arc`.** The guard is taken on the blocking pool, never on the
/// event loop (a `MutexGuard` is not `Send`, and a Kafka round trip is not
/// something to hold the event loop for), so the slot has to outlive the map
/// lookup that found it. The `Arc` is cloned out under the map's lock and the
/// socket lock is taken afterwards, which is also what keeps the two locks from
/// ever being held at once.
type ProtocolSlot = Arc<Mutex<Option<ProtocolClient>>>;

/// What the shell needs from a core session: ask it to finish, idempotently,
/// from a thread that is not the one draining it.
///
/// Every session in the core already has exactly this method. The trait is what
/// lets one [`SessionMap`] keep the books for tails, searches, bulk runs,
/// queries and copies instead of five copies of the same bookkeeping drifting
/// apart — and the lifecycle rules here (stop before removing, drop off the
/// event loop, take a profile's sessions with the profile) are the ones that
/// must not drift.
trait Stoppable: Send + Sync + 'static {
    fn stop(&self);
}

impl Stoppable for TailSession {
    fn stop(&self) {
        TailSession::stop(self);
    }
}

impl Stoppable for SearchSession {
    fn stop(&self) {
        SearchSession::stop(self);
    }
}

impl Stoppable for BulkSession {
    fn stop(&self) {
        BulkSession::stop(self);
    }
}

impl Stoppable for SqlSession {
    fn stop(&self) {
        SqlSession::stop(self);
    }
}

impl Stoppable for CopySession {
    fn stop(&self) {
        CopySession::stop(self);
    }
}

/// The "I am listening" handshake for one session.
///
/// A condvar rather than a channel because the emitter is a plain OS thread and
/// there is exactly one thing to hear, once; the flag is what makes a `ready`
/// that arrives *before* the emitter reaches the gate still work.
#[derive(Default)]
struct ReadyGate {
    subscribed: Mutex<bool>,
    signal: std::sync::Condvar,
}

impl ReadyGate {
    /// The UI's side: idempotent, and safe to call for a session that has
    /// already started emitting.
    fn open(&self) {
        *self.subscribed.lock().unwrap() = true;
        self.signal.notify_all();
    }

    /// The emitter's side. `true` when the UI confirmed, `false` when the
    /// fallback timeout ran out — the caller emits either way, so the answer is
    /// only worth a log line.
    fn wait(&self, timeout: Duration) -> bool {
        let subscribed = self.subscribed.lock().unwrap();
        let (subscribed, wait) = self
            .signal
            .wait_timeout_while(subscribed, timeout, |ready| !*ready)
            .unwrap_or_else(|e| e.into_inner());
        *subscribed && !wait.timed_out()
    }
}

/// One kind of running session, keyed by the id its events are addressed to.
///
/// Each entry remembers the profile it belongs to, so disconnecting a cluster —
/// or deleting it — takes its sessions down with it instead of leaving a client
/// working for a view nobody can open again.
struct SessionMap<T> {
    entries: Mutex<HashMap<String, SessionEntry<T>>>,
}

struct SessionEntry<T> {
    profile_id: String,
    /// A SECOND cluster this session is bound to: the destination of a copy,
    /// which reads from `profile_id`'s workspace and writes somewhere else.
    /// `None` for every session that only ever touches one cluster, which is
    /// all of them except a copy.
    ///
    /// Disconnecting *either* end takes the copy down. Keying it by the source
    /// alone would leave a session WRITING to a cluster the user has just
    /// walked away from, which is the more expensive half of the thing this
    /// bookkeeping exists to prevent — and the destination is exactly the end
    /// the source's workspace does not show.
    dest_profile_id: Option<String>,
    session: Arc<T>,
}

impl<T> SessionEntry<T> {
    /// Whether this session belongs to `profile_id` at either end.
    fn touches(&self, profile_id: &str) -> bool {
        self.profile_id == profile_id || self.dest_profile_id.as_deref() == Some(profile_id)
    }
}

impl<T: Stoppable> SessionMap<T> {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, id: String, profile_id: &str, session: &Arc<T>) {
        self.insert_between(id, profile_id, None, session);
    }

    /// [`SessionMap::insert`] for a session that spans two connections — see
    /// [`SessionEntry::dest_profile_id`]. A copy whose destination is its own
    /// source is not a special case: `retain` visits each entry once, so an
    /// entry naming one profile twice is still taken exactly once.
    fn insert_between(
        &self,
        id: String,
        profile_id: &str,
        dest_profile_id: Option<&str>,
        session: &Arc<T>,
    ) {
        self.entries.lock().unwrap().insert(
            id,
            SessionEntry {
                profile_id: profile_id.to_string(),
                dest_profile_id: dest_profile_id.map(str::to_string),
                session: Arc::clone(session),
            },
        );
    }

    /// Removes one session and asks it to finish. The handle comes back so the
    /// caller can destroy it off the event loop: the last reference joins the
    /// core's worker thread.
    fn take(&self, id: &str) -> Option<Arc<T>> {
        let entry = self.entries.lock().unwrap().remove(id)?;
        entry.session.stop();
        Some(entry.session)
    }

    /// Same, for every session belonging to one profile — at either end.
    fn take_of(&self, profile_id: &str) -> Vec<Arc<T>> {
        let mut taken = Vec::new();
        self.entries.lock().unwrap().retain(|_, entry| {
            if !entry.touches(profile_id) {
                return true;
            }
            entry.session.stop();
            // Cloned before the entry goes: dropping the last reference here
            // would join a worker thread while holding this lock, and the
            // worker takes the same lock to retire itself.
            taken.push(Arc::clone(&entry.session));
            false
        });
        taken
    }

    /// A session retiring itself, from its own emitter thread.
    fn forget(&self, id: &str) {
        let retired = self.entries.lock().unwrap().remove(id);
        // Dropped after the guard, deliberately: a last-reference drop joins a
        // worker thread, and this thread's own handle is still alive anyway.
        drop(retired);
    }

    /// Sets every live session's stop flag and joins nothing. This runs on the
    /// way out of the event loop, where waiting on a broker is the one thing
    /// that must not happen — the worker threads are detached and the process
    /// is about to end regardless.
    fn stop_all(&self) {
        for entry in self.entries.lock().unwrap().values() {
            entry.session.stop();
        }
    }
}

/// Everything one profile had running, taken off the books in one go.
struct ProfileSessions {
    tails: Vec<Arc<TailSession>>,
    searches: Vec<Arc<SearchSession>>,
    bulks: Vec<Arc<BulkSession>>,
    sqls: Vec<Arc<SqlSession>>,
    /// Copies with this profile at *either* end.
    copies: Vec<Arc<CopySession>>,
}

impl ProfileSessions {
    fn is_empty(&self) -> bool {
        self.tails.is_empty()
            && self.searches.is_empty()
            && self.bulks.is_empty()
            && self.sqls.is_empty()
            && self.copies.is_empty()
    }
}

// ── Monitoring: the lag sampler, the metrics scrape, and the alert loop ────
//
// Phase 4 adds the one thing Kafka itself does not keep: time. A broker will
// say what the lag is now and has no idea what it was an hour ago, so the
// history is Kavka's own — sampled while a connection is up, written to a local
// redb file, pruned at 7 days. Everything in this section exists to make that
// sentence true, and to make the *gaps* in it legible: a hole in a chart is a
// hole in Kavka's attendance record, not an outage on the cluster, and
// `sampler_status` is how a screen can tell the two apart.
//
// NOTHING HERE MUTATES A CLUSTER. Sampling is `ListGroups` + `OffsetFetch` +
// `ListOffsets`, the metrics scrape is an HTTP GET at an address the user gave
// us, and alert rules are a local file. That is why none of it is gated on
// `ensure_writable`: a read-only connection is exactly the kind of connection
// somebody wants to watch.

/// How long a monitor waits for its first tick after the connection opens.
///
/// Zero, deliberately: a freshly connected cluster with an empty chart and a
/// "next sample in 15s" is the state this whole feature exists to avoid, and
/// the first tick is also what gives the alert evaluator something to evaluate.
const FIRST_TICK: Duration = Duration::ZERO;

/// How far the sampler's retry interval is allowed to stretch after repeated
/// dead ticks, as a shift: `interval << 3` is eight intervals, two minutes at
/// the default cadence.
///
/// **Why back off at all.** A cluster that has gone away does not fail fast —
/// each `groups_list` spends the core's metadata timeout before it gives up, so
/// a fixed cadence turns an outage into a thread that is permanently inside a
/// timeout, adding load to a broker that is already in trouble and writing the
/// same sentence into the log every fifteen seconds.
///
/// **Why the cap is this low.** The sampler has to notice recovery on a human
/// timescale: every tick it skips is a gap in a chart somebody will read as
/// "the cluster was quiet". Two minutes is the longest hole worth trading for
/// the quiet, and a recovered cluster is sampled again within one.
const BACKOFF_SHIFT_MAX: u32 = 3;

/// The lag-history files, one redb database per profile, opened on first use.
///
/// **One handle per profile, process-wide.** redb takes an exclusive lock on
/// its file, so the sampler writing and a `history_query` reading have to be the
/// same handle — opening it twice fails, and failing on the *read* would be an
/// empty chart for a cluster that is being sampled perfectly well.
///
/// **Opened lazily rather than on connect, and not closed on disconnect.**
/// History outlives the connection that produced it: it is on disk and pruned at
/// seven days, so the Monitoring tab can show last week for a profile nobody is
/// connected to. The handle is only taken back when the profile is deleted,
/// where the file has to be removable.
struct HistoryStores {
    dir: PathBuf,
    open: Mutex<HashMap<String, Arc<HistoryStore>>>,
}

impl HistoryStores {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// This profile's store, opening it — and the history directory — if this
    /// is the first use.
    ///
    /// Blocking: creates a directory and opens a file, so every caller is on the
    /// blocking pool or on the monitor's own thread.
    fn get(&self, profile_id: &str) -> kavka_core::Result<Arc<HistoryStore>> {
        // Poison-tolerant, like the protocol slot: what this guards is a map of
        // file handles, which a panic elsewhere cannot corrupt, and a poisoned
        // lock would turn one failed command into a profile whose history can
        // never be read again.
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(store) = open.get(profile_id) {
            return Ok(Arc::clone(store));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| {
            kavka_core::Error::Other(format!(
                "Kavka couldn't create {} to keep this connection's lag history in: {e}",
                self.dir.display()
            ))
        })?;
        let store = Arc::new(HistoryStore::open(self.path_of(profile_id))?);
        open.insert(profile_id.to_string(), Arc::clone(&store));
        Ok(store)
    }

    /// Takes one profile's handle off the books, so the file can be removed —
    /// an open redb database cannot be deleted on Windows.
    fn take(&self, profile_id: &str) -> Option<Arc<HistoryStore>> {
        self.open
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(profile_id)
    }

    fn path_of(&self, profile_id: &str) -> PathBuf {
        history::store_path(&self.dir, profile_id)
    }
}

/// One connected profile's background loop: sample lag, scrape metrics,
/// evaluate the alert rules, deliver whatever fired.
///
/// One per connection, started by `cluster_connect` and stopped by
/// `cluster_disconnect`, `profiles_delete` and app exit — the same lifecycle
/// discipline as a tail, and for the same reason: a loop that outlives its
/// connection is a client working a cluster the user has walked away from.
struct Monitor {
    profile_id: String,
    interval: Duration,
    interval_ms: u32,
    /// `None` when the profile has no `metrics_endpoint`, which is a fully
    /// functional connection — lag history needs no broker cooperation at all,
    /// and the throughput charts are the only thing missing.
    ///
    /// The collector's 24-hour ring lives here and nowhere else: it is a window
    /// of readings Kavka took while it was watching, not a record of the
    /// cluster, so it goes when the connection does.
    metrics: Option<Arc<MetricsCollector>>,
    inner: Mutex<MonitorState>,
    /// What makes `stop` prompt. The loop waits out its interval on this
    /// condvar rather than sleeping, so disconnecting costs milliseconds
    /// instead of up to a full sampling interval.
    wake: Condvar,
}

#[derive(Default)]
struct MonitorState {
    running: bool,
    stopped: bool,
    /// The last tick whose readings actually reached the history file.
    ///
    /// A tick that failed does not move it, and neither does one that read the
    /// cluster perfectly and then could not write what it read — "last sample"
    /// has to mean a sample somebody can now go and look at, or a chart that
    /// has stopped growing still claims to be current. A tick that found
    /// nothing to write *does* move it: a cluster with no consumer groups, or
    /// none that has ever committed, is a working sampler and an empty chart,
    /// and calling that "behind" would send somebody to debug a sampler that is
    /// doing its job.
    last_sample_ms: Option<i64>,
    last_error: Option<String>,
}

impl Monitor {
    /// Builds a profile's monitor. Blocking: [`MetricsCollector::new`] resolves
    /// the endpoint's password from the keychain, so this is only ever called
    /// from the blocking pool (see `cluster_connect`).
    fn for_profile(profile: &ConnectionProfile) -> Self {
        let interval_ms = history::clamp_interval_ms(
            profile
                .sampler_interval_ms
                .unwrap_or(history::DEFAULT_INTERVAL_MS),
        );
        Self {
            profile_id: profile.id.clone(),
            interval: Duration::from_millis(u64::from(interval_ms)),
            interval_ms,
            metrics: profile
                .metrics_endpoint
                .as_ref()
                .map(|config| Arc::new(MetricsCollector::new(config))),
            inner: Mutex::new(MonitorState::default()),
            wake: Condvar::new(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, MonitorState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn status(&self) -> SamplerStatus {
        let state = self.state();
        SamplerStatus {
            running: state.running,
            interval_ms: self.interval_ms,
            last_sample_ms: state.last_sample_ms,
            last_error: state.last_error.clone(),
        }
    }

    /// What one tick learned, for the status bar.
    fn record(&self, sampled_ms: Option<i64>, trouble: Option<String>) {
        let mut state = self.state();
        if let Some(ts_ms) = sampled_ms {
            state.last_sample_ms = Some(ts_ms);
        }
        state.last_error = trouble;
    }

    /// Waits out one interval, returning early the moment [`Monitor::stop`] is
    /// called. `false` means the loop is over.
    fn wait(&self, delay: Duration) -> bool {
        let state = self.state();
        let (state, _) = self
            .wake
            .wait_timeout_while(state, delay, |state| !state.stopped)
            .unwrap_or_else(|e| e.into_inner());
        !state.stopped
    }

    /// Idempotent, and safe from any thread — including one that is not the
    /// loop's.
    fn stop(&self) {
        self.state().stopped = true;
        self.wake.notify_all();
    }

    fn began(&self) {
        self.state().running = true;
    }

    /// The loop saying it has ended. `running: false` on a connected profile is
    /// a fact the Monitoring tab states plainly rather than a chart that simply
    /// stops.
    fn ended(&self) {
        self.state().running = false;
    }
}

/// The event name one profile's alert firings and resolutions are addressed to.
/// Emitted to every window, so the profile id in the name is what keeps two
/// clusters' alerts apart.
fn alerts_event(profile_id: &str) -> String {
    format!("kavka://alerts/{profile_id}")
}

/// One monitor's loop. Runs on its own thread — see [`spawn_emitter`], and the
/// same argument applies twice over here: this thread is alive for as long as
/// the connection is, so a blocking-pool slot would be a slot every keychain
/// read and metadata fetch queues behind for hours.
///
/// **Nothing in here is fatal.** A tick that cannot reach the cluster, an
/// endpoint that stopped answering, a rules file that will not parse, a webhook
/// that refuses — each is recorded and the loop goes round again. The only way
/// out is [`Monitor::stop`]. A sampler that dies on the first bad tick is a
/// sampler that is not running when the incident happens, which is the only time
/// anybody looks.
fn run_monitor(
    app: AppHandle,
    monitor: Arc<Monitor>,
    conn: Arc<ClusterConnection>,
    histories: Arc<HistoryStores>,
    alerts: Arc<AlertStore>,
) {
    let mut alert_state = AlertState::default();
    let mut consecutive_failures: u32 = 0;

    if !monitor.wait(FIRST_TICK) {
        // Nothing can be firing yet — the first tick has not happened — but the
        // rule is that no way out of this function leaves an incident open, and
        // an invariant with an exception in it is one nobody can rely on.
        close_open_incidents(&app, &alerts, &monitor.profile_id, &alert_state);
        monitor.ended();
        return;
    }

    loop {
        let now_ms = history::now_ms();
        let mut trouble: Vec<String> = Vec::new();
        let mut samples: Vec<LagSample> = Vec::new();
        // Whether this tick learned anything. A cluster with no consumer groups
        // is a perfectly good tick with nothing to record; a cluster that
        // refused every describe is not, and only the second one backs off.
        let mut measured = false;
        // Whether what it learned reached the disk, which is a different
        // question with a different answer: a broker that answered and a store
        // that refused the write are separate failures. Only the first is a
        // reason to back off — retrying more slowly does not fix a full disk —
        // and only this one may move `last_sample_ms`.
        let mut recorded = false;

        match histories.get(&monitor.profile_id) {
            Ok(store) => {
                let tick = history::sample_groups(&conn, &store);
                if let Some(summary) = tick.run.error_summary() {
                    trouble.push(summary);
                }
                measured = tick.run.groups > 0 || tick.run.errors.is_empty();
                recorded = reached_the_disk(&tick);
                samples = tick.samples;
            }
            Err(e) => trouble.push(e.to_string()),
        }

        let readings = match &monitor.metrics {
            Some(collector) => {
                if let Err(e) = collector.scrape(now_ms) {
                    // Deliberately NOT `sampler_status.last_error`: the scrape
                    // and the sampler fail independently, `metrics_status`
                    // carries this one in the endpoint's own words, and saying
                    // "the sampler is broken" because an exporter is down would
                    // send somebody to debug the wrong machine.
                    tracing::debug!("metrics scrape for {}: {e}", monitor.profile_id);
                }
                collector.latest(now_ms)
            }
            None => Vec::new(),
        };

        // The rules are re-read every tick rather than held: saving a rule has
        // to take effect on the next sample, and the alternative is a rules
        // editor whose changes need a reconnect to mean anything.
        match alerts.rules(&monitor.profile_id) {
            Ok(rules) => {
                // Raw measurements on both sides — `SampleTick::samples` and
                // `MetricsCollector::latest`, never the downsampled query paths
                // — so no alert ever fires, or fails to, because of how a chart
                // was compressed.
                let observation = Observation {
                    samples,
                    metrics: readings,
                };
                let (next, events) = alerts::evaluate(&rules, &observation, &alert_state, now_ms);
                alert_state = next;
                if !events.is_empty() {
                    let channels = alerts.channels(&monitor.profile_id).unwrap_or_default();
                    for event in &events {
                        raise(&app, &alerts, &monitor.profile_id, &channels, event);
                    }
                }
            }
            Err(e) => trouble.push(format!("couldn't read this connection's alert rules: {e}")),
        }

        consecutive_failures = if measured {
            0
        } else {
            consecutive_failures.saturating_add(1)
        };
        let delay = retry_delay(monitor.interval, consecutive_failures);
        let reported = (!trouble.is_empty()).then(|| {
            let mut line = trouble.join(" · ");
            if !measured {
                // Why the chart has stopped moving, and when it will start
                // again — the second half is the part a user can act on
                // (docs/DESIGN.md §7).
                line.push_str(&format!(" — trying again in {}", spoken(delay)));
            }
            line
        });
        monitor.record(recorded.then_some(now_ms), reported);

        if !monitor.wait(delay) {
            break;
        }
    }
    close_open_incidents(&app, &alerts, &monitor.profile_id, &alert_state);
    monitor.ended();
}

/// Closes out whatever was still firing when a monitor stopped.
///
/// An incident is a fire and a resolve carrying the same `fired_ms`. A loop
/// that ends while a rule is firing leaves the fire on its own — and the fire
/// on its own is not "an alert nobody resolved", it is a row the history will
/// show as still happening forever, on a connection nobody is watching. Worse,
/// the next connect starts from a fresh [`AlertState`], so the same rule fires
/// again and opens a *second* one beside it. One disconnect a day is a week of
/// them, and the count of how many times something actually happened — the
/// question the history exists to answer — becomes unanswerable.
///
/// **The detail says monitoring stopped. It does not say the condition
/// cleared,** because Kavka does not know: it stopped looking. Writing "back
/// under the threshold" here would invent the one fact this whole feature is
/// for, and a week later nobody could tell the invented resolve from a measured
/// one.
///
/// Recorded and emitted, and deliberately nothing further. An OS notification
/// and a webhook announce something that happened on the cluster; this happened
/// to Kavka. Waking somebody at 3am to tell them a laptop went to sleep is how
/// a person learns to ignore the channel that matters.
fn close_open_incidents(app: &AppHandle, store: &AlertStore, profile_id: &str, state: &AlertState) {
    if state.firing().is_empty() {
        return;
    }
    // One read for the names. A rule deleted since it fired cannot appear here
    // — `evaluate` drops the state along with the rule — but the file can still
    // refuse to be read, and an incident closed under its own id is worth more
    // than one left open because its name was unavailable.
    let rules = store.rules(profile_id).unwrap_or_default();
    for event in closing_events(&rules, state, history::now_ms()) {
        if let Err(e) = store.record_event(profile_id, &event) {
            tracing::warn!("closing alert {} for {profile_id}: {e}", event.rule_id);
        }
        if let Err(e) = app.emit(&alerts_event(profile_id), event.clone()) {
            tracing::warn!(
                "emitting the close of alert {} for {profile_id}: {e}",
                event.rule_id
            );
        }
    }
}

/// The resolves a stopped monitor owes its history: one per firing rule,
/// quoting the `fired_ms` of the incident it closes.
///
/// The arithmetic half of [`close_open_incidents`], with neither the store nor
/// the app in it — so what a synthesised resolve actually says is testable
/// without a running desktop.
fn closing_events(rules: &[AlertRule], state: &AlertState, now_ms: i64) -> Vec<AlertEvent> {
    state
        .firing()
        .into_iter()
        .map(|(rule_id, fired_ms)| AlertEvent {
            rule_id: rule_id.to_string(),
            // A rule with no name in the file is a rule the file could not be
            // read for; its id is a worse label than its name and a far better
            // one than nothing.
            rule_name: rules
                .iter()
                .find(|rule| rule.id() == rule_id)
                .map_or_else(|| rule_id.to_string(), |rule| rule.name().to_string()),
            fired_ms,
            resolved_ms: Some(now_ms),
            detail: "Kavka stopped monitoring this connection while this rule was firing, so \
                     the incident ends here. Whether the condition itself cleared is not \
                     something Kavka can say — it stopped watching."
                .to_string(),
        })
        .collect()
}

/// Whether one tick's readings actually reached the history file — the whole
/// of what [`MonitorState::last_sample_ms`] is allowed to move on.
///
/// Three cases, and the middle one is the bug this exists to keep out:
///
/// - **Samples were committed** (`written > 0`): a reading somebody can now go
///   and look at. Yes.
/// - **Nothing failed and there was nothing to write**: also yes. A cluster
///   with no consumer groups, or none that has ever committed an offset, is a
///   working sampler and an empty chart, and calling that "behind" sends
///   somebody to debug a sampler that is doing its job.
/// - **Anything else** — a describe that failed, an append the store refused, a
///   store that would not open: no. A tick that read the cluster perfectly and
///   then lost what it read has not taken a sample, whatever the brokers did,
///   and `written` staying at zero is exactly how [`history::SampleRun`] says
///   so. The failure itself reaches the user through `last_error`, which the
///   caller fills from the same run's `error_summary`.
fn reached_the_disk(tick: &history::SampleTick) -> bool {
    tick.run.written > 0 || (tick.samples.is_empty() && tick.run.errors.is_empty())
}

/// How long to wait before the next tick, after `consecutive_failures` dead
/// ones in a row (`0` while everything is working).
///
/// The FIRST failure retries at the normal cadence — a single dead tick is a
/// rebalance or a blip, and slowing down for it would put a hole in the chart
/// for something that was over before the next sample would have been. It is
/// the second and later ones that double, up to [`BACKOFF_SHIFT_MAX`].
fn retry_delay(interval: Duration, consecutive_failures: u32) -> Duration {
    interval
        * (1 << consecutive_failures
            .saturating_sub(1)
            .min(BACKOFF_SHIFT_MAX))
}

/// A retry delay as a person would say it. Whole units only: "in 1 min 45 s"
/// is precision about something nobody is timing.
fn spoken(delay: Duration) -> String {
    let seconds = delay.as_secs();
    if seconds < 90 {
        format!("{seconds}s")
    } else {
        format!("{} min", (seconds + 30) / 60)
    }
}

/// One alert event on its way to every channel the profile has.
///
/// **Ordered by what must not be lost.** The incident is recorded first, so a
/// panel that refetches its history when the event arrives finds it there; the
/// in-app event goes second; the OS toast third; the webhook last, because it is
/// the only step that can block for seconds and it must not delay the three
/// that cannot.
///
/// Every step is best-effort and none of them can cost the loop. A webhook that
/// does not answer is a thing to notice in the log, not a reason for the alert
/// to stop existing — it is already recorded, already on screen and already on
/// the desktop.
fn raise(
    app: &AppHandle,
    store: &AlertStore,
    profile_id: &str,
    channels: &AlertChannels,
    event: &AlertEvent,
) {
    if let Err(e) = store.record_event(profile_id, event) {
        tracing::warn!("recording alert {} for {profile_id}: {e}", event.rule_id);
    }
    if let Err(e) = app.emit(&alerts_event(profile_id), event.clone()) {
        tracing::warn!("emitting alert {} for {profile_id}: {e}", event.rule_id);
    }
    if channels.os_notification {
        notify(app, event);
    }
    // The URL never reaches this line: the core's message names the host and
    // nothing else, deliberately — a Slack incoming webhook is a bearer
    // credential in path form, and a log line ends up in a screenshot.
    if let Some(refused) = alerts::deliver(channels, event) {
        tracing::warn!("alert webhook for {profile_id}: {refused}");
    }
}

/// The desktop notification for one event.
///
/// Title and body rather than one sentence, because that is the native shape —
/// and the rule's own name goes in the title because it is what the user wrote
/// and what they will recognise at 3am (docs/DESIGN.md §7). A resolve says so in
/// the title for the same reason: the two have to be tellable apart from a
/// glance at a corner of the screen.
///
/// A refusal is logged and not retried. The OS is entitled to say no — the
/// notification centre is off, permission was withdrawn, the session is not
/// interactive — and none of that is a reason to lose an alert that is already
/// recorded and already at the webhook.
fn notify(app: &AppHandle, event: &AlertEvent) {
    let title = if event.is_resolved() {
        format!("{} cleared", event.rule_name)
    } else {
        event.rule_name.clone()
    };
    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body(event.detail.clone())
        .show()
    {
        tracing::warn!("OS notification for alert {}: {e}", event.rule_id);
    }
}

impl AppState {
    /// One profile's compiled masking rules, from the cache or freshly built.
    ///
    /// **Every path that emits a record calls this, and an error here is fatal
    /// to that path on purpose.** A `masking.json` this build cannot read is
    /// the one failure where carrying on is worse than stopping: the rules a
    /// user believes are hiding a customer's card number are exactly the ones
    /// that would silently not be applied, and they would find out from the
    /// screenshot. Fail closed — the message names the file.
    ///
    /// A rule stored on disk that will not COMPILE is a different case and is
    /// not fatal: the core skips it, names it, and this logs the sentence once
    /// per compile rather than once per record.
    fn mask_set(&self, profile_id: &str) -> kavka_core::Result<Arc<MaskSet>> {
        if let Some(cached) = self.mask_sets.lock().unwrap().get(profile_id) {
            return Ok(Arc::clone(cached));
        }
        let (set, refused) = self.masks.mask_set(profile_id)?;
        for problem in refused {
            tracing::warn!(profile = profile_id, "{problem}");
        }
        let set = Arc::new(set);
        self.mask_sets
            .lock()
            .unwrap()
            .insert(profile_id.to_string(), Arc::clone(&set));
        Ok(set)
    }

    /// Drops one profile's compiled rules, so the next record-emitting call
    /// recompiles them. Called by every masking command that changes anything.
    fn forget_mask_set(&self, profile_id: &str) {
        self.mask_sets.lock().unwrap().remove(profile_id);
    }

    fn connection(&self, profile_id: &str) -> CmdResult<Arc<ClusterConnection>> {
        self.connections
            .lock()
            .unwrap()
            .get(profile_id)
            .cloned()
            .ok_or_else(|| format!("not connected: {profile_id}"))
    }

    /// Unique for the life of the process, which is exactly the life of the
    /// event names it addresses — a session id never leaves this machine or
    /// outlives the app, so a counter plus the start time answers "keep two
    /// browsers of the same topic apart" without a uuid dependency. One counter
    /// serves every kind: they share a namespace, so a mixed-up id is a miss
    /// rather than a collision.
    fn next_id(&self) -> String {
        let seq = self.id_seq.fetch_add(1, Ordering::Relaxed);
        format!("{:x}-{seq:x}", self.id_epoch)
    }

    /// Stops everything one profile has running — every tail, search and bulk
    /// run, plus its in-flight fetch — and hands the sessions back so the
    /// caller can destroy them off the event loop.
    ///
    /// A session that outlives its cluster is a client reading (or writing) for
    /// a view that can never be opened again; the fetch is cancelled for the
    /// same reason, and it is holding a blocking-pool slot besides.
    fn take_sessions_of(&self, profile_id: &str) -> ProfileSessions {
        self.cancel_fetch(profile_id);
        // The monitor goes with them. It is not carried back for an off-loop
        // drop like the others because dropping this handle joins nothing: the
        // loop runs on a detached thread that holds its own reference, and
        // `stop` is what ends it.
        drop(self.take_monitor(profile_id));
        ProfileSessions {
            tails: self.tails.take_of(profile_id),
            searches: self.searches.take_of(profile_id),
            bulks: self.bulks.take_of(profile_id),
            sqls: self.sqls.take_of(profile_id),
            copies: self.copies.take_of(profile_id),
        }
    }

    /// This profile's monitor, if it is connected.
    fn monitor(&self, profile_id: &str) -> Option<Arc<Monitor>> {
        self.monitors.lock().unwrap().get(profile_id).cloned()
    }

    /// Takes one profile's monitor off the books and asks it to finish.
    fn take_monitor(&self, profile_id: &str) -> Option<Arc<Monitor>> {
        let monitor = self.monitors.lock().unwrap().remove(profile_id)?;
        monitor.stop();
        Some(monitor)
    }

    /// Puts a freshly built monitor on the books and starts its loop.
    ///
    /// A reconnect replaces whatever was there and stops it first: the old loop
    /// is sampling over a connection authenticated with the profile as it was
    /// before the edit, and two loops writing one history file would double
    /// every series.
    fn start_monitor(
        &self,
        app: &AppHandle,
        monitor: &Arc<Monitor>,
        conn: &Arc<ClusterConnection>,
    ) {
        let replaced = self
            .monitors
            .lock()
            .unwrap()
            .insert(monitor.profile_id.clone(), Arc::clone(monitor));
        if let Some(previous) = replaced {
            previous.stop();
        }
        monitor.began();
        let started = spawn_emitter("kavka-monitor", {
            let app = app.clone();
            let monitor = Arc::clone(monitor);
            let conn = Arc::clone(conn);
            let histories = Arc::clone(&self.histories);
            let alerts = Arc::clone(&self.alerts);
            move || run_monitor(app, monitor, conn, histories, alerts)
        });
        if let Err(e) = started {
            // The connection is fine and the cluster screens all work; the only
            // thing missing is the history, and `sampler_status` is where a
            // screen goes to find that out. Nothing to close out here: the loop
            // never ran, so no rule of this profile's has fired yet and
            // `close_open_incidents` would have nothing to close.
            monitor.ended();
            monitor.record(
                None,
                Some(format!(
                    "Kavka couldn't start the sampler for this connection: {e}. Disconnect and \
                     connect again to retry."
                )),
            );
        }
    }

    /// Opens a session's subscribe handshake. Called **before** the id is
    /// handed to the UI, so a `session_ready` that arrives while the emitter is
    /// still starting has something to set.
    fn arm_ready(&self, id: &str) -> Arc<ReadyGate> {
        let gate = Arc::new(ReadyGate::default());
        self.ready
            .lock()
            .unwrap()
            .insert(id.to_string(), Arc::clone(&gate));
        gate
    }

    /// The UI is listening. An id with no gate is not an error: the session has
    /// already started emitting (or is over), and this call is exactly as
    /// harmless as it looks.
    fn open_ready(&self, id: &str) {
        let gate = self.ready.lock().unwrap().get(id).map(Arc::clone);
        if let Some(gate) = gate {
            gate.open();
        }
    }

    /// Retires a handshake once its emitter has passed the gate — including the
    /// spawn-failure path, where nothing will ever pass it.
    fn disarm_ready(&self, id: &str) {
        self.ready.lock().unwrap().remove(id);
    }

    /// Registers a fetch for one profile and cancels whatever it replaces:
    /// one browse per connection is all the UI can show.
    fn begin_fetch(&self, profile_id: &str) -> CancelToken {
        let token = CancelToken::new();
        let replaced = self
            .fetches
            .lock()
            .unwrap()
            .insert(profile_id.to_string(), token.clone());
        if let Some(previous) = replaced {
            previous.cancel();
        }
        token
    }

    /// Retires a finished fetch — unless a newer one has already taken the
    /// slot, in which case that newer token must survive.
    fn end_fetch(&self, profile_id: &str, token: &CancelToken) {
        let mut fetches = self.fetches.lock().unwrap();
        if fetches
            .get(profile_id)
            .is_some_and(|current| current.is_same(token))
        {
            fetches.remove(profile_id);
        }
    }

    /// Cancels the in-flight fetch of one profile. Disconnecting or deleting a
    /// connection makes its answer worthless too, and the fetch is holding a
    /// consumer on a cluster the user is walking away from.
    fn cancel_fetch(&self, profile_id: &str) {
        if let Some(token) = self.fetches.lock().unwrap().remove(profile_id) {
            token.cancel();
        }
    }

    /// This profile's protocol slot, created empty on first use. The socket
    /// itself is opened later, inside the slot's own lock and on the blocking
    /// pool — see [`protocol_call`].
    fn protocol_slot(&self, profile_id: &str) -> ProtocolSlot {
        Arc::clone(
            self.protocol
                .lock()
                .unwrap()
                .entry(profile_id.to_string())
                .or_default(),
        )
    }

    /// Takes one profile's protocol slot off the books.
    ///
    /// Called on disconnect, on delete, and on RECONNECT: the cached socket is
    /// authenticated with the credentials the profile had when it was opened,
    /// so a profile that has just been edited and reconnected must not keep
    /// answering over the old one.
    fn take_protocol(&self, profile_id: &str) -> Option<ProtocolSlot> {
        self.protocol.lock().unwrap().remove(profile_id)
    }

    /// Asks every live session of every kind to finish, on the way out.
    fn stop_all_sessions(&self) {
        self.tails.stop_all();
        self.searches.stop_all();
        self.bulks.stop_all();
        self.sqls.stop_all();
        self.copies.stop_all();
        // The monitors too: each is a thread holding a librdkafka client and a
        // redb write handle, and each is woken by this rather than joined —
        // quitting must never wait on a broker.
        for monitor in self.monitors.lock().unwrap().values() {
            monitor.stop();
        }
    }
}

type CmdResult<T> = std::result::Result<T, String>;

async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> kavka_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// The connection for `profile_id`, opening one if the user has not.
///
/// **Only for the FAR END of a cross-cluster command** — the destination of a
/// copy or an offset migration, side B of a config diff. Everything addressed
/// to a single cluster still goes through [`AppState::connection`] and still
/// refuses when that cluster is not open: a workspace command is asked from a
/// screen the user opened, and "not connected" is the truth there.
///
/// A cross-cluster command is different. The destination is named in the
/// request, chosen from the full profile list, and is very often a cluster this
/// window has never opened — refusing it would mean "connect to prod first,
/// then come back and copy into it", which is more clicks *and* leaves a prod
/// workspace sitting open afterwards.
///
/// **Lifetime, which is the whole design.**
///
/// - A connection the user already has open is **borrowed and never closed
///   here**. The map keeps its own reference, so the clone this returns is one
///   more `Arc`, and dropping it does nothing.
/// - A connection opened here is **not put on the books**. It lives exactly as
///   long as the command that needed it: every caller moves it straight into a
///   `blocking` closure, so the last reference — and the librdkafka client
///   teardown — goes with that closure, on the blocking pool.
///
/// Not caching it is deliberate. `connections` is the set of clusters the UI
/// says are open, and a hidden entry in it is a client Kavka holds against a
/// cluster whose status bar reads "not connected" and whose disconnect button
/// does not exist. In an app whose sixth guardrail layer is *prod is visible
/// wherever data is*, an invisible connection to prod is the wrong side of the
/// trade — a copy pays one connect for it, once, and a `CopySession` owns its
/// own producer from that point on (see `xcluster::CopySession::start`), so
/// nothing outlives the command that needs the connection anyway.
async fn connection_or_open(
    state: &AppState,
    profile_id: &str,
) -> CmdResult<Arc<ClusterConnection>> {
    // Bound to a local rather than tested inline: the guard must be gone before
    // the `await` below, and a `MutexGuard` living across one is how an async
    // command stops being `Send`.
    let open = state.connections.lock().unwrap().get(profile_id).cloned();
    if let Some(open) = open {
        return Ok(open);
    }
    let store = state.store.clone();
    let id = profile_id.to_string();
    blocking(move || {
        let profile = store
            .list()?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| {
                kavka_core::Error::Other(format!(
                    "Kavka has no saved connection with the id {id}. If it was deleted while \
                     this screen was open, pick the cluster again."
                ))
            })?;
        ClusterConnection::connect(profile).map(Arc::new)
    })
    .await
}

/// Runs one wire-protocol call on this profile's kept-alive connection,
/// opening it if there isn't one and dropping it if the transport breaks.
///
/// **What it saves.** Every protocol call used to be a TCP connect, a TLS
/// handshake, an ApiVersions exchange and a full SASL handshake — for SCRAM,
/// four round trips plus a PBKDF2 derivation on the calling thread. The
/// reassignment monitor polls `reassign_list` every two seconds, so that was a
/// login every two seconds for as long as a move was on screen.
///
/// **Lifetimes.** Everything the closure touches is owned by the blocking task:
/// the `Arc` slot is cloned out of the map before this returns to the caller,
/// the `MutexGuard` is taken and released entirely inside `spawn_blocking` (it
/// is not `Send`, and holding a Kafka round trip on the event loop would be
/// worse than if it were), and the `&mut ProtocolClient` handed to `f` borrows
/// from that guard. Nothing borrowed escapes, which is why `f` must return an
/// OWNED `T` — the value crosses back to the async side after the guard is
/// gone.
///
/// **Invalidation.** A failure that leaves the socket's framing untrustworthy
/// (a write that didn't go out, a reply that didn't come back, a correlation id
/// that didn't match) clears [`ProtocolClient::is_healthy`], and this drops the
/// client so the next call dials again. That is also the path a broker-closed
/// connection takes — an idle timeout, or `connections.max.reauth.ms` expiring
/// an OAUTHBEARER token — so the cache heals itself rather than needing the
/// user to reconnect. A Kafka-level refusal (read-only, an invalid replica set)
/// leaves a perfectly good connection in place.
async fn protocol_call<T, F>(state: &AppState, profile_id: &str, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut ProtocolClient) -> kavka_core::Result<T> + Send + 'static,
{
    // The same gate as every other command: an unconnected profile is refused
    // before anything is opened, and the `ClusterConnection` is what carries
    // the profile, its credentials and its read-only flag into the core.
    let conn = state.connection(profile_id)?;
    let slot = state.protocol_slot(profile_id);
    blocking(move || {
        // Poison-tolerant: what this guards is a socket, which a panic on
        // another command cannot corrupt — and a poisoned lock would turn one
        // failed call into a permanently dead connection.
        let mut client = slot.lock().unwrap_or_else(|e| e.into_inner());
        if client.is_none() {
            *client = Some(ProtocolClient::for_connection(&conn)?);
        }
        let live = client.as_mut().expect("just opened");
        let outcome = f(live);
        if !live.is_healthy() {
            *client = None;
        }
        outcome
    })
    .await
}

/// [`protocol_call`] for a MUTATION, with the read-only check in front of it.
///
/// The check is the core's own (`ClusterConnection::ensure_writable`) and it
/// runs before the slot is even looked at, which keeps docs/ARCHITECTURE.md
/// D5's stronger promise intact now that connections are cached: a read-only
/// connection must not so much as authenticate on behalf of a write. The core
/// makes the same judgement again inside `ProtocolClient`, so holding a client
/// cannot route around it either.
async fn protocol_write<T, F>(
    state: &AppState,
    profile_id: &str,
    op: &'static str,
    f: F,
) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut ProtocolClient) -> kavka_core::Result<T> + Send + 'static,
{
    state
        .connection(profile_id)?
        .ensure_writable(op)
        .map_err(|e| e.to_string())?;
    protocol_call(state, profile_id, f).await
}

/// Destroys a stopped session off the event loop. Dropping the last reference
/// joins the core's worker thread, which is a librdkafka client teardown — and
/// for a bulk run it is also the flush that makes the final counts true.
async fn retire<T: Stoppable>(session: Arc<T>) -> CmdResult<()> {
    session.stop();
    tauri::async_runtime::spawn_blocking(move || drop(session))
        .await
        .map_err(|e| e.to_string())
}

/// Spawns a session's emitter thread, detached.
///
/// Detached because these threads live as long as their session does, and
/// `stop` unblocks every one of them — so quitting never waits on a broker. A
/// dedicated OS thread rather than the blocking pool for the same reason: a
/// pool slot held for the length of a tail (or a search over a large topic) is
/// a slot every keychain read and metadata fetch queues behind.
fn spawn_emitter(name: &str, body: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .map(|_detached| ())
}

/// The event name a session's batches are addressed to. Batches go to every
/// window, so the id in the name is what keeps two tails apart.
fn tail_event(tail_id: &str) -> String {
    format!("kavka://tail/{tail_id}")
}

/// One IPC payload from a live tail.
///
/// `ended` is absent on a normal batch and `true` exactly once, when the
/// session dies — the UI has to be able to tell "quiet topic" from "gone", and
/// a second event name for one boolean is worse than a flag.
#[derive(Clone, Serialize)]
struct TailBatch {
    records: Vec<MessageRecord>,
    /// Cumulative since the session started, not per batch.
    dropped: u64,
    #[serde(skip_serializing_if = "is_false")]
    ended: bool,
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

/// One live tail's reader loop. Runs on its own thread — see [`spawn_emitter`].
fn pump_tail(app: &AppHandle, tail_id: &str, profile_id: &str, session: &TailSession) {
    let event = tail_event(tail_id);
    let mut reported_drops = 0;
    // `None` is the end of the session; `Some(empty)` is a quiet topic, which
    // is a state, not an ending.
    while let Some(mut records) = session.next_batch(TAIL_POLL) {
        let dropped = session.dropped();
        // A quiet topic costs nothing: the view says "listening" from its own
        // timer, and 2 empty events a second per open tail is pure IPC. The
        // masking pass is below this rather than above it for the same reason —
        // a quiet topic must stay free.
        if records.is_empty() && dropped == reported_drops {
            continue;
        }
        reported_drops = dropped;
        // Read per batch rather than captured once, so a rule the user switches
        // on while a tail is running masks the records that arrive after it —
        // which is the whole point of a switch. It is a cache hit and an `Arc`
        // clone; the regexes are compiled once (`AppState::mask_set`).
        match mask_session(app, profile_id) {
            Some(rules) => {
                mask_batch(&rules, &mut records);
            }
            // Fail closed: the rules could not be read, so nothing goes out.
            None => {
                session.stop();
                break;
            }
        }
        if let Err(e) = app.emit(
            &event,
            TailBatch {
                records,
                dropped,
                ended: false,
            },
        ) {
            // Nobody can receive this session any more; don't keep fetching it.
            tracing::warn!("live tail {tail_id}: {e}");
            session.stop();
            break;
        }
    }

    // Forget it first, so a `tail_stop` racing the last payload is the no-op it
    // claims to be, then say so once. The drop counter is read before the
    // handle goes anywhere: it is still meaningful on the final payload.
    if let Some(state) = app.try_state::<AppState>() {
        state.tails.forget(tail_id);
    }
    report_session_serde_errors(app, profile_id);
    let _ = app.emit(
        &event,
        TailBatch {
            records: Vec::new(),
            dropped: session.dropped(),
            ended: true,
        },
    );
}

// ── Display masking ────────────────────────────────────────────────────────
//
// EVERY RECORD THE WEBVIEW EVER SEES PASSES THROUGH ONE OF THE FUNCTIONS
// BELOW. That is the whole guarantee: masking is applied HERE, on the shell
// side of the IPC boundary, so while a rule is on the raw text is not in the
// webview at all — not in a devtools console, not in a copied cell, not in a
// screen share, not in an export. A masking pass done in the UI would leave the
// real value one inspector click away, which is a costume rather than a mask
// (see `kavka_core::masking`'s module docs, which own this argument).
//
// The four record-emitting paths, and the one place each is masked:
//
//   messages_fetch   the returned Vec, before the command resolves
//   tail             `pump_tail`,   each batch, before `app.emit`
//   search           `pump_search`, each batch, before `app.emit`
//   sql              `pump_sql`,    each row batch, before `app.emit`
//
// Cross-cluster copy is deliberately NOT in that list: a copy moves raw bytes
// from one broker to another and shows the user nothing, so masking it would
// corrupt the destination topic while redacting nothing. `copy_dry_run` returns
// watermark arithmetic and no payloads at all. Produce, bulk produce and the
// admin surfaces carry no records either.
//
// Exports and clipboard copies need no pass of their own, and that is the point
// of doing it here: the records the UI hands back to `export_records` are the
// masked ones it was given. What the export DOES add is the notice — see
// `mask_notice_for`.

/// One profile's compiled rules, for an emitter thread that only has an
/// [`AppHandle`].
///
/// `None` is "do not emit": either the state is gone (the app is shutting down)
/// or `masking.json` could not be read. Both are cases where sending records on
/// would mean sending them **unmasked**, which is the one outcome this feature
/// exists to prevent — so every caller stops the session instead.
fn mask_session(app: &AppHandle, profile_id: &str) -> Option<Arc<MaskSet>> {
    let state = app.try_state::<AppState>()?;
    match state.mask_set(profile_id) {
        Ok(rules) => Some(rules),
        Err(e) => {
            tracing::error!(profile = profile_id, "masking rules unreadable: {e}");
            None
        }
    }
}

/// Takes and logs the first RUNTIME failure each of a connection's WASM decoder
/// plugins hit — a trap, an exhausted fuel budget, an answer that was not an
/// ABI v1 result block.
///
/// Once per session, not once per record: the core hands each error out exactly
/// once (`take_serde_errors`), on the same rule the Schema Registry's own
/// errors follow, because a plugin that fails on every record of a
/// 400,000-record scan must not write 400,000 lines.
///
/// A plugin that would not LOAD is the other half and is reported elsewhere —
/// at `cluster_connect`, where it happens, once.
///
/// Neither is an error the user's read failed on: a plugin that declines or
/// crashes falls through to the built-in ladder, which is why the read still
/// returns records and why this has to be said out loud somewhere.
fn report_serde_errors(conn: &ClusterConnection) {
    for error in conn.take_serde_errors() {
        tracing::warn!("{error}; those records fell through to the built-in decoders");
    }
}

/// [`report_serde_errors`] for an emitter thread, which only has an
/// [`AppHandle`]. Silent when the cluster has since been disconnected.
fn report_session_serde_errors(app: &AppHandle, profile_id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Ok(conn) = state.connection(profile_id) {
        report_serde_errors(&conn);
    }
}

/// The key a masked export's notice rides under, in both JSON shapes.
///
/// Leading underscore because it sits in the same object position as a record's
/// own fields and is not one of them; the name is stable because a reader may
/// legitimately filter on it.
const MASK_NOTICE_KEY: &str = "_kavka_notice";

/// The sentence a masked export or a masked copy carries.
///
/// **Count-free, and that is a gap rather than a choice.**
/// `kavka_core::masking::mask_notice` writes the better sentence — "3 masking
/// rules applied" — but the number is a property of the SESSION and the only
/// thing that survives to the webview and back is `MessageRecord::masked`, a
/// boolean. Inventing a count would be worse than omitting one, so this says
/// everything except the number, keeps `MASK_NOTICE_MARKER` so a file can still
/// be searched for it, and is the string to replace the day the export commands
/// are given the rule count (see the note on `export_rows`).
fn mask_notice_text() -> String {
    format!("{MASK_NOTICE_MARKER} — some values here are not the values on the topic.")
}

/// The notice for a batch of records, or `None` when none of them was masked.
///
/// Driven by `MessageRecord::masked`, which the core sets and never clears —
/// so an export of a selection where ONE record was rewritten says so, which is
/// the right way round: the question a reader has is "can I trust this file",
/// and the answer is no as soon as any of it is redacted.
fn mask_notice_for(records: &[MessageRecord]) -> Option<String> {
    records
        .iter()
        .any(|record| record.masked)
        .then(mask_notice_text)
}

/// Masks a batch in place, and answers whether anything changed.
///
/// A no-op — one boolean per batch — when the profile has no enabled rules,
/// which is nearly every session.
fn mask_batch(rules: &MaskSet, records: &mut [MessageRecord]) -> bool {
    if rules.is_empty() {
        return false;
    }
    let mut masked = false;
    for record in records.iter_mut() {
        masked |= rules.mask_record(record);
    }
    masked
}

/// Masks a batch of SQL result rows in place, by column name.
///
/// The policy — which column is masked as which part of a record, and what
/// happens to a computed one — is `kavka_core::masking::mask_sql_rows`, and it
/// lives there because the MCP server runs the same pass over the same rows for
/// an agent. This is the adapter that turns the shell's `SqlColumn`s into the
/// names that function reads; two copies of the reasoning would be two answers
/// to one question.
fn mask_rows(rules: &MaskSet, columns: &[SqlColumn], rows: &mut [Vec<serde_json::Value>]) -> bool {
    let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    kavka_core::masking::mask_sql_rows(rules, &names, rows)
}

#[tauri::command]
fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── MCP server ─────────────────────────────────────────────────────────────

/// Everything the About/Settings surface needs to wire Kavka's MCP server
/// (`crates/kavka-mcp`) into Claude Code or Cursor: where the binary is, and a
/// snippet for each that can be pasted without editing.
#[derive(Serialize)]
struct McpInfo {
    binary_path: String,
    snippet_claude: String,
    snippet_cursor: String,
}

/// The MCP server binary, and the two configuration snippets for it.
///
/// **Where the binary is, in both worlds.** It is always a SIBLING of the
/// running executable, which is one rule rather than two:
///
/// - *Development* — `cargo tauri dev` runs `target/debug/kavka-desktop`, and
///   `cargo build -p kavka-mcp` puts `kavka-mcp` in that same directory. Build
///   it first: this command reports where the binary belongs whether or not it
///   is there yet, because a path somebody can act on beats an empty panel.
/// - *Packaged* — the server ships beside the app binary: next to `Kavka.exe`
///   on Windows, inside `Kavka.app/Contents/MacOS/` on macOS. That is where
///   Tauri's bundler places a sidecar (`bundle.externalBin`), and wiring it is
///   the packaging step this path is written against — **it is not wired yet**,
///   because `externalBin` fails the build outright when the named
///   per-target-triple binary is absent, which would break the release workflow
///   the day it lands rather than the day the sidecar is built. Until then a
///   packaged build reports the path the sidecar will have.
///
/// The snippets are deliberately the READ-ONLY configuration. Enabling the two
/// write tools is a decision with a blast radius, so it is a line the person
/// adds themselves — the Claude snippet shows exactly which one, and
/// `crates/kavka-mcp/src/gate.rs` is where the rules live.
#[tauri::command]
fn mcp_info() -> McpInfo {
    let binary = mcp_binary_path();
    let (snippet_claude, snippet_cursor) = mcp_snippets(&binary);
    McpInfo {
        binary_path: binary,
        snippet_claude,
        snippet_cursor,
    }
}

fn mcp_binary_path() -> String {
    let name = format!("kavka-mcp{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .map_or(name, |path| path.display().to_string())
}

/// The two snippets, as pure string work so they can be checked without a
/// window. Split out from the command for exactly that reason.
fn mcp_snippets(binary: &str) -> (String, String) {
    let claude = format!(
        "claude mcp add kavka -- \"{binary}\"\n\
         \n\
         # Read-only by default. To also allow the two write tools (produce a\n\
         # record, reset a group's offsets) — production connections still\n\
         # refuse without KAVKA_MCP_ALLOW_PROD=1, and connections marked\n\
         # read-only always refuse:\n\
         claude mcp add kavka -e KAVKA_MCP_ALLOW_WRITES=1 -- \"{binary}\"\n"
    );
    // Built with serde_json rather than by hand: a Windows path is full of
    // backslashes, and a snippet that has to be escaped by the person pasting
    // it is not a snippet.
    let cursor = serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "kavka": {
                "command": binary,
                "args": [],
                "env": {},
            }
        }
    }))
    .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
    (claude, cursor)
}

// ── The local playground — "try Kavka without a cluster" ───────────────────
//
// KAVKA DOES NOT BUNDLE A BROKER, and this feature is built around saying so.
// Apache Kafka is a JVM application: shipping one inside the app would mean
// shipping a JRE, which is the single thing the product's positioning is
// against (README — "no-Docker, no-JVM desktop client"). What Kavka *can* do
// is drive a container runtime the user already has. So the whole feature is
// three commands over `docker compose`, and when Docker is absent the UI gets
// one honest sentence instead of a button that cannot work.
//
// NOTHING HERE CAN TOUCH A SAVED CONNECTION. Look at the signatures: not one
// of these commands takes a `profile_id`, and none of them opens a
// `ClusterConnection`. That is what makes them safe to offer on a machine with
// production connections in the sidebar — not a check somebody has to remember
// to write, but an API with nothing to point at a cluster with. `read_only` is
// irrelevant for the same reason: that flag is about what Kavka sends to *your*
// brokers, and the playground is not one of them.
//
// The one thing they do write is a NEW dev profile called "Playground", and
// they refuse to touch one that is not still `dev` — see `save_playground`.

/// Pinned so `up`, `ps` and `down` are always talking about the same thing.
///
/// Without `-p`, Compose derives the project name from the compose file's
/// PARENT DIRECTORY, which is `playground` beside a dev build's binary and
/// `Resources` inside an installed `.app`. `down` would then look for a
/// project `up` never created, and the user would be left with a container the
/// app claims it stopped.
const PLAYGROUND_PROJECT: &str = "kavka-playground";

/// Where the compose file sits inside the bundle — see `tauri.conf.json`'s
/// `bundle.resources`, and `playground/docker-compose.yml` for why it is a
/// second file rather than `dev/docker-compose.yml`.
const PLAYGROUND_COMPOSE: &str = "playground/docker-compose.yml";

/// The playground's address. **Not 9092**, and that is deliberate: 9092 is the
/// port whatever Kafka the user already runs is on, and "port is already
/// allocated" is a first run that reads as Kavka being broken. See the header
/// of `playground/docker-compose.yml`.
const PLAYGROUND_BOOTSTRAP: &str = "localhost:19092";

/// A fixed id, so starting the playground twice finds the connection it made
/// last time instead of filling the sidebar with copies of it.
const PLAYGROUND_PROFILE_ID: &str = "kavka-playground";

/// The 3-second ceiling the brief asks for, and the right one: this runs on
/// first paint, and a probe that can hang is a first-run empty state that can
/// hang. Docker answers `info` in well under a second when it is up, and when
/// it is not, three seconds is already longer than anybody will wait.
const DOCKER_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// The ceiling on `compose up --wait`. Generous because the first run pulls
/// roughly 400 MB of Kafka image over whatever connection the user has, and a
/// timeout that fires mid-pull would leave a half-downloaded layer and a
/// message blaming Kavka.
const PLAYGROUND_START_TIMEOUT: Duration = Duration::from_secs(900);

/// `down` has nothing to download and should be quick; if it is not, the user
/// needs to hear that rather than watch a button spin.
const PLAYGROUND_STOP_TIMEOUT: Duration = Duration::from_secs(120);

/// How often a bounded run looks at its child.
const CHILD_POLL: Duration = Duration::from_millis(120);

/// How often a bounded run is allowed to tell the UI where it is. Same trade as
/// [`PROGRESS_EVERY`], one order of magnitude slower: nothing here changes
/// faster than a Docker layer.
const CHILD_TICK: Duration = Duration::from_secs(1);

/// How many output lines a bounded run keeps. Compose can print a hundred lines
/// of pull progress and the one that explains a failure is always at the end.
const OUTPUT_KEEP: usize = 64;

fn sandbox_step_event() -> &'static str {
    "kavka://sandbox/step"
}

/// What Kavka found when it looked for Docker.
///
/// Four states rather than a bool because each one has a different sentence and
/// a different next click, and "Docker isn't working" is exactly the kind of
/// message docs/DESIGN.md §7 exists to prevent. Serialises as a plain string
/// (`"absent"`, `"stopped"`, …): a unit enum carries no data, so it is not
/// `{ "kind": … }`-tagged like the payload enums in the IPC contract.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DockerState {
    /// No `docker` on PATH at all.
    Absent,
    /// The CLI is there; the daemon did not answer.
    Stopped,
    /// The daemon answered, but it is **somebody else's machine** — a `tcp://`
    /// or `ssh://` context, or a `DOCKER_HOST` pointing off-box. See
    /// [`resolve_endpoint`].
    Remote,
    /// The daemon answered, but `docker compose` is not a thing on this
    /// machine — an old Docker with the standalone `docker-compose` script.
    NoCompose,
    /// Everything the playground needs.
    Ready,
}

#[derive(Serialize)]
struct SandboxStatus {
    docker: DockerState,
    /// Whether the playground's own containers are up, as Compose reports
    /// them. Always `false` when `docker` is anything but `Ready` — there is
    /// nothing to ask.
    running: bool,
    /// The bundled compose file, or `None` when this build has no such
    /// resource. A separate signal from Docker's state because it is a
    /// different failure with a different owner: that one is the user's
    /// machine, this one is Kavka's packaging.
    compose_file: Option<String>,
    /// `localhost:19092`, mirrored so the UI never hard-codes it.
    bootstrap: String,
    /// The Playground connection's id, if it is already on this machine.
    profile_id: Option<String>,
    /// What Docker actually said, when it said something Kavka does not
    /// recognise. The `Show details ▾` half of docs/DESIGN.md §7's error
    /// doctrine — verbatim, never the title.
    ///
    /// The one state where it is not a broker-style reply is
    /// [`DockerState::Remote`], where it is **the endpoint itself** — the UI
    /// puts that in the sentence rather than behind a disclosure, because it
    /// is the only part of that refusal anybody can act on.
    detail: Option<String>,
}

/// One line of the streaming checklist (docs/DESIGN.md §5.4).
///
/// The label rides along on every emission so the UI can upsert by `id` and
/// never has to hold a copy of Kavka's own wording.
#[derive(Serialize, Clone)]
struct SandboxStep {
    id: &'static str,
    label: &'static str,
    /// `running` · `ok` · `fail` · `skipped`.
    state: &'static str,
    note: Option<String>,
}

/// The emitter for one run of the ladder.
struct Ladder<'a> {
    app: &'a AppHandle,
}

impl Ladder<'_> {
    fn emit(
        &self,
        id: &'static str,
        label: &'static str,
        state: &'static str,
        note: Option<String>,
    ) {
        let _ = self.app.emit(
            sandbox_step_event(),
            SandboxStep {
                id,
                label,
                state,
                note,
            },
        );
    }

    fn running(&self, id: &'static str, label: &'static str, note: Option<String>) {
        self.emit(id, label, "running", note);
    }

    fn ok(&self, id: &'static str, label: &'static str, note: Option<String>) {
        self.emit(id, label, "ok", note);
    }

    fn fail(&self, id: &'static str, label: &'static str, note: String) {
        self.emit(id, label, "fail", Some(note));
    }

    fn skipped(&self, id: &'static str, label: &'static str, note: &str) {
        self.emit(id, label, "skipped", Some(note.to_string()));
    }
}

/// One finished — or abandoned — child process.
struct Ran {
    /// It started, exited, and exited zero.
    ok: bool,
    /// It could not be started at all. The difference between "Docker isn't
    /// installed" and "Docker said no", which is two different sentences.
    spawn_failed: bool,
    timed_out: bool,
    out: Vec<String>,
    err: Vec<String>,
}

impl Ran {
    /// The last few lines, for a note or a `Show details` block. Prefers
    /// stderr, which is where both Docker and Compose put everything that
    /// explains a failure — and where Compose puts its progress, too.
    fn tail(&self, lines: usize) -> String {
        let from = if self.err.is_empty() {
            &self.out
        } else {
            &self.err
        };
        let start = from.len().saturating_sub(lines);
        from[start..].join("\n")
    }
}

/// Runs a program with a hard ceiling, capturing its output and reporting
/// progress while it works.
///
/// **Why not `Command::output()`.** That blocks until the child exits, with no
/// ceiling and nothing to show: a `docker info` against a daemon that is
/// starting up can sit there for a minute, and `compose up` legitimately takes
/// several. This polls `try_wait`, kills on the timeout, and hands the caller
/// the newest output line once a second so the ladder can say what is
/// happening.
///
/// **Why the reader threads.** With `Stdio::piped()` and nobody reading, a
/// child that prints more than the pipe buffer holds blocks forever — which is
/// precisely `compose up` pulling an image. So each stream gets a thread that
/// drains it into a bounded buffer, and the loop only ever looks at what they
/// have collected.
///
/// Called only from the blocking pool.
fn run_bounded(
    program: &str,
    args: &[&str],
    timeout: Duration,
    mut on_tick: impl FnMut(Option<&str>, Duration),
) -> Ran {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_console(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Ran {
                ok: false,
                spawn_failed: true,
                timed_out: false,
                out: Vec::new(),
                err: vec![e.to_string()],
            }
        }
    };

    // The newest line from EITHER stream, so the tick can report Compose's
    // progress (stderr) and a plain command's answer (stdout) without the
    // caller knowing which one it is reading.
    let latest: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let out_lines = Arc::new(Mutex::new(Vec::new()));
    let err_lines = Arc::new(Mutex::new(Vec::new()));
    let pumps = [
        pump(child.stdout.take(), &out_lines, &latest),
        pump(child.stderr.take(), &err_lines, &latest),
    ];

    let started = Instant::now();
    let mut last_tick = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            // The handle is unusable; treat it as a failure rather than
            // spinning on a process we can no longer ask about.
            Err(_) => break None,
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        if last_tick.elapsed() >= CHILD_TICK {
            last_tick = Instant::now();
            // Cloned out from under the lock: `on_tick` emits an IPC event, and
            // holding the readers' mutex across that would stall them.
            let line = latest.lock().unwrap().clone();
            on_tick(line.as_deref(), started.elapsed());
        }
        std::thread::sleep(CHILD_POLL);
    };

    // Joined so the buffers are complete before they are read. Both threads see
    // EOF the moment the child exits — or is killed — so this does not wait.
    for pump in pumps.into_iter().flatten() {
        let _ = pump.join();
    }

    // Bound to locals so the two `MutexGuard` temporaries are dropped before
    // the `Arc`s they borrow from go out of scope at the end of the function.
    let out = std::mem::take(&mut *out_lines.lock().unwrap());
    let err = std::mem::take(&mut *err_lines.lock().unwrap());
    Ran {
        ok: status.is_some_and(|status| status.success()),
        spawn_failed: false,
        timed_out,
        out,
        err,
    }
}

/// Drains one of a child's streams into a bounded buffer.
fn pump<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
    into: &Arc<Mutex<Vec<String>>>,
    latest: &Arc<Mutex<Option<String>>>,
) -> Option<std::thread::JoinHandle<()>> {
    let stream = stream?;
    let into = Arc::clone(into);
    let latest = Arc::clone(latest);
    Some(std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            // A non-UTF-8 byte ends the read rather than the process: whatever
            // came before it is still the useful part.
            let Ok(line) = line else { break };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            *latest.lock().unwrap() = Some(line.clone());
            let mut lines = into.lock().unwrap();
            // `remove(0)` on a 64-element Vec, at most once per line of Docker
            // output: a VecDeque would be the same code with an import.
            if lines.len() >= OUTPUT_KEEP {
                lines.remove(0);
            }
            lines.push(line);
        }
    }))
}

/// Keeps a spawned process from flashing a console window on Windows.
///
/// `CREATE_NO_WINDOW`. Without it every probe — including the one that runs on
/// first paint — pops a black rectangle in front of the app and takes focus
/// with it.
#[cfg(windows)]
fn no_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn no_console(_command: &mut Command) {}

/// Where the Docker CLI is actually pointing, once `DOCKER_HOST` and the
/// active context have both had their say.
///
/// The remote arm carries the endpoint verbatim, because the whole point of
/// this check is to be able to name it: "Docker isn't local" is a sentence
/// nobody can act on, and `tcp://build-07.internal:2376` is one they can.
#[derive(Debug, PartialEq, Eq)]
enum Endpoint {
    /// A pipe or a socket on this machine — or nothing at all, which means
    /// the platform default, which is also on this machine.
    Local,
    /// Somebody else's daemon. The string is what Docker reported.
    Remote(String),
}

/// The two inputs Docker resolves an endpoint from, resolved the way Docker
/// resolves them — pure, so the policy can be tested without a daemon.
///
/// **`DOCKER_HOST` beats the context.** That is Docker's own precedence
/// (`docker context inspect` reports the *context's* endpoint even when the
/// environment has overridden it), and getting it backwards is exactly the
/// case this guard exists for: a developer with `DOCKER_HOST=tcp://…` exported
/// in their shell profile has a `default` context that still says
/// `npipe://…`.
///
/// **Local is a pipe, a socket, or nothing.** `npipe://` on Windows,
/// `unix://` everywhere else, and an unset/empty endpoint, which means the
/// platform default and is therefore this machine. Everything else —
/// `tcp://`, `ssh://`, `http(s)://`, `fd://` — is treated as remote. That
/// includes `tcp://localhost:2375`, deliberately: a TCP daemon may be a
/// tunnel to a build box, and this check refuses rather than guesses.
///
/// The one thing treated as "nothing" rather than as an endpoint is Go's
/// `<no value>`, which is what a `docker context inspect --format` prints
/// when the field is absent instead of failing. Refusing the playground over
/// that would be a guardrail firing on its own instrumentation.
fn resolve_endpoint(docker_host: Option<&str>, context_host: Option<&str>) -> Endpoint {
    /// What a Go template prints for a field that is not there.
    const NO_VALUE: &str = "<no value>";

    let stated = [docker_host, context_host]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty() && *value != NO_VALUE);
    let Some(endpoint) = stated else {
        return Endpoint::Local;
    };
    let scheme = endpoint.to_ascii_lowercase();
    if scheme.starts_with("npipe://") || scheme.starts_with("unix://") {
        return Endpoint::Local;
    }
    Endpoint::Remote(endpoint.to_string())
}

/// What the active context says its Docker endpoint is, if it says anything.
///
/// One line of stdout. A failure here is `None` rather than an error: an old
/// Docker without `context` support is a Docker with no context to be pointed
/// away by, and refusing to start a playground because a *query about* the
/// endpoint failed would be the wrong direction to fail in.
fn docker_context_host() -> Option<String> {
    let ran = run_bounded(
        "docker",
        &[
            "context",
            "inspect",
            "--format",
            "{{.Endpoints.docker.Host}}",
        ],
        DOCKER_PROBE_TIMEOUT,
        |_, _| {},
    );
    ran.ok.then(|| ran.out.first().cloned()).flatten()
}

/// Whether Docker is installed, running, **on this machine**, and has Compose.
fn probe_docker() -> (DockerState, Option<String>) {
    let info = run_bounded(
        "docker",
        &["info", "--format", "{{.ServerVersion}}"],
        DOCKER_PROBE_TIMEOUT,
        |_, _| {},
    );
    if info.spawn_failed {
        return (DockerState::Absent, None);
    }
    if info.timed_out {
        return (
            DockerState::Stopped,
            Some(format!(
                "`docker info` didn't answer within {}s.",
                DOCKER_PROBE_TIMEOUT.as_secs()
            )),
        );
    }
    if !info.ok {
        return (DockerState::Stopped, Some(info.tail(4)));
    }
    // BEFORE anything else the daemon could be asked, and before `Ready` can be
    // returned: a daemon that answered is not necessarily a daemon on this
    // desk. `docker compose up` against a `tcp://` context creates containers,
    // a volume and a published port on somebody else's host — silently, because
    // the CLI is identical either way — and the connection Kavka would then
    // save says `localhost:19092`, which is a port on the wrong machine.
    let from_env = std::env::var("DOCKER_HOST").ok();
    let from_context = docker_context_host();
    if let Endpoint::Remote(endpoint) =
        resolve_endpoint(from_env.as_deref(), from_context.as_deref())
    {
        return (DockerState::Remote, Some(endpoint));
    }
    let compose = run_bounded(
        "docker",
        &["compose", "version", "--short"],
        DOCKER_PROBE_TIMEOUT,
        |_, _| {},
    );
    if !compose.ok {
        return (DockerState::NoCompose, Some(compose.tail(4)));
    }
    (DockerState::Ready, None)
}

/// Whether the playground's containers are up right now.
fn playground_running(compose_file: &Path) -> bool {
    let file = compose_file.to_string_lossy().into_owned();
    let ran = run_bounded(
        "docker",
        &[
            "compose",
            "-p",
            PLAYGROUND_PROJECT,
            "-f",
            &file,
            "ps",
            "--status",
            "running",
            "--quiet",
        ],
        DOCKER_PROBE_TIMEOUT,
        |_, _| {},
    );
    // STDOUT only. Compose writes warnings to stderr, and a warning is not a
    // container: reading the merged output would report a running playground
    // to anybody whose Docker has something to complain about.
    ran.ok && !ran.out.is_empty()
}

/// The bundled compose file, or `None` when this build does not carry one.
///
/// Resolves through Tauri's resource directory, which is the app bundle's
/// `Resources` when installed and the cargo target directory in development —
/// `tauri-build` copies `bundle.resources` there on every build, so one lookup
/// covers `npm run tauri dev` and a signed installer.
fn playground_compose(app: &AppHandle) -> Option<PathBuf> {
    let path = app
        .path()
        .resolve(PLAYGROUND_COMPOSE, tauri::path::BaseDirectory::Resource)
        .ok()?;
    path.exists().then_some(path)
}

/// Where the playground command is, in Kafka's own vocabulary, so somebody who
/// would rather run it themselves can.
fn playground_cli_hint(compose_file: &Path) -> String {
    format!(
        "docker compose -p {PLAYGROUND_PROJECT} -f \"{}\" up -d --wait",
        compose_file.display()
    )
}

#[tauri::command]
async fn sandbox_status(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SandboxStatus> {
    let compose = playground_compose(&app);
    let store = Arc::clone(&state.store);
    // On the blocking pool like everything else that leaves the process: this
    // spawns up to three short-lived children.
    blocking(move || {
        let (docker, detail) = probe_docker();
        let running = match (&compose, docker) {
            (Some(file), DockerState::Ready) => playground_running(file),
            _ => false,
        };
        Ok(SandboxStatus {
            docker,
            running,
            compose_file: compose.map(|path| path.display().to_string()),
            bootstrap: PLAYGROUND_BOOTSTRAP.to_string(),
            // A profile store Kavka cannot read is already the subject of its
            // own banner (docs/DESIGN.md §7, "Profiles failed to load"); it
            // must not also take the playground panel down.
            profile_id: store.list().ok().and_then(|profiles| {
                profiles
                    .into_iter()
                    .find(|p| p.id == PLAYGROUND_PROFILE_ID)
                    .map(|p| p.id)
            }),
            detail,
        })
    })
    .await
}

/// Starts the playground and returns the id of the connection to open.
///
/// Streams a checklist on `kavka://sandbox/step` while it works. The UI
/// subscribes to a FIXED event name **before** it calls this, so — unlike the
/// session commands in this file — there is no id to hand out and therefore no
/// subscribe race to close: there is exactly one playground per machine.
#[tauri::command]
async fn sandbox_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let compose = playground_compose(&app);
    let store = Arc::clone(&state.store);
    let handle = app.clone();
    blocking(move || start_playground(&handle, &store, compose.as_deref())).await
}

fn start_playground(
    app: &AppHandle,
    store: &ProfileStore,
    compose: Option<&Path>,
) -> kavka_core::Result<String> {
    let _busy = PlaygroundGuard::acquire()?;
    let ladder = Ladder { app };

    const DOCKER: (&str, &str) = ("docker", "Looking for Docker");
    const FILE: (&str, &str) = ("compose-file", "Reading the bundled compose file");
    const UP: (&str, &str) = ("up", "Starting a single-node Kafka");
    const PROFILE: (&str, &str) = ("profile", "Saving a connection called Playground");

    ladder.running(DOCKER.0, DOCKER.1, None);
    let (docker, detail) = probe_docker();
    if docker != DockerState::Ready {
        // `detail` IS the endpoint for the remote state, so it is handed to the
        // sentence rather than only appended to it as a broker reply would be.
        let trouble = docker_trouble(docker, detail.as_deref());
        ladder.fail(DOCKER.0, DOCKER.1, trouble.clone());
        ladder.skipped(FILE.0, FILE.1, "skipped");
        ladder.skipped(UP.0, UP.1, "skipped");
        ladder.skipped(PROFILE.0, PROFILE.1, "skipped");
        return Err(kavka_core::Error::Other(match detail {
            // The remote endpoint is already in the sentence; quoting it a
            // second time under "Docker said" would read as two problems.
            Some(_) if docker == DockerState::Remote => trouble,
            Some(detail) => format!("{trouble}\n\nDocker said:\n{detail}"),
            None => trouble,
        }));
    }
    ladder.ok(DOCKER.0, DOCKER.1, None);

    ladder.running(FILE.0, FILE.1, None);
    let Some(compose) = compose else {
        let trouble = format!(
            "Kavka couldn't find its own {PLAYGROUND_COMPOSE}. This build is missing that \
             resource, which is a packaging fault rather than anything on this machine — \
             please report it."
        );
        ladder.fail(FILE.0, FILE.1, trouble.clone());
        ladder.skipped(UP.0, UP.1, "skipped");
        ladder.skipped(PROFILE.0, PROFILE.1, "skipped");
        return Err(kavka_core::Error::Other(trouble));
    };
    ladder.ok(FILE.0, FILE.1, None);

    let file = compose.to_string_lossy().into_owned();
    ladder.running(
        UP.0,
        UP.1,
        Some("The first run pulls the Kafka image — about 400 MB.".to_string()),
    );
    let up = run_bounded(
        "docker",
        &[
            "compose",
            "-p",
            PLAYGROUND_PROJECT,
            "-f",
            &file,
            "up",
            "-d",
            "--wait",
        ],
        PLAYGROUND_START_TIMEOUT,
        |line, elapsed| {
            // Docker's own words plus the clock. A progress bar here would be a
            // lie — Compose does not tell us how much of the pull is left — and
            // docs/DESIGN.md §7 rule 6 asks for a sentence rather than a
            // spinner anyway.
            let secs = elapsed.as_secs();
            ladder.running(
                UP.0,
                UP.1,
                Some(match line {
                    Some(line) => format!("{secs}s · {line}"),
                    None => format!("{secs}s · waiting for Docker"),
                }),
            );
        },
    );
    if up.timed_out {
        let trouble = format!(
            "Docker was still working after {} minutes, so Kavka stopped waiting. The \
             containers may still be coming up — check Docker, or run it yourself:\n\n{}",
            PLAYGROUND_START_TIMEOUT.as_secs() / 60,
            playground_cli_hint(compose)
        );
        ladder.fail(UP.0, UP.1, trouble.clone());
        ladder.skipped(PROFILE.0, PROFILE.1, "skipped");
        return Err(kavka_core::Error::Other(trouble));
    }
    if !up.ok {
        let said = up.tail(8);
        ladder.fail(UP.0, UP.1, said.clone());
        ladder.skipped(PROFILE.0, PROFILE.1, "skipped");
        return Err(kavka_core::Error::Other(format!(
            "Docker couldn't start the playground.\n\nIt said:\n{said}\n\nTo try it yourself:\n{}",
            playground_cli_hint(compose)
        )));
    }
    ladder.ok(
        UP.0,
        UP.1,
        Some(format!("Listening on {PLAYGROUND_BOOTSTRAP}")),
    );

    ladder.running(PROFILE.0, PROFILE.1, None);
    match save_playground(store) {
        Ok(id) => {
            ladder.ok(PROFILE.0, PROFILE.1, Some(PLAYGROUND_BOOTSTRAP.to_string()));
            Ok(id)
        }
        Err(e) => {
            // The broker IS up — that half succeeded and the ladder says so.
            // Failing the command without that distinction would read as "the
            // playground didn't start", and the user would press it again.
            ladder.fail(PROFILE.0, PROFILE.1, e.to_string());
            Err(e)
        }
    }
}

/// The Playground connection, created once and never overwritten.
///
/// **Why it refuses a profile that is no longer `dev`.** The id is fixed, so
/// somebody can open this connection, retag it `prod` and point it at a real
/// cluster — at which point "start the playground" would silently rewrite a
/// production connection's address. Kavka would rather say no. This is the
/// single place in the playground feature where a saved profile is written at
/// all, which is why the whole guardrail fits in one function.
fn save_playground(store: &ProfileStore) -> kavka_core::Result<String> {
    if let Some(existing) = store
        .list()?
        .into_iter()
        .find(|p| p.id == PLAYGROUND_PROFILE_ID)
    {
        if existing.environment != Environment::Dev {
            return Err(kavka_core::Error::Other(format!(
                "The connection called \"{}\" isn't tagged dev any more, so Kavka left it \
                 alone. The playground is running on {PLAYGROUND_BOOTSTRAP} — point a \
                 connection at that address yourself.",
                existing.name
            )));
        }
        // Already there and still a dev connection: keep whatever the user has
        // done to it (a rename, a read-only flag, a masking rule) rather than
        // resetting their work every time they press start.
        return Ok(existing.id);
    }

    store.upsert(ConnectionProfile {
        id: PLAYGROUND_PROFILE_ID.to_string(),
        name: "Playground".to_string(),
        environment: Environment::Dev,
        bootstrap_servers: vec![PLAYGROUND_BOOTSTRAP.to_string()],
        auth: AuthConfig::Plaintext,
        read_only: false,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
        wasm_serdes: Vec::new(),
    })?;
    Ok(PLAYGROUND_PROFILE_ID.to_string())
}

/// Stops the playground's containers.
///
/// `down`, not `down -v`: the named volume stays, so starting it again picks up
/// where it left off, and Kavka never deletes somebody's data on their behalf.
/// The Playground connection stays in the sidebar too — it works again the next
/// time this runs, and a connection that vanishes when you press stop is a
/// connection people stop trusting.
#[tauri::command]
async fn sandbox_stop(app: AppHandle) -> CmdResult<()> {
    let compose = playground_compose(&app);
    let handle = app.clone();
    blocking(move || stop_playground(&handle, compose.as_deref())).await
}

fn stop_playground(app: &AppHandle, compose: Option<&Path>) -> kavka_core::Result<()> {
    let _busy = PlaygroundGuard::acquire()?;
    let ladder = Ladder { app };
    const DOWN: (&str, &str) = ("down", "Stopping the playground");

    let Some(compose) = compose else {
        let trouble = format!("Kavka couldn't find its own {PLAYGROUND_COMPOSE}.");
        ladder.fail(DOWN.0, DOWN.1, trouble.clone());
        return Err(kavka_core::Error::Other(trouble));
    };

    let file = compose.to_string_lossy().into_owned();
    ladder.running(DOWN.0, DOWN.1, None);
    let down = run_bounded(
        "docker",
        &["compose", "-p", PLAYGROUND_PROJECT, "-f", &file, "down"],
        PLAYGROUND_STOP_TIMEOUT,
        |line, elapsed| {
            let secs = elapsed.as_secs();
            ladder.running(
                DOWN.0,
                DOWN.1,
                Some(match line {
                    Some(line) => format!("{secs}s · {line}"),
                    None => format!("{secs}s · waiting for Docker"),
                }),
            );
        },
    );
    if down.spawn_failed || down.timed_out || !down.ok {
        let said = if down.spawn_failed {
            docker_trouble(DockerState::Absent, None)
        } else if down.timed_out {
            "Docker didn't finish stopping the playground in time.".to_string()
        } else {
            down.tail(8)
        };
        ladder.fail(DOWN.0, DOWN.1, said.clone());
        return Err(kavka_core::Error::Other(said));
    }
    ladder.ok(
        DOWN.0,
        DOWN.1,
        Some(
            "The containers are gone. Your playground data is still in its Docker volume."
                .to_string(),
        ),
    );
    Ok(())
}

/// What to say about Docker, in the shape docs/DESIGN.md §7 asks for: what
/// happened, then the next click. The honest sentence about not bundling a
/// broker lives in the UI, beside the button that would have started one —
/// this is the machine-specific half.
///
/// `endpoint` is [`DockerState::Remote`]'s and is ignored by every other
/// state: naming the host is the whole content of that refusal, and a message
/// that says "somewhere else" teaches nobody which context to switch back.
fn docker_trouble(state: DockerState, endpoint: Option<&str>) -> String {
    match state {
        DockerState::Remote => format!(
            "Docker on this machine is pointing at {where_}, which isn't this computer. The \
             playground starts containers — on THIS machine only — so Kavka won't create one on a \
             host you'd then have to go and clean up, on a port it can't reach. Switch back with \
             `docker context use default` (or clear DOCKER_HOST), then check again.",
            where_ = endpoint.unwrap_or("another machine"),
        ),
        DockerState::Absent => {
            "There's no `docker` command on this machine. Install Docker Desktop (or any \
             Docker-compatible runtime with the `compose` plugin) and Kavka can start a broker \
             for you."
                .to_string()
        }
        DockerState::Stopped => {
            "Docker is installed but its engine isn't answering. Start Docker Desktop, wait for \
             it to say it's running, then try again."
                .to_string()
        }
        DockerState::NoCompose => {
            "This Docker doesn't have the `compose` plugin. Kavka needs `docker compose` — the \
             standalone `docker-compose` script isn't the same command. Updating Docker Desktop \
             installs it."
                .to_string()
        }
        DockerState::Ready => "Docker is ready.".to_string(),
    }
}

/// One playground operation at a time, process-wide.
///
/// Not in `AppState`: there is one Docker on this machine and one project name,
/// so two windows racing `up` and `down` is the same collision as one window
/// double-clicking. A `static` says that; a field on the app's state would
/// quietly permit it per-window.
static PLAYGROUND_BUSY: AtomicBool = AtomicBool::new(false);

struct PlaygroundGuard;

impl PlaygroundGuard {
    fn acquire() -> kavka_core::Result<Self> {
        if PLAYGROUND_BUSY.swap(true, Ordering::SeqCst) {
            return Err(kavka_core::Error::Other(
                "Kavka is already starting or stopping the playground. Give it a moment."
                    .to_string(),
            ));
        }
        Ok(Self)
    }
}

impl Drop for PlaygroundGuard {
    fn drop(&mut self) {
        PLAYGROUND_BUSY.store(false, Ordering::SeqCst);
    }
}

// ── Diagnostics — opt-in, local, and nothing else ──────────────────────────
//
// WHAT THIS IS: a panic hook and a webview error handler that append lines to
// rotating files in the app's data directory, so that "Kavka closed itself and
// I don't know why" can become a GitHub issue with something attached to it.
//
// WHAT THIS IS NOT, and the reason every sentence in the About section is
// written the way it is: **there is no telemetry endpoint.** Not a disabled
// one, not one behind a flag — Kavka has no code anywhere that sends a report,
// so there is nothing to trust us about. The file is on the user's disk, they
// open it with `Open logs folder`, they read it, and they decide whether to
// paste it anywhere. That is the entire design, and it is why the toggle can
// honestly default to OFF: nobody is being asked to opt into a transmission,
// they are being asked whether Kavka may write a file.
//
// DEFAULT OFF is deliberate even though it costs the first crash. A log that
// records a payload fragment, a topic name or a bootstrap address is a log that
// can carry something out of a regulated network in a screenshot, and a Kafka
// GUI's users are frequently inside one. So Kavka writes nothing until asked,
// and says exactly what it will write before it is.

/// Where the log files go. Set once, in `setup`. `None` before that — which is
/// what makes the panic hook safe to install before the app exists.
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Whether anything is written at all. The shipped state is `false`; the only
/// thing that sets it is the user's toggle, or reading back what the user's
/// toggle wrote last time.
static DIAGNOSTICS_ON: AtomicBool = AtomicBool::new(false);

/// The current file. Older ones are `kavka.1.log` … `kavka.4.log`.
const LOG_NAME: &str = "kavka.log";

/// Five files, as the brief asks. Small enough that the whole set fits in a
/// GitHub issue's attachment and old enough to cover more than one session.
const LOG_KEEP: usize = 5;

/// When the current file passes this, it rotates. 512 KB × 5 caps the feature's
/// total disk cost at 2.5 MB, which is a number worth being able to state in
/// the About panel.
const LOG_MAX_BYTES: u64 = 512 * 1024;

/// Longest single entry. A JavaScript stack trace is the thing most likely to
/// arrive long, and past a couple of thousand characters it is noise.
const LOG_MAX_ENTRY: usize = 2000;

/// The file Kavka remembers the toggle in, beside `profiles.json`.
const DIAGNOSTICS_FILE: &str = "diagnostics.json";

#[derive(Serialize)]
struct DiagnosticsStatus {
    enabled: bool,
    /// Always reported, even when nothing has been written — a person deciding
    /// whether to turn this on is entitled to know where the files would go.
    dir: String,
    files: usize,
    bytes: u64,
}

/// Appends one line, if the user has asked for that.
///
/// Every failure is swallowed on purpose. This is called from a panic hook: a
/// diagnostics write that panics turns one crash into a recursive one, and a
/// disk that is full is not a thing to report by writing to disk.
fn diagnostics_write(kind: &str, message: &str) {
    if !DIAGNOSTICS_ON.load(Ordering::Relaxed) {
        return;
    }
    let Some(dir) = LOG_DIR.get() else { return };
    append_log(dir, kind, message);
}

fn append_log(dir: &Path, kind: &str, message: &str) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let file = dir.join(LOG_NAME);
    if std::fs::metadata(&file).is_ok_and(|meta| meta.len() >= LOG_MAX_BYTES) {
        rotate_logs(dir);
    }
    let Ok(mut handle) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
    else {
        return;
    };
    // One entry is one line, whatever it contains: an embedded newline in a
    // stack trace would otherwise turn one entry into fifteen and make the file
    // impossible to skim.
    let flat: String = message
        .chars()
        .take(LOG_MAX_ENTRY)
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let _ = writeln!(handle, "{} {kind} {}", iso8601(now_ms()), flat.trim());
}

/// `kavka.log` → `kavka.1.log` → … → `kavka.4.log`, oldest dropped.
fn rotate_logs(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(format!("kavka.{}.log", LOG_KEEP - 1)));
    for n in (1..LOG_KEEP - 1).rev() {
        let _ = std::fs::rename(
            dir.join(format!("kavka.{n}.log")),
            dir.join(format!("kavka.{}.log", n + 1)),
        );
    }
    let _ = std::fs::rename(dir.join(LOG_NAME), dir.join("kavka.1.log"));
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = vec![dir.join(LOG_NAME)];
    files.extend((1..LOG_KEEP).map(|n| dir.join(format!("kavka.{n}.log"))));
    files.retain(|path| path.is_file());
    files
}

fn read_diagnostics_status(dir: &Path) -> DiagnosticsStatus {
    let files = log_files(dir);
    DiagnosticsStatus {
        enabled: DIAGNOSTICS_ON.load(Ordering::Relaxed),
        dir: dir.display().to_string(),
        files: files.len(),
        bytes: files
            .iter()
            .filter_map(|path| std::fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum(),
    }
}

/// Reads the toggle back. Anything unreadable — missing file, truncated JSON,
/// a value written by a future build — means OFF, because the safe answer to
/// "should Kavka write a log?" is always no.
fn read_diagnostics_pref(config_dir: &Path) -> bool {
    std::fs::read_to_string(config_dir.join(DIAGNOSTICS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("enabled").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

fn write_diagnostics_pref(config_dir: &Path, enabled: bool) -> kavka_core::Result<()> {
    std::fs::create_dir_all(config_dir).map_err(|e| {
        kavka_core::Error::Other(format!(
            "Kavka couldn't create {}: {e}",
            config_dir.display()
        ))
    })?;
    let path = config_dir.join(DIAGNOSTICS_FILE);
    std::fs::write(&path, format!("{{\n  \"enabled\": {enabled}\n}}\n")).map_err(|e| {
        kavka_core::Error::Other(format!("Kavka couldn't write {}: {e}", path.display()))
    })
}

/// Records a panic, then hands the payload to whatever hook was there before,
/// so the terminal still gets Rust's own message and a `RUST_BACKTRACE=1` run
/// still prints a backtrace.
///
/// Installed at the very top of [`run`], before the builder — a panic while
/// Tauri is starting up is exactly the crash somebody would want a file for,
/// and the hook is safe that early because it does nothing until `LOG_DIR` has
/// been set and the user has opted in.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panicked with no message".to_string());
        let at = info.location().map_or_else(
            || "an unknown location".to_string(),
            |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        );
        diagnostics_write("panic", &format!("{what} — at {at}"));
        previous(info);
    }));
}

/// The one line every log starts with, so a file that then records a crash also
/// records what was running when it happened.
fn log_session_header() {
    diagnostics_write(
        "session",
        &format!(
            "Kavka {} · {} {} · webview session started",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
}

#[tauri::command]
async fn diagnostics_status() -> CmdResult<DiagnosticsStatus> {
    let dir = logs_dir()?;
    blocking(move || Ok(read_diagnostics_status(&dir))).await
}

#[tauri::command]
async fn diagnostics_set_enabled(app: AppHandle, enabled: bool) -> CmdResult<DiagnosticsStatus> {
    let dir = logs_dir()?;
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    blocking(move || {
        write_diagnostics_pref(&config, enabled)?;
        DIAGNOSTICS_ON.store(enabled, Ordering::Relaxed);
        if enabled {
            log_session_header();
        }
        // Turning it off does NOT delete what is already there. The user may
        // have just captured the crash they are about to report, and a toggle
        // that silently destroys evidence is worse than one that leaves a file
        // behind — `diagnostics_clear` is the button that deletes, and it says
        // so on its face.
        Ok(read_diagnostics_status(&dir))
    })
    .await
}

/// One line from the webview: an uncaught error or a rejected promise.
///
/// Returns whether anything was actually written, so the UI can say "nothing
/// was recorded — diagnostics is off" instead of implying a file exists.
#[tauri::command]
async fn diagnostics_record(kind: String, message: String) -> CmdResult<bool> {
    // The webview is not trusted to invent categories: the file has to stay
    // skimmable, so anything unrecognised becomes `ui`.
    let kind = match kind.as_str() {
        "error" | "rejection" | "note" => kind,
        _ => "ui".to_string(),
    };
    if !DIAGNOSTICS_ON.load(Ordering::Relaxed) {
        return Ok(false);
    }
    blocking(move || {
        diagnostics_write(&kind, &message);
        Ok(true)
    })
    .await
}

#[tauri::command]
async fn diagnostics_open_logs(app: AppHandle) -> CmdResult<()> {
    let dir = logs_dir()?;
    blocking(move || {
        // Created first: opening a folder that does not exist is a dead end,
        // and the folder legitimately does not exist until something is logged.
        std::fs::create_dir_all(&dir).map_err(|e| {
            kavka_core::Error::Other(format!("Kavka couldn't create {}: {e}", dir.display()))
        })?;
        app.opener()
            .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
            .map_err(|e| {
                kavka_core::Error::Other(format!(
                    "Kavka couldn't ask this machine to open {}: {e}. The path is above — open \
                     it yourself.",
                    dir.display()
                ))
            })
    })
    .await
}

/// Deletes every log file. The counterpart to the toggle: a feature whose whole
/// claim is "this stays on your machine" has to let you take it off your
/// machine.
#[tauri::command]
async fn diagnostics_clear() -> CmdResult<DiagnosticsStatus> {
    let dir = logs_dir()?;
    blocking(move || {
        for path in log_files(&dir) {
            std::fs::remove_file(&path).map_err(|e| {
                kavka_core::Error::Other(format!("Kavka couldn't delete {}: {e}", path.display()))
            })?;
        }
        Ok(read_diagnostics_status(&dir))
    })
    .await
}

fn logs_dir() -> CmdResult<PathBuf> {
    LOG_DIR.get().cloned().ok_or_else(|| {
        "Kavka doesn't know where its data directory is on this machine, so it can't write logs \
         there."
            .to_string()
    })
}

/// RFC 3339 in UTC to millisecond precision — `2023-11-14T22:13:20.000Z`.
///
/// A deliberate second copy of `kavka_core::produce`'s private helper (Howard
/// Hinnant's civil-from-days). The alternative is making that one public, which
/// would put a date formatter in the core's API surface for the sake of a log
/// line in the shell, or adding `chrono` to this crate for fifteen lines.
fn iso8601(now_ms: i64) -> String {
    let days = now_ms.div_euclid(86_400_000);
    let ms_of_day = now_ms.rem_euclid(86_400_000);
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
    let year = if month <= 2 { year + 1 } else { year };
    let (hour, minute, second, milli) = (
        ms_of_day / 3_600_000,
        (ms_of_day / 60_000) % 60,
        (ms_of_day / 1_000) % 60,
        ms_of_day % 1_000,
    );
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis()),
    )
    .unwrap_or(i64::MAX)
}

// ── Plain English → a query ────────────────────────────────────────────────

/// Translates one plain-English line into a CEL filter or a SQL statement.
///
/// **A grammar, not a model.** `kavka_core::nlq` is a pure function over a
/// curated pattern list: no network, no weights, no state between calls, and
/// the same words always produce the same query. This command is a two-line
/// forward precisely because there is nothing else to it — and the UI says so
/// in as many words (`NlQueryBar`), because a "plain English" box that looks
/// like an assistant and is a lookup table would be the most dishonest control
/// in the product.
///
/// **The answer fills the editor and is never run.** That is the safety model:
/// a grammar that mis-reads a sentence produces a query the user reads before
/// pressing the button they were always going to press. Nothing here touches a
/// cluster, so there is no connection to check and no read-only rule to apply.
///
/// Not on the blocking pool, unlike almost everything else in this file: it is
/// a tokenizer over one short line, it opens no file and takes no lock, and a
/// hop to another thread would cost more than the work.
#[tauri::command]
fn nl_to_query(input: String, mode: String, schema_hint: SchemaHint) -> CmdResult<Translation> {
    // Parsed rather than taken as the enum so an unknown mode is the core's own
    // sentence — expected "cel" or "sql" — rather than a serde error about a
    // variant.
    let mode = mode.parse().map_err(|e: kavka_core::Error| e.to_string())?;
    Ok(nlq::nl_to_query(&input, mode, &schema_hint))
}

/// Every sentence the translator accepts, in the core's own words.
///
/// The text is `kavka_core::nlq::NLQ_GRAMMAR`, which a test checks against the
/// implementation — so a pattern the grammar grew and the doc did not mention
/// fails the build rather than the user. That is why this is a command and not
/// a constant copied into the webview: a cheatsheet that drifts from the engine
/// teaches expressions that do nothing.
#[tauri::command]
fn nl_grammar() -> String {
    nlq::nlq_grammar().to_string()
}

// ── Masking ────────────────────────────────────────────────────────────────
//
// Rules live in masking.json beside profiles.json and alerts.json, and like
// alert rules they are deliberately NOT part of a profile export: one person's
// redaction policy is not portable, and a rule somebody relies on silently not
// travelling with the connection it was written for is worse than it not
// travelling at all.
//
// Every command here is local — nothing is read from or written to a cluster —
// so, exactly like the alert commands, none of them is gated on read-only.
// Masking is about what reaches the screen; read-only is about what leaves
// Kavka. Tying them together would mean turning off a guardrail to get a
// redaction.
//
// All four drop the profile's compiled `MaskSet` afterwards, which is what
// makes an edit take effect on the next batch of a tail that is already
// running.

#[tauri::command]
async fn masking_list(state: State<'_, AppState>, profile_id: String) -> CmdResult<Vec<MaskRule>> {
    let masks = Arc::clone(&state.masks);
    blocking(move || masks.rules(&profile_id)).await
}

/// Upsert by `rule.id`, so the editor's "save" is one call whether the rule is
/// new or not.
///
/// The pattern is compiled by the core **before** anything is written: a stored
/// rule that cannot compile is a rule that silently does not mask, which is the
/// worst failure this feature has — the user believes the screen is redacted.
#[tauri::command]
async fn masking_save(
    state: State<'_, AppState>,
    profile_id: String,
    rule: MaskRule,
) -> CmdResult<()> {
    let masks = Arc::clone(&state.masks);
    let id = profile_id.clone();
    let saved = blocking(move || masks.save_rule(&id, rule)).await;
    state.forget_mask_set(&profile_id);
    saved
}

#[tauri::command]
async fn masking_delete(
    state: State<'_, AppState>,
    profile_id: String,
    rule_id: String,
) -> CmdResult<()> {
    let masks = Arc::clone(&state.masks);
    let id = profile_id.clone();
    let deleted = blocking(move || masks.delete_rule(&id, &rule_id)).await;
    // Dropped even if the delete failed: the cheap mistake is recompiling a set
    // that did not change, and the expensive one is a rule that is gone from
    // the file and still masking (or, worse, still believed to be).
    state.forget_mask_set(&profile_id);
    deleted
}

/// Turns one rule on or off without rewriting it — a different action from the
/// editor's save, and routing it through save would mean re-validating (and
/// potentially refusing) a pattern the user is not editing.
#[tauri::command]
async fn masking_toggle(
    state: State<'_, AppState>,
    profile_id: String,
    rule_id: String,
    enabled: bool,
) -> CmdResult<()> {
    let masks = Arc::clone(&state.masks);
    let id = profile_id.clone();
    let toggled = blocking(move || masks.set_enabled(&id, &rule_id, enabled).map(|_| ())).await;
    state.forget_mask_set(&profile_id);
    toggled
}

// ── WASM decoder plugins ───────────────────────────────────────────────────
//
// Unlike masking rules these live ON the profile (ConnectionProfile
// ::wasm_serdes), because a plugin is part of how a cluster's payloads are read
// rather than a policy about who is looking. The .wasm file itself never
// travels: the profile carries a PATH, so an imported profile naming a plugin
// this machine does not have reports that the same way a missing CA file does.
//
// **A change takes effect on the next connect.** The plugin set is compiled
// when `ClusterConnection::connect` opens the cluster (see
// `ClusterConnection::decoder_for`), which is what keeps one scan's records
// decoded one way from beginning to end — a module swapped underneath a running
// search would produce a result set assembled from two decoders.

/// The one thing three commands do: read the profile, change its plugin list,
/// write it back.
///
/// A read-modify-write over the whole profile, because that is the store's unit
/// ([`ProfileStore::upsert`]). It races the ProfileEditor's own save the way any
/// two edits of one document race — last writer wins — which is the same
/// exposure the editor has had since Phase 0 and is bounded by both being
/// user-driven actions on one window.
fn edit_wasm_serdes(
    store: &ProfileStore,
    profile_id: &str,
    edit: impl FnOnce(&mut Vec<WasmSerdeConfig>),
) -> kavka_core::Result<()> {
    let mut profile = store
        .list()?
        .into_iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| kavka_core::Error::Other(format!("unknown profile: {profile_id}")))?;
    edit(&mut profile.wasm_serdes);
    store.upsert(profile)
}

#[tauri::command]
async fn wasm_serdes_list(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<Vec<WasmSerdeConfig>> {
    let store = state.store.clone();
    blocking(move || {
        Ok(store
            .list()?
            .into_iter()
            .find(|p| p.id == profile_id)
            .map(|profile| profile.wasm_serdes)
            .unwrap_or_default())
    })
    .await
}

/// The one thing checked before a plugin is written into a profile: **where it
/// comes from**.
///
/// `kavka_core::wasm_serde::validate_plugin_path` owns the rule and the
/// sentences — a relative path resolves against whatever folder the app was
/// launched from and would mean something else again on the machine an exported
/// profile lands on, and a UNC path moves the execution source onto somebody
/// else's computer. This is the save-time half; the loader checks the same
/// thing again, because `profiles.json` can be hand-edited and imported.
///
/// It is deliberately the ONLY save-time check. The module itself is not loaded
/// here: `WasmSerdes::load` compiles it when the connection opens and says what
/// is wrong with it in a sentence, and a plugin file that is fine today and
/// missing tomorrow would have passed an existence check at save time anyway.
fn validated_wasm_serde(config: WasmSerdeConfig) -> kavka_core::Result<WasmSerdeConfig> {
    kavka_core::wasm_serde::validate_plugin_path(&config.path)
        .map_err(|e| kavka_core::Error::Other(e.to_string()))?;
    Ok(config)
}

/// Upsert by `config.name` — the name is the identity, and it is what the
/// payload inspector's provenance line shows.
#[tauri::command]
async fn wasm_serdes_save(
    state: State<'_, AppState>,
    profile_id: String,
    config: WasmSerdeConfig,
) -> CmdResult<()> {
    let store = state.store.clone();
    blocking(move || {
        // Before the read-modify-write, so a refused path leaves the profile
        // exactly as it was.
        let config = validated_wasm_serde(config)?;
        edit_wasm_serdes(&store, &profile_id, |plugins| {
            match plugins.iter_mut().find(|kept| kept.name == config.name) {
                Some(slot) => *slot = config,
                None => plugins.push(config),
            }
        })
    })
    .await
}

/// Idempotent: removing a plugin that is not there is not an error — the other
/// window already did it.
#[tauri::command]
async fn wasm_serdes_delete(
    state: State<'_, AppState>,
    profile_id: String,
    name: String,
) -> CmdResult<()> {
    let store = state.store.clone();
    blocking(move || {
        edit_wasm_serdes(&store, &profile_id, |plugins| {
            plugins.retain(|plugin| plugin.name != name);
        })
    })
    .await
}

#[tauri::command]
async fn profiles_list(state: State<'_, AppState>) -> CmdResult<Vec<ConnectionProfile>> {
    let store = state.store.clone();
    blocking(move || store.list()).await
}

/// Writes a profile.
///
/// **The WASM plugin list is not this command's to write.** It is edited
/// through `wasm_serdes_save`/`wasm_serdes_delete` (the ProfileEditor's plugin
/// section writes as you type, exactly as the alert rules do), and the profile
/// object this command receives is BUILT FIELD BY FIELD by that editor — so a
/// plugin list it does not carry would be an empty list here, and an upsert
/// would wipe every plugin on the connection the first time somebody renamed
/// it. That is silent data loss of a file path the user had to go and find,
/// and it is exactly the class of bug `serde(default)` on an additive field
/// makes easy: the field parses fine and arrives empty.
///
/// So an incoming empty list means "I have nothing to say about plugins" and
/// the stored one is kept. Emptying the list is `wasm_serdes_delete`'s job,
/// which is the only surface that can say it and mean it.
#[tauri::command]
async fn profiles_save(state: State<'_, AppState>, profile: ConnectionProfile) -> CmdResult<()> {
    let store = state.store.clone();
    blocking(move || {
        let mut profile = profile;
        if profile.wasm_serdes.is_empty() {
            if let Some(stored) = store.list()?.into_iter().find(|p| p.id == profile.id) {
                profile.wasm_serdes = stored.wasm_serdes;
            }
        }
        store.upsert(profile)
    })
    .await
}

#[tauri::command]
async fn profiles_delete(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    // A session that outlives the profile it belongs to is a client working a
    // cluster the user just deleted, feeding a view that can never be reopened.
    // `take_sessions_of` stops the monitor too, so nothing is still sampling
    // into a history file that is about to be removed.
    let sessions = state.take_sessions_of(&profile_id);
    state.forget_mask_set(&profile_id);
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    let protocol = state.take_protocol(&profile_id);
    let history = state.histories.take(&profile_id);
    let history_path = state.histories.path_of(&profile_id);
    let histories = Arc::clone(&state.histories);
    let alerts = Arc::clone(&state.alerts);
    let masks = Arc::clone(&state.masks);
    let store = state.store.clone();
    blocking(move || {
        drop(sessions); // joins each worker thread, off the event loop
        drop(conn); // librdkafka client destroy, off the event loop
        drop(protocol); // and the kept-alive wire-protocol socket
        drop(history); // and the redb handle, so the file can be removed

        // Read the profile BEFORE it is deleted, for the secrets no constant
        // can name. A Connect cluster's password entry is
        // `{id}/connect_password/{cluster}` — the cluster's own name is in it —
        // so the only enumeration of them is the profile itself, and once
        // `store.delete` has run there is nothing left to enumerate. Failing to
        // read is not failing to delete: the purge is best-effort throughout.
        let connect_entries = store
            .list()
            .ok()
            .and_then(|profiles| profiles.into_iter().find(|p| p.id == profile_id))
            .map(|profile| profile.connect_secret_entries())
            .unwrap_or_default();

        store.delete(&profile_id)?;
        // Best-effort purge: an orphaned keychain entry is harmless, a ghost
        // profile in the UI is not — so a purge failure doesn't fail the
        // delete. The list is the core's, not a copy: it is one half of a
        // contract with the ProfileEditor's `entry` vocabulary, and a local
        // literal here is exactly how `sr_password` came to be written on save
        // and left behind on delete.
        for suffix in kavka_core::secrets::SECRET_SUFFIXES {
            let _ =
                kavka_core::secrets::delete(&kavka_core::secrets::entry_name(&profile_id, suffix));
        }
        // The same regression one level down: these are already full entry
        // names, read off the stored refs, so a cluster renamed by hand still
        // purges the entry it actually references.
        for entry in connect_entries {
            let _ = kavka_core::secrets::delete(&entry);
        }
        // And the same argument a third time, for the two local files that are
        // not secrets but are just as much about a cluster the user can no
        // longer see: this profile's alert rules, channels and incident
        // history, and its week of lag samples.
        let _ = alerts.forget_profile(&profile_id);
        // And its masking rules, by the same argument a fourth time: a
        // redaction policy for a cluster nobody can reach again is state
        // nobody can find to delete.
        let _ = masks.forget_profile(&profile_id);
        // Taken a second time on purpose. Stopping the monitor does not join
        // its thread, so a tick that was already in flight can have reopened
        // the handle between the two lines above and this one. If the file is
        // still locked the removal fails and is left alone: an orphaned history
        // file is local, unreachable (its name is derived from a profile id
        // that no longer exists) and never written to again.
        drop(histories.take(&profile_id));
        let _ = std::fs::remove_file(&history_path);
        Ok(())
    })
    .await
}

/// Secret-free by construction — profiles hold keychain refs, not values.
#[tauri::command]
async fn profiles_export(state: State<'_, AppState>) -> CmdResult<String> {
    let store = state.store.clone();
    blocking(move || Ok(export_json(&store.list()?))).await
}

#[tauri::command]
async fn profiles_import(
    state: State<'_, AppState>,
    json: String,
    strategy: String,
) -> CmdResult<ImportReport> {
    let store = state.store.clone();
    blocking(move || {
        let strategy: ImportStrategy = strategy.parse()?;
        store.import(import_json(&json)?, strategy)
    })
    .await
}

#[tauri::command]
async fn secret_set(entry: String, value: String) -> CmdResult<()> {
    blocking(move || kavka_core::secrets::set(&entry, &value)).await
}

#[tauri::command]
async fn secret_delete(entry: String) -> CmdResult<()> {
    blocking(move || kavka_core::secrets::delete(&entry)).await
}

/// Presence only — the value never crosses the IPC boundary. The editor asks
/// this to decide whether a blank secret field really means "keep the stored
/// one", instead of inferring it from the sign-in method and being wrong.
#[tauri::command]
async fn secret_exists(entry: String) -> CmdResult<bool> {
    blocking(move || kavka_core::secrets::exists(&entry)).await
}

#[tauri::command]
async fn cluster_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<ClusterOverview> {
    let store = state.store.clone();
    let id = profile_id.clone();
    // The monitor is built here, on the same blocking task, because building it
    // reads the metrics endpoint's password out of the keychain — and a
    // keychain read on the event loop is the one thing this file's header
    // forbids.
    let (conn, overview, monitor) = blocking(move || {
        let profile = store
            .list()?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| kavka_core::Error::Other(format!("unknown profile: {id}")))?;
        let conn = ClusterConnection::connect(profile)?;
        // Said once, here, where it happened: a plugin that would not load is
        // not a reason to refuse the cluster (the built-in decoders are always
        // there), so the only alternative to saying so now is never saying so.
        for problem in conn.serde_problems() {
            tracing::warn!(profile = %conn.profile().id, "{problem}");
        }
        let overview = conn.overview()?;
        let monitor = Arc::new(Monitor::for_profile(conn.profile()));
        Ok((Arc::new(conn), overview, monitor))
    })
    .await?;

    // Sampling starts with the connection and stops with it: history is only
    // ever collected while somebody has this cluster open, which is the fact
    // every gap in every chart has to be read against.
    state.start_monitor(&app, &monitor, &conn);

    // The cached protocol socket belongs to the connection being replaced: it
    // was authenticated with the profile as it was when it opened, so a
    // reconnect after an edit must not keep answering over it.
    let stale_protocol = state.take_protocol(&profile_id);
    let replaced = state.connections.lock().unwrap().insert(profile_id, conn);
    if replaced.is_some() || stale_protocol.is_some() {
        // Reconnect over an existing session: destroy the old clients off-loop.
        tauri::async_runtime::spawn_blocking(move || {
            drop(replaced);
            drop(stale_protocol);
        });
    }
    Ok(overview)
}

#[tauri::command]
async fn cluster_disconnect(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    // Sessions first: each owns its own client, so disconnecting without them
    // leaves tails emitting into a UI that thinks it is offline, searches and
    // queries fetching, and bulk runs still writing. It takes the copies with
    // this cluster at EITHER end, too — a copy started elsewhere is still a
    // producer writing into this one. The same argument applies to a fetch
    // that is still polling — and to the monitor, which `take_sessions_of`
    // stops with them. Sampling is a thing Kavka does *while you are watching a
    // cluster*, and a sampler that outlived the disconnect would keep a broker
    // answering `ListGroups` for a window nobody has open.
    //
    // The history file itself stays open and stays put: it is on disk, pruned at
    // seven days, and the Monitoring tab can read last week for a profile nobody
    // is connected to. Only deleting the profile takes it away.
    let sessions = state.take_sessions_of(&profile_id);
    state.forget_mask_set(&profile_id);
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    // The kept-alive protocol socket goes with them, for the same reason: it is
    // authenticated to a cluster the user has just walked away from.
    let protocol = state.take_protocol(&profile_id);
    if !sessions.is_empty() || conn.is_some() || protocol.is_some() {
        tauri::async_runtime::spawn_blocking(move || {
            drop(sessions);
            drop(conn);
            drop(protocol);
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn topics_list(state: State<'_, AppState>, profile_id: String) -> CmdResult<Vec<TopicInfo>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || conn.list_topics()).await
}

// ── Topics ─────────────────────────────────────────────────────────────────

#[tauri::command]
async fn topic_detail(
    state: State<'_, AppState>,
    profile_id: String,
    topic: String,
) -> CmdResult<TopicDetail> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::topic_detail(&conn, &topic)).await
}

/// Mutating: `create_topic` calls `ensure_writable` before it touches the
/// cluster, so read-only is enforced in core and not here.
#[tauri::command]
async fn topic_create(
    state: State<'_, AppState>,
    profile_id: String,
    name: String,
    partitions: u32,
    replication_factor: u16,
    configs: Vec<TopicConfig>,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::create_topic(&conn, &name, partitions, replication_factor, &configs))
        .await
}

/// Mutating: guarded in core (see `topic_create`).
#[tauri::command]
async fn topic_delete(
    state: State<'_, AppState>,
    profile_id: String,
    topic: String,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::delete_topic(&conn, &topic)).await
}

// ── Consumer groups ────────────────────────────────────────────────────────

#[tauri::command]
async fn groups_list(state: State<'_, AppState>, profile_id: String) -> CmdResult<Vec<GroupInfo>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::groups_list(&conn)).await
}

#[tauri::command]
async fn group_detail(
    state: State<'_, AppState>,
    profile_id: String,
    group_id: String,
) -> CmdResult<GroupDetail> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::group_detail(&conn, &group_id)).await
}

/// Mutating, and guarded twice in core: read-only, then the active-group check
/// that `spec.force` overrides. Resolves with the offsets after the reset.
#[tauri::command]
async fn offsets_reset(
    state: State<'_, AppState>,
    profile_id: String,
    spec: OffsetResetSpec,
) -> CmdResult<Vec<GroupOffset>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::offsets_reset(&conn, &spec)).await
}

// ── Messages ───────────────────────────────────────────────────────────────

/// One browse per connection: starting this cancels the profile's previous
/// fetch, and the core returns whatever that one had already read. The UI
/// discards it anyway — `MessagesView`'s `fetchSeq` only accepts the newest
/// answer — so the point is the seconds and the pool slot, not the payload.
#[tauri::command]
async fn messages_fetch(
    state: State<'_, AppState>,
    profile_id: String,
    spec: FetchSpec,
) -> CmdResult<Vec<MessageRecord>> {
    let conn = state.connection(&profile_id)?;
    // Resolved BEFORE the fetch, so an unreadable `masking.json` is an error
    // instead of a screenful of unmasked payloads (see `AppState::mask_set`).
    let rules = state.mask_set(&profile_id).map_err(|e| e.to_string())?;
    let token = state.begin_fetch(&profile_id);
    let running = token.clone();
    let result = blocking(move || {
        // The registry is the profile's, not a global: two clusters can have
        // different registries, and one of them can have none.
        let mut records = consume::fetch_messages(
            &conn,
            conn.profile().schema_registry.as_ref(),
            &spec,
            Some(&running),
        )?;
        // On the blocking pool, with the records still on this side of IPC.
        mask_batch(&rules, &mut records);
        // Once per fetch rather than once per record — the same rule the
        // Schema Registry's own errors follow.
        report_serde_errors(&conn);
        Ok(records)
    })
    .await;
    state.end_fetch(&profile_id, &token);
    result
}

#[tauri::command]
async fn tail_start(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    topic: String,
    partitions: Option<Vec<i32>>,
) -> CmdResult<String> {
    let conn = state.connection(&profile_id)?;
    // Compiled before the session exists, so an unreadable `masking.json` is an
    // error the user reads on the button they pressed rather than a tail that
    // stops on its first batch (`AppState::mask_set` says why this fails
    // closed). The set is cached: the pump re-reads it per batch to pick up a
    // rule switched on mid-tail, and pays a hashmap lookup for it.
    state.mask_set(&profile_id).map_err(|e| e.to_string())?;
    // Started on the blocking pool because the consumer is created and assigned
    // before `start` returns: an unknown topic or a dead broker fails here,
    // where the caller can be told, rather than as a session that ends a moment
    // later for no stated reason.
    let session = blocking(move || {
        TailSession::start(
            &conn,
            conn.profile().schema_registry.as_ref(),
            &topic,
            partitions.as_deref(),
        )
        .map(Arc::new)
    })
    .await?;

    let tail_id = state.next_id();
    state.tails.insert(tail_id.clone(), &profile_id, &session);

    let spawned = spawn_emitter("kavka-tail-emit", {
        let tail_id = tail_id.clone();
        let profile_id = profile_id.clone();
        move || pump_tail(&app, &tail_id, &profile_id, &session)
    });
    if let Err(e) = spawned {
        // Nothing will ever drain this session, so it must not be left running.
        if let Some(orphan) = state.tails.take(&tail_id) {
            retire(orphan).await?;
        }
        return Err(format!("starting the live tail reader: {e}"));
    }
    Ok(tail_id)
}

/// Idempotent: an unknown id is `Ok(())`. The UI stops a tail when the view
/// unmounts *and* when the `ended` payload arrives, and neither of those is a
/// mistake worth an error.
#[tauri::command]
async fn tail_stop(state: State<'_, AppState>, tail_id: String) -> CmdResult<()> {
    match state.tails.take(&tail_id) {
        Some(session) => retire(session).await,
        None => Ok(()),
    }
}

// ── Search ─────────────────────────────────────────────────────────────────

/// The two event names one search's payloads are addressed to. Both go to every
/// window, so the id in the name is what keeps two searches of the same topic
/// apart.
fn search_results_event(search_id: &str) -> String {
    format!("kavka://search/{search_id}/results")
}

fn search_progress_event(search_id: &str) -> String {
    format!("kavka://search/{search_id}/progress")
}

/// One batch of matches. Only ever emitted while the core still has room in its
/// result buffer — past `max_buffered` the search keeps scanning and keeps
/// counting, which is what `SearchProgress` is for.
#[derive(Clone, Serialize)]
struct SearchResults {
    records: Vec<MessageRecord>,
}

/// Blocks until this session's window has confirmed its listeners, or until
/// [`READY_TIMEOUT`] gives up on it. Shared by both emitters, because both have
/// the same race and the same answer to it.
fn await_subscriber(app: &AppHandle, session_id: &str, what: &str) {
    let gate = app
        .try_state::<AppState>()
        .and_then(|state| state.ready.lock().unwrap().get(session_id).map(Arc::clone));
    // No gate at all means the state is gone (shutdown) — emit and let the
    // emit itself fail, rather than inventing a wait nobody will end.
    if let Some(gate) = gate {
        if !gate.wait(READY_TIMEOUT) {
            tracing::warn!(
                "{what} {session_id}: no window confirmed its listeners within {READY_TIMEOUT:?}; \
                 reporting anyway"
            );
        }
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.disarm_ready(session_id);
    }
}

/// One search's reader loop. Runs on its own thread — see [`spawn_emitter`].
fn pump_search(app: &AppHandle, search_id: &str, profile_id: &str, session: &SearchSession) {
    let results_event = search_results_event(search_id);
    let progress_event = search_progress_event(search_id);

    // Nothing at all is emitted until the UI says its listeners are up — see
    // READY_TIMEOUT. The clock starts before the wait so a search whose window
    // never confirmed still reports on the first pass rather than 250ms later.
    let mut last_progress = Instant::now();
    await_subscriber(app, search_id, "search");

    // `None` is the end of the search; `Some(empty)` is a scan that has not
    // matched anything yet, which is a state the UI has to be able to say out
    // loud (docs/DESIGN.md §7: never "no results" while a search is running).
    while let Some(mut records) = session.next_results(SEARCH_POLL) {
        if !records.is_empty() {
            // Same placement and same reason as the live tail's: on this side
            // of the boundary, per batch, fail closed.
            match mask_session(app, profile_id) {
                Some(rules) => {
                    mask_batch(&rules, &mut records);
                }
                None => {
                    session.stop();
                    break;
                }
            }
            if let Err(e) = app.emit(&results_event, SearchResults { records }) {
                // Nobody can receive this search any more; don't keep scanning.
                tracing::warn!("search {search_id}: {e}");
                session.stop();
                break;
            }
        }
        if last_progress.elapsed() < PROGRESS_EVERY {
            continue;
        }
        last_progress = Instant::now();
        if let Err(e) = app.emit(&progress_event, session.progress()) {
            tracing::warn!("search {search_id}: {e}");
            session.stop();
            break;
        }
    }

    // Forgotten first, so a `search_stop` racing the last event is the no-op it
    // claims to be, then the one event the UI cannot do without: the final
    // counts, the per-partition cursors that show where a cancel stopped, and
    // the error if the search died rather than finished.
    if let Some(state) = app.try_state::<AppState>() {
        state.searches.forget(search_id);
    }
    report_session_serde_errors(app, profile_id);
    let mut final_progress = session.progress();
    // Forced rather than read, for the one case where it would be false: a loop
    // that left early because the emit failed. As far as anything downstream is
    // concerned this search is over, and the contract is that the last progress
    // event says so.
    final_progress.done = true;
    let _ = app.emit(&progress_event, final_progress);
}

/// Starts a search and answers with the id its two channels are named for.
///
/// Everything a user can get wrong fails here rather than as a search that ends
/// a moment later having found nothing: `SearchSession::start` compiles the CEL
/// expression before it makes a single broker call, then resolves partitions
/// and creates every consumer on the calling thread — which is the blocking
/// pool, because all of that talks to librdkafka.
#[tauri::command]
async fn search_start(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    spec: SearchSpec,
) -> CmdResult<String> {
    let conn = state.connection(&profile_id)?;
    // Before the scan starts, for the reason `tail_start` gives.
    state.mask_set(&profile_id).map_err(|e| e.to_string())?;
    let session = blocking(move || {
        // The registry is the profile's, not a global: two clusters can have
        // different registries, and one of them can have none.
        SearchSession::start(&conn, conn.profile().schema_registry.as_ref(), &spec).map(Arc::new)
    })
    .await?;

    let search_id = state.next_id();
    state
        .searches
        .insert(search_id.clone(), &profile_id, &session);
    // Armed before the id leaves this function, so a `session_ready` racing the
    // emitter's own start has a gate to open.
    state.arm_ready(&search_id);

    let spawned = spawn_emitter("kavka-search-emit", {
        let search_id = search_id.clone();
        let profile_id = profile_id.clone();
        move || pump_search(&app, &search_id, &profile_id, &session)
    });
    if let Err(e) = spawned {
        // Nothing will ever drain this session, and a search nobody drains is
        // eight consumers fetching at full speed.
        state.disarm_ready(&search_id);
        if let Some(orphan) = state.searches.take(&search_id) {
            retire(orphan).await?;
        }
        return Err(format!("starting the search reader: {e}"));
    }
    Ok(search_id)
}

/// The other half of the subscribe handshake: the window calling this has its
/// listeners registered, so the session may start emitting (see
/// [`READY_TIMEOUT`]).
///
/// Idempotent, and an unknown id is `Ok(())` — the session may already be
/// emitting, or already over, and neither is a mistake worth an error. One
/// command serves searches, bulk runs, SQL queries and copies because they are
/// keyed by the same id namespace and all have the same race.
#[tauri::command]
async fn session_ready(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    state.open_ready(&session_id);
    Ok(())
}

/// Idempotent: an unknown id is `Ok(())`. The UI stops a search when the view
/// unmounts, when `Esc` cancels it, and again when the final progress says
/// `done` — none of those is a mistake worth an error.
#[tauri::command]
async fn search_stop(state: State<'_, AppState>, search_id: String) -> CmdResult<()> {
    match state.searches.take(&search_id) {
        Some(session) => retire(session).await,
        None => Ok(()),
    }
}

// ── SQL over a topic ───────────────────────────────────────────────────────

/// The three event names one query's payloads are addressed to. All go to every
/// window, so the id in the name is what keeps two queries apart.
fn sql_schema_event(sql_id: &str) -> String {
    format!("kavka://sql/{sql_id}/schema")
}

fn sql_rows_event(sql_id: &str) -> String {
    format!("kavka://sql/{sql_id}/rows")
}

fn sql_progress_event(sql_id: &str) -> String {
    format!("kavka://sql/{sql_id}/progress")
}

/// The answer's columns, emitted **once** and before any row.
///
/// A third channel rather than a field on the first row batch, because the
/// column list is the only answer a query that matched nothing has: a grid that
/// learned its schema from the first batch would render nothing at all for it.
#[derive(Clone, Serialize)]
struct SqlSchema {
    columns: Vec<SqlColumn>,
}

/// One batch of result rows, positional against [`SqlSchema::columns`].
#[derive(Clone, Serialize)]
struct SqlRows {
    rows: Vec<Vec<serde_json::Value>>,
}

/// One query's reader loop. Runs on its own thread — see [`spawn_emitter`].
fn pump_sql(app: &AppHandle, sql_id: &str, profile_id: &str, session: &SqlSession) {
    let schema_event = sql_schema_event(sql_id);
    let rows_event = sql_rows_event(sql_id);
    let progress_event = sql_progress_event(sql_id);

    // Nothing is emitted until the UI says its listeners are up — and the
    // handshake matters more here than for a search, because the schema is
    // emitted once and never again: a query whose first event is lost has a
    // result set with no column names for the rest of its life.
    let mut last_progress = Instant::now();
    await_subscriber(app, sql_id, "sql query");

    // Read once: the schema event needs it, and so does the masking pass, which
    // decides what a cell is by the name of the column it is in.
    let columns = session.columns();
    if let Err(e) = app.emit(
        &schema_event,
        SqlSchema {
            columns: columns.clone(),
        },
    ) {
        // Nobody can receive this query any more; don't scan a topic for it.
        tracing::warn!("sql {sql_id}: {e}");
        session.stop();
    } else {
        // `None` is the end of the query; `Some(empty)` is a scan that has not
        // produced a row yet, which is a state the UI has to be able to say out
        // loud (docs/DESIGN.md §7: never "no results" while something runs).
        while let Some(mut rows) = session.next_rows(SQL_POLL) {
            if !rows.is_empty() {
                // A result set is projected columns rather than records, so the
                // pass is by column — see `mask_rows`, which documents what it
                // can and cannot know about a computed one.
                match mask_session(app, profile_id) {
                    Some(rules) => {
                        mask_rows(&rules, &columns, &mut rows);
                    }
                    None => {
                        session.stop();
                        break;
                    }
                }
                if let Err(e) = app.emit(&rows_event, SqlRows { rows }) {
                    tracing::warn!("sql {sql_id}: {e}");
                    session.stop();
                    break;
                }
            }
            if last_progress.elapsed() < PROGRESS_EVERY {
                continue;
            }
            last_progress = Instant::now();
            if let Err(e) = app.emit(&progress_event, session.progress()) {
                tracing::warn!("sql {sql_id}: {e}");
                session.stop();
                break;
            }
        }
    }

    // Forgotten first, so a `sql_stop` racing the last event is the no-op it
    // claims to be, then the one event the UI cannot do without: the final
    // counts, the error if the query died — and `capped`, which is the whole
    // honesty of the feature and is only trustworthy once the scan is over.
    if let Some(state) = app.try_state::<AppState>() {
        state.sqls.forget(sql_id);
    }
    report_session_serde_errors(app, profile_id);
    let mut final_progress = session.progress();
    // Forced rather than read, for the one case where it would be false: a loop
    // that left early because an emit failed. As far as anything downstream is
    // concerned this query is over, and the contract is that the last progress
    // event says so.
    final_progress.done = true;
    let _ = app.emit(&progress_event, final_progress);
}

/// Starts a query and answers with the id its three channels are named for.
///
/// Everything a user can get wrong fails here rather than as a query that ends
/// a moment later having answered nothing: `SqlSession::start` parses and plans
/// the SQL — and refuses every write statement — **before a single broker
/// call**, then resolves partitions and watermarks on the calling thread, which
/// is the blocking pool because all of that talks to librdkafka.
#[tauri::command]
async fn sql_start(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    spec: SqlSpec,
) -> CmdResult<String> {
    let conn = state.connection(&profile_id)?;
    // Before the scan starts, for the reason `tail_start` gives.
    state.mask_set(&profile_id).map_err(|e| e.to_string())?;
    let session = blocking(move || {
        // The registry is the profile's, not a global — same rule as search:
        // `key_text`/`value_text` are decoded values, and two clusters can have
        // different registries or none.
        SqlSession::start(&conn, conn.profile().schema_registry.as_ref(), &spec).map(Arc::new)
    })
    .await?;

    let sql_id = state.next_id();
    state.sqls.insert(sql_id.clone(), &profile_id, &session);
    // Armed before the id leaves this function, so a `session_ready` racing the
    // emitter's own start has a gate to open.
    state.arm_ready(&sql_id);

    let spawned = spawn_emitter("kavka-sql-emit", {
        let sql_id = sql_id.clone();
        let profile_id = profile_id.clone();
        move || pump_sql(&app, &sql_id, &profile_id, &session)
    });
    if let Err(e) = spawned {
        // Nothing will ever drain this session, and a query nobody drains is a
        // consumer fetching at full speed into an Arrow buffer.
        state.disarm_ready(&sql_id);
        if let Some(orphan) = state.sqls.take(&sql_id) {
            retire(orphan).await?;
        }
        return Err(format!("starting the SQL reader: {e}"));
    }
    Ok(sql_id)
}

/// Idempotent: an unknown id is `Ok(())`. The UI stops a query when the view
/// unmounts, when `Esc` cancels it, and again when the final progress says
/// `done` — none of those is a mistake worth an error.
#[tauri::command]
async fn sql_stop(state: State<'_, AppState>, sql_id: String) -> CmdResult<()> {
    match state.sqls.take(&sql_id) {
        Some(session) => retire(session).await,
        None => Ok(()),
    }
}

// ── Produce ────────────────────────────────────────────────────────────────

/// Mutating: `produce::send` calls `ensure_writable` before it encodes a value,
/// asks a Schema Registry anything or opens a socket, so read-only is enforced
/// in core and not here (D5).
#[tauri::command]
async fn produce_send(
    state: State<'_, AppState>,
    profile_id: String,
    topic: String,
    record: ProduceRecordSpec,
) -> CmdResult<Delivery> {
    let conn = state.connection(&profile_id)?;
    blocking(move || {
        produce::send(
            &conn,
            conn.profile().schema_registry.as_ref(),
            &topic,
            &record,
        )
    })
    .await
}

/// The event name one bulk run's progress is addressed to.
fn bulk_event(bulk_id: &str) -> String {
    format!("kavka://bulk/{bulk_id}")
}

/// One bulk run's reporter loop. Runs on its own thread — see [`spawn_emitter`].
fn pump_bulk(app: &AppHandle, bulk_id: &str, session: &BulkSession) {
    let event = bulk_event(bulk_id);

    // The same handshake as a search, and it matters more here: 500 records at
    // interval 0 finish long before the panel has subscribed, and a `done`
    // emitted into that window leaves it counting forever.
    let mut last = Instant::now();
    await_subscriber(app, bulk_id, "bulk produce");

    // A poll, not a blocking read: progress is a snapshot of counters the
    // delivery callbacks write.
    loop {
        let progress = session.progress();
        if progress.done {
            break;
        }
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            if let Err(e) = app.emit(&event, progress) {
                // Nobody can receive this run any more. It is a run that WRITES,
                // so it stops rather than finishing unobserved.
                tracing::warn!("bulk produce {bulk_id}: {e}");
                session.stop();
                break;
            }
        }
        std::thread::sleep(BULK_POLL);
    }

    // Forgotten first, so a `bulk_stop` racing the last payload is the no-op it
    // claims to be, then the final counts — which are the acknowledged ones,
    // because `stop` flushes what librdkafka already accepted.
    if let Some(state) = app.try_state::<AppState>() {
        state.bulks.forget(bulk_id);
    }
    let mut final_progress = session.progress();
    final_progress.done = true;
    let _ = app.emit(&event, final_progress);
}

/// Mutating. Answers with the id its progress events are addressed to.
///
/// Read-only, both templates, the topic and the partition are all checked in
/// `BulkSession::start`, on this side of the id: a run that returns an id is a
/// run that has begun. A template that only failed on record 40 000 would leave
/// 39 999 records of garbage in a topic.
#[tauri::command]
async fn produce_bulk(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    topic: String,
    spec: BulkSpec,
) -> CmdResult<String> {
    let conn = state.connection(&profile_id)?;
    let session = blocking(move || BulkSession::start(&conn, &topic, &spec).map(Arc::new)).await?;

    let bulk_id = state.next_id();
    state.bulks.insert(bulk_id.clone(), &profile_id, &session);
    state.arm_ready(&bulk_id);

    let spawned = spawn_emitter("kavka-bulk-emit", {
        let bulk_id = bulk_id.clone();
        move || pump_bulk(&app, &bulk_id, &session)
    });
    if let Err(e) = spawned {
        // Nobody would ever report this run, and it is a run that writes.
        state.disarm_ready(&bulk_id);
        if let Some(orphan) = state.bulks.take(&bulk_id) {
            retire(orphan).await?;
        }
        return Err(format!("starting the bulk produce reporter: {e}"));
    }
    Ok(bulk_id)
}

/// Idempotent. Stopping flushes what librdkafka has already accepted so the
/// final counts are true rather than merely prompt — and that flush happens in
/// `retire`, on the blocking pool, never on the event loop.
#[tauri::command]
async fn bulk_stop(state: State<'_, AppState>, bulk_id: String) -> CmdResult<()> {
    match state.bulks.take(&bulk_id) {
        Some(session) => retire(session).await,
        None => Ok(()),
    }
}

// ── Cross-cluster: copy, config diff, offset migration ─────────────────────
//
// The one family of commands with TWO connections. Every rule about the second
// one is in [`connection_or_open`]; the rules that belong to the commands are:
//
//  - **The command's own `profile_id` is the SOURCE**, and it must already be
//    connected, like every other command in this file. The destination is
//    named in the request and is opened on demand.
//  - **Read-only is the destination's, and the core decides it.** `copy_start`
//    and `offsets_migrate_apply` both call `ensure_writable` on the connection
//    they are about to WRITE to, which is not the one the command was addressed
//    to — a writable source cannot copy into a read-only destination (D5).
//  - **A copy is on the books under both profiles**, so disconnecting either
//    end stops it — see [`SessionEntry::dest_profile_id`].

/// The event name one copy's progress is addressed to.
fn copy_event(copy_id: &str) -> String {
    format!("kavka://copy/{copy_id}")
}

/// One copy's reporter loop. Runs on its own thread — see [`spawn_emitter`].
fn pump_copy(app: &AppHandle, copy_id: &str, session: &CopySession) {
    let event = copy_event(copy_id);

    // The same handshake as a bulk run, and it matters for the same reason: a
    // copy of forty records finishes long before the wizard has subscribed, and
    // a `done` emitted into that window leaves it counting forever over a write
    // that has already happened.
    let mut last = Instant::now();
    await_subscriber(app, copy_id, "copy");

    // A poll, not a blocking read: `copied` is written by delivery callbacks on
    // librdkafka's thread, so progress is a snapshot of counters.
    loop {
        let progress = session.progress();
        if progress.done {
            break;
        }
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            if let Err(e) = app.emit(&event, progress) {
                // Nobody can receive this copy any more. It is a run that
                // WRITES, so it stops rather than finishing unobserved.
                tracing::warn!("copy {copy_id}: {e}");
                session.stop();
                break;
            }
        }
        std::thread::sleep(COPY_POLL);
    }

    // Forgotten first, so a `copy_stop` racing the last payload is the no-op it
    // claims to be, then the final counts — which are the acknowledged ones,
    // because the reader flushes what librdkafka already accepted before it
    // sets `done`.
    if let Some(state) = app.try_state::<AppState>() {
        state.copies.forget(copy_id);
    }
    let mut final_progress = session.progress();
    final_progress.done = true;
    let _ = app.emit(&event, final_progress);
}

/// Read-only, and the destination is never contacted: the estimate is
/// watermark arithmetic over the source alone.
///
/// `estimate_only` in the answer is what says the number is a scan size rather
/// than a match count — the core sets it whenever a filter is in play, and no
/// screen may render the number without it (docs/DESIGN.md §7 rule 5).
#[tauri::command]
async fn copy_dry_run(
    state: State<'_, AppState>,
    profile_id: String,
    spec: CopySpec,
) -> CmdResult<CopyEstimate> {
    let conn = state.connection(&profile_id)?;
    blocking(move || xcluster::copy_dry_run(&conn, &spec)).await
}

/// Mutating — **at the destination**. Answers with the id its progress events
/// are addressed to.
///
/// Read-only at the destination, a filter that does not compile, a partition
/// mapping that cannot work and a topic that is not there are all decided in
/// `CopySession::start`, on this side of the id: a copy that returns an id is a
/// copy that has begun. Discovering the destination has four partitions on
/// record 40 000 would leave 40 000 records already written.
#[tauri::command]
async fn copy_start(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    spec: CopySpec,
) -> CmdResult<String> {
    let source = state.connection(&profile_id)?;
    let dest_profile_id = spec.dest_profile_id.clone();
    let dest = connection_or_open(&state, &dest_profile_id).await?;

    let session = blocking(move || {
        // `dest` is captured by this closure and destroyed with it — on the
        // blocking pool, and only if nothing else holds it (see
        // `connection_or_open`). The session does not need it to survive: it
        // built its own producer from the profile inside `start`.
        CopySession::start(&source, &dest, &spec).map(Arc::new)
    })
    .await?;

    let copy_id = state.next_id();
    // Under BOTH profiles: disconnecting the destination has to stop a copy
    // that is writing into it, and the destination's workspace is not the one
    // this session was started from.
    state.copies.insert_between(
        copy_id.clone(),
        &profile_id,
        Some(&dest_profile_id),
        &session,
    );
    state.arm_ready(&copy_id);

    let spawned = spawn_emitter("kavka-copy-emit", {
        let copy_id = copy_id.clone();
        move || pump_copy(&app, &copy_id, &session)
    });
    if let Err(e) = spawned {
        // Nobody would ever report this copy, and it is a copy that writes.
        state.disarm_ready(&copy_id);
        if let Some(orphan) = state.copies.take(&copy_id) {
            retire(orphan).await?;
        }
        return Err(format!("starting the copy reporter: {e}"));
    }
    Ok(copy_id)
}

/// Idempotent. Records the destination has already acknowledged stay written —
/// stopping flushes what librdkafka accepted so the final counts describe the
/// destination rather than our intentions, and that flush happens in `retire`,
/// on the blocking pool, never on the event loop.
#[tauri::command]
async fn copy_stop(state: State<'_, AppState>, copy_id: String) -> CmdResult<()> {
    match state.copies.take(&copy_id) {
        Some(session) => retire(session).await,
        None => Ok(()),
    }
}

/// **v1 is topics only**, and this is where that is enforced.
///
/// The wire type accepts `null` for either side because the contract's shape
/// allows a broker-level diff; the answer is a sentence rather than a silent
/// empty table, and it names the screen that does have broker configs. The
/// check is here rather than in the core because the core takes two lists of
/// config entries and has no opinion about where they came from.
fn diff_topic(topic: Option<&str>, side: &str) -> CmdResult<String> {
    match topic.map(str::trim) {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        _ => Err(format!(
            "Kavka compares the configuration of two TOPICS, and side {side} names none. A \
             broker-level diff isn't something this screen does — the Brokers tab shows each \
             broker's configuration in full."
        )),
    }
}

/// Read-only on both sides: two `DescribeConfigs` calls and a pure comparison.
///
/// The verdict is the core's ([`xcluster::config_diff`]), not a string
/// comparison here — one rule decides `differs` for every caller, so the row
/// that is highlighted and the row that is counted cannot come to disagree.
#[tauri::command]
async fn config_diff(
    state: State<'_, AppState>,
    profile_id_a: String,
    topic_a: Option<String>,
    profile_id_b: String,
    topic_b: Option<String>,
) -> CmdResult<Vec<ConfigDiffRow>> {
    // Before either connection is looked at, let alone opened: a missing topic
    // name is a mistake about the request, not about a cluster.
    let topic_a = diff_topic(topic_a.as_deref(), "A")?;
    let topic_b = diff_topic(topic_b.as_deref(), "B")?;
    // Both sides are peers here — neither is "the workspace" — so both may be
    // opened on demand. This is the one command where that is true of side A.
    let conn_a = connection_or_open(&state, &profile_id_a).await?;
    let conn_b = match connection_or_open(&state, &profile_id_b).await {
        Ok(conn_b) => conn_b,
        Err(e) => {
            // The only place in this file where a connection is held across a
            // second fallible step. If side A was opened *here*, this is its
            // last reference and dropping it is a librdkafka client teardown —
            // which never happens on the event loop (see the file header). An
            // `Arc` the user's own connection map still holds drops for free,
            // so this costs a task and nothing else.
            tauri::async_runtime::spawn_blocking(move || drop(conn_a));
            return Err(e);
        }
    };
    blocking(move || {
        let a = admin::topic_detail(&conn_a, &topic_a)?.configs;
        let b = admin::topic_detail(&conn_b, &topic_b)?.configs;
        Ok(xcluster::config_diff(&a, &b))
    })
    .await
}

/// Read-only, on both clusters. Works out where each of the source group's
/// partitions would land on the destination, and says how it worked each one
/// out — including the rows with nowhere to go.
///
/// A plan is a snapshot: both clusters keep moving, so nothing here is cached
/// and the modal re-plans rather than holding one.
#[tauri::command]
async fn offsets_migrate_plan(
    state: State<'_, AppState>,
    profile_id: String,
    group_id: String,
    topic: String,
    dest_profile_id: String,
    dest_group_id: String,
    dest_topic: String,
) -> CmdResult<Vec<OffsetMigrationRow>> {
    let source = state.connection(&profile_id)?;
    let dest = connection_or_open(&state, &dest_profile_id).await?;
    blocking(move || {
        xcluster::offsets_migrate_plan(
            &source,
            &group_id,
            &topic,
            &dest,
            &dest_group_id,
            &dest_topic,
        )
    })
    .await
}

/// Mutating, **at the destination** — which is the only cluster this one
/// touches, so it is addressed to the destination profile directly rather than
/// to a workspace.
///
/// Guarded in core with `offsets_reset`'s own vocabulary: read-only refuses
/// first, then a destination group that exists and is not `Empty`. There is no
/// `force`, deliberately.
#[tauri::command]
async fn offsets_migrate_apply(
    state: State<'_, AppState>,
    dest_profile_id: String,
    dest_group_id: String,
    dest_topic: String,
    plan: Vec<OffsetMigrationRow>,
) -> CmdResult<Vec<GroupOffset>> {
    let dest = connection_or_open(&state, &dest_profile_id).await?;
    blocking(move || xcluster::offsets_migrate_apply(&dest, &dest_group_id, &dest_topic, &plan))
        .await
}

// ── ACLs ───────────────────────────────────────────────────────────────────

#[tauri::command]
async fn acls_list(
    state: State<'_, AppState>,
    profile_id: String,
    filter: AclFilter,
) -> CmdResult<Vec<AclBinding>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::acl::acls_list(&conn, &filter)).await
}

/// Mutating: guarded in core (see `topic_create`). Not transactional — Kafka
/// can write some bindings and reject others, and the core's error names the
/// binding that failed rather than reporting a code for the batch.
#[tauri::command]
async fn acls_create(
    state: State<'_, AppState>,
    profile_id: String,
    bindings: Vec<AclBinding>,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::acl::acls_create(&conn, &bindings)).await
}

/// Mutating: guarded in core (see `topic_create`). Resolves with the bindings
/// that were *actually* removed, which is the only way the UI can tell "gone"
/// from "was already gone" — an empty answer means the filter matched nothing
/// and the click changed the cluster not at all.
///
/// The filter is an `AclBinding` because Kafka's own delete API takes an
/// ACL-shaped filter; blanks read as "any" there, which they do not when
/// creating. The core owns that asymmetry.
#[tauri::command]
async fn acls_delete(
    state: State<'_, AppState>,
    profile_id: String,
    filter: AclBinding,
) -> CmdResult<Vec<AclBinding>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::acl::acls_delete(&conn, &filter)).await
}

// ── Broker configuration ───────────────────────────────────────────────────

#[tauri::command]
async fn broker_configs(
    state: State<'_, AppState>,
    profile_id: String,
    broker_id: i32,
) -> CmdResult<Vec<ConfigEntry>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::broker_configs(&conn, broker_id)).await
}

/// Mutating: guarded in core (see `topic_create`), and *incremental* — only the
/// named key is touched. `value: None` deletes the dynamic override so the
/// broker falls back to what it computes as the default; it is "revert", never
/// "set to empty", and the two are different requests with different outcomes.
#[tauri::command]
async fn broker_config_set(
    state: State<'_, AppState>,
    profile_id: String,
    broker_id: i32,
    name: String,
    value: Option<String>,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || admin::broker_config_set(&conn, broker_id, &name, value.as_deref())).await
}

// ── Quorum, leader election, replica moves, quotas ─────────────────────────
//
// These seven answer over the core's hand-rolled Kafka wire client
// (`kavka_core::protocol`, docs/ARCHITECTURE.md D2) instead of librdkafka,
// which wraps none of ApiKeys 43, 45, 46, 48/49 and 55. That is invisible from
// here and meant to stay so: the same `blocking()`, the same `CmdResult`, and
// broker errors arrive already mapped into the classified vocabulary the rest
// of the UI reads — no command in this file knows which transport answered it.
//
// **One kept-alive protocol connection per profile**, held in `AppState` and
// reached through `protocol_call` / `protocol_write` — which is where the
// caching, the per-profile serialization and the invalidation rules are all
// documented. The short version: these calls are not all one-offs. The
// reassignment monitor polls `reassign_list` every two seconds, and paying a
// TCP connect, a TLS handshake, ApiVersions and a full SASL exchange per poll
// (four round trips and a PBKDF2 derivation, for SCRAM) is a login every two
// seconds for a call that reads one small list.
//
// An unconnected cluster is still refused by `state.connection` before anything
// is opened, exactly as everywhere else, and the four mutating ones still check
// `ensure_writable` in the core BEFORE a socket exists (D5): a read-only
// connection must not so much as authenticate on behalf of a write.

/// The KRaft metadata quorum as its leader sees it — voters, observers, and how
/// far behind each one is.
///
/// DescribeQuorum is controller-bound; the core does the routing, so nothing
/// here has to know which broker ends up answering. A cluster with no quorum to
/// describe (ZooKeeper) is refused with a sentence saying so, not a protocol
/// error.
#[tauri::command]
async fn quorum_describe(state: State<'_, AppState>, profile_id: String) -> CmdResult<QuorumInfo> {
    protocol_call(&state, &profile_id, ProtocolClient::quorum_describe).await
}

/// Mutating: guarded in core (see `topic_create`). `topic: None` covers every
/// eligible partition in the cluster; a topic with `partitions: None` covers
/// all of that topic's.
///
/// Resolves with a row per partition rather than failing the call: Kafka can
/// elect some and refuse others, and one rejected promise would lose exactly
/// the half the user needs to see. A partition that already had its preferred
/// leader is a success (`error: null`) — the core folds Kafka's
/// ELECTION_NOT_NEEDED into the benign set, because on a healthy cluster it is
/// the answer for nearly every partition.
#[tauri::command]
async fn elect_leaders(
    state: State<'_, AppState>,
    profile_id: String,
    topic: Option<String>,
    partitions: Option<Vec<i32>>,
) -> CmdResult<Vec<PartitionResult>> {
    protocol_write(&state, &profile_id, "elect_leaders", move |client| {
        client.elect_leaders(topic.as_deref(), partitions.as_deref())
    })
    .await
}

/// Mutating: guarded in core (see `topic_create`). Resolves when the cluster
/// has ACCEPTED the plan, not when the data has moved — the copying runs in the
/// background and is watched with `reassign_list`.
///
/// Per-partition outcomes, for the same reason as `elect_leaders`: a plan can
/// be half accepted, and that is a real state the cluster ends up in.
#[tauri::command]
async fn reassign_alter(
    state: State<'_, AppState>,
    profile_id: String,
    specs: Vec<ReassignmentSpec>,
) -> CmdResult<Vec<PartitionResult>> {
    protocol_write(&state, &profile_id, "reassign_alter", move |client| {
        client.reassign_alter(&specs)
    })
    .await
}

/// Mutating: guarded in core (see `topic_create`). Reverts each partition to
/// the replicas it had before the move, discarding whatever the joining brokers
/// had already copied.
///
/// A partition that was not moving is a success (`error: null`): Kafka's
/// NO_REASSIGNMENT_IN_PROGRESS is in the core's benign set, because "the
/// requested end state already holds" is not a failure.
#[tauri::command]
async fn reassign_cancel(
    state: State<'_, AppState>,
    profile_id: String,
    parts: Vec<TopicPartition>,
) -> CmdResult<Vec<PartitionResult>> {
    protocol_write(&state, &profile_id, "reassign_cancel", move |client| {
        client.reassign_cancel(&parts)
    })
    .await
}

/// The moves still in flight; `topic: None` asks about the whole cluster. An
/// empty answer means the cluster is settled — a finished partition stops
/// appearing rather than reporting completion.
///
/// A NAMED topic is resolved to its partition indexes first (Kafka's request
/// has no "all partitions of this topic" shorthand — see the core), so a topic
/// that does not exist is an error naming it rather than a cheerful empty list.
#[tauri::command]
async fn reassign_list(
    state: State<'_, AppState>,
    profile_id: String,
    topic: Option<String>,
) -> CmdResult<Vec<ReassignmentState>> {
    protocol_call(&state, &profile_id, move |client| {
        client.reassign_list(topic.as_deref())
    })
    .await
}

/// Every client quota the cluster holds, `<default>` entities included.
#[tauri::command]
async fn quotas_list(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<Vec<QuotaEntity>> {
    protocol_call(&state, &profile_id, ProtocolClient::quotas_list).await
}

/// Mutating: guarded in core (see `topic_create`), and *incremental* — keys not
/// named in `ops` are left alone. An op with `value: None` REMOVES that quota;
/// it is never "set to zero", which would be a throttle to a standstill.
///
/// Note for callers: the controller commits this, but a subsequent
/// `quotas_list` reads the broker's metadata image, so a set is visible
/// eventually rather than immediately.
#[tauri::command]
async fn quotas_alter(
    state: State<'_, AppState>,
    profile_id: String,
    entity: Vec<QuotaEntityPart>,
    ops: Vec<QuotaOp>,
) -> CmdResult<()> {
    protocol_write(&state, &profile_id, "quotas_alter", move |client| {
        client.quotas_alter(&entity, &ops)
    })
    .await
}

// ── Kafka Connect ──────────────────────────────────────────────────────────
//
// Every call names the Connect cluster it goes to: a profile can hold several
// worker groups (a source cluster and a sink cluster is the ordinary shape),
// and the core resolves the name — and its keychain credentials — off the
// profile the connection was opened with. An unknown name is answered with the
// list of real ones, because the failure mode is a stale UI or a rename.

#[tauri::command]
async fn connect_list(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
) -> CmdResult<Vec<ConnectorSummary>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::list(&conn, &cluster)).await
}

#[tauri::command]
async fn connect_config(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    name: String,
) -> CmdResult<BTreeMap<String, String>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::config(&conn, &cluster, &name)).await
}

/// Not mutating, deliberately: validation runs the candidate config through the
/// plugin's own config definition on the worker and creates nothing. It is what
/// lets the form say what is wrong *before* anyone commits to a connector, so a
/// read-only connection can still use it.
#[tauri::command]
async fn connect_validate(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    connector_class: String,
    config: BTreeMap<String, String>,
) -> CmdResult<ConfigValidation> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::validate(&conn, &cluster, &connector_class, &config))
        .await
}

/// Mutating: guarded in core (see `topic_create`). Create-or-update — Connect's
/// `PUT /connectors/{name}/config` is one verb for both, and so is this.
#[tauri::command]
async fn connect_apply(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    name: String,
    config: BTreeMap<String, String>,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::apply(&conn, &cluster, &name, &config)).await
}

/// Mutating: guarded in core (see `topic_create`).
#[tauri::command]
async fn connect_delete(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    name: String,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::delete(&conn, &cluster, &name)).await
}

/// Mutating: guarded in core (see `topic_create`). `task` picks a single task;
/// with `None`, `include_tasks` decides whether the restart carries the
/// connector's tasks with it or is the connector instance alone.
#[tauri::command]
async fn connect_restart(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    name: String,
    task: Option<i32>,
    include_tasks: bool,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::restart(&conn, &cluster, &name, task, include_tasks))
        .await
}

/// Mutating: guarded in core (see `topic_create`).
#[tauri::command]
async fn connect_pause(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    name: String,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::pause(&conn, &cluster, &name)).await
}

/// Mutating: guarded in core (see `topic_create`).
#[tauri::command]
async fn connect_resume(
    state: State<'_, AppState>,
    profile_id: String,
    cluster: String,
    name: String,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::connect::resume(&conn, &cluster, &name)).await
}

// ── Schema Registry ────────────────────────────────────────────────────────
//
// The registry is the *profile's*, resolved inside the core rather than passed
// in: two clusters can have different registries and one of them can have none,
// and a connection with no registry gets a message saying so instead of a
// connection error. The mutating pair build the client only after the
// read-only gate, so a read-only profile costs no keychain read and no socket.

#[tauri::command]
async fn sr_subject_versions(
    state: State<'_, AppState>,
    profile_id: String,
    subject: String,
) -> CmdResult<Vec<SubjectVersion>> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::sr::subject_versions(&conn, &subject)).await
}

#[tauri::command]
async fn sr_check_compat(
    state: State<'_, AppState>,
    profile_id: String,
    subject: String,
    schema: String,
    schema_type: String,
) -> CmdResult<CompatibilityCheck> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::sr::check_compatibility(&conn, &subject, &schema, &schema_type))
        .await
}

/// Mutating: guarded in core (see `topic_create`), which also runs the
/// compatibility check against the registry itself before it writes. A UI that
/// skipped the check button therefore still cannot sneak an incompatible schema
/// past the subject's level, and the registry's own refusal is what surfaces.
#[tauri::command]
async fn sr_register(
    state: State<'_, AppState>,
    profile_id: String,
    subject: String,
    schema: String,
    schema_type: String,
) -> CmdResult<RegisteredId> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::sr::register(&conn, &subject, &schema, &schema_type)).await
}

/// `subject: None` reads the registry-wide default; a subject with no level of
/// its own falls back to it, and the answer says which of the two happened
/// (`inherited`). The UI has a sentence for each, and cannot pick one from a
/// bare level string.
#[tauri::command]
async fn sr_get_compat(
    state: State<'_, AppState>,
    profile_id: String,
    subject: Option<String>,
) -> CmdResult<CompatibilityInForce> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::sr::compatibility(&conn, subject.as_deref())).await
}

/// Mutating: guarded in core (see `topic_create`). `subject: None` sets the
/// registry-wide default — every subject without a level of its own.
#[tauri::command]
async fn sr_set_compat(
    state: State<'_, AppState>,
    profile_id: String,
    subject: Option<String>,
    level: String,
) -> CmdResult<()> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::sr::set_compatibility(&conn, subject.as_deref(), &level)).await
}

// ── Lag history ────────────────────────────────────────────────────────────
//
// Read straight off the disk store rather than through the connection, and that
// is the whole point: the samples are Kavka's, not the cluster's, so a profile
// nobody is connected to still has last week to show. Neither command asks
// `AppState::connection` for anything, and neither one can fail because a
// broker is down.

/// One group's lag over a window, downsampled in core to at most `max_points`
/// **per partition** — keeping the highest-lag point in each bucket, so a spike
/// that lasted one sample survives every zoom level.
#[tauri::command]
async fn history_query(
    state: State<'_, AppState>,
    profile_id: String,
    group_id: String,
    topic: Option<String>,
    from_ms: i64,
    to_ms: i64,
    max_points: u32,
) -> CmdResult<Vec<LagSample>> {
    let histories = Arc::clone(&state.histories);
    blocking(move || {
        histories
            .get(&profile_id)?
            .query(&group_id, topic.as_deref(), from_ms, to_ms, max_points)
    })
    .await
}

/// Which groups this profile has history for, and the window each one covers.
#[tauri::command]
async fn history_groups(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<Vec<GroupWindow>> {
    let histories = Arc::clone(&state.histories);
    blocking(move || histories.get(&profile_id)?.groups()).await
}

/// What the sampler is doing for this profile right now.
///
/// A profile with no monitor is not an error — it is a profile nobody is
/// connected to, and `running: false` is the honest answer. The interval still
/// comes off the profile so a screen can say how often sampling *would* happen
/// without being wrong about a cluster configured to sample every minute.
#[tauri::command]
async fn sampler_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<SamplerStatus> {
    if let Some(monitor) = state.monitor(&profile_id) {
        return Ok(monitor.status());
    }
    let store = state.store.clone();
    blocking(move || {
        let interval_ms = store
            .list()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .and_then(|profile| profile.sampler_interval_ms)
            .map_or(history::DEFAULT_INTERVAL_MS, history::clamp_interval_ms);
        Ok(SamplerStatus {
            running: false,
            interval_ms,
            last_sample_ms: None,
            last_error: None,
        })
    })
    .await
}

// ── Broker metrics ─────────────────────────────────────────────────────────
//
// In memory only, and only while the connection is open — the collector's
// 24-hour ring belongs to the monitor. A profile with no monitor therefore has
// no readings, which is a state to describe rather than an error to raise: the
// panel has four states to tell apart (never configured / configured but not
// scraping / unreachable / answering with nothing Kavka recognises) and
// collapsing any of them into "no data" sends somebody to debug the wrong
// machine.

/// One series over a window. An unknown series — or a profile that is not
/// being scraped — is empty rather than an error, for the same reason the core
/// answers an unknown series with no points: a saved chart naming a series a
/// rebuilt exporter no longer exposes should go blank, not break the screen.
#[tauri::command]
async fn metrics_query(
    state: State<'_, AppState>,
    profile_id: String,
    series: String,
    from_ms: i64,
    to_ms: i64,
    max_points: u32,
) -> CmdResult<Vec<MetricPoint>> {
    let Some(collector) = state
        .monitor(&profile_id)
        .and_then(|monitor| monitor.metrics.clone())
    else {
        return Ok(Vec::new());
    };
    // On the blocking pool rather than inline: the collector's window is behind
    // a lock the monitor thread holds while it folds a scrape.
    blocking(move || Ok(collector.query(&series, from_ms, to_ms, max_points))).await
}

#[tauri::command]
async fn metrics_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<MetricsStatus> {
    if let Some(monitor) = state.monitor(&profile_id) {
        return match monitor.metrics.clone() {
            Some(collector) => blocking(move || Ok(collector.status())).await,
            // Connected, and this cluster simply has no endpoint. Lag history
            // works without one.
            None => Ok(MetricsStatus::unconfigured()),
        };
    }
    let store = state.store.clone();
    blocking(move || {
        let configured = store
            .list()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .and_then(|profile| profile.metrics_endpoint)
            .is_some();
        if !configured {
            return Ok(MetricsStatus::unconfigured());
        }
        Ok(MetricsStatus {
            configured: true,
            reachable: false,
            last_scrape_ms: None,
            last_error: Some(
                "Kavka scrapes this endpoint only while the connection is open. Connect to this \
                 cluster to start collecting."
                    .into(),
            ),
            series_available: Vec::new(),
        })
    })
    .await
}

// ── Alerts ─────────────────────────────────────────────────────────────────
//
// Local throughout: rules, channels and incident history live in `alerts.json`
// beside `profiles.json`, and none of these commands touches a cluster. That is
// why none of them asks for a connection and none of them is read-only-gated —
// watching a cluster you are not allowed to write to is the ordinary case.

#[tauri::command]
async fn alerts_list(state: State<'_, AppState>, profile_id: String) -> CmdResult<Vec<AlertRule>> {
    let alerts = Arc::clone(&state.alerts);
    blocking(move || alerts.rules(&profile_id)).await
}

/// Upsert by `rule.id` — the editor's "save" is one call whether the rule is
/// new or not. The running monitor re-reads the rules every tick, so a saved
/// rule is in force at the next sample without a reconnect.
#[tauri::command]
async fn alerts_save(
    state: State<'_, AppState>,
    profile_id: String,
    rule: AlertRule,
) -> CmdResult<()> {
    let alerts = Arc::clone(&state.alerts);
    blocking(move || alerts.save_rule(&profile_id, rule)).await
}

#[tauri::command]
async fn alerts_delete(
    state: State<'_, AppState>,
    profile_id: String,
    rule_id: String,
) -> CmdResult<()> {
    let alerts = Arc::clone(&state.alerts);
    blocking(move || alerts.delete_rule(&profile_id, &rule_id)).await
}

/// The most recent incidents, newest first. A fire and its resolution are one
/// row, not two — `resolved_ms` is what tells them apart.
#[tauri::command]
async fn alerts_history(
    state: State<'_, AppState>,
    profile_id: String,
    limit: u32,
) -> CmdResult<Vec<AlertEvent>> {
    let alerts = Arc::clone(&state.alerts);
    blocking(move || alerts.history(&profile_id, limit)).await
}

#[tauri::command]
async fn alerts_channels_get(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<AlertChannels> {
    let alerts = Arc::clone(&state.alerts);
    blocking(move || alerts.channels(&profile_id)).await
}

#[tauri::command]
async fn alerts_channels_set(
    state: State<'_, AppState>,
    profile_id: String,
    channels: AlertChannels,
) -> CmdResult<()> {
    let alerts = Arc::clone(&state.alerts);
    blocking(move || alerts.set_channels(&profile_id, channels)).await
}

/// Sends one test firing through whatever channels this profile has.
///
/// **Not in the Phase 4 IPC contract — added deliberately rather than
/// smuggled.** A webhook is the one part of alerting that cannot be verified
/// after the fact: the wrong body shape at the right URL fails silently at the
/// far end, and the failure surfaces during the incident the alert was for. The
/// only honest test is the request Kavka will really send, from the process that
/// will really send it, which is this one.
///
/// The test event is delivered and **not recorded**: it is not an incident, and
/// a history that lists it is a history somebody has to learn to discount. The
/// webhook's refusal is returned rather than logged, because here — unlike in
/// the loop — somebody is standing in front of it waiting to be told.
#[tauri::command]
async fn alerts_channels_test(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<()> {
    let alerts = Arc::clone(&state.alerts);
    let id = profile_id.clone();
    let channels = blocking(move || alerts.channels(&id)).await?;

    let has_webhook = channels
        .webhook_url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    if !channels.os_notification && !has_webhook {
        return Err(
            "There's nowhere to send a test yet. Turn on the desktop notification, or \
                    add a webhook address, then test again."
                .into(),
        );
    }

    let event = AlertEvent {
        rule_id: "kavka-test".into(),
        rule_name: "Test alert".into(),
        fired_ms: history::now_ms(),
        resolved_ms: None,
        detail: "Kavka sent this from the alert settings. No rule fired.".into(),
    };
    if channels.os_notification {
        notify(&app, &event);
    }
    if !has_webhook {
        return Ok(());
    }
    // Off the event loop: a webhook that does not answer holds this for the
    // core's five-second timeout.
    let refused = tauri::async_runtime::spawn_blocking(move || alerts::deliver(&channels, &event))
        .await
        .map_err(|e| e.to_string())?;
    match refused {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

// ── Share groups (KIP-932) ─────────────────────────────────────────────────
//
// Wire protocol, not librdkafka: these three APIs have no client-library
// binding, and the core's negotiation refuses by naming the broker feature
// (`share.version`) rather than reporting an unknown API key.

#[tauri::command]
async fn share_groups_list(
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<Vec<ShareGroupInfo>> {
    protocol_call(&state, &profile_id, ProtocolClient::share_groups_list).await
}

#[tauri::command]
async fn share_group_detail(
    state: State<'_, AppState>,
    profile_id: String,
    group_id: String,
) -> CmdResult<ShareGroupDetail> {
    protocol_call(&state, &profile_id, move |client| {
        client.share_group_detail(&group_id)
    })
    .await
}

// ── Kafka Streams topology (inferred) ──────────────────────────────────────

/// The shape of the Streams application behind one consumer group, as far as it
/// can be worked out.
///
/// Two ordinary reads — the group's members and the cluster's topic list — and
/// then arithmetic on Kafka's internal-topic naming convention. Nothing about a
/// Streams topology is published by a broker, so the answer carries `inferred:
/// true` and its own list of what the inference cannot know, and the view is
/// required to show every caveat.
#[tauri::command]
async fn streams_topology(
    state: State<'_, AppState>,
    profile_id: String,
    group_id: String,
) -> CmdResult<StreamsTopology> {
    let conn = state.connection(&profile_id)?;
    blocking(move || kavka_core::streams::topology(&conn, &group_id)).await
}

// ── Export ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ExportFormat {
    Csv,
    Json,
    Ndjson,
}

fn export_format(format: &str) -> kavka_core::Result<ExportFormat> {
    match format {
        "csv" => Ok(ExportFormat::Csv),
        "json" => Ok(ExportFormat::Json),
        "ndjson" => Ok(ExportFormat::Ndjson),
        other => Err(kavka_core::Error::Other(format!(
            "Kavka can't export as {other:?} — it writes csv, json and ndjson."
        ))),
    }
}

/// Writes the records the user is looking at to the file they picked.
///
/// The path is trusted: it came from the OS save dialog, which is the user's own
/// consent, and the dialog has already asked about overwriting. Nothing else is
/// trusted — the format is parsed **before** the file is opened, so a bad format
/// cannot truncate a file the user already had.
///
/// **The records arrive already masked** — masking happens on the way OUT of
/// this process, so the text the webview holds is the text this writes and
/// there is nothing here to redact. What this adds is the notice: see
/// [`mask_notice_for`], and `kavka_core::masking::mask_notice` for the sentence
/// and why a masked file that does not say so is the failure worth preventing.
#[tauri::command]
async fn export_records(
    path: String,
    format: String,
    records: Vec<MessageRecord>,
) -> CmdResult<()> {
    blocking(move || write_export(&path, &format, &records)).await
}

fn write_export(path: &str, format: &str, records: &[MessageRecord]) -> kavka_core::Result<()> {
    let format = export_format(format)?;
    let notice = mask_notice_for(records);
    let file = std::fs::OpenOptions::new()
        // Spelled out rather than `File::create`: this command overwrites what
        // the user pointed it at, and that is worth saying in the code that
        // does it.
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| file_trouble(path, &e))?;

    let mut out = std::io::BufWriter::new(file);
    let written = match format {
        ExportFormat::Csv => write_csv(&mut out, records, notice.as_deref()),
        ExportFormat::Json => write_json(&mut out, records, notice.as_deref()),
        ExportFormat::Ndjson => write_ndjson(&mut out, records, notice.as_deref()),
    };
    // Flushed explicitly: a `BufWriter` that fails while flushing in `drop`
    // fails silently, and "Kavka said it exported, and the file is half a record
    // short" is the one outcome this command must not have.
    written
        .and_then(|()| out.flush())
        .map_err(|e| file_trouble(path, &e))
}

/// RFC 4180, one column per thing a user can act on: the address, the time, the
/// two payloads exactly as they were rendered on screen, and the headers.
fn write_csv(
    out: &mut impl Write,
    records: &[MessageRecord],
    notice: Option<&str>,
) -> std::io::Result<()> {
    // A `#` line above the header row. CSV has no comment syntax, so this is
    // not free — a strict reader sees a one-column first row — and it is still
    // the right trade: every tool that opens this shows the sentence, and the
    // alternative is a spreadsheet of redacted values that looks exactly like a
    // spreadsheet of real ones.
    if let Some(notice) = notice {
        writeln!(out, "# {notice}\r")?;
    }
    out.write_all(b"partition,offset,timestamp_ms,key_text,value_text,headers_json\r\n")?;
    for record in records {
        // Absent is an empty field, never the word "null" (docs/DESIGN.md §7):
        // a tombstone's value and a keyless record's key are both blank here,
        // and the JSON formats keep the distinction for anything that needs it.
        let key = record
            .key
            .as_ref()
            .map_or("", |payload| payload.text.as_str());
        let value = record
            .value
            .as_ref()
            .map_or("", |payload| payload.text.as_str());
        // The header LIST, not an object: Kafka allows the same header key
        // twice, and an object would silently keep one of them.
        let headers = serde_json::to_string(&record.headers)?;
        let timestamp = record
            .timestamp_ms
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        write!(
            out,
            "{},{},{},{},{},{}",
            record.partition,
            record.offset,
            timestamp,
            csv_field(key),
            csv_field(value),
            csv_field(&headers),
        )?;
        out.write_all(b"\r\n")?;
    }
    Ok(())
}

/// RFC 4180: a field is wrapped in quotes when it contains a comma, a quote or
/// a line break, and an embedded quote is doubled. Everything else is written
/// bare, so a column of offsets stays a column of offsets.
fn csv_field(field: &str) -> Cow<'_, str> {
    if !field.contains([',', '"', '\n', '\r']) {
        return Cow::Borrowed(field);
    }
    let mut quoted = String::with_capacity(field.len() + 2);
    quoted.push('"');
    for character in field.chars() {
        if character == '"' {
            quoted.push('"');
        }
        quoted.push(character);
    }
    quoted.push('"');
    Cow::Owned(quoted)
}

/// The whole selection as one array, pretty-printed — this is the format people
/// read and diff, and `ndjson` is the one they pipe.
fn write_json(
    out: &mut impl Write,
    records: &[MessageRecord],
    notice: Option<&str>,
) -> std::io::Result<()> {
    let Some(notice) = notice else {
        serde_json::to_writer_pretty(&mut *out, records)?;
        return out.write_all(b"\n");
    };
    // The array shape is KEPT and the notice rides as its first element, rather
    // than the file becoming an object with a `records` key: a masked export is
    // still a list of records, every `.map()` over it still works, and the one
    // entry with `_kavka_notice` instead of `partition` is impossible to read
    // past by accident. It is only ever present when something was masked.
    let mut items: Vec<serde_json::Value> = Vec::with_capacity(records.len() + 1);
    items.push(serde_json::json!({ MASK_NOTICE_KEY: notice }));
    for record in records {
        items.push(serde_json::to_value(record)?);
    }
    serde_json::to_writer_pretty(&mut *out, &items)?;
    out.write_all(b"\n")
}

fn write_ndjson(
    out: &mut impl Write,
    records: &[MessageRecord],
    notice: Option<&str>,
) -> std::io::Result<()> {
    // The first LINE, which is where a reader of an NDJSON file looks and what
    // `head -1` shows.
    if let Some(notice) = notice {
        serde_json::to_writer(&mut *out, &serde_json::json!({ MASK_NOTICE_KEY: notice }))?;
        out.write_all(b"\n")?;
    }
    for record in records {
        serde_json::to_writer(&mut *out, record)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

/// The same write, for a table that is not a list of messages: a SQL result
/// set, whose columns are whatever the query asked for.
///
/// `columns` carries the header row rather than being inferred from the first
/// row's shape, for the reason the SQL panel exists to respect: **a result set
/// with zero rows still has a schema**, and a CSV of it is a header line rather
/// than an empty file. The rows are positional against that list, exactly as
/// they arrive over `kavka://sql/{id}/rows`.
///
/// `masked` is the one argument a result set cannot infer for itself. A
/// `MessageRecord` carries its own `masked` flag; a row of projected columns
/// carries nothing, so the caller — which knows whether the query ran under a
/// masking rule — has to say. **Optional, and today the UI does not send it**:
/// the argument is here so a masked result set can be labelled the moment
/// `SqlView` passes the flag it already has, rather than the export command
/// having to change shape later. Absent means "not masked", which is the
/// truthful default for every session with no rules on.
#[tauri::command]
async fn export_rows(
    path: String,
    format: String,
    columns: Vec<SqlColumn>,
    rows: Vec<Vec<serde_json::Value>>,
    masked: Option<bool>,
) -> CmdResult<()> {
    blocking(move || write_row_export(&path, &format, &columns, &rows, masked.unwrap_or(false)))
        .await
}

fn write_row_export(
    path: &str,
    format: &str,
    columns: &[SqlColumn],
    rows: &[Vec<serde_json::Value>],
    masked: bool,
) -> kavka_core::Result<()> {
    // Parsed before the file is opened, exactly as in `write_export`: a bad
    // format must not truncate a file the user already had.
    let format = export_format(format)?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| file_trouble(path, &e))?;

    let notice = masked.then(mask_notice_text);
    let mut out = std::io::BufWriter::new(file);
    let written = match format {
        ExportFormat::Csv => write_rows_csv(&mut out, columns, rows, notice.as_deref()),
        ExportFormat::Json => write_rows_json(&mut out, columns, rows, notice.as_deref()),
        ExportFormat::Ndjson => write_rows_ndjson(&mut out, columns, rows, notice.as_deref()),
    };
    written
        .and_then(|()| out.flush())
        .map_err(|e| file_trouble(path, &e))
}

/// RFC 4180, with the query's own column names as the header row and
/// [`csv_field`]'s quoting — the same writer the message export uses, so one
/// bug fix serves both.
fn write_rows_csv(
    out: &mut impl Write,
    columns: &[SqlColumn],
    rows: &[Vec<serde_json::Value>],
    notice: Option<&str>,
) -> std::io::Result<()> {
    if let Some(notice) = notice {
        writeln!(out, "# {notice}\r")?;
    }
    let header: Vec<Cow<'_, str>> = columns
        .iter()
        .map(|column| csv_field(&column.name))
        .collect();
    out.write_all(header.join(",").as_bytes())?;
    out.write_all(b"\r\n")?;

    for row in rows {
        let cells: Vec<Cow<'_, str>> = row
            .iter()
            .map(|value| match csv_cell(value) {
                Cow::Borrowed(text) => csv_field(text),
                Cow::Owned(text) => Cow::Owned(csv_field(&text).into_owned()),
            })
            .collect();
        out.write_all(cells.join(",").as_bytes())?;
        out.write_all(b"\r\n")?;
    }
    Ok(())
}

/// One result cell as CSV text.
///
/// A SQL NULL is an EMPTY FIELD, never the word "null" — the same rule the
/// message export follows for a tombstone (docs/DESIGN.md §7), and the reason
/// the JSON formats exist for anything that needs to tell an empty string from
/// an absent value. A string cell is its own text, unquoted by JSON: exporting
/// `"order-7"` with the quotes would put them in the spreadsheet.
fn csv_cell(value: &serde_json::Value) -> Cow<'_, str> {
    match value {
        serde_json::Value::Null => Cow::Borrowed(""),
        serde_json::Value::String(text) => Cow::Borrowed(text.as_str()),
        // A number, a bool, or a nested structure DataFusion produced (an
        // `array_agg`, say) — its compact JSON is the only faithful rendering.
        other => Cow::Owned(other.to_string()),
    }
}

/// The whole result set as one array of OBJECTS keyed by column name,
/// pretty-printed. Positional arrays would be smaller and unreadable: the
/// column list is right there, so the export spends the bytes and keeps the
/// names.
fn write_rows_json(
    out: &mut impl Write,
    columns: &[SqlColumn],
    rows: &[Vec<serde_json::Value>],
    notice: Option<&str>,
) -> std::io::Result<()> {
    let mut objects: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    if let Some(notice) = notice {
        let mut first = serde_json::Map::new();
        first.insert(MASK_NOTICE_KEY.to_string(), notice.into());
        objects.push(first);
    }
    objects.extend(rows.iter().map(|row| row_object(columns, row)));
    serde_json::to_writer_pretty(&mut *out, &objects)?;
    out.write_all(b"\n")
}

fn write_rows_ndjson(
    out: &mut impl Write,
    columns: &[SqlColumn],
    rows: &[Vec<serde_json::Value>],
    notice: Option<&str>,
) -> std::io::Result<()> {
    if let Some(notice) = notice {
        serde_json::to_writer(&mut *out, &serde_json::json!({ MASK_NOTICE_KEY: notice }))?;
        out.write_all(b"\n")?;
    }
    for row in rows {
        serde_json::to_writer(&mut *out, &row_object(columns, row))?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

/// One row as `{column: value}`.
///
/// A row shorter than the column list gets `null` for the columns it does not
/// reach, and a row longer than it keeps the extra cells under positional names
/// (`column_7`) rather than dropping them: this is an export, and silently
/// losing a column is the one outcome worse than an ugly key. Neither case can
/// arise from the core, which sends rows positional against the schema it
/// already emitted — they are here because a writer that drops data on a shape
/// it did not expect is a writer nobody can trust.
fn row_object(
    columns: &[SqlColumn],
    row: &[serde_json::Value],
) -> serde_json::Map<String, serde_json::Value> {
    let mut object = serde_json::Map::with_capacity(columns.len().max(row.len()));
    for (index, column) in columns.iter().enumerate() {
        object.insert(
            column.name.clone(),
            row.get(index).cloned().unwrap_or(serde_json::Value::Null),
        );
    }
    for (index, value) in row.iter().enumerate().skip(columns.len()) {
        object.insert(format!("column_{index}"), value.clone());
    }
    object
}

/// What went wrong with the file, in the shape docs/DESIGN.md §7 asks for: what
/// happened, then the next click. The OS message is kept for everything Kavka
/// does not recognise — it usually names the path or the process holding the
/// file.
fn file_trouble(path: &str, cause: &std::io::Error) -> kavka_core::Error {
    let detail = match cause.kind() {
        std::io::ErrorKind::PermissionDenied => format!(
            "Kavka isn't allowed to write {path}. Pick another folder, or close whatever has that file open, then export again."
        ),
        std::io::ErrorKind::NotFound => format!(
            "There's no folder for {path} any more. Pick another location and export again."
        ),
        _ => format!("Kavka couldn't write {path}: {cause}"),
    };
    kavka_core::Error::Other(detail)
}

pub fn run() {
    // FIRST, before anything can panic. The hook does nothing until `setup`
    // below tells it where the logs live and the user has opted in — but
    // installing it here means a panic during Tauri's own start-up is covered
    // on the second launch, which is exactly when somebody is trying to work
    // out why the first one died.
    install_panic_hook();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // The Rust half of @tauri-apps/plugin-dialog. Only the save dialog is
        // granted (capabilities/default.json): the shell writes files the user
        // named, and nothing in Kavka opens one.
        .plugin(tauri_plugin_dialog::init())
        // The Rust half of the notification plugin, for one job: an alert that
        // fires while Kavka is behind another window still has to reach the
        // person. Granted `notification:default` in capabilities/default.json,
        // and every toast is sent from Rust — the webview never asks for one.
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            // Alert rules sit beside profiles.json in the config dir; the
            // history databases go in the DATA dir, because a week of samples
            // is data the app produced rather than configuration the user
            // wrote, and it is the one thing here that can reach a gigabyte.
            let alerts = Arc::new(AlertStore::new(dir.clone()));
            // Masking rules sit beside both, for the same reason and with the
            // same discipline (tmp-then-rename, one write lock).
            let masks = Arc::new(MaskStore::new(dir.clone()));
            let data_dir = app.path().app_data_dir()?;
            let histories = Arc::new(HistoryStores::new(data_dir.join("history")));
            // Diagnostics: the logs go in the DATA directory beside the history
            // databases, for the same reason those do — they are something the
            // app produced, not configuration the user wrote. The toggle itself
            // is configuration, so it sits in the config dir with profiles.json.
            // The directory is only *named* here; nothing creates it until the
            // user turns diagnostics on.
            let _ = LOG_DIR.set(data_dir.join("logs"));
            DIAGNOSTICS_ON.store(read_diagnostics_pref(&dir), Ordering::Relaxed);
            log_session_header();
            app.manage(AppState {
                store: Arc::new(ProfileStore::new(dir)),
                connections: Mutex::new(HashMap::new()),
                tails: SessionMap::new(),
                searches: SessionMap::new(),
                bulks: SessionMap::new(),
                sqls: SessionMap::new(),
                copies: SessionMap::new(),
                ready: Mutex::new(HashMap::new()),
                fetches: Mutex::new(HashMap::new()),
                protocol: Mutex::new(HashMap::new()),
                alerts,
                masks,
                mask_sets: Mutex::new(HashMap::new()),
                histories,
                monitors: Mutex::new(HashMap::new()),
                id_seq: AtomicU64::new(0),
                id_epoch: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| u64::try_from(since.as_nanos()).unwrap_or(0)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            mcp_info,
            sandbox_status,
            sandbox_start,
            sandbox_stop,
            diagnostics_status,
            diagnostics_set_enabled,
            diagnostics_record,
            diagnostics_open_logs,
            diagnostics_clear,
            nl_to_query,
            nl_grammar,
            masking_list,
            masking_save,
            masking_delete,
            masking_toggle,
            wasm_serdes_list,
            wasm_serdes_save,
            wasm_serdes_delete,
            profiles_list,
            profiles_save,
            profiles_delete,
            profiles_export,
            profiles_import,
            secret_set,
            secret_delete,
            secret_exists,
            cluster_connect,
            cluster_disconnect,
            topics_list,
            topic_detail,
            topic_create,
            topic_delete,
            groups_list,
            group_detail,
            offsets_reset,
            messages_fetch,
            tail_start,
            tail_stop,
            search_start,
            search_stop,
            session_ready,
            sql_start,
            sql_stop,
            produce_send,
            produce_bulk,
            bulk_stop,
            copy_dry_run,
            copy_start,
            copy_stop,
            config_diff,
            offsets_migrate_plan,
            offsets_migrate_apply,
            acls_list,
            acls_create,
            acls_delete,
            broker_configs,
            broker_config_set,
            quorum_describe,
            elect_leaders,
            reassign_alter,
            reassign_cancel,
            reassign_list,
            quotas_list,
            quotas_alter,
            connect_list,
            connect_config,
            connect_validate,
            connect_apply,
            connect_delete,
            connect_restart,
            connect_pause,
            connect_resume,
            sr_subject_versions,
            sr_check_compat,
            sr_register,
            sr_get_compat,
            sr_set_compat,
            history_query,
            history_groups,
            sampler_status,
            metrics_query,
            metrics_status,
            alerts_list,
            alerts_save,
            alerts_delete,
            alerts_history,
            alerts_channels_get,
            alerts_channels_set,
            alerts_channels_test,
            share_groups_list,
            share_group_detail,
            streams_topology,
            export_records,
            export_rows,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Kavka");

    app.run(|handle, event| {
        // Ask every live session to finish on the way out. Nothing is joined
        // here: the worker threads are detached, `stop` unblocks them, and
        // quitting the app must never wait on a broker.
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = handle.try_state::<AppState>() {
                state.stop_all_sessions();
            }
        }
    });
}

/// The shell is a bridge, so there is almost nothing here to test — every
/// command hands its arguments to the core and its answer back. There are five
/// exceptions, and they are the places the shell decides something rather than
/// forwarding it: the export writer, where a quoting bug is a corrupted file
/// rather than a visible error; the monitor loop's own arithmetic — how long it
/// waits after a failure, what it calls a failure, and what it publishes while
/// it is doing so; the MCP snippets, which are strings this file invents and
/// a person pastes into another program's configuration; the masking pass,
/// which is the one place a record is CHANGED on its way to the webview —
/// which cell of a result set is masked as what, and what a file that carries
/// masked text says about itself; and the one save-time refusal, which is the
/// path a WASM decoder plugin may be loaded from. Everything the loop *does*
/// (sampling, scraping, evaluating) and everything a rule does to a string
/// belongs to the core and is tested there.
#[cfg(test)]
mod tests {
    use super::*;
    use kavka_core::masking::MaskTarget;
    use kavka_core::profiles::{AuthConfig, Environment, MetricsEndpointConfig};
    use kavka_core::serdes::{DecodedPayload, Encoding, HeaderEntry};

    fn payload(text: &str) -> DecodedPayload {
        DecodedPayload {
            encoding: Encoding::Utf8,
            text: text.to_string(),
            json: None,
            raw_len: text.len(),
            truncated: false,
            schema: None,
            decoded_by: None,
        }
    }

    fn record(offset: i64) -> MessageRecord {
        MessageRecord {
            partition: 3,
            offset,
            timestamp_ms: Some(1_700_000_000_000),
            key: Some(payload("order-7")),
            value: Some(payload("{\"id\":7}")),
            headers: Vec::new(),
            dlq: None,
            masked: false,
        }
    }

    fn csv_of(records: &[MessageRecord]) -> String {
        let mut out = Vec::new();
        write_csv(&mut out, records, None).expect("a Vec never fails to write");
        String::from_utf8(out).expect("the writer only ever emits UTF-8")
    }

    // ── MCP snippets ───────────────────────────────────────────────────────

    /// A Windows path is mostly backslashes, and the Cursor snippet is JSON —
    /// so the one thing that must never happen is a snippet the person has to
    /// repair before it works.
    #[test]
    fn the_cursor_snippet_is_json_that_names_the_binary() {
        let path = r"C:\Program Files\Kavka\kavka-mcp.exe";
        let (_, cursor) = mcp_snippets(path);
        let parsed: serde_json::Value =
            serde_json::from_str(&cursor).expect("the snippet is valid JSON");
        assert_eq!(parsed["mcpServers"]["kavka"]["command"], path);
        assert_eq!(parsed["mcpServers"]["kavka"]["args"], serde_json::json!([]));
        // The env block is present and empty: writes are opt-in, and this is
        // where the person opts in.
        assert_eq!(parsed["mcpServers"]["kavka"]["env"], serde_json::json!({}));
        // The raw text carries escaped separators, not literal ones.
        assert!(cursor.contains(r"C:\\Program Files\\Kavka"), "{cursor}");
    }

    #[test]
    fn the_claude_snippet_quotes_the_path_and_shows_how_to_allow_writes() {
        let path = "/Applications/Kavka.app/Contents/MacOS/kavka-mcp";
        let (claude, _) = mcp_snippets(path);
        let first = claude.lines().next().expect("a first line");
        assert_eq!(first, format!("claude mcp add kavka -- \"{path}\""));
        // The default is the read-only server; enabling writes is a separate,
        // visible line naming the variable that does it.
        assert!(claude.contains("KAVKA_MCP_ALLOW_WRITES=1"), "{claude}");
        assert!(claude.contains("KAVKA_MCP_ALLOW_PROD=1"), "{claude}");
        assert!(claude.contains("read-only always refuse"), "{claude}");
    }

    /// The path is a sibling of the running binary on every platform — the one
    /// rule that covers `cargo tauri dev` and a packaged app at once.
    #[test]
    fn the_binary_is_looked_for_beside_this_one() {
        let path = std::path::PathBuf::from(mcp_binary_path());
        let expected = format!("kavka-mcp{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(&*expected));
        assert_eq!(
            path.parent(),
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(PathBuf::from))
                .as_deref(),
        );
    }

    #[test]
    fn a_plain_field_is_written_bare() {
        assert_eq!(csv_field("order-7"), "order-7");
        assert_eq!(csv_field(""), "");
    }

    #[test]
    fn a_field_is_quoted_only_when_rfc4180_requires_it() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("line\nbreak"), "\"line\nbreak\"");
        assert_eq!(csv_field("carriage\rreturn"), "\"carriage\rreturn\"");
        // The doubling rule — and the reason a naive writer corrupts JSON.
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_leads_with_the_agreed_columns() {
        let csv = csv_of(&[]);
        assert_eq!(
            csv,
            "partition,offset,timestamp_ms,key_text,value_text,headers_json\r\n"
        );
    }

    #[test]
    fn a_csv_row_carries_the_address_the_time_and_both_payloads() {
        let csv = csv_of(&[record(8412)]);
        let row = csv.lines().nth(1).expect("a record produces a row");
        assert_eq!(row, "3,8412,1700000000000,order-7,\"{\"\"id\"\":7}\",[]");
        // CRLF, not LF: RFC 4180's line ending, and Excel's.
        assert!(csv.ends_with("\r\n"), "rows end with CRLF: {csv:?}");
    }

    #[test]
    fn an_absent_key_value_or_timestamp_is_an_empty_field() {
        let mut tombstone = record(12);
        tombstone.key = None;
        tombstone.value = None;
        tombstone.timestamp_ms = None;
        let row = csv_of(&[tombstone])
            .lines()
            .nth(1)
            .expect("a record produces a row")
            .to_string();
        // Never the word "null" (docs/DESIGN.md §7). The JSON formats keep the
        // distinction between "no value" and "an empty value" for anything that
        // needs it.
        assert_eq!(row, "3,12,,,,[]");
    }

    #[test]
    fn headers_are_a_json_list_so_a_repeated_key_survives() {
        let mut repeated = record(1);
        repeated.headers = vec![
            HeaderEntry {
                key: "trace".into(),
                value: Some("a".into()),
                is_text: true,
            },
            HeaderEntry {
                key: "trace".into(),
                value: Some("b".into()),
                is_text: true,
            },
        ];
        let csv = csv_of(&[repeated]);
        assert!(
            csv.contains(
                "\"[{\"\"key\"\":\"\"trace\"\",\"\"value\"\":\"\"a\"\",\"\"is_text\"\":true},"
            ),
            "both headers, quoted per RFC 4180: {csv}"
        );
        assert!(csv.contains("\"\"value\"\":\"\"b\"\""), "{csv}");
    }

    #[test]
    fn ndjson_is_one_record_per_line() {
        let mut out = Vec::new();
        write_ndjson(&mut out, &[record(1), record(2)], None).expect("a Vec never fails to write");
        let text = String::from_utf8(out).expect("the writer only ever emits UTF-8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let parsed: serde_json::Value = serde_json::from_str(line).expect("each line is JSON");
            assert_eq!(parsed["partition"], 3);
        }
    }

    #[test]
    fn json_is_one_array_of_records() {
        let mut out = Vec::new();
        write_json(&mut out, &[record(1), record(2)], None).expect("a Vec never fails to write");
        let parsed: serde_json::Value =
            serde_json::from_slice(&out).expect("the whole file is one JSON document");
        let array = parsed.as_array().expect("an array");
        assert_eq!(array.len(), 2);
        assert_eq!(array[1]["offset"], 2);
    }

    #[test]
    fn an_unknown_format_names_the_three_kavka_writes() {
        let refused = export_format("parquet").expect_err("not a format Kavka writes");
        let message = refused.to_string();
        assert!(message.contains("parquet"), "{message}");
        assert!(message.contains("csv"), "{message}");
        assert!(message.contains("ndjson"), "{message}");
    }

    /// The reason the format is parsed before the file is opened: a typo must
    /// not cost the user a file they already had.
    #[test]
    fn a_bad_format_never_touches_the_file() {
        let path = std::env::temp_dir().join(format!(
            "kavka-export-{}.txt",
            std::process::id() as u64 * 31 + 7
        ));
        std::fs::write(&path, b"not Kavka's").expect("the temp dir is writable");
        let path_text = path.to_string_lossy().to_string();

        write_export(&path_text, "parquet", &[record(1)]).expect_err("not a format Kavka writes");
        let after = std::fs::read(&path).expect("the file is still there");
        assert_eq!(after, b"not Kavka's");

        write_export(&path_text, "ndjson", &[record(1)]).expect("ndjson is a format Kavka writes");
        let written = std::fs::read_to_string(&path).expect("the file is still there");
        assert!(written.starts_with('{'), "{written}");
        let _ = std::fs::remove_file(&path);
    }

    // ── The SQL result-set writers ─────────────────────────────────────────
    //
    // A second export path, and the same reason the first one is tested here:
    // a quoting bug is a corrupted file rather than a visible error, and the
    // shell is the only place that decides how a result set becomes text.

    fn columns(names: &[&str]) -> Vec<SqlColumn> {
        names
            .iter()
            .map(|name| SqlColumn {
                name: (*name).to_string(),
                data_type: "VARCHAR".into(),
            })
            .collect()
    }

    fn rows_csv_of(cols: &[SqlColumn], rows: &[Vec<serde_json::Value>]) -> String {
        let mut out = Vec::new();
        write_rows_csv(&mut out, cols, rows, None).expect("a Vec never fails to write");
        String::from_utf8(out).expect("the writer only ever emits UTF-8")
    }

    /// A RESULT SET WITH NO ROWS STILL HAS A SCHEMA — which is exactly why the
    /// command takes the columns rather than inferring them from row one.
    #[test]
    fn a_result_set_with_no_rows_still_exports_its_header() {
        let csv = rows_csv_of(&columns(&["partition", "n"]), &[]);
        assert_eq!(csv, "partition,n\r\n");
    }

    #[test]
    fn result_cells_are_written_as_their_own_text_never_as_json_literals() {
        let csv = rows_csv_of(
            &columns(&["key_text", "n", "ok", "value_text"]),
            &[vec![
                serde_json::json!("order-7"),
                serde_json::json!(42),
                serde_json::json!(true),
                serde_json::Value::Null,
            ]],
        );
        // A string keeps neither its JSON quotes nor an escape; a NULL is an
        // empty field, never the word "null" (docs/DESIGN.md §7).
        assert_eq!(csv, "key_text,n,ok,value_text\r\norder-7,42,true,\r\n");
    }

    #[test]
    fn a_result_cell_is_quoted_only_when_rfc4180_requires_it() {
        let csv = rows_csv_of(
            &columns(&["value_text"]),
            &[
                vec![serde_json::json!("a,b")],
                vec![serde_json::json!("say \"hi\"")],
                vec![serde_json::json!("line\nbreak")],
                // A structure DataFusion produced: compact JSON, then quoted
                // because that JSON contains commas and quotes.
                vec![serde_json::json!({"status": "failed"})],
            ],
        );
        // Split on the RECORD separator only: a quoted field is allowed to
        // contain a bare newline, and that is the whole point of quoting it.
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], "value_text");
        assert_eq!(lines[1], "\"a,b\"");
        assert_eq!(lines[2], "\"say \"\"hi\"\"\"");
        assert_eq!(lines[3], "\"line\nbreak\"");
        assert_eq!(lines[4], "\"{\"\"status\"\":\"\"failed\"\"}\"");
        assert_eq!(lines[5], "", "the last record ends with a separator");
    }

    /// A column name is user text — a query can alias a column to anything at
    /// all — so the header row goes through the same quoting as the cells.
    #[test]
    fn a_column_name_with_a_comma_is_quoted_in_the_header() {
        let csv = rows_csv_of(&columns(&["last, first"]), &[]);
        assert_eq!(csv, "\"last, first\"\r\n");
    }

    #[test]
    fn rows_json_is_one_array_of_objects_keyed_by_column() {
        let mut out = Vec::new();
        write_rows_json(
            &mut out,
            &columns(&["partition", "n"]),
            &[
                vec![serde_json::json!(0), serde_json::json!(17)],
                vec![serde_json::json!(1), serde_json::json!(16)],
            ],
            None,
        )
        .expect("a Vec never fails to write");
        let parsed: serde_json::Value =
            serde_json::from_slice(&out).expect("the whole file is one JSON document");
        let array = parsed.as_array().expect("an array");
        assert_eq!(array.len(), 2);
        assert_eq!(array[0]["partition"], 0);
        assert_eq!(array[1]["n"], 16);
    }

    #[test]
    fn rows_ndjson_is_one_object_per_line() {
        let mut out = Vec::new();
        write_rows_ndjson(
            &mut out,
            &columns(&["key_text"]),
            &[
                vec![serde_json::json!("order-1")],
                vec![serde_json::Value::Null],
            ],
            None,
        )
        .expect("a Vec never fails to write");
        let text = String::from_utf8(out).expect("the writer only ever emits UTF-8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], r#"{"key_text":"order-1"}"#);
        // Null survives as null here, which is the whole reason the JSON
        // formats exist beside the CSV one.
        assert_eq!(lines[1], r#"{"key_text":null}"#);
    }

    /// Neither shape can come from the core, which sends rows positional
    /// against the schema it already emitted. They are asserted because a
    /// writer that silently drops a column on a shape it did not expect is a
    /// writer nobody can trust with an export.
    #[test]
    fn a_row_that_does_not_match_the_column_list_loses_nothing() {
        let short = row_object(&columns(&["a", "b"]), &[serde_json::json!(1)]);
        assert_eq!(short["a"], 1);
        assert_eq!(short["b"], serde_json::Value::Null);

        let long = row_object(
            &columns(&["a"]),
            &[serde_json::json!(1), serde_json::json!(2)],
        );
        assert_eq!(long["a"], 1);
        assert_eq!(long["column_1"], 2);
    }

    #[test]
    fn a_bad_row_format_never_touches_the_file() {
        let path = std::env::temp_dir().join(format!(
            "kavka-rows-{}.txt",
            std::process::id() as u64 * 37 + 11
        ));
        std::fs::write(&path, b"not Kavka's").expect("the temp dir is writable");
        let path_text = path.to_string_lossy().to_string();
        let cols = columns(&["n"]);
        let rows = vec![vec![serde_json::json!(1)]];

        write_row_export(&path_text, "parquet", &cols, &rows, false)
            .expect_err("not a format Kavka writes");
        assert_eq!(
            std::fs::read(&path).expect("the file is still there"),
            b"not Kavka's"
        );

        write_row_export(&path_text, "csv", &cols, &rows, false)
            .expect("csv is a format Kavka writes");
        let written = std::fs::read_to_string(&path).expect("the file is still there");
        assert_eq!(written, "n\r\n1\r\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn writing_into_a_folder_that_isnt_there_says_so() {
        let missing = std::env::temp_dir()
            .join("kavka-no-such-folder-2f9c")
            .join("export.csv");
        let refused = write_export(&missing.to_string_lossy(), "csv", &[])
            .expect_err("there is no such folder");
        let message = refused.to_string();
        assert!(message.contains("export.csv"), "{message}");
        // What to do next, not just what failed (docs/DESIGN.md §7).
        assert!(message.contains("Pick another location"), "{message}");
    }

    // ── The monitor ────────────────────────────────────────────────────────

    fn profile(id: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: id.into(),
            name: "Orders".into(),
            environment: Environment::Dev,
            bootstrap_servers: vec!["localhost:9092".into()],
            auth: AuthConfig::Plaintext,
            read_only: false,
            schema_registry: None,
            connect_clusters: Vec::new(),
            // No endpoint on purpose: building a collector for one would read
            // the keychain, and a unit test must not touch the machine's.
            metrics_endpoint: None,
            sampler_interval_ms: None,
            wasm_serdes: Vec::new(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kavka-shell-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The contract's event name, letter for letter — the UI builds the same
    /// string in `alertsEventName`, and a mismatch is alerts that fire into
    /// nothing.
    #[test]
    fn alerts_are_addressed_to_the_profile() {
        assert_eq!(alerts_event("p-1"), "kavka://alerts/p-1");
    }

    /// A working sampler, and a single blip, both wait one interval. The point
    /// of the second row is the point of the whole function: backing off after
    /// one dead tick would put a hole in the chart for a rebalance.
    #[test]
    fn one_dead_tick_does_not_slow_the_sampler_down() {
        let interval = Duration::from_secs(15);
        assert_eq!(retry_delay(interval, 0), interval);
        assert_eq!(retry_delay(interval, 1), interval);
    }

    #[test]
    fn repeated_failures_double_the_wait_and_then_stop_doubling() {
        let interval = Duration::from_secs(15);
        assert_eq!(retry_delay(interval, 2), Duration::from_secs(30));
        assert_eq!(retry_delay(interval, 3), Duration::from_secs(60));
        assert_eq!(retry_delay(interval, 4), Duration::from_secs(120));
        // The cap: a sampler that waited longer than this would leave a gap
        // somebody reads as "the cluster was quiet".
        assert_eq!(retry_delay(interval, 50), Duration::from_secs(120));
        assert_eq!(retry_delay(interval, u32::MAX), Duration::from_secs(120));
    }

    #[test]
    fn a_retry_is_spoken_in_whole_units() {
        assert_eq!(spoken(Duration::from_secs(15)), "15s");
        assert_eq!(spoken(Duration::from_secs(60)), "60s");
        assert_eq!(spoken(Duration::from_secs(120)), "2 min");
        assert_eq!(spoken(Duration::from_secs(300)), "5 min");
    }

    /// An absent interval is the core's default, and a too-fast one is clamped
    /// rather than refused — a profile hand-edited to 1ms should sample a
    /// little less often than asked, not stop sampling.
    #[test]
    fn the_sampler_interval_comes_off_the_profile_and_is_clamped() {
        let mut fixture = profile("p-1");
        assert_eq!(
            Monitor::for_profile(&fixture).interval_ms,
            kavka_core::history::DEFAULT_INTERVAL_MS
        );

        fixture.sampler_interval_ms = Some(60_000);
        let monitor = Monitor::for_profile(&fixture);
        assert_eq!(monitor.interval_ms, 60_000);
        assert_eq!(monitor.interval, Duration::from_secs(60));

        fixture.sampler_interval_ms = Some(1);
        assert_eq!(
            Monitor::for_profile(&fixture).interval_ms,
            kavka_core::history::MIN_INTERVAL_MS
        );
    }

    /// A profile with no endpoint has no collector, which is what makes
    /// `metrics_status` answer `unconfigured` rather than "unreachable" — a
    /// cluster nobody has given Kavka an exporter for is not a broken one.
    #[test]
    fn no_metrics_endpoint_means_no_collector() {
        assert!(Monitor::for_profile(&profile("p-1")).metrics.is_none());
        let mut configured = profile("p-2");
        configured.metrics_endpoint = Some(MetricsEndpointConfig {
            url: "http://broker-1:9404/metrics".into(),
            // Anonymous, so building this cannot reach the keychain.
            username: None,
            password: None,
        });
        assert!(Monitor::for_profile(&configured).metrics.is_some());
    }

    /// Before the loop starts and after it ends, the status says so. The
    /// Monitoring tab states `running: false` on a connected profile plainly,
    /// because a chart that simply stops is indistinguishable from a cluster
    /// that went quiet.
    #[test]
    fn a_monitor_publishes_whether_it_is_running() {
        let monitor = Monitor::for_profile(&profile("p-1"));
        let idle = monitor.status();
        assert!(!idle.running);
        assert_eq!(idle.interval_ms, kavka_core::history::DEFAULT_INTERVAL_MS);
        assert!(idle.last_sample_ms.is_none());
        assert!(idle.last_error.is_none());

        monitor.began();
        assert!(monitor.status().running);
        monitor.ended();
        assert!(!monitor.status().running);
    }

    /// A failed tick must not move `last_sample_ms` — "last sample" has to mean
    /// the last sample, or a chart that stopped updating still claims to be
    /// current — and a tick that works clears the error rather than leaving
    /// yesterday's failure on screen.
    #[test]
    fn a_failed_tick_reports_itself_without_claiming_a_sample() {
        let monitor = Monitor::for_profile(&profile("p-1"));
        monitor.record(Some(1_000), None);
        monitor.record(None, Some("couldn't list consumer groups".into()));
        let after = monitor.status();
        assert_eq!(after.last_sample_ms, Some(1_000));
        assert_eq!(
            after.last_error.as_deref(),
            Some("couldn't list consumer groups")
        );

        monitor.record(Some(2_000), None);
        let recovered = monitor.status();
        assert_eq!(recovered.last_sample_ms, Some(2_000));
        assert!(recovered.last_error.is_none());
    }

    /// `last_sample_ms` means "when Kavka last got a reading onto disk", and
    /// the case that matters is the middle one: the brokers answered, the store
    /// refused the write, and a status line reading "last reading 5s ago" beside
    /// a chart that has not grown since yesterday is the lie this prevents.
    #[test]
    fn a_tick_claims_a_sample_only_when_one_reached_the_disk() {
        fn tick(written: usize, samples: usize, errors: &[&str]) -> history::SampleTick {
            let sample = LagSample {
                ts_ms: 1_000,
                group_id: "checkout".into(),
                topic: "orders.v2".into(),
                partition: 0,
                committed: Some(10),
                end_offset: 12,
                lag: Some(2),
            };
            history::SampleTick {
                run: history::SampleRun {
                    ts_ms: 1_000,
                    groups: 1,
                    written,
                    errors: errors.iter().map(|e| (*e).to_string()).collect(),
                },
                samples: vec![sample; samples],
            }
        }

        // Samples on disk, with or without a group that could not be described
        // alongside them.
        assert!(reached_the_disk(&tick(40, 40, &[])));
        assert!(reached_the_disk(&tick(
            40,
            40,
            &["orders: authorization failed"]
        )));

        // The whole point: the cluster answered and the write did not.
        assert!(
            !reached_the_disk(&tick(0, 40, &["couldn't commit the samples: disk full"])),
            "a tick that lost what it read has not taken a sample"
        );

        // A cluster with nothing to record is a working sampler...
        assert!(reached_the_disk(&tick(0, 0, &[])));
        // ...and one that refused every describe is not.
        assert!(!reached_the_disk(&tick(
            0,
            0,
            &["couldn't list consumer groups: timed out"]
        )));
    }

    /// A rule that is firing when the loop stops gets a resolve, and that
    /// resolve belongs to the incident that is open — same `fired_ms`, which is
    /// what makes `record_event` close the row rather than write a second one.
    /// Without it the history shows an incident that never ended, and every
    /// reconnect opens another beside it.
    #[test]
    fn a_stopped_monitor_closes_the_incidents_it_leaves_firing() {
        let rules = vec![
            AlertRule::UnderReplicated {
                id: "r1".into(),
                name: "Replicas falling behind".into(),
                for_ms: 0,
            },
            AlertRule::OfflinePartitions {
                id: "r2".into(),
                name: "Partitions with no leader".into(),
            },
        ];
        // Only the first rule's condition holds, so only the first fires.
        let observation = Observation {
            samples: Vec::new(),
            metrics: vec![
                ("under_replicated_partitions".to_string(), 3.0),
                ("offline_partitions".to_string(), 0.0),
            ],
        };
        let (state, fired) = alerts::evaluate(&rules, &observation, &AlertState::default(), 1_000);
        assert_eq!(fired.len(), 1, "{fired:#?}");

        let closing = closing_events(&rules, &state, 9_000);
        assert_eq!(
            closing.len(),
            1,
            "one resolve per FIRING rule: {closing:#?}"
        );
        let event = &closing[0];
        assert_eq!(event.rule_id, "r1");
        assert_eq!(event.rule_name, "Replicas falling behind");
        assert_eq!(
            event.fired_ms, 1_000,
            "the resolve carries the fire's own instant, or it opens a second \
             incident instead of closing this one"
        );
        assert_eq!(event.resolved_ms, Some(9_000));
        assert!(event.is_resolved());
        // Honesty: monitoring stopped is what happened. Whether the condition
        // cleared is the one thing Kavka cannot say here.
        assert!(
            event.detail.contains("stopped monitoring"),
            "{}",
            event.detail
        );

        // Nothing firing, nothing to close — the ordinary case, on every
        // disconnect of a healthy cluster.
        assert!(closing_events(&rules, &AlertState::default(), 9_000).is_empty());
    }

    /// The rules file is read for a name, not for permission to close the
    /// incident: a file that will not parse must not leave the history with an
    /// incident that never ends.
    #[test]
    fn an_incident_is_closed_even_when_its_rule_cannot_be_named() {
        let rules = vec![AlertRule::UnderReplicated {
            id: "r1".into(),
            name: "Replicas falling behind".into(),
            for_ms: 0,
        }];
        let observation = Observation {
            samples: Vec::new(),
            metrics: vec![("under_replicated_partitions".to_string(), 1.0)],
        };
        let (state, _) = alerts::evaluate(&rules, &observation, &AlertState::default(), 1_000);

        let closing = closing_events(&[], &state, 9_000);
        assert_eq!(closing.len(), 1);
        assert_eq!(closing[0].rule_id, "r1");
        assert_eq!(closing[0].rule_name, "r1", "the id, rather than nothing");
    }

    /// `stop` is what makes a disconnect prompt: the loop is waiting out its
    /// interval on the condvar, and this has to cut through it rather than
    /// costing up to a full sampling interval.
    #[test]
    fn stopping_a_monitor_cuts_its_wait_short() {
        let monitor = Arc::new(Monitor::for_profile(&profile("p-1")));
        monitor.stop();
        let began = Instant::now();
        // A minute's wait, already stopped: returns now, and says the loop is
        // over.
        assert!(!monitor.wait(Duration::from_secs(60)));
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "returned promptly"
        );

        // Idempotent, and callable from a thread that is not the loop's.
        let other = Arc::clone(&monitor);
        std::thread::spawn(move || other.stop())
            .join()
            .expect("the stopping thread finished");
        assert!(!monitor.wait(Duration::ZERO));
    }

    /// One handle per profile, process-wide: redb locks its file, so the
    /// sampler writing and a `history_query` reading have to be the same
    /// object. Two profiles get two files.
    #[test]
    fn a_history_store_is_opened_once_per_profile() {
        let dir = scratch("stores");
        let stores = HistoryStores::new(dir.clone());
        let first = stores.get("p-1").expect("the temp dir is writable");
        let again = stores.get("p-1").expect("already open");
        assert!(Arc::ptr_eq(&first, &again), "the same handle comes back");

        let other = stores.get("p-2").expect("a second profile, a second file");
        assert!(!Arc::ptr_eq(&first, &other));
        assert_ne!(first.path(), other.path());
        // The directory is created on first use rather than at startup — a
        // Kavka that has never connected to anything leaves nothing behind.
        assert!(dir.exists());

        // Taking it back is what lets `profiles_delete` remove the file.
        assert!(stores.take("p-1").is_some());
        assert!(stores.take("p-1").is_none());
        drop(first);
        drop(again);
        drop(other);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The store path is the core's, so a profile id that arrived in an import
    /// carrying `../` cannot become a write outside the history directory.
    #[test]
    fn a_store_path_stays_inside_the_history_directory() {
        let dir = scratch("paths");
        let stores = HistoryStores::new(dir.clone());
        let escaped = stores.path_of("../../evil");
        assert_eq!(escaped.parent(), Some(dir.as_path()));
        // The dots survive — they are legal in a file name — but the
        // separators do not, which is what makes the name one component of the
        // directory above rather than a route out of it.
        let name = escaped
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        assert!(!name.contains('/') && !name.contains('\\'), "{name}");
        assert!(name.ends_with(".redb"), "{name}");
    }

    // ── WASM decoder plugins: where one may come from ──────────────────────
    //
    // The rule and its sentences are `kavka_core::wasm_serde`'s. What is tested
    // here is that the SAVE path runs it — the loader's own check is defence in
    // depth, and a refusal that only happens when the connection is next opened
    // is a refusal the person who typed the path never sees.

    fn plugin(path: &str) -> WasmSerdeConfig {
        WasmSerdeConfig {
            name: "acme".into(),
            path: path.into(),
            applies_to_topics: vec!["*".into()],
        }
    }

    #[test]
    fn saving_a_relative_or_network_plugin_path_is_refused_before_anything_is_written() {
        let relative = validated_wasm_serde(plugin("decoder.wasm")).expect_err("relative");
        assert!(relative.to_string().contains("travels"), "{relative}");

        let unc = validated_wasm_serde(plugin(r"\\build-server\share\decoder.wasm"))
            .expect_err("a network path");
        assert!(unc.to_string().contains("network path"), "{unc}");

        // …and an absolute local path goes through untouched.
        let local = std::env::temp_dir().join("acme.wasm").display().to_string();
        let kept = validated_wasm_serde(plugin(&local)).expect("an absolute path");
        assert_eq!(kept.path, local);
    }

    // ── Masking: the things this file decides ──────────────────────────────
    //
    // The rules, their compilation and what a rule does to a string all belong
    // to `kavka_core::masking` and are tested there. What is tested HERE is the
    // shell's own decisions: which cell of a SQL result set is masked as what,
    // that a batch pass sets the flag the UI reads, and that an export of
    // anything masked says so in a way the file itself carries.

    fn rules(pattern: &str, applies_to: MaskTarget) -> MaskSet {
        let (set, refused) = MaskSet::compile(&[MaskRule {
            id: "r1".into(),
            name: "test".into(),
            pattern: pattern.into(),
            replacement: "•••".into(),
            applies_to,
            enabled: true,
        }]);
        assert!(refused.is_empty(), "the fixture rule must compile");
        set
    }

    fn masked_record(key: &str, value: &str) -> MessageRecord {
        MessageRecord {
            partition: 0,
            offset: 1,
            timestamp_ms: Some(1_700_000_000_000),
            key: Some(payload(key)),
            value: Some(payload(value)),
            headers: Vec::new(),
            dlq: None,
            masked: false,
        }
    }

    #[test]
    fn a_batch_pass_masks_every_record_and_sets_the_flag() {
        let set = rules(r"\d{4}", MaskTarget::All);
        let mut batch = vec![masked_record("k-1111", "v-2222"), masked_record("k", "v")];
        assert!(mask_batch(&set, &mut batch));
        assert_eq!(batch[0].key.as_ref().expect("a key").text, "k-•••");
        assert_eq!(batch[0].value.as_ref().expect("a value").text, "v-•••");
        assert!(batch[0].masked);
        // A record nothing matched is NOT flagged: the flag has to mean "this
        // one was rewritten", or an export's notice means nothing.
        assert!(!batch[1].masked);
    }

    /// The cheap path, and the one nearly every session takes.
    #[test]
    fn a_session_with_no_rules_leaves_a_batch_alone() {
        let mut batch = vec![masked_record("k-1111", "v-2222")];
        assert!(!mask_batch(&MaskSet::none(), &mut batch));
        assert_eq!(batch[0].key.as_ref().expect("a key").text, "k-1111");
        assert!(!batch[0].masked);
    }

    /// The four columns of the `messages` table whose provenance is known are
    /// masked AS that part of a record — so a rule scoped to keys behaves in a
    /// result set exactly as it does in the grid.
    #[test]
    fn a_key_scoped_rule_masks_key_text_and_not_value_text() {
        let set = rules(r"\d{4}", MaskTarget::Key);
        let columns = columns(&["key_text", "value_text"]);
        let mut rows = vec![vec![
            serde_json::json!("key-1111"),
            serde_json::json!("value-2222"),
        ]];
        assert!(mask_rows(&set, &columns, &mut rows));
        assert_eq!(rows[0][0], serde_json::json!("key-•••"));
        assert_eq!(rows[0][1], serde_json::json!("value-2222"));
    }

    /// THE UNDER-MASKING RULE. A column that is not one of the four has no
    /// provenance Kavka can know — `upper(key_text) || value_text` came from
    /// where? — so every enabled rule runs over it, whatever each is scoped to.
    #[test]
    fn a_computed_column_is_masked_by_every_rule_whatever_its_scope() {
        let set = rules(r"\d{4}", MaskTarget::Key);
        let columns = columns(&["mixed"]);
        let mut rows = vec![vec![serde_json::json!("anything-1111")]];
        assert!(mask_rows(&set, &columns, &mut rows));
        assert_eq!(rows[0][0], serde_json::json!("anything-•••"));
    }

    /// Numbers are left alone in a result set for the reason they are left
    /// alone inside a payload: a regex over a rendered number masks an account
    /// id and a quantity with equal enthusiasm.
    #[test]
    fn numbers_booleans_and_nulls_pass_through_a_result_set_untouched() {
        let set = rules(r"\d{4}", MaskTarget::All);
        let columns = columns(&["n", "flag", "nothing"]);
        let mut rows = vec![vec![
            serde_json::json!(1111),
            serde_json::json!(true),
            serde_json::Value::Null,
        ]];
        assert!(!mask_rows(&set, &columns, &mut rows));
        assert_eq!(rows[0][0], serde_json::json!(1111));
    }

    /// A nested cell — a DataFusion `array_agg`, say — is walked to its
    /// strings. A masked value hiding one level down is still a masked value.
    #[test]
    fn a_nested_cell_is_walked_to_its_strings() {
        let set = rules(r"\d{4}", MaskTarget::All);
        let columns = columns(&["agg"]);
        let mut rows = vec![vec![serde_json::json!([{"card": "4111"}, "x-2222"])]];
        assert!(mask_rows(&set, &columns, &mut rows));
        assert_eq!(rows[0][0], serde_json::json!([{"card": "•••"}, "x-•••"]));
    }

    #[test]
    fn a_result_set_with_no_rules_is_not_walked_at_all() {
        let columns = columns(&["value_text"]);
        let mut rows = vec![vec![serde_json::json!("value-2222")]];
        assert!(!mask_rows(&MaskSet::none(), &columns, &mut rows));
        assert_eq!(rows[0][0], serde_json::json!("value-2222"));
    }

    // ── The export notice ──────────────────────────────────────────────────

    /// The marker is the string somebody greps a file for to find out whether
    /// it was masked, so it comes from the core rather than from here.
    #[test]
    fn the_notice_carries_the_core_s_marker() {
        assert!(mask_notice_text().starts_with(MASK_NOTICE_MARKER));
    }

    #[test]
    fn an_unmasked_selection_gets_no_notice() {
        assert!(mask_notice_for(&[record(1), record(2)]).is_none());
    }

    #[test]
    fn one_masked_record_puts_the_notice_on_the_whole_file() {
        let mut records = vec![record(1), record(2)];
        records[1].masked = true;
        assert!(mask_notice_for(&records).is_some());
    }

    /// CSV has no comment syntax, so the notice is a `#` line above the header
    /// — visible in every tool that opens the file, which is the point.
    #[test]
    fn a_masked_csv_says_so_above_its_header_row() {
        let mut out = Vec::new();
        let mut records = vec![record(1)];
        records[0].masked = true;
        let notice = mask_notice_for(&records).expect("a masked selection");
        write_csv(&mut out, &records, Some(&notice)).expect("a Vec never fails to write");
        let text = String::from_utf8(out).expect("the writer only ever emits UTF-8");
        let first = text.lines().next().expect("a first line");
        assert!(first.starts_with("# "), "{first}");
        assert!(first.contains(MASK_NOTICE_MARKER), "{first}");
        // And the header row is still the header row, on line two.
        assert_eq!(
            text.lines().nth(1),
            Some("partition,offset,timestamp_ms,key_text,value_text,headers_json")
        );
    }

    /// The ARRAY SHAPE SURVIVES: a masked JSON export is still a list, so every
    /// reader that maps over it still works — the notice is its first element.
    #[test]
    fn a_masked_json_export_keeps_its_array_and_leads_with_the_notice() {
        let mut out = Vec::new();
        let records = vec![record(1), record(2)];
        write_json(&mut out, &records, Some(&mask_notice_text()))
            .expect("a Vec never fails to write");
        let parsed: serde_json::Value =
            serde_json::from_slice(&out).expect("the whole file is one JSON document");
        let array = parsed.as_array().expect("an array");
        assert_eq!(array.len(), 3);
        assert!(array[0][MASK_NOTICE_KEY]
            .as_str()
            .expect("the notice is a string")
            .contains(MASK_NOTICE_MARKER));
        assert_eq!(array[1]["offset"], 1);
        assert_eq!(array[2]["offset"], 2);
    }

    /// `head -1` is how somebody looks at an NDJSON file, so that is where the
    /// notice goes.
    #[test]
    fn a_masked_ndjson_export_leads_with_the_notice_line() {
        let mut out = Vec::new();
        write_ndjson(&mut out, &[record(1)], Some(&mask_notice_text()))
            .expect("a Vec never fails to write");
        let text = String::from_utf8(out).expect("the writer only ever emits UTF-8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value =
            serde_json::from_str(lines[0]).expect("the notice line is JSON");
        assert!(first[MASK_NOTICE_KEY]
            .as_str()
            .expect("a string")
            .contains(MASK_NOTICE_MARKER));
        let second: serde_json::Value =
            serde_json::from_str(lines[1]).expect("the record line is JSON");
        assert_eq!(second["offset"], 1);
    }

    /// AN UNMASKED EXPORT IS BYTE-FOR-BYTE WHAT IT ALWAYS WAS. The notice is
    /// additive and conditional; a session with no rules on must not have its
    /// files change shape.
    #[test]
    fn an_unmasked_export_is_unchanged_in_every_format() {
        let records = [record(1)];
        for (format, expected_lines) in [("json", 0usize), ("ndjson", 1), ("csv", 2)] {
            let mut out = Vec::new();
            match format {
                "json" => write_json(&mut out, &records, None),
                "ndjson" => write_ndjson(&mut out, &records, None),
                _ => write_csv(&mut out, &records, None),
            }
            .expect("a Vec never fails to write");
            let text = String::from_utf8(out).expect("the writer only ever emits UTF-8");
            assert!(
                !text.contains(MASK_NOTICE_MARKER),
                "{format} leaked a notice"
            );
            if expected_lines > 0 {
                assert_eq!(text.lines().count(), expected_lines, "{format}");
            }
        }
    }

    /// A masked result set is labelled the same way, once the caller says it is
    /// one — see the note on `export_rows` about the UI not sending it yet.
    #[test]
    fn a_masked_result_set_is_labelled_in_every_format() {
        let cols = columns(&["value_text"]);
        let rows = vec![vec![serde_json::json!("x")]];
        let notice = mask_notice_text();

        let mut csv = Vec::new();
        write_rows_csv(&mut csv, &cols, &rows, Some(&notice)).expect("a Vec never fails");
        let csv = String::from_utf8(csv).expect("UTF-8");
        assert!(csv
            .lines()
            .next()
            .expect("a line")
            .contains(MASK_NOTICE_MARKER));

        let mut json = Vec::new();
        write_rows_json(&mut json, &cols, &rows, Some(&notice)).expect("a Vec never fails");
        let parsed: serde_json::Value = serde_json::from_slice(&json).expect("one document");
        assert_eq!(parsed.as_array().expect("an array").len(), 2);

        let mut ndjson = Vec::new();
        write_rows_ndjson(&mut ndjson, &cols, &rows, Some(&notice)).expect("a Vec never fails");
        let ndjson = String::from_utf8(ndjson).expect("UTF-8");
        assert_eq!(ndjson.lines().count(), 2);
    }

    // ── The NL translator, as this file forwards it ────────────────────────

    /// The command's only decision: an unknown mode is the CORE's sentence,
    /// which names the two it accepts, rather than a serde error about an enum
    /// variant nobody typed.
    #[test]
    fn an_unknown_query_mode_is_refused_by_name() {
        let error = nl_to_query(
            "status is failed".into(),
            "sparql".into(),
            SchemaHint::default(),
        )
        .expect_err("sparql is not a mode Kavka has");
        assert!(error.contains("cel"), "{error}");
        assert!(error.contains("sql"), "{error}");
    }

    /// A translation is forwarded whole — the query, the confidence, the
    /// explanation and the phrases the grammar could not place. The grammar
    /// itself is the core's and is tested exhaustively there.
    #[test]
    fn a_translation_is_forwarded_with_all_four_fields() {
        let hint = SchemaHint {
            json_fields: vec!["status".to_string()],
        };
        let answer =
            nl_to_query("status is failed".into(), "cel".into(), hint).expect("cel is a mode");
        assert!(!answer.query.is_empty());
        assert!(!answer.explanation.is_empty());
    }

    /// The cheatsheet is a call rather than a copy for one reason: the core's
    /// own test checks it against the implementation, so a pattern the grammar
    /// grew and the doc did not mention fails the build.
    #[test]
    fn the_grammar_command_answers_with_the_core_s_text() {
        assert_eq!(nl_grammar(), kavka_core::nlq::NLQ_GRAMMAR);
    }

    // ── The playground ─────────────────────────────────────────────────────
    //
    // Almost nothing here is testable without a Docker daemon, and a unit test
    // that shells out to one is an integration test wearing a disguise. What
    // IS tested is the part that is pure string and policy work: which Docker
    // state produces which sentence, and which stream a failure is quoted
    // from — the two places this feature decides something rather than
    // forwarding it.

    /// docs/DESIGN.md §5.5's "every disabled control says why" applied to a
    /// state machine: a Docker state with no sentence is a first-run screen
    /// that offers nothing and explains nothing. This fails the day somebody
    /// adds a fifth variant and forgets the prose.
    #[test]
    fn every_docker_state_has_a_sentence_naming_the_next_click() {
        for state in [
            DockerState::Absent,
            DockerState::Stopped,
            DockerState::Remote,
            DockerState::NoCompose,
            DockerState::Ready,
        ] {
            let said = docker_trouble(state, None);
            assert!(!said.is_empty(), "{state:?} has no sentence");
        }
        // The four that are actually trouble each name what to do, not just
        // what is wrong (§7 rule 4).
        assert!(docker_trouble(DockerState::Absent, None).contains("Install Docker"));
        assert!(docker_trouble(DockerState::Stopped, None).contains("Start Docker"));
        assert!(docker_trouble(DockerState::NoCompose, None).contains("docker compose"));
        assert!(docker_trouble(DockerState::Remote, None).contains("docker context use default"));
    }

    /// The refusal NAMES THE HOST, because that is the only part of it the
    /// reader can act on — and it says why, because "Kavka won't" without a
    /// reason reads as a bug rather than as a guardrail.
    #[test]
    fn a_remote_endpoint_is_named_in_the_refusal() {
        let said = docker_trouble(DockerState::Remote, Some("tcp://build-07.internal:2376"));
        assert!(said.contains("tcp://build-07.internal:2376"), "{said}");
        assert!(said.contains("on THIS machine only"), "{said}");
        // …and with nothing to name it still refuses rather than proceeding.
        assert!(docker_trouble(DockerState::Remote, None).contains("another machine"));
    }

    /// The whole remote guard, as a pure function over its two inputs.
    ///
    /// This is the part worth testing without a daemon: the states above are
    /// prose, and `probe_docker` is three subprocesses, but WHICH ENDPOINT
    /// WINS and WHICH SCHEMES COUNT AS THIS MACHINE is policy — and getting
    /// either wrong is a container on somebody else's host.
    #[test]
    fn the_endpoint_kavka_would_start_a_container_on() {
        // Nothing set anywhere: the platform default, which is local.
        assert_eq!(resolve_endpoint(None, None), Endpoint::Local);
        // An empty string is Docker's own "unset", from either source.
        assert_eq!(resolve_endpoint(Some(""), Some("   ")), Endpoint::Local);
        // …and so is the Go template's answer for a field that is not there.
        // A guardrail that fires on its own instrumentation is a first-run
        // screen that refuses a perfectly local Docker.
        assert_eq!(resolve_endpoint(None, Some("<no value>")), Endpoint::Local);

        // The two local forms, from the context.
        assert_eq!(
            resolve_endpoint(None, Some("npipe:////./pipe/dockerDesktopLinuxEngine")),
            Endpoint::Local
        );
        assert_eq!(
            resolve_endpoint(None, Some("unix:///var/run/docker.sock")),
            Endpoint::Local
        );
        // Case is Docker's, not ours.
        assert_eq!(
            resolve_endpoint(None, Some("UNIX:///var/run/docker.sock")),
            Endpoint::Local
        );

        // A remote context, with the endpoint carried through verbatim.
        assert_eq!(
            resolve_endpoint(None, Some("tcp://build-07.internal:2376")),
            Endpoint::Remote("tcp://build-07.internal:2376".to_string())
        );
        assert_eq!(
            resolve_endpoint(None, Some("ssh://deploy@10.0.0.4")),
            Endpoint::Remote("ssh://deploy@10.0.0.4".to_string())
        );

        // DOCKER_HOST OVERRIDES THE CONTEXT, which is the case this exists for:
        // `docker context inspect` still reports the context's own endpoint,
        // so reading only that would call this machine local while every
        // command actually went to the build box.
        assert_eq!(
            resolve_endpoint(
                Some("tcp://build-07.internal:2376"),
                Some("npipe:////./pipe/docker_engine")
            ),
            Endpoint::Remote("tcp://build-07.internal:2376".to_string())
        );
        // …and the other way: an environment pointing at the local socket wins
        // over a remote context.
        assert_eq!(
            resolve_endpoint(
                Some("unix:///var/run/docker.sock"),
                Some("tcp://build-07.internal:2376")
            ),
            Endpoint::Local
        );

        // tcp:// to this very machine is still refused. A TCP daemon can be a
        // tunnel to anywhere, and this check does not guess.
        assert_eq!(
            resolve_endpoint(Some("tcp://localhost:2375"), None),
            Endpoint::Remote("tcp://localhost:2375".to_string())
        );
        // Surrounding whitespace is a shell profile's, not an endpoint's.
        assert_eq!(
            resolve_endpoint(Some("  tcp://10.1.2.3:2376  "), None),
            Endpoint::Remote("tcp://10.1.2.3:2376".to_string())
        );
    }

    /// A failure is quoted from stderr, which is where Docker and Compose put
    /// everything that explains one. Falling back to stdout matters just as
    /// much: `docker compose ps` answers on stdout, and a tail that only ever
    /// read stderr would show an empty `Show details` block.
    #[test]
    fn the_tail_prefers_stderr_and_falls_back_to_stdout() {
        let both = Ran {
            ok: false,
            spawn_failed: false,
            timed_out: false,
            out: vec!["container id".into()],
            err: vec!["Error response from daemon".into()],
        };
        assert_eq!(both.tail(4), "Error response from daemon");

        let quiet = Ran {
            ok: true,
            spawn_failed: false,
            timed_out: false,
            out: vec!["a".into(), "b".into(), "c".into()],
            err: Vec::new(),
        };
        // Bounded, and the LAST lines: a compose failure prints a hundred
        // lines of pull progress and the useful one is always at the end.
        assert_eq!(quiet.tail(2), "b\nc");
        assert_eq!(quiet.tail(99), "a\nb\nc");
    }

    /// The compose project name is pinned on every invocation, or `down` looks
    /// for a project `up` never created — see the constant's own comment.
    #[test]
    fn the_cli_hint_pins_the_project_and_quotes_the_path() {
        let hint = playground_cli_hint(Path::new("/Applications/Kavka.app/x/docker-compose.yml"));
        assert!(hint.contains(&format!("-p {PLAYGROUND_PROJECT}")), "{hint}");
        // Quoted, because the packaged path contains spaces on every platform
        // Kavka ships to, and a hint somebody has to repair is not a hint.
        assert!(hint.contains("-f \"/Applications/Kavka.app"), "{hint}");
    }

    // ── Diagnostics ────────────────────────────────────────────────────────
    //
    // The rotation is the one piece with a data-loss failure mode: get it
    // wrong and turning diagnostics on destroys the crash somebody turned it
    // on to keep. `LOG_DIR` is a `OnceLock` that only `setup` fills, so these
    // drive `append_log` directly rather than `diagnostics_write` — which is
    // also what keeps them from racing the process-wide toggle.

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kavka-diagnostics-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a scratch directory");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_entry_is_one_line_whatever_the_stack_trace_contains() {
        let scratch = Scratch::new();
        append_log(
            &scratch.0,
            "error",
            "TypeError: x\n  at f (a.js:1:1)\r\n  at g",
        );
        let text = std::fs::read_to_string(scratch.0.join(LOG_NAME)).expect("the log exists");
        // Exactly one newline: the trailing one. An embedded newline would
        // turn one entry into three and make the file unskimmable.
        assert_eq!(text.matches('\n').count(), 1, "{text:?}");
        assert!(
            text.starts_with("20"),
            "the line leads with a timestamp: {text}"
        );
        assert!(text.contains(" error "), "{text}");
        assert!(text.contains("at f (a.js:1:1)"), "{text}");
    }

    #[test]
    fn an_entry_is_capped_so_one_runaway_stack_cannot_fill_the_file() {
        let scratch = Scratch::new();
        append_log(&scratch.0, "error", &"x".repeat(LOG_MAX_ENTRY * 3));
        let text = std::fs::read_to_string(scratch.0.join(LOG_NAME)).expect("the log exists");
        assert!(text.len() < LOG_MAX_ENTRY + 64, "{} bytes", text.len());
    }

    /// Five files, and the oldest is the one that goes. The number is the
    /// About panel's claim about this feature's total disk cost, so it is
    /// asserted rather than assumed.
    #[test]
    fn rotation_keeps_five_files_and_drops_the_oldest() {
        let scratch = Scratch::new();
        // Six rotations against five slots: the sixth must not create a sixth
        // file, and the content that was in the oldest slot must be gone.
        for generation in 0..6 {
            append_log(&scratch.0, "note", &format!("generation-{generation}"));
            rotate_logs(&scratch.0);
        }
        append_log(&scratch.0, "note", "current");

        let files = log_files(&scratch.0);
        assert_eq!(files.len(), LOG_KEEP, "{files:?}");

        let read = |name: &str| std::fs::read_to_string(scratch.0.join(name)).unwrap_or_default();
        assert!(read(LOG_NAME).contains("current"));
        // Newest first after the current file: generation 5 rotated last.
        assert!(
            read("kavka.1.log").contains("generation-5"),
            "{:?}",
            read("kavka.1.log")
        );
        assert!(
            read("kavka.4.log").contains("generation-2"),
            "{:?}",
            read("kavka.4.log")
        );
        // Generations 0 and 1 fell off the end, which is the whole point.
        for n in 1..LOG_KEEP {
            let text = read(&format!("kavka.{n}.log"));
            assert!(
                !text.contains("generation-0"),
                "generation 0 survived in slot {n}"
            );
            assert!(
                !text.contains("generation-1"),
                "generation 1 survived in slot {n}"
            );
        }
    }

    #[test]
    fn writing_past_the_ceiling_rotates_rather_than_growing_one_file() {
        let scratch = Scratch::new();
        let file = scratch.0.join(LOG_NAME);
        std::fs::write(
            &file,
            vec![b'x'; usize::try_from(LOG_MAX_BYTES).unwrap_or(usize::MAX)],
        )
        .expect("a full log file");
        append_log(&scratch.0, "note", "after the ceiling");

        let current = std::fs::read_to_string(&file).expect("a fresh current file");
        assert!(current.contains("after the ceiling"), "{current}");
        // The full file was moved aside, not truncated: it is somebody's
        // evidence until it falls off the end of the five.
        let rotated = std::fs::metadata(scratch.0.join("kavka.1.log")).expect("the old file moved");
        assert_eq!(rotated.len(), LOG_MAX_BYTES);
    }

    /// A status is answerable before anything has ever been written — a person
    /// deciding whether to turn this on is entitled to know where the files
    /// would go, and an empty folder is a legitimate answer rather than an
    /// error.
    #[test]
    fn the_status_of_an_empty_folder_is_zero_files_and_a_path() {
        let scratch = Scratch::new();
        let status = read_diagnostics_status(&scratch.0);
        assert_eq!(status.files, 0);
        assert_eq!(status.bytes, 0);
        assert_eq!(status.dir, scratch.0.display().to_string());
    }

    /// Anything unreadable means OFF, because the safe answer to "may Kavka
    /// write a log?" is always no. A future build's richer file, a truncated
    /// write and a missing file all take the same branch.
    #[test]
    fn an_unreadable_preference_means_off() {
        let scratch = Scratch::new();
        assert!(!read_diagnostics_pref(&scratch.0), "a missing file is off");

        for text in ["", "{", "{}", "null", "{\"enabled\": \"yes\"}", "[]"] {
            std::fs::write(scratch.0.join(DIAGNOSTICS_FILE), text).expect("a preference file");
            assert!(
                !read_diagnostics_pref(&scratch.0),
                "{text:?} should read as off"
            );
        }

        write_diagnostics_pref(&scratch.0, true).expect("the preference is writable");
        assert!(read_diagnostics_pref(&scratch.0));
        write_diagnostics_pref(&scratch.0, false).expect("the preference is writable");
        assert!(!read_diagnostics_pref(&scratch.0));
    }

    /// This formatter is a deliberate second copy of `kavka_core::produce`'s
    /// private one, so it gets the same fixtures: the epoch, a leap day, and a
    /// date past 2038.
    #[test]
    fn the_log_timestamp_is_rfc3339_in_utc() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1_700_000_000_000), "2023-11-14T22:13:20.000Z");
        // 2024-02-29, the case a hand-rolled civil-from-days gets wrong.
        assert_eq!(iso8601(1_709_208_000_123), "2024-02-29T12:00:00.123Z");
        // Past the 32-bit second, which is the other one.
        assert_eq!(iso8601(2_500_000_000_000), "2049-03-22T04:26:40.000Z");
    }
}
