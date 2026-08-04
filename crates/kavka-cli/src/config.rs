//! Finding the app's config directory — the one holding `profiles.json`.
//!
//! **This program must read the file the desktop app writes, or it is a
//! different product.** The app gets that path from Tauri
//! (`app.path().app_config_dir()`, in `apps/desktop/src-tauri/src/lib.rs`),
//! which is `dirs::config_dir()` joined with the bundle identifier from
//! `tauri.conf.json`. Tauri is not in this crate's dependency list and must not
//! be (see the crate docs), so the same rule is stated here, in fifteen lines.
//!
//! The rule, per platform, is exactly `dirs::config_dir()/{identifier}`:
//!
//! | OS      | base                              |
//! |---------|-----------------------------------|
//! | Windows | `%APPDATA%` (roaming)             |
//! | macOS   | `$HOME/Library/Application Support`|
//! | Linux   | `$XDG_CONFIG_HOME`, else `$HOME/.config` |
//!
//! # This is the THIRD copy of that rule, and it is tripwired to the other two
//!
//! The app owns it, `kavka-mcp`'s `config.rs` restates it, and so does this.
//! Three copies of a path rule is how a CLI ends up answering "no connections"
//! to a user who has ten — so the tests at the bottom of this file read
//! `apps/desktop/src-tauri/tauri.conf.json` **and** `crates/kavka-mcp/src/config.rs`
//! at compile time and fail if either identifier moves. The duplication buys
//! two things a shared module would cost: this crate depends on neither Tauri
//! nor the MCP server (a CLI that pulls in an MCP server to find a folder is
//! the wrong shape), and each front end names its own override variable, which
//! is the one part of the rule that genuinely differs.
//!
//! Promoting the shared half into `kavka_core::config` is the right end state
//! and is a change to kavka-core, not to this crate.

use std::path::PathBuf;

/// Points the CLI at a different config directory. For tests, for a second
/// profile set, and for anyone whose app data lives somewhere unusual.
///
/// It changes which `profiles.json` (and the `masking.json` and
/// `environments.json` beside it) is read and nothing
/// else: secrets still come from the OS keychain, which is per-user and has no
/// equivalent knob.
pub const CONFIG_DIR_ENV: &str = "KAVKA_CONFIG_DIR";

/// The bundle identifier from `apps/desktop/src-tauri/tauri.conf.json`. The
/// last path segment of the app's config dir on every platform.
pub const APP_IDENTIFIER: &str = "io.kavka.desktop";

/// The config directory this program reads `profiles.json` from.
///
/// The error names the variable that fixes it, because the two ways this fails
/// — a stripped environment (a cron job with no `HOME`/`APPDATA`) and an
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

/// `dirs::config_dir()`, by hand — three `env::var_os` calls, which is the
/// whole of what the crate would be used for here.
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
    /// tests that move [`CONFIG_DIR_ENV`] take this first.
    ///
    /// Poison-tolerant: what it guards is one environment variable, which a
    /// failed assertion elsewhere cannot corrupt.
    static ENV: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Tripwire one: the app's own bundle identifier, read at compile time.
    #[test]
    fn the_identifier_is_the_apps_own() {
        let conf: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/desktop/src-tauri/tauri.conf.json"
        ))
        .expect("tauri.conf.json is valid JSON");
        assert_eq!(
            conf["identifier"].as_str(),
            Some(APP_IDENTIFIER),
            "the app's bundle identifier changed; the CLI reads \
             <config dir>/{APP_IDENTIFIER}/profiles.json"
        );
    }

    /// Tripwire two: the MCP server resolves the same directory. Two front ends
    /// of one app that disagree about where its connections live is a bug
    /// neither of them can see from the inside.
    #[test]
    fn the_mcp_server_looks_in_the_same_place() {
        let mcp = include_str!("../../kavka-mcp/src/config.rs");
        assert!(
            mcp.contains(&format!("APP_IDENTIFIER: &str = \"{APP_IDENTIFIER}\"")),
            "kavka-mcp's config.rs no longer names {APP_IDENTIFIER}"
        );
        // And the platform rule itself: the three bases, spelled the same way.
        for base in [
            "APPDATA",
            "Library",
            "Application Support",
            "XDG_CONFIG_HOME",
            ".config",
        ] {
            assert!(
                mcp.contains(base),
                "kavka-mcp's config.rs no longer uses {base}"
            );
        }
    }

    #[test]
    fn the_override_is_taken_verbatim() {
        let _guard = env_lock();
        let scratch = std::env::temp_dir().join("kavka-cli-config-dir-test");
        std::env::set_var(CONFIG_DIR_ENV, &scratch);
        let resolved = config_dir().expect("an override always resolves");
        std::env::remove_var(CONFIG_DIR_ENV);
        assert_eq!(resolved, scratch);
    }

    #[test]
    fn the_discovered_directory_ends_with_the_identifier() {
        let _guard = env_lock();
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
            // A stripped environment is a legitimate state; the message has to
            // name the way out.
            let message = config_dir().unwrap_err();
            assert!(message.contains(CONFIG_DIR_ENV), "{message}");
        }
    }
}
