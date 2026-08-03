//! Finding the app's config directory — the one holding `profiles.json`.
//!
//! **This server must read the file the desktop app writes, or it is a
//! different product.** The app gets that path from Tauri
//! (`app.path().app_config_dir()`, in `apps/desktop/src-tauri/src/lib.rs`),
//! which is `dirs::config_dir()` joined with the bundle identifier from
//! `tauri.conf.json`. Tauri is not in this crate's dependency list and must not
//! be (see the crate docs), so the same rule is stated here, in fifteen lines,
//! with a test that reads the identifier straight out of `tauri.conf.json` so
//! the two can never drift silently.
//!
//! The rule, per platform, is exactly `dirs::config_dir()/{identifier}`:
//!
//! | OS      | base                              |
//! |---------|-----------------------------------|
//! | Windows | `%APPDATA%` (roaming)             |
//! | macOS   | `$HOME/Library/Application Support`|
//! | Linux   | `$XDG_CONFIG_HOME`, else `$HOME/.config` |

use std::path::PathBuf;

/// Points the server at a different config directory. For tests, for a
/// second profile set, and for anyone whose app data lives somewhere unusual.
///
/// It changes which `profiles.json` is read and nothing else: secrets still
/// come from the OS keychain, which is per-user and has no equivalent knob.
pub const CONFIG_DIR_ENV: &str = "KAVKA_MCP_CONFIG_DIR";

/// The bundle identifier from `apps/desktop/src-tauri/tauri.conf.json`. The
/// last path segment of the app's config dir on every platform.
pub const APP_IDENTIFIER: &str = "io.kavka.desktop";

/// The config directory this server reads `profiles.json` from.
///
/// The error names the variable that fixes it, because the two ways this fails
/// — a stripped environment (a launcher with no `HOME`/`APPDATA`) and an
/// unusual layout — have the same one-line answer.
pub fn config_dir() -> Result<PathBuf, String> {
    // An empty override is a launcher passing through an unset variable, not a
    // request to read the current working directory.
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    base_config_dir()
        .map(|base| base.join(APP_IDENTIFIER))
        .ok_or_else(|| {
            format!(
                "can't tell where Kavka keeps its connections on this machine: {missing} is not \
                 set. Set {CONFIG_DIR_ENV} to the folder holding profiles.json — the Kavka app \
                 writes it under {APP_IDENTIFIER} in this user's config directory.",
                missing = MISSING_VAR,
            )
        })
}

/// `profiles.json` itself — for messages, so a "no connections" answer can say
/// which file it looked in.
pub fn profiles_file(dir: &std::path::Path) -> PathBuf {
    dir.join("profiles.json")
}

/// The environment variable named in the failure message, per platform.
#[cfg(windows)]
const MISSING_VAR: &str = "APPDATA";
#[cfg(target_os = "macos")]
const MISSING_VAR: &str = "HOME";
#[cfg(not(any(windows, target_os = "macos")))]
const MISSING_VAR: &str = "XDG_CONFIG_HOME (or HOME)";

/// `dirs::config_dir()`, by hand.
///
/// Hand-written rather than a dependency for the same reason kavka-core parses
/// the Prometheus text format itself: this is the whole of what `dirs` would
/// be used for, and it is three `env::var_os` calls. The one thing given up is
/// Windows' `SHGetKnownFolderPath`, which resolves the roaming folder even when
/// `%APPDATA%` has been unset or redirected by the parent process — a real
/// difference, and exactly what [`CONFIG_DIR_ENV`] is the answer to.
#[cfg(windows)]
fn base_config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn base_config_dir() -> Option<PathBuf> {
    home().map(|home| home.join("Library").join("Application Support"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn base_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(".config")))
}

#[cfg(not(windows))]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// The environment is process-wide and cargo runs tests on threads, so the
    /// two tests that move [`CONFIG_DIR_ENV`] take this first — otherwise one
    /// clears the variable the other just set.
    ///
    /// Poison-tolerant: what it guards is one environment variable, which a
    /// failed assertion elsewhere cannot corrupt.
    static ENV: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The tripwire the module docs promise: this crate's copy of the
    /// identifier is checked against the app's own `tauri.conf.json` at compile
    /// time. Renaming the bundle without touching this file would otherwise
    /// point the MCP server at an empty directory and answer "no connections"
    /// to a user who has ten.
    #[test]
    fn the_identifier_is_the_apps_own() {
        let conf: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/desktop/src-tauri/tauri.conf.json"
        ))
        .expect("tauri.conf.json is valid JSON");
        assert_eq!(
            conf["identifier"].as_str(),
            Some(APP_IDENTIFIER),
            "the app's bundle identifier changed; the MCP server reads \
             <config dir>/{APP_IDENTIFIER}/profiles.json"
        );
    }

    #[test]
    fn the_override_is_taken_verbatim() {
        let _guard = env_lock();
        let scratch = std::env::temp_dir().join("kavka-mcp-config-dir-test");
        std::env::set_var(CONFIG_DIR_ENV, &scratch);
        let resolved = config_dir().expect("an override always resolves");
        std::env::remove_var(CONFIG_DIR_ENV);
        assert_eq!(resolved, scratch);
    }

    #[test]
    fn the_discovered_directory_ends_with_the_identifier() {
        let _guard = env_lock();
        // Without the override, whatever the platform rule produced must end in
        // the bundle id — that is the half shared with Tauri's resolver.
        std::env::remove_var(CONFIG_DIR_ENV);
        if let Ok(dir) = config_dir() {
            assert_eq!(
                dir.file_name().and_then(|name| name.to_str()),
                Some(APP_IDENTIFIER),
                "{dir:?}"
            );
            assert!(dir.is_absolute(), "{dir:?}");
            assert_eq!(
                profiles_file(&dir).file_name().and_then(|n| n.to_str()),
                Some("profiles.json")
            );
        } else {
            // A stripped environment is a legitimate state (a launcher with no
            // HOME); the message has to name the way out.
            let message = config_dir().unwrap_err();
            assert!(message.contains(CONFIG_DIR_ENV), "{message}");
        }
    }
}
