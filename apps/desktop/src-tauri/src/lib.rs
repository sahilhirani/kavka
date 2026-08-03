//! Tauri shell: thin IPC layer over kavka-core. Commands stay dumb — all
//! logic, validation, and read-only enforcement live in the core crate.
//! Anything that blocks (file I/O, keychain, librdkafka calls — including
//! client drop) runs on the blocking pool, never the event loop.

use kavka_core::admin::TopicInfo;
use kavka_core::connection::{ClusterConnection, ClusterOverview};
use kavka_core::profiles::{ConnectionProfile, ProfileStore};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

struct AppState {
    store: Arc<ProfileStore>,
    connections: Mutex<HashMap<String, Arc<ClusterConnection>>>,
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
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    let store = state.store.clone();
    blocking(move || {
        drop(conn); // librdkafka client destroy, off the event loop
        store.delete(&profile_id)?;
        // Best-effort purge: an orphaned keychain entry is harmless, a ghost
        // profile in the UI is not — so a purge failure doesn't fail the
        // delete.
        let _ = kavka_core::secrets::delete(&format!("{profile_id}/password"));
        Ok(())
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
    let conn = state.connections.lock().unwrap().remove(&profile_id);
    if let Some(conn) = conn {
        tauri::async_runtime::spawn_blocking(move || drop(conn))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn topics_list(state: State<'_, AppState>, profile_id: String) -> CmdResult<Vec<TopicInfo>> {
    let conn = state
        .connections
        .lock()
        .unwrap()
        .get(&profile_id)
        .cloned()
        .ok_or_else(|| format!("not connected: {profile_id}"))?;
    blocking(move || conn.list_topics()).await
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AppState {
                store: Arc::new(ProfileStore::new(dir)),
                connections: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            profiles_list,
            profiles_save,
            profiles_delete,
            secret_set,
            secret_delete,
            cluster_connect,
            cluster_disconnect,
            topics_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kavka");
}
