//! Tauri shell: thin IPC layer over kavka-core. Commands stay dumb — all
//! logic, validation, and read-only enforcement live in the core crate.
//! Anything that blocks (file I/O, keychain, librdkafka calls — including
//! client drop) runs on the blocking pool, never the event loop.

use kavka_core::acl::{AclBinding, AclFilter};
use kavka_core::admin::{
    self, ConfigEntry, GroupDetail, GroupInfo, GroupOffset, OffsetResetSpec, TopicConfig,
    TopicDetail, TopicInfo,
};
use kavka_core::cancel::CancelToken;
use kavka_core::connect::{ConfigValidation, ConnectorSummary};
use kavka_core::connection::{ClusterConnection, ClusterOverview};
use kavka_core::consume::{self, FetchSpec, TailSession};
use kavka_core::produce::{self, BulkSession, BulkSpec, Delivery, ProduceRecordSpec};
use kavka_core::profiles::{
    export_json, import_json, ConnectionProfile, ImportReport, ImportStrategy, ProfileStore,
};
// `TopicPartition` here is the protocol module's — `admin` has an
// identically-shaped one for group assignments, which is why it is reached
// through `admin::` everywhere rather than imported alongside this.
use kavka_core::protocol::{
    PartitionResult, ProtocolClient, QuorumInfo, QuotaEntity, QuotaEntityPart, QuotaOp,
    ReassignmentSpec, ReassignmentState, TopicPartition,
};
use kavka_core::search::{SearchSession, SearchSpec};
use kavka_core::serdes::MessageRecord;
use kavka_core::sr::{CompatibilityCheck, CompatibilityInForce, RegisteredId, SubjectVersion};
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

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
        ProfileSessions {
            tails: self.tails.take_of(profile_id),
            searches: self.searches.take_of(profile_id),
            bulks: self.bulks.take_of(profile_id),
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
    let sessions = state.take_sessions_of(&profile_id);
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    let protocol = state.take_protocol(&profile_id);
    let store = state.store.clone();
    blocking(move || {
        drop(sessions); // joins each worker thread, off the event loop
        drop(conn); // librdkafka client destroy, off the event loop
        drop(protocol); // and the kept-alive wire-protocol socket

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
    state: State<'_, AppState>,
    profile_id: String,
) -> CmdResult<ClusterOverview> {
    let store = state.store.clone();
    let id = profile_id.clone();
    let (conn, overview) = blocking(move || {
        let profile = store
            .list()?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| kavka_core::Error::Other(format!("unknown profile: {id}")))?;
        let conn = ClusterConnection::connect(profile)?;
        let overview = conn.overview()?;
        Ok((Arc::new(conn), overview))
    })
    .await?;

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
    // fetch that is still polling.
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
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AppState {
                store: Arc::new(ProfileStore::new(dir)),
                connections: Mutex::new(HashMap::new()),
                tails: SessionMap::new(),
                searches: SessionMap::new(),
                bulks: SessionMap::new(),
                ready: Mutex::new(HashMap::new()),
                fetches: Mutex::new(HashMap::new()),
                protocol: Mutex::new(HashMap::new()),
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
/// command hands its arguments to the core and its answer back. The exception
/// is the export writer, which is the one place the shell decides what bytes a
/// user ends up with, and a quoting bug there is a corrupted file rather than a
/// visible error.
#[cfg(test)]
mod tests {
    use super::*;
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
}
