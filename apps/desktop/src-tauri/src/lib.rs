//! Tauri shell: thin IPC layer over kavka-core. Commands stay dumb — all
//! logic, validation, and read-only enforcement live in the core crate.
//! Anything that blocks (file I/O, keychain, librdkafka calls — including
//! client drop) runs on the blocking pool, never the event loop.

use kavka_core::admin::{
    self, GroupDetail, GroupInfo, GroupOffset, OffsetResetSpec, TopicConfig, TopicDetail, TopicInfo,
};
use kavka_core::cancel::CancelToken;
use kavka_core::connection::{ClusterConnection, ClusterOverview};
use kavka_core::consume::{self, FetchSpec, TailSession};
use kavka_core::profiles::{
    export_json, import_json, ConnectionProfile, ImportReport, ImportStrategy, ProfileStore,
};
use kavka_core::serdes::MessageRecord;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// How long a tail's reader waits for records before looking at the world
/// again. Also the worst-case latency of `tail_stop` and of the `ended`
/// payload, which is why it is short rather than "as long as the topic is
/// quiet".
const TAIL_POLL: Duration = Duration::from_millis(500);

struct AppState {
    store: Arc<ProfileStore>,
    connections: Mutex<HashMap<String, Arc<ClusterConnection>>>,
    /// Live tails, keyed by the id their events are addressed to. Each entry
    /// remembers the profile it belongs to, so disconnecting a cluster — or
    /// deleting it — takes its tails down with it instead of leaving a
    /// consumer fetching from a cluster nobody is looking at any more.
    tails: Mutex<HashMap<String, TailHandleEntry>>,
    tail_seq: AtomicU64,
    tail_epoch: u64,
    /// The in-flight `messages_fetch` of each profile, so a newer browse can
    /// stop the one it replaces. A fetch is interactive and can hold a
    /// blocking-pool slot for the core's full 30s deadline: the moment the
    /// user changes the range, that slot is being spent on an answer the UI
    /// has already decided to throw away (`fetchSeq` in MessagesView).
    fetches: Mutex<HashMap<String, CancelToken>>,
}

struct TailHandleEntry {
    profile_id: String,
    session: Arc<TailSession>,
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
    /// event names it addresses — a tail id never leaves this machine or
    /// outlives the app, so a counter plus the start time answers "keep two
    /// browsers of the same topic apart" without a uuid dependency.
    fn next_tail_id(&self) -> String {
        let seq = self.tail_seq.fetch_add(1, Ordering::Relaxed);
        format!("{:x}-{seq:x}", self.tail_epoch)
    }

    /// Removes one tail and asks it to finish. The handle comes back so the
    /// caller can destroy it off the event loop: the last reference joins the
    /// core's reader thread.
    fn take_tail(&self, tail_id: &str) -> Option<Arc<TailSession>> {
        let entry = self.tails.lock().unwrap().remove(tail_id)?;
        entry.session.stop();
        Some(entry.session)
    }

    /// Same, for every tail belonging to one profile.
    fn take_tails_of(&self, profile_id: &str) -> Vec<Arc<TailSession>> {
        let mut taken = Vec::new();
        self.tails.lock().unwrap().retain(|_, entry| {
            if entry.profile_id != profile_id {
                return true;
            }
            entry.session.stop();
            // Cloned before the entry goes: dropping the last reference here
            // would join a reader thread while holding this lock, and the
            // reader takes the same lock to retire itself.
            taken.push(Arc::clone(&entry.session));
            false
        });
        taken
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

    /// Sets every live tail's stop flag and joins nothing. This runs on the way
    /// out of the event loop, where waiting on a broker is the one thing that
    /// must not happen — the reader threads are detached and the process is
    /// about to end regardless.
    fn stop_all_tails(&self) {
        for entry in self.tails.lock().unwrap().values() {
            entry.session.stop();
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

/// Destroys a stopped session off the event loop. Dropping the last reference
/// joins the core's reader thread, which is a librdkafka client teardown.
async fn retire_tail(session: Arc<TailSession>) -> CmdResult<()> {
    session.stop();
    tauri::async_runtime::spawn_blocking(move || drop(session))
        .await
        .map_err(|e| e.to_string())
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

/// One live tail's reader loop.
///
/// Runs on a dedicated OS thread rather than the blocking pool: this loop lives
/// for as long as someone watches the topic, and a pool slot held for an hour
/// is a slot every keychain read and metadata fetch queues behind. The thread
/// is detached and never joined at exit, and `TailSession::stop` unblocks
/// `next_batch`, so shutting down is never a wait.
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
        let retired = state.tails.lock().unwrap().remove(tail_id);
        // Dropped after the guard, deliberately: a last-reference drop joins a
        // reader thread, and this thread's own handle is still alive anyway.
        drop(retired);
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
    // A tail that outlives the profile it belongs to is a consumer reading a
    // cluster the user just deleted, feeding a view that can never be reopened.
    let tails = state.take_tails_of(&profile_id);
    state.cancel_fetch(&profile_id);
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    let store = state.store.clone();
    blocking(move || {
        drop(tails); // joins each reader thread, off the event loop
        drop(conn); // librdkafka client destroy, off the event loop
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

    let replaced = state.connections.lock().unwrap().insert(profile_id, conn);
    if let Some(old) = replaced {
        // Reconnect over an existing session: destroy the old client off-loop.
        tauri::async_runtime::spawn_blocking(move || drop(old));
    }
    Ok(overview)
}

#[tauri::command]
async fn cluster_disconnect(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    // Tails first: each owns its own consumer, so disconnecting without them
    // leaves live sessions emitting into a UI that thinks it is offline. The
    // same argument applies to a fetch that is still polling.
    let tails = state.take_tails_of(&profile_id);
    state.cancel_fetch(&profile_id);
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    if !tails.is_empty() || conn.is_some() {
        tauri::async_runtime::spawn_blocking(move || {
            drop(tails);
            drop(conn);
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

    let tail_id = state.next_tail_id();
    state.tails.lock().unwrap().insert(
        tail_id.clone(),
        TailHandleEntry {
            profile_id,
            session: Arc::clone(&session),
        },
    );

    let spawned = std::thread::Builder::new()
        .name("kavka-tail-emit".into())
        .spawn({
            let tail_id = tail_id.clone();
            move || pump_tail(&app, &tail_id, &session)
        });
    if let Err(e) = spawned {
        // Nothing will ever drain this session, so it must not be left running.
        if let Some(orphan) = state.take_tail(&tail_id) {
            retire_tail(orphan).await?;
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
    match state.take_tail(&tail_id) {
        Some(session) => retire_tail(session).await,
        None => Ok(()),
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AppState {
                store: Arc::new(ProfileStore::new(dir)),
                connections: Mutex::new(HashMap::new()),
                tails: Mutex::new(HashMap::new()),
                fetches: Mutex::new(HashMap::new()),
                tail_seq: AtomicU64::new(0),
                tail_epoch: std::time::SystemTime::now()
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
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Kavka");

    app.run(|handle, event| {
        // Ask every live tail to finish on the way out. Nothing is joined here:
        // the reader threads are detached, `stop` unblocks them, and quitting
        // the app must never wait on a broker.
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = handle.try_state::<AppState>() {
                state.stop_all_tails();
            }
        }
    });
}
