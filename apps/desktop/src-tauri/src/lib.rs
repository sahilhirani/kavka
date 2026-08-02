//! Tauri shell: thin IPC layer over kavka-core. Commands stay dumb — all
//! logic, validation, and read-only enforcement live in the core crate.

#[tauri::command]
fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![core_version])
        .run(tauri::generate_context!())
        .expect("error while running Kavka");
}
