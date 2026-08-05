//! Updates: what Kavka knows about newer versions of itself, and the only
//! network request the app makes on its own.
//!
//! **Two channels, two different questions.** They are not two flavours of one
//! query, and they do not share a source of truth:
//!
//! - **stable** asks GitHub's `releases/latest` redirect for the signed
//!   `latest.json` a human published. No plain `v*` tag has ever been pushed,
//!   so that endpoint 404s today — and [`UpdateCheck::NoStableRelease`] says so
//!   plainly, because "nobody has cut a stable release yet" is a fact about the
//!   project, not a failure of the check. It is a claim about the project,
//!   though, so it is only ever made about an OBSERVED 404 — see
//!   [`stable_absence`], which exists because the plugin reports a rate limit
//!   and an outage under the same name as a genuine absence.
//! - **builds** asks the GitHub REST API which `v<version>-build.<run>`
//!   prereleases exist. Every merge to main mints one (see
//!   `.github/workflows/release.yml`), and they ALL carry the same
//!   `tauri.conf.json` version — `0.1.0` — because MSI/WiX refuses a prerelease
//!   string in a product version. What tells build 1 from build 400 is the run
//!   number compiled into the binary as `KAVKA_BUILD`.
//!
//! That last point is why this module exists rather than two calls to
//! `app.updater()`. The updater plugin decides "is there an update?" by
//! comparing the `version` field of `latest.json` against the running version,
//! and for the builds channel that field reads `0.1.0` on every single build.
//! It cannot order them, so it is not asked to: the channel logic here makes
//! the decision, and the plugin is handed the answer.
//!
//! **Nothing here installs anything on its own.** [`check`] is a read and
//! returns a verdict; [`install`] runs only because somebody clicked Install.
//! Both are safe to call concurrently — every call builds its own client and
//! nothing in this module is shared or mutable.

use std::cmp::Ordering;
use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The repository every release comes from. Also the repository the release
/// workflow publishes to — the two must agree, and there is no configuration
/// that lets a user point Kavka's updater somewhere else, because "install
/// whatever this URL says, it is signed by a key I trust" is the whole security
/// model and a redirectable endpoint would be a hole in it.
const REPO: &str = "sahilhirani/kavka";

/// The stable manifest. GitHub resolves `releases/latest` to the newest
/// NON-prerelease release, which is exactly the stable channel's definition;
/// while none exists it answers 404, and the plugin turns that into
/// `ReleaseNotFound` — along with every other answer it could not read, which
/// is why [`stable_absence`] goes and looks at the status itself.
const STABLE_MANIFEST: &str =
    "https://github.com/sahilhirani/kavka/releases/latest/download/latest.json";

/// The GitHub REST API root for the builds channel.
const GITHUB_API: &str = "https://api.github.com";

/// api.github.com rejects requests without a User-Agent. This one names the
/// program and nothing else — no version, no machine, no install id, nothing
/// that could tell two Kavka users apart. The About screen's copy says exactly
/// that, and it has to stay true.
const USER_AGENT: &str = "Kavka (+https://github.com/sahilhirani/kavka)";

/// How long any single update request may take before Kavka gives up. Short
/// enough that a dead network is a message rather than a spinner nobody can
/// cancel; long enough for a slow connection to fetch a small JSON file.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);

/// How many releases the builds channel reads. The run number is monotonic and
/// GitHub returns releases newest-first, so one page is far more than enough —
/// it is a page rather than a single release because the newest release may be
/// a stable tag, and the newest *build* can sit a few entries down.
const RELEASES_PER_PAGE: usize = 30;

/// The asset the updater plugin needs from a build release.
const MANIFEST_ASSET: &str = "latest.json";

// ── test hooks ─────────────────────────────────────────────────────────────
//
// TEST-ONLY, both of them, and read at call time rather than cached so a test
// can set them after the app is up:
//
//   KAVKA_UPDATE_ENDPOINT  replaces the stable latest.json URL
//   KAVKA_UPDATE_API       replaces the GitHub API root for the builds channel
//
// Nothing in the shipped UI sets either, and neither is documented to users.
// They exist so the update flow can be driven against a local fixture server
// instead of against real GitHub releases — which is the only way to test
// "there is a newer build" without publishing one. An http:// endpoint also
// needs `dangerousInsecureTransportProtocol`, which the shipped
// tauri.conf.json deliberately does NOT set; a test passes it with
// `--config`, so a release build can never be talked into plain HTTP.

/// An environment override, or `None` when it is unset, empty, or blank.
fn env_override(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn stable_manifest_url() -> String {
    env_override("KAVKA_UPDATE_ENDPOINT").unwrap_or_else(|| STABLE_MANIFEST.to_string())
}

fn api_base() -> String {
    let base = env_override("KAVKA_UPDATE_API").unwrap_or_else(|| GITHUB_API.to_string());
    base.trim_end_matches('/').to_string()
}

// ── the contract with the UI ───────────────────────────────────────────────

/// Which question to ask. Deserialised straight from the webview, so an
/// unknown string is a deserialisation error rather than a silent fallback to
/// one channel or the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Builds,
}

/// The answer. Every outcome — including failure — is a variant rather than a
/// rejected promise, because the UI has something honest to say in all four
/// cases and a thrown error would only give it a stack trace to render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum UpdateCheck {
    /// There is something newer. `url` is the release page for a human to read
    /// — the download URL is the updater plugin's business, not the UI's.
    Update {
        version: String,
        notes: Option<String>,
        url: String,
    },
    /// Nothing newer on this channel.
    Current,
    /// The stable endpoint 404s: no stable release has ever been published.
    /// Distinct from an error because nothing went wrong.
    NoStableRelease,
    /// Something failed. `message` is a finished sentence, already fit to show.
    Error { message: String },
}

// ── which build this is ────────────────────────────────────────────────────

/// The run number this binary was built from, or `None` for a stable or local
/// build.
///
/// Compile-time on purpose. `KAVKA_BUILD` is set by the release workflow and
/// baked in, so a build install cannot be talked into claiming it is a
/// different build by an environment variable at runtime — which matters,
/// because this number is half of the builds channel's "is there anything
/// newer" arithmetic.
pub fn build_number() -> Option<u64> {
    parse_build_number(option_env!("KAVKA_BUILD"))
}

/// `KAVKA_BUILD` as a number. Absent, empty, blank and non-numeric all mean
/// "not a build install" — the workflow sets it to `""` on paths that are not
/// automated builds, and `""` must not become build zero.
fn parse_build_number(raw: Option<&str>) -> Option<u64> {
    parse_run_number(raw?.trim())
}

/// A run number: ASCII digits and nothing else. Rejects `+7`, `-1`, `7.0`,
/// `0x10` and anything wide enough to overflow, all of which `str::parse`
/// alone would either accept or panic-adjacent surprise us with.
fn parse_run_number(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

/// A build tag split into the version it was cut from and its run number —
/// `v0.1.0-build.42` becomes `(0.1.0, 42)`.
///
/// The shape is the release workflow's, and this is deliberately strict about
/// it: anything that is not exactly `v<semver>-build.<digits>` is not a build
/// release, and the builds channel skips it rather than guessing. Stable tags
/// (`v0.1.0`), branch junk, and whatever a future workflow invents all fall
/// out here as `None`.
fn parse_build_tag(tag: &str) -> Option<(Version, u64)> {
    // `rsplit_once` rather than `split_once`, matching the greedy `(.+)` of
    // `^v(.+)-build\.(\d+)$`: a version that itself contains the separator
    // belongs to the base, not to the run number.
    let (base, run) = tag.strip_prefix('v')?.rsplit_once("-build.")?;
    Some((Version::parse(base).ok()?, parse_run_number(run)?))
}

// ── the two channel decisions ──────────────────────────────────────────────

/// Does the stable channel offer `remote` to an install running `current`?
///
/// Two rules, and the second one is the reason this is not the plugin's
/// default comparator:
///
/// 1. A higher version is an update. Obviously.
/// 2. The SAME version is an update when this install carries a build number.
///    A build install is `0.1.0` plus some run number of main; when `0.1.0`
///    itself is finally released, that stable build is the thing this install
///    should move to — it is the reviewed one. Refusing it would strand every
///    build user on main forever.
fn stable_offers(current: &Version, remote: &Version, build: Option<u64>) -> bool {
    match remote.cmp(current) {
        Ordering::Greater => true,
        Ordering::Equal => build.is_some(),
        Ordering::Less => false,
    }
}

/// Does the builds channel offer a release cut from `version` at run `run`?
///
/// Which comparison applies depends on what this install is:
///
/// - **A build install** compares run numbers. The run number is a GitHub
///   Actions counter and only ever goes up, so "newer" is arithmetic rather
///   than semver — which is the whole point, since every build shares one
///   version string.
/// - **A stable install** has no run number to compare, so the only honest
///   reason to offer it a build is that the build was cut from a LATER version
///   than it is running. Offering `0.1.0-build.400` to a `0.1.0` stable
///   install would be handing somebody unreviewed main and calling it an
///   upgrade.
fn builds_offers(current: &Version, version: &Version, run: u64, build: Option<u64>) -> bool {
    match build {
        Some(mine) => run > mine,
        None => version > current,
    }
}

// ── GitHub's release list ──────────────────────────────────────────────────

/// The handful of fields the builds channel reads from a GitHub release.
///
/// `#[serde(default)]` on the container so a field GitHub stops sending is a
/// default rather than a hard failure — this runs against a third party's API
/// and the cost of being strict is an update check that breaks on a schema
/// change nobody told us about. `body` is `Option` because GitHub genuinely
/// sends `null` for a release with no notes.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    html_url: String,
    body: Option<String>,
    assets: Vec<ApiAsset>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// A build release, once its tag has been understood.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildRelease {
    version: Version,
    run: u64,
    tag: String,
    /// The release page a human can read. Empty when GitHub did not send one,
    /// in which case [`BuildRelease::page`] reconstructs it from the tag.
    html_url: String,
    notes: Option<String>,
    /// The signed manifest the updater plugin needs. `None` when the release
    /// has no `latest.json` asset — which happens for releases cut before
    /// `createUpdaterArtifacts` was turned on, and is a refusal to install
    /// rather than a crash.
    manifest: Option<String>,
}

impl BuildRelease {
    /// The version string the UI shows: the tag without its leading `v`, so
    /// `0.1.0-build.42` — the whole thing, because `0.1.0` alone would be a
    /// lie about which build this is.
    fn version_label(&self) -> String {
        self.tag.strip_prefix('v').unwrap_or(&self.tag).to_string()
    }

    fn page(&self) -> String {
        if self.html_url.is_empty() {
            format!("https://github.com/{REPO}/releases/tag/{}", self.tag)
        } else {
            self.html_url.clone()
        }
    }
}

/// The newest build release in a page of GitHub releases, or `None` if there
/// are none.
///
/// Drafts are excluded because a draft is a release that is still uploading —
/// the workflow keeps every branch build a draft until all three platform legs
/// have attached their installers, precisely so nobody is ever offered a
/// half-finished one.
///
/// "Newest" is the highest run number rather than the API's ordering. GitHub
/// sorts by creation date, which is *usually* the same answer, but the run
/// number is the thing the install decision actually compares — sorting by
/// anything else would mean the release we offer and the number we compare
/// could disagree.
fn newest_build(releases: Vec<ApiRelease>) -> Option<BuildRelease> {
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let (version, run) = parse_build_tag(&release.tag_name)?;
            let manifest = release
                .assets
                .iter()
                .find(|asset| asset.name == MANIFEST_ASSET)
                .map(|asset| asset.browser_download_url.clone());
            Some(BuildRelease {
                version,
                run,
                tag: release.tag_name,
                html_url: release.html_url,
                notes: release.body,
                manifest,
            })
        })
        .max_by_key(|release| release.run)
}

/// One page of the repository's releases.
async fn fetch_releases() -> Result<Vec<ApiRelease>, String> {
    let url = format!(
        "{}/repos/{REPO}/releases?per_page={RELEASES_PER_PAGE}",
        api_base()
    );
    let response = http_client()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        // Pins the response shape against a future default. GitHub has changed
        // its media type defaults before, and a silent shape change here reads
        // as "there are no builds" rather than as an error.
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| plain_http_error(&error))?;

    let status = response.status();
    if !status.is_success() {
        return Err(plain_status(status.as_u16()));
    }

    response.json::<Vec<ApiRelease>>().await.map_err(|_| {
        "GitHub answered, but not with a release list Kavka could read. Try again later, or \
         check the Releases page yourself."
            .to_string()
    })
}

/// The HTTP client for the builds channel.
///
/// rustls with the `ring` provider, matching what `kavka-core` links and what
/// `tauri-plugin-updater` installs — one TLS stack in this process, not two.
/// The provider has to be installed BEFORE the first client is built, and the
/// plugin only installs it inside its own `check`, which may not have run yet
/// (the builds channel reaches GitHub before any updater is built). The guard
/// is the plugin's own: install only if nothing else already did, and ignore
/// the race where somebody else won.
fn http_client() -> Result<reqwest::Client, String> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(NETWORK_TIMEOUT)
        .build()
        .map_err(|_| {
            "Kavka couldn't open a secure connection on this machine, so it can't ask GitHub \
             about updates."
                .to_string()
        })
}

// ── checking ───────────────────────────────────────────────────────────────

/// Asks a channel whether there is anything newer. Never fails: a failure is
/// [`UpdateCheck::Error`] carrying a sentence.
pub async fn check(app: &AppHandle, channel: Channel) -> UpdateCheck {
    match channel {
        Channel::Stable => check_stable(app).await,
        Channel::Builds => check_builds(app).await,
    }
}

async fn check_stable(app: &AppHandle) -> UpdateCheck {
    let updater = match stable_updater(app, None) {
        Ok(updater) => updater,
        Err(message) => return UpdateCheck::Error { message },
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let url = release_page(&update.version);
            UpdateCheck::Update {
                version: update.version,
                notes: update.body,
                url,
            }
        }
        Ok(None) => UpdateCheck::Current,
        // Maybe the one error that is not an error — but the plugin cannot
        // tell us which, so ask the endpoint directly rather than guess.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => match probe_stable_manifest().await {
            StableAbsence::NoRelease => UpdateCheck::NoStableRelease,
            StableAbsence::Failed(message) => UpdateCheck::Error { message },
        },
        Err(error) => UpdateCheck::Error {
            message: plain_updater_error(&error),
        },
    }
}

// ── what a missing stable manifest actually means ──────────────────────────
//
// `tauri_plugin_updater::Error::ReleaseNotFound` is the plugin's name for
// "I ended up with no release", and it is the SAME name for every way that can
// happen. Its `check` logs a non-success status and falls through without
// recording it, so the function ends at `remote_release.ok_or(ReleaseNotFound)`
// whether GitHub answered 404, 403 (rate limit), 429, 500, 502, 503, or a
// captive portal answered 407 with a login page.
//
// Treating all of that as `NoStableRelease` would have Kavka state, as a fact
// about the project, something that is really a fact about GitHub's last five
// minutes: "No stable release has been published yet." A rate limit shared with
// whatever else is on the network would produce it, and the day a stable
// release DOES exist it gets worse rather than better — a stable user hitting a
// 503 would be told the release they are running does not exist. Nothing else
// in this module asserts anything it has not established, and neither does
// this: it costs one more request, on a path that is already failing, to
// replace a guess with an observation.

/// Why the stable manifest could not be read, once somebody has looked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StableAbsence {
    /// GitHub says there is nothing at that address. On `releases/latest` that
    /// is the real thing: no non-prerelease release has ever been published.
    NoRelease,
    /// Anything else. The string is a finished sentence, fit to show.
    Failed(String),
}

/// What an observed status on the stable manifest means.
///
/// Only 404 earns [`StableAbsence::NoRelease`]. A 2xx is its own kind of odd —
/// the file downloaded and the plugin still could not make a release of it, so
/// the manifest is malformed, which is a broken release rather than an absent
/// one and is worth saying differently.
fn stable_absence(status: u16) -> StableAbsence {
    match status {
        404 => StableAbsence::NoRelease,
        200..=299 => StableAbsence::Failed(MANIFEST_UNREADABLE.to_string()),
        other => StableAbsence::Failed(plain_status(other)),
    }
}

/// The sentence for a stable manifest that arrived and did not parse.
const MANIFEST_UNREADABLE: &str =
    "GitHub returned Kavka's update manifest, but not in a shape Kavka could read. Try again \
     later, or download from the Releases page yourself.";

/// Asks the stable endpoint what it actually answers.
///
/// One request, to the URL the check just used, and only on the path where the
/// plugin has already refused to say why. It carries the module's own
/// User-Agent and nothing else — no cookie, no token, nothing that could tell
/// two Kavka users apart — so the disclosure in Settings stays true of it.
async fn probe_stable_manifest() -> StableAbsence {
    let client = match http_client() {
        Ok(client) => client,
        Err(message) => return StableAbsence::Failed(message),
    };
    match client.get(stable_manifest_url()).send().await {
        Ok(response) => stable_absence(response.status().as_u16()),
        // A request that never got an answer is not evidence of absence. It is
        // the same failure the builds channel reports, worded the same way.
        Err(error) => StableAbsence::Failed(plain_http_error(&error)),
    }
}

async fn check_builds(app: &AppHandle) -> UpdateCheck {
    let current = app.package_info().version.clone();
    let releases = match fetch_releases().await {
        Ok(releases) => releases,
        Err(message) => return UpdateCheck::Error { message },
    };
    // No build releases at all is genuinely "nothing newer" — there is no
    // fourth status for it, and inventing one would be inventing a problem.
    let Some(newest) = newest_build(releases) else {
        return UpdateCheck::Current;
    };
    if builds_offers(&current, &newest.version, newest.run, build_number()) {
        UpdateCheck::Update {
            version: newest.version_label(),
            notes: newest.notes.clone(),
            url: newest.page(),
        }
    } else {
        UpdateCheck::Current
    }
}

/// The release page for a stable version. Reconstructed from the tag rather
/// than read from the manifest because `latest.json` carries a download URL,
/// not a page — and `v<version>` is the release workflow's tag contract.
fn release_page(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

// ── installing ─────────────────────────────────────────────────────────────

/// Downloads the update this channel would offer, verifies its signature
/// against the public key in `tauri.conf.json`, and hands it to the installer.
///
/// **What "done" means is not the same on every platform**, and the UI has to
/// know which one it is on:
///
/// - **Windows** — the plugin launches the NSIS/MSI installer and immediately
///   ends this process (`std::process::exit(0)`), because a Windows installer
///   cannot replace a running executable. This function does not return; the
///   `on_before_exit` hook below is the last code Kavka runs.
/// - **macOS / Linux** — the new bundle is put in place and this returns
///   normally. The app is still the old one until it is relaunched, which is
///   why the UI follows a successful install with `updates_restart`.
///
/// The channel is re-checked here rather than reusing whatever the last
/// `check` found. It costs one request and it buys the guarantee that the
/// bytes installed are the bytes the rules currently point at — with no
/// pending-update state to go stale, and nothing shared between concurrent
/// callers.
pub async fn install(app: &AppHandle, channel: Channel) -> Result<(), String> {
    let update = match channel {
        Channel::Stable => stable_update(app).await?,
        Channel::Builds => build_update(app).await?,
    };
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|error| plain_updater_error(&error))
}

/// The message for an Install click on a channel that has nothing published.
const NO_STABLE_TO_INSTALL: &str =
    "There's no stable release to install — nobody has published one yet.";

async fn stable_update(app: &AppHandle) -> Result<Update, String> {
    let updater = stable_updater(app, Some(app.clone()))?;
    match updater.check().await {
        Ok(Some(update)) => Ok(update),
        Ok(None) => Err(nothing_to_install()),
        // Same distinction the check makes, and for the same reason: telling
        // somebody who just clicked Install that nobody has published a stable
        // release is a claim, and a 503 is not evidence for it.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            Err(match probe_stable_manifest().await {
                StableAbsence::NoRelease => NO_STABLE_TO_INSTALL.to_string(),
                StableAbsence::Failed(message) => message,
            })
        }
        Err(error) => Err(plain_updater_error(&error)),
    }
}

async fn build_update(app: &AppHandle) -> Result<Update, String> {
    let current = app.package_info().version.clone();
    let newest = newest_build(fetch_releases().await?).ok_or_else(nothing_to_install)?;
    if !builds_offers(&current, &newest.version, newest.run, build_number()) {
        return Err(nothing_to_install());
    }
    let manifest = newest.manifest.clone().ok_or_else(|| {
        format!(
            "The {} release has no signed manifest for Kavka to install from. Download the \
             installer by hand from {}.",
            newest.tag,
            newest.page()
        )
    })?;
    let endpoint = Url::parse(&manifest)
        .map_err(|_| "GitHub gave Kavka a download address it couldn't read.".to_string())?;

    let updater = app
        .updater_builder()
        .timeout(NETWORK_TIMEOUT)
        // Always true, and that is not laziness. `latest.json` carries the
        // `tauri.conf.json` version, which reads `0.1.0` for every build ever
        // cut — the plugin's comparison would refuse build 400 to build 1 as
        // "not newer". The decision was already made above, by run number,
        // against the tag; this comparator only has to not un-make it.
        .version_comparator(|_current, _remote| true)
        .on_before_exit({
            let app = app.clone();
            move || crate::stop_sessions_before_update_exit(&app)
        })
        .endpoints(vec![endpoint])
        .map_err(|error| plain_updater_error(&error))?
        .build()
        .map_err(|error| plain_updater_error(&error))?;

    match updater.check().await {
        Ok(Some(update)) => Ok(update),
        Ok(None) => Err(nothing_to_install()),
        Err(error) => Err(plain_updater_error(&error)),
    }
}

/// The stable channel's updater.
///
/// `exiting` carries the app handle on the install path only: the hook it
/// installs runs immediately before the Windows installer takes over and this
/// process ends, and it is the only chance Kavka gets to put its live sessions
/// down first. A plain check must not install it — there is no exit coming.
fn stable_updater(
    app: &AppHandle,
    exiting: Option<AppHandle>,
) -> Result<tauri_plugin_updater::Updater, String> {
    let endpoint = Url::parse(&stable_manifest_url())
        .map_err(|_| "Kavka's update address isn't a URL it can read.".to_string())?;
    let mut builder = app
        .updater_builder()
        .timeout(NETWORK_TIMEOUT)
        .version_comparator(|current, remote| {
            stable_offers(&current, &remote.version, build_number())
        });
    if let Some(app) = exiting {
        builder = builder.on_before_exit(move || crate::stop_sessions_before_update_exit(&app));
    }
    builder
        .endpoints(vec![endpoint])
        .map_err(|error| plain_updater_error(&error))?
        .build()
        .map_err(|error| plain_updater_error(&error))
}

/// The message for "you clicked Install, but by the time we looked there was
/// nothing to install". Rare, and always because something changed between the
/// check and the click — a release was unpublished, or the app already updated.
fn nothing_to_install() -> String {
    "There's nothing to install — Kavka is already on the newest version for this channel."
        .to_string()
}

// ── errors, as sentences ───────────────────────────────────────────────────

/// A plugin error the UI can show. The plugin's own `Display` strings are
/// mostly finished sentences already, so the fallback uses one rather than
/// inventing a vaguer message — but the cases a person can actually act on get
/// wording that says what to do next.
fn plain_updater_error(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as Failure;
    match error {
        Failure::EmptyEndpoints => {
            "This build of Kavka has no update address configured, so it can't check for \
             updates. Download from the Releases page instead."
                .to_string()
        }
        Failure::ReleaseNotFound => {
            "GitHub didn't return an update manifest Kavka could read.".to_string()
        }
        // The signature is the entire point of the updater: an installer that
        // does not verify is a download that anyone on the path can replace.
        // A mismatch is never retried and never explained away.
        Failure::Minisign(_) | Failure::SignatureUtf8(_) | Failure::Base64(_) => {
            "The download's signature didn't match Kavka's key, so nothing was installed. \
             Download the installer from the Releases page instead."
                .to_string()
        }
        Failure::TargetNotFound(_) | Failure::TargetsNotFound(_) => {
            "That release doesn't carry an installer for this platform.".to_string()
        }
        Failure::UnsupportedArch | Failure::UnsupportedOs => {
            "Kavka can't update itself on this platform. Download the installer from the \
             Releases page instead."
                .to_string()
        }
        Failure::InsecureTransportProtocol => {
            "Kavka only downloads updates over https, and this address isn't.".to_string()
        }
        Failure::Reqwest(_) | Failure::Network(_) => connection_failed(),
        Failure::AuthenticationFailed => {
            "The installer needed permission it wasn't given, so nothing was installed.".to_string()
        }
        Failure::PackageInstallFailed | Failure::DebInstallFailed => {
            "The installer wouldn't run. Download the installer from the Releases page and run \
             it yourself."
                .to_string()
        }
        other => format!("Kavka couldn't finish the update: {other}"),
    }
}

/// A `reqwest` failure the UI can show. Deliberately not the library's
/// `Display` — that one reads "error sending request for url (…)", which is a
/// developer's sentence, not a person's.
fn plain_http_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return format!(
            "GitHub didn't answer within {} seconds. Try again when the connection is better.",
            NETWORK_TIMEOUT.as_secs()
        );
    }
    if error.is_connect() {
        return connection_failed();
    }
    if error.is_decode() {
        return "GitHub's answer wasn't in a shape Kavka could read.".to_string();
    }
    connection_failed()
}

/// A non-2xx answer from github.com — from the REST API on the builds channel,
/// or from the manifest endpoint when [`probe_stable_manifest`] goes to look.
fn plain_status(status: u16) -> String {
    match status {
        // Unauthenticated api.github.com allows 60 requests an hour per
        // address. Kavka's own check is once a day, so hitting this means
        // something else on the network is also asking — worth saying, because
        // "try again" is genuinely the fix.
        403 | 429 => format!(
            "GitHub is rate-limiting this network right now (HTTP {status}). Kavka checks at \
             most once a day, so something else here is asking too — try again in an hour."
        ),
        // Only the API path reaches this: a 404 on the stable MANIFEST is
        // `StableAbsence::NoRelease` and never becomes a sentence here.
        404 => "GitHub has no release list at that address.".to_string(),
        500..=599 => format!(
            "GitHub answered with HTTP {status} — that's GitHub having a bad minute, not Kavka. \
             Try again shortly."
        ),
        _ => format!("GitHub answered with HTTP {status}, which Kavka didn't expect."),
    }
}

fn connection_failed() -> String {
    "Kavka couldn't reach github.com. Check your internet connection, or your proxy if you're \
     behind one."
        .to_string()
}

// ── tests ──────────────────────────────────────────────────────────────────

/// Everything decided in this module rather than fetched by it: which strings
/// are build tags, which build is newest, and — for each channel — whether an
/// install is offered at all. Those last two are the ones with teeth: getting
/// `stable_offers` wrong strands every build user on main, and getting
/// `builds_offers` wrong hands a stable user unreviewed main and calls it an
/// upgrade.
///
/// The serialised shape of [`UpdateCheck`] is tested too, because it is a
/// contract with TypeScript that no compiler checks.
#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("test fixture is a valid semver")
    }

    fn releases(json: &str) -> Vec<ApiRelease> {
        serde_json::from_str(json).expect("test fixture is valid JSON")
    }

    // ── KAVKA_BUILD ────────────────────────────────────────────────────────

    /// The workflow sets `KAVKA_BUILD=""` on stable and local builds, so the
    /// empty string is the common case and must not become build zero — a
    /// build-zero install would be offered every build ever cut.
    #[test]
    fn an_empty_build_number_is_not_a_build_install() {
        assert_eq!(parse_build_number(None), None);
        assert_eq!(parse_build_number(Some("")), None);
        assert_eq!(parse_build_number(Some("   ")), None);
    }

    #[test]
    fn a_build_number_is_the_run_number() {
        assert_eq!(parse_build_number(Some("42")), Some(42));
        assert_eq!(parse_build_number(Some(" 7 ")), Some(7));
        assert_eq!(parse_build_number(Some("0")), Some(0));
    }

    #[test]
    fn anything_that_is_not_digits_is_not_a_build_number() {
        for garbage in ["abc", "-1", "+7", "7.0", "0x10", "1e3", "٤٢", "7a"] {
            assert_eq!(parse_build_number(Some(garbage)), None, "{garbage}");
        }
        // Wider than u64. `parse` says no; the point is that it says no
        // rather than wrapping.
        assert_eq!(parse_build_number(Some("99999999999999999999999")), None);
    }

    // ── tags ───────────────────────────────────────────────────────────────

    #[test]
    fn a_build_tag_splits_into_version_and_run() {
        assert_eq!(
            parse_build_tag("v0.1.0-build.42"),
            Some((version("0.1.0"), 42))
        );
        assert_eq!(
            parse_build_tag("v12.34.56-build.7"),
            Some((version("12.34.56"), 7))
        );
    }

    /// The base is greedy: a version that already carries a prerelease keeps
    /// it, and the run number is whatever follows the LAST separator.
    #[test]
    fn the_run_number_is_the_last_segment() {
        assert_eq!(
            parse_build_tag("v1.2.3-rc.1-build.9"),
            Some((version("1.2.3-rc.1"), 9))
        );
        assert_eq!(
            parse_build_tag("v0.1.0-build.1-build.2"),
            Some((version("0.1.0-build.1"), 2))
        );
    }

    /// A stable tag is not a build tag. This is the line that keeps the builds
    /// channel from offering a stable release as if it were a build.
    #[test]
    fn a_stable_tag_is_not_a_build_tag() {
        assert_eq!(parse_build_tag("v0.1.0"), None);
        assert_eq!(parse_build_tag("v1.0.0-rc.1"), None);
    }

    /// The parser runs on strings a third party controls, so garbage has to
    /// come back as `None` — never a panic, never a half-read version.
    #[test]
    fn garbage_tags_are_refused_rather_than_guessed() {
        for garbage in [
            "",
            "v",
            "-build.1",
            "0.1.0-build.1",  // no leading v
            "v-build.1",      // empty base
            "v0.1.0-build.",  // no run number
            "v0.1.0-build.x", // run number is not digits
            "v0.1.0-build.-1",
            "vnot-a-version-build.4",
            "v0.1.0build.4", // separator is not the separator
            "vv0.1.0-build.4",
            "release-build.4",
            "v🦀-build.4",
            "v0.1.0-BUILD.4", // the workflow writes it lowercase
        ] {
            assert_eq!(parse_build_tag(garbage), None, "{garbage:?}");
        }
    }

    // ── the stable channel's decision ──────────────────────────────────────

    #[test]
    fn stable_offers_a_higher_version_to_anyone() {
        assert!(stable_offers(&version("0.1.0"), &version("0.2.0"), None));
        assert!(stable_offers(
            &version("0.1.0"),
            &version("0.2.0"),
            Some(42)
        ));
    }

    /// The rule the plugin's default comparator would get wrong: a build
    /// install of `0.1.0` must be offered the stable `0.1.0`, because the
    /// stable one is the reviewed build of the same version.
    #[test]
    fn stable_offers_its_own_version_to_a_build_install() {
        assert!(stable_offers(
            &version("0.1.0"),
            &version("0.1.0"),
            Some(42)
        ));
    }

    #[test]
    fn stable_offers_nothing_to_an_install_already_on_it() {
        assert!(!stable_offers(&version("0.1.0"), &version("0.1.0"), None));
    }

    #[test]
    fn stable_never_offers_a_downgrade() {
        assert!(!stable_offers(&version("0.2.0"), &version("0.1.0"), None));
        assert!(!stable_offers(
            &version("0.2.0"),
            &version("0.1.0"),
            Some(42)
        ));
    }

    // ── the builds channel's decision ──────────────────────────────────────

    #[test]
    fn a_build_install_compares_run_numbers() {
        let current = version("0.1.0");
        assert!(builds_offers(&current, &version("0.1.0"), 43, Some(42)));
        assert!(!builds_offers(&current, &version("0.1.0"), 42, Some(42)));
        assert!(!builds_offers(&current, &version("0.1.0"), 41, Some(42)));
    }

    /// Every build shares one version string, so the version must not be
    /// allowed to veto a higher run number.
    #[test]
    fn a_build_install_ignores_the_version_string() {
        assert!(builds_offers(
            &version("0.9.0"),
            &version("0.1.0"),
            43,
            Some(42)
        ));
    }

    /// A stable install is only ever offered a build cut from a LATER version
    /// — otherwise "check for builds" would quietly move somebody off a
    /// reviewed release onto whatever main happens to be.
    #[test]
    fn a_stable_install_is_only_offered_a_later_version() {
        let current = version("0.1.0");
        assert!(builds_offers(&current, &version("0.2.0"), 400, None));
        assert!(!builds_offers(&current, &version("0.1.0"), 400, None));
        assert!(!builds_offers(&current, &version("0.0.9"), 400, None));
    }

    // ── picking a build out of GitHub's answer ─────────────────────────────

    #[test]
    fn the_newest_build_is_the_highest_run_number() {
        let picked = newest_build(releases(
            r#"[
              {"tag_name":"v0.1.0-build.7","draft":false,"html_url":"https://x/7","assets":[]},
              {"tag_name":"v0.1.0-build.9","draft":false,"html_url":"https://x/9","assets":[]},
              {"tag_name":"v0.1.0-build.8","draft":false,"html_url":"https://x/8","assets":[]}
            ]"#,
        ))
        .expect("three build releases");
        assert_eq!(picked.run, 9);
        assert_eq!(picked.version_label(), "0.1.0-build.9");
        assert_eq!(picked.page(), "https://x/9");
    }

    /// A draft is a release still uploading its installers — the workflow
    /// keeps every branch build a draft until all three platform legs have
    /// finished, so offering one would be offering a release with missing
    /// platforms.
    #[test]
    fn drafts_are_never_offered() {
        let picked = newest_build(releases(
            r#"[
              {"tag_name":"v0.1.0-build.9","draft":true,"assets":[]},
              {"tag_name":"v0.1.0-build.8","draft":false,"assets":[]}
            ]"#,
        ))
        .expect("one published build");
        assert_eq!(picked.run, 8);
    }

    #[test]
    fn stable_releases_are_not_builds() {
        assert!(newest_build(releases(
            r#"[{"tag_name":"v0.1.0","draft":false,"assets":[]}]"#
        ))
        .is_none());
        assert!(newest_build(Vec::new()).is_none());
    }

    /// GitHub is a third party. A release object with nothing in it, a null
    /// body, an unknown field, or a tag nobody can parse must all come back as
    /// "no build here" — never a panic.
    #[test]
    fn a_malformed_release_is_skipped_rather_than_fatal() {
        assert!(newest_build(releases("[{}]")).is_none());
        assert!(newest_build(releases(r#"[{"tag_name":"garbage","draft":false}]"#)).is_none());
        let picked = newest_build(releases(
            r#"[
              {},
              {"tag_name":"not-a-tag"},
              {"tag_name":"v0.1.0-build.3","draft":false,"body":null,
               "something_new":true,"assets":[]}
            ]"#,
        ))
        .expect("the one readable build");
        assert_eq!(picked.run, 3);
        assert_eq!(picked.notes, None);
        // No html_url in the fixture: the page is rebuilt from the tag rather
        // than handed to the UI as an empty string it would render as a dead
        // link.
        assert_eq!(
            picked.page(),
            "https://github.com/sahilhirani/kavka/releases/tag/v0.1.0-build.3"
        );
    }

    #[test]
    fn the_manifest_asset_is_found_by_name() {
        let picked = newest_build(releases(
            r#"[{"tag_name":"v0.1.0-build.3","draft":false,"assets":[
              {"name":"Kavka_0.1.0_x64-setup.exe","browser_download_url":"https://x/exe"},
              {"name":"latest.json","browser_download_url":"https://x/latest.json"}
            ]}]"#,
        ))
        .expect("one build");
        assert_eq!(picked.manifest.as_deref(), Some("https://x/latest.json"));
    }

    /// Releases cut before `createUpdaterArtifacts` was turned on have no
    /// manifest. That is a refusal with an explanation, not a crash — the
    /// install path turns this `None` into a sentence pointing at the page.
    #[test]
    fn a_release_without_a_manifest_is_still_read() {
        let picked = newest_build(releases(
            r#"[{"tag_name":"v0.1.0-build.3","draft":false,"assets":[
              {"name":"Kavka_0.1.0_x64.msi","browser_download_url":"https://x/msi"}
            ]}]"#,
        ))
        .expect("one build");
        assert_eq!(picked.manifest, None);
    }

    // ── what a missing stable manifest means ───────────────────────────────

    /// The distinction the whole `no-stable-release` verdict rests on. The
    /// plugin hands back one error name for "404" and for "GitHub is having a
    /// bad minute", and only the first of those licenses Kavka to tell somebody
    /// that no stable release exists — a claim about the project, made out of
    /// somebody else's rate limiter otherwise.
    #[test]
    fn only_an_observed_404_means_no_stable_release() {
        assert_eq!(stable_absence(404), StableAbsence::NoRelease);

        for status in [403, 429, 500, 502, 503, 407, 418] {
            let StableAbsence::Failed(message) = stable_absence(status) else {
                panic!("HTTP {status} must not claim there is no stable release");
            };
            assert_eq!(message, plain_status(status), "HTTP {status}");
        }
    }

    /// A 2xx that still produced `ReleaseNotFound` means the manifest arrived
    /// and did not parse. That is a broken release, not an absent one, and it
    /// gets its own sentence rather than either of the other two verdicts.
    #[test]
    fn a_readable_answer_that_did_not_parse_is_its_own_failure() {
        for status in [200, 201, 204, 299] {
            assert_eq!(
                stable_absence(status),
                StableAbsence::Failed(MANIFEST_UNREADABLE.to_string()),
                "HTTP {status}"
            );
        }
    }

    /// The two failures a person can actually act on have to arrive with the
    /// wording that says what to do — a rate limit says wait an hour, a 5xx
    /// says it is GitHub rather than Kavka. Both go through `plain_status`,
    /// so this pins that they keep saying so from the stable path too.
    #[test]
    fn a_rate_limit_and_an_outage_read_as_themselves() {
        let StableAbsence::Failed(limited) = stable_absence(429) else {
            panic!("429 is a failure");
        };
        assert!(limited.contains("rate-limiting"), "{limited}");

        let StableAbsence::Failed(down) = stable_absence(503) else {
            panic!("503 is a failure");
        };
        assert!(down.contains("503"), "{down}");
        assert!(down.contains("not Kavka"), "{down}");
    }

    // ── the key that makes installing acceptable at all ────────────────────

    /// The updater's public key, exactly as `tauri.conf.json` carries it, has
    /// to be something the verifier can parse.
    ///
    /// Nothing else checks. `cargo check` does not look at the config, and
    /// `tauri-plugin-updater` only decodes the key inside `verify_signature` —
    /// so a mistyped key is silent until the first person clicks Install and
    /// gets a signature failure that reads like a tampered download. It fails
    /// closed, which is the right direction, but "closed" here means the
    /// updater never works and the reason is invisible. This turns that into a
    /// red test.
    ///
    /// The shape is minisign's, and `minisign_verify::PublicKey::decode` is
    /// strict about it: the config value is base64 of the whole `minisign.pub`
    /// FILE, whose first line is an untrusted comment and whose second line is
    /// the base64 key — 42 bytes, `Ed`, then the 8-byte key id the comment
    /// names, little-endian. A single-line form parses as none of that.
    #[test]
    fn the_updater_public_key_is_a_key_minisign_can_read() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("the config is JSON");
        let pubkey = config["plugins"]["updater"]["pubkey"]
            .as_str()
            .expect("the config carries plugins.updater.pubkey");

        let file = String::from_utf8(base64_decode(pubkey).expect("the config value is base64"))
            .expect("a minisign public key file is text");
        let mut lines = file.lines();
        let comment = lines.next().expect("line 1 is the untrusted comment");
        let encoded = lines
            .next()
            .expect("line 2 is the key — a one-line key is not a key");
        assert!(
            comment.starts_with("untrusted comment:"),
            "line 1 is minisign's comment line: {comment:?}"
        );

        let key = base64_decode(encoded).expect("line 2 is base64");
        assert_eq!(key.len(), 42, "a minisign public key is 42 bytes");
        assert_eq!(&key[..2], b"Ed", "the algorithm is Ed25519");

        // The comment is untrusted — anybody can write anything there — but the
        // workflow that generated this pair wrote the real id into it, so a key
        // whose id disagrees with its own comment is a key that got edited.
        let id: String = key[2..10]
            .iter()
            .rev()
            .map(|byte| format!("{byte:02X}"))
            .collect();
        assert!(
            comment.ends_with(&id),
            "the key id {id} disagrees with its own comment line: {comment:?}"
        );
    }

    /// Standard-alphabet base64, for the one fixed string above. Hand-rolled
    /// rather than pulled in as a dev-dependency: the test exists to make a
    /// malformed key loud, and it should not be able to fail because a crate
    /// could not be resolved.
    fn base64_decode(text: &str) -> Option<Vec<u8>> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut accumulator: u32 = 0;
        let mut bits: u32 = 0;
        for byte in text.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
            if byte == b'=' {
                break;
            }
            let value = ALPHABET.iter().position(|entry| *entry == byte)? as u32;
            accumulator = (accumulator << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((accumulator >> bits) as u8);
            }
        }
        Some(out)
    }

    // ── the shape the UI is written against ────────────────────────────────

    #[test]
    fn the_check_result_serialises_to_the_pinned_shape() {
        let json = |check: UpdateCheck| serde_json::to_value(check).expect("serialises");

        assert_eq!(
            json(UpdateCheck::Update {
                version: "0.2.0".into(),
                notes: Some("Fixes things".into()),
                url: "https://github.com/sahilhirani/kavka/releases/tag/v0.2.0".into(),
            }),
            serde_json::json!({
                "status": "update",
                "version": "0.2.0",
                "notes": "Fixes things",
                "url": "https://github.com/sahilhirani/kavka/releases/tag/v0.2.0",
            })
        );
        assert_eq!(
            json(UpdateCheck::Update {
                version: "0.2.0".into(),
                notes: None,
                url: "https://x".into(),
            }),
            serde_json::json!({"status": "update", "version": "0.2.0", "notes": null, "url": "https://x"})
        );
        assert_eq!(
            json(UpdateCheck::Current),
            serde_json::json!({"status": "current"})
        );
        assert_eq!(
            json(UpdateCheck::NoStableRelease),
            serde_json::json!({"status": "no-stable-release"})
        );
        assert_eq!(
            json(UpdateCheck::Error {
                message: "Kavka couldn't reach github.com.".into()
            }),
            serde_json::json!({"status": "error", "message": "Kavka couldn't reach github.com."})
        );
    }

    #[test]
    fn the_channel_is_one_of_exactly_two_strings() {
        let channel = |text: &str| serde_json::from_str::<Channel>(text);
        assert_eq!(channel("\"stable\"").ok(), Some(Channel::Stable));
        assert_eq!(channel("\"builds\"").ok(), Some(Channel::Builds));
        // Not silently one of the two: a typo in the UI has to be visible.
        assert!(channel("\"nightly\"").is_err());
        assert!(channel("\"Stable\"").is_err());
        assert!(channel("\"\"").is_err());
    }

    // ── the sentences ──────────────────────────────────────────────────────

    /// Every message the UI can show is a finished sentence — no Debug dumps,
    /// no bare error codes, no "Error:" prefixes for the UI to strip.
    #[test]
    fn error_messages_are_sentences() {
        let messages = [
            plain_status(403),
            plain_status(404),
            plain_status(503),
            plain_status(418),
            connection_failed(),
            nothing_to_install(),
            MANIFEST_UNREADABLE.to_string(),
            NO_STABLE_TO_INSTALL.to_string(),
        ];
        for message in messages {
            assert!(message.ends_with('.'), "{message}");
            assert!(message.starts_with(|c: char| c.is_uppercase()), "{message}");
            assert!(!message.contains('{'), "{message}");
        }
    }
}
