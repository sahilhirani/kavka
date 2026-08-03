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
use kavka_core::metrics::{MetricPoint, MetricsCollector, MetricsStatus};
use kavka_core::produce::{self, BulkSession, BulkSpec, Delivery, ProduceRecordSpec};
use kavka_core::profiles::{
    export_json, import_json, ConnectionProfile, ImportReport, ImportStrategy, ProfileStore,
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
use kavka_core::sr::{CompatibilityCheck, CompatibilityInForce, RegisteredId, SubjectVersion};
use kavka_core::streams::StreamsTopology;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

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
    id_seq: AtomicU64,
    id_epoch: u64,
    /// The subscribe handshake of every session that has not started emitting
    /// yet, keyed by the same id its events are addressed to (see
    /// [`READY_TIMEOUT`]). Searches and bulk runs share it because they share
    /// the id namespace and the race.
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
/// lets one [`SessionMap`] keep the books for tails, searches and bulk runs
/// instead of three copies of the same bookkeeping drifting apart — and the
/// lifecycle rules here (stop before removing, drop off the event loop, take a
/// profile's sessions with the profile) are the ones that must not drift.
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
    session: Arc<T>,
}

impl<T: Stoppable> SessionMap<T> {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, id: String, profile_id: &str, session: &Arc<T>) {
        self.entries.lock().unwrap().insert(
            id,
            SessionEntry {
                profile_id: profile_id.to_string(),
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

    /// Same, for every session belonging to one profile.
    fn take_of(&self, profile_id: &str) -> Vec<Arc<T>> {
        let mut taken = Vec::new();
        self.entries.lock().unwrap().retain(|_, entry| {
            if entry.profile_id != profile_id {
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
}

impl ProfileSessions {
    fn is_empty(&self) -> bool {
        self.tails.is_empty() && self.searches.is_empty() && self.bulks.is_empty()
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
    /// serves all three kinds: they share a namespace, so a mixed-up id is a
    /// miss rather than a collision.
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
fn pump_tail(app: &AppHandle, tail_id: &str, session: &TailSession) {
    let event = tail_event(tail_id);
    let mut reported_drops = 0;
    // `None` is the end of the session; `Some(empty)` is a quiet topic, which
    // is a state, not an ending.
    while let Some(records) = session.next_batch(TAIL_POLL) {
        let dropped = session.dropped();
        // A quiet topic costs nothing: the view says "listening" from its own
        // timer, and 2 empty events a second per open tail is pure IPC.
        if records.is_empty() && dropped == reported_drops {
            continue;
        }
        reported_drops = dropped;
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
    let _ = app.emit(
        &event,
        TailBatch {
            records: Vec::new(),
            dropped: session.dropped(),
            ended: true,
        },
    );
}

#[tauri::command]
fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn profiles_list(state: State<'_, AppState>) -> CmdResult<Vec<ConnectionProfile>> {
    let store = state.store.clone();
    blocking(move || store.list()).await
}

#[tauri::command]
async fn profiles_save(state: State<'_, AppState>, profile: ConnectionProfile) -> CmdResult<()> {
    let store = state.store.clone();
    blocking(move || store.upsert(profile)).await
}

#[tauri::command]
async fn profiles_delete(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    // A session that outlives the profile it belongs to is a client working a
    // cluster the user just deleted, feeding a view that can never be reopened.
    // `take_sessions_of` stops the monitor too, so nothing is still sampling
    // into a history file that is about to be removed.
    let sessions = state.take_sessions_of(&profile_id);
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    let protocol = state.take_protocol(&profile_id);
    let history = state.histories.take(&profile_id);
    let history_path = state.histories.path_of(&profile_id);
    let histories = Arc::clone(&state.histories);
    let alerts = Arc::clone(&state.alerts);
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
    // leaves tails emitting into a UI that thinks it is offline, searches
    // fetching, and bulk runs still writing. The same argument applies to a
    // fetch that is still polling — and to the monitor, which `take_sessions_of`
    // stops with them. Sampling is a thing Kavka does *while you are watching a
    // cluster*, and a sampler that outlived the disconnect would keep a broker
    // answering `ListGroups` for a window nobody has open.
    //
    // The history file itself stays open and stays put: it is on disk, pruned at
    // seven days, and the Monitoring tab can read last week for a profile nobody
    // is connected to. Only deleting the profile takes it away.
    let sessions = state.take_sessions_of(&profile_id);
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
    let token = state.begin_fetch(&profile_id);
    let running = token.clone();
    let result = blocking(move || {
        // The registry is the profile's, not a global: two clusters can have
        // different registries, and one of them can have none.
        consume::fetch_messages(
            &conn,
            conn.profile().schema_registry.as_ref(),
            &spec,
            Some(&running),
        )
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
        move || pump_tail(&app, &tail_id, &session)
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
fn pump_search(app: &AppHandle, search_id: &str, session: &SearchSession) {
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
    while let Some(records) = session.next_results(SEARCH_POLL) {
        if !records.is_empty() {
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
        move || pump_search(&app, &search_id, &session)
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
/// command serves searches and bulk runs because both are keyed by the same id
/// namespace and both have the same race.
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
        ExportFormat::Csv => write_csv(&mut out, records),
        ExportFormat::Json => write_json(&mut out, records),
        ExportFormat::Ndjson => write_ndjson(&mut out, records),
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
fn write_csv(out: &mut impl Write, records: &[MessageRecord]) -> std::io::Result<()> {
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
fn write_json(out: &mut impl Write, records: &[MessageRecord]) -> std::io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, records)?;
    out.write_all(b"\n")
}

fn write_ndjson(out: &mut impl Write, records: &[MessageRecord]) -> std::io::Result<()> {
    for record in records {
        serde_json::to_writer(&mut *out, record)?;
        out.write_all(b"\n")?;
    }
    Ok(())
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
            let histories = Arc::new(HistoryStores::new(
                app.path().app_data_dir()?.join("history"),
            ));
            app.manage(AppState {
                store: Arc::new(ProfileStore::new(dir)),
                connections: Mutex::new(HashMap::new()),
                tails: SessionMap::new(),
                searches: SessionMap::new(),
                bulks: SessionMap::new(),
                ready: Mutex::new(HashMap::new()),
                fetches: Mutex::new(HashMap::new()),
                protocol: Mutex::new(HashMap::new()),
                alerts,
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
            produce_send,
            produce_bulk,
            bulk_stop,
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
/// command hands its arguments to the core and its answer back. There are two
/// exceptions, and they are the two places the shell decides something rather
/// than forwarding it: the export writer, where a quoting bug is a corrupted
/// file rather than a visible error, and the monitor loop's own arithmetic —
/// how long it waits after a failure, what it calls a failure, and what it
/// publishes while it is doing so. Everything the loop *does* (sampling,
/// scraping, evaluating) belongs to the core and is tested there.
#[cfg(test)]
mod tests {
    use super::*;
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
        }
    }

    fn csv_of(records: &[MessageRecord]) -> String {
        let mut out = Vec::new();
        write_csv(&mut out, records).expect("a Vec never fails to write");
        String::from_utf8(out).expect("the writer only ever emits UTF-8")
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
        write_ndjson(&mut out, &[record(1), record(2)]).expect("a Vec never fails to write");
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
        write_json(&mut out, &[record(1), record(2)]).expect("a Vec never fails to write");
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
}
