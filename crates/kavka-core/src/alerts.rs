//! Alert rules: what they are, when they fire, where they go
//! (docs/ARCHITECTURE.md D6).
//!
//! # Everything that decides is a pure function
//!
//! [`evaluate`] takes the rules, an [`Observation`], the previous
//! [`AlertState`] and the time, and returns the next state plus the events to
//! emit. No clock, no cluster, no disk, no I/O of any kind. That is not
//! decoration: an alert engine is a state machine whose interesting behaviour
//! only appears over minutes — `for_ms` hysteresis, resolution, a rule that
//! flaps every thirty seconds — and the only way to actually test those is to
//! drive a synthetic week through it in a millisecond. Every timeline in this
//! module's tests does exactly that.
//!
//! # The three things that stop an alert being noise
//!
//! 1. **`for_ms` — fire only after the condition holds continuously.** A single
//!    sample over the threshold is a fetch that happened to land mid-batch. A
//!    condition that has held for five minutes is a thing to wake someone for.
//!    The timer resets the moment the condition clears, so "continuously" means
//!    it.
//! 2. **Resolution.** Every fire is matched by a resolve carrying the *same*
//!    `fired_ms`, so the pair is one incident on the screen rather than two
//!    rows. The resolve's `detail` describes the state that cleared it, not the
//!    state that caused it — "no partitions are under-replicated" is the useful
//!    sentence at that moment.
//! 3. **[`COOLDOWN_MS`] — a cleared rule cannot re-fire immediately.** A
//!    metric sitting exactly on its threshold would otherwise produce a fire
//!    and a resolve every tick, forever, which is how people learn to ignore
//!    an alerting system. The breach is not forgotten while the cooldown runs:
//!    if it is still true when the cooldown ends, it fires then.
//!
//! # What "no data" means
//!
//! Absence of measurement is never evidence of a breach. A rule whose series is
//! missing — no metrics endpoint, an exporter that stopped answering, a group
//! that stopped committing — is **not breaching**, and a firing rule that loses
//! its measurement **resolves**. The alternative is a `throughput_floor` rule
//! that fires every time Prometheus restarts, which is an alert about Kavka's
//! plumbing wearing a Kafka costume. [`crate::metrics::FRESH_MS`] is the other
//! half of the same stance.
//!
//! # Secret discipline
//!
//! A webhook URL is a credential in disguise (a Slack incoming webhook *is* its
//! own password), which is why it is *not* stored here as a plain field on the
//! profile — it lives in `alerts.json`, is never part of a profile export, and
//! never appears in an error message: [`send_webhook`] reports the host and the
//! status, never the path.

use crate::history::LagSample;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// How long after a rule resolves before it may fire again. Five minutes is the
/// smallest gap that stops a metric hovering on its threshold from producing a
/// notification every tick, and is short enough that a real second incident
/// inside the window still surfaces — it fires the moment the cooldown ends.
pub const COOLDOWN_MS: i64 = 5 * 60 * 1000;

/// A webhook is fired from a background tick and must not hold a blocking-pool
/// slot open on a receiver that has stopped answering. Same budget as a metrics
/// scrape, for the same reason.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// How many events are kept per profile. The morning after an incident, the
/// question is "when did this start and how long did it last" — which needs the
/// history to survive a restart, so it is written to disk with the rules. Two
/// hundred is a few weeks of a healthy cluster and one bad afternoon of an
/// unhealthy one.
const HISTORY_LIMIT: usize = 200;

/// Envelope version for `alerts.json`, versioned for the same reason profile
/// exports are: a future shape gets migrated or refused, never half-parsed.
pub const ALERTS_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// The IPC contract. Field names are mirrored by the TypeScript in
// apps/desktop/src, so renaming one is a breaking change on both sides.
// ---------------------------------------------------------------------------

/// One alert rule.
///
/// `offline_partitions` has no `for_ms`, and that is a statement rather than an
/// omission: a partition with no leader is not serving reads or writes, there is
/// no such thing as an acceptably brief outage of one, and asking a user to
/// choose a tolerance for it would only teach them to set one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertRule {
    LagThreshold {
        id: String,
        name: String,
        group_id: String,
        /// `None` watches every topic the group reads.
        topic: Option<String>,
        threshold: i64,
        for_ms: i64,
    },
    UnderReplicated {
        id: String,
        name: String,
        for_ms: i64,
    },
    OfflinePartitions {
        id: String,
        name: String,
    },
    ThroughputFloor {
        id: String,
        name: String,
        /// A [`crate::metrics`] series key, cluster-level or per-topic.
        series: String,
        below: f64,
        for_ms: i64,
    },
}

impl AlertRule {
    pub fn id(&self) -> &str {
        match self {
            Self::LagThreshold { id, .. }
            | Self::UnderReplicated { id, .. }
            | Self::OfflinePartitions { id, .. }
            | Self::ThroughputFloor { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::LagThreshold { name, .. }
            | Self::UnderReplicated { name, .. }
            | Self::OfflinePartitions { name, .. }
            | Self::ThroughputFloor { name, .. } => name,
        }
    }

    /// How long the condition must hold before this rule fires. Negative is
    /// clamped to zero rather than refused — a rule stored by an older build,
    /// or hand-edited, should behave like "fire immediately", not vanish.
    pub fn for_ms(&self) -> i64 {
        match self {
            Self::LagThreshold { for_ms, .. }
            | Self::UnderReplicated { for_ms, .. }
            | Self::ThroughputFloor { for_ms, .. } => (*for_ms).max(0),
            Self::OfflinePartitions { .. } => 0,
        }
    }
}

/// One incident. A fire and its resolve are the *same* incident: they carry the
/// same `fired_ms`, and `resolved_ms` is what tells them apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertEvent {
    pub rule_id: String,
    pub rule_name: String,
    pub fired_ms: i64,
    pub resolved_ms: Option<i64>,
    pub detail: String,
}

impl AlertEvent {
    pub fn is_resolved(&self) -> bool {
        self.resolved_ms.is_some()
    }
}

/// Where a profile's alerts go.
///
/// `os_notification` is honoured by the Tauri shell, not here — core has no
/// notification API and should not grow one. This module owns the webhook,
/// which is the half that is testable without a desktop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertChannels {
    #[serde(default = "yes")]
    pub os_notification: bool,
    #[serde(default)]
    pub webhook_url: Option<String>,
    /// Whether the webhook expects Slack's `{"text": …}` envelope. A Slack
    /// incoming webhook rejects anything else with a 400 that says `invalid
    /// payload`, and a generic receiver has no idea what `text` means — there
    /// is no payload that satisfies both, so the user tells us which they have.
    #[serde(default)]
    pub webhook_is_slack: bool,
}

fn yes() -> bool {
    true
}

impl Default for AlertChannels {
    /// An OS notification and nothing else: alerting that is on by default and
    /// goes nowhere the user has not asked for.
    fn default() -> Self {
        Self {
            os_notification: true,
            webhook_url: None,
            webhook_is_slack: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Everything the evaluator is allowed to see: the freshest lag samples and the
/// freshest metric readings.
///
/// Both come from the *raw* sources — [`crate::history::SampleTick::samples`]
/// and [`crate::metrics::MetricsCollector::latest`] — never from the
/// downsampled query paths, so no alert ever fires (or fails to) because of how
/// a chart was compressed.
#[derive(Debug, Clone, Default)]
pub struct Observation {
    pub samples: Vec<LagSample>,
    pub metrics: Vec<(String, f64)>,
}

impl Observation {
    fn metric(&self, series: &str) -> Option<f64> {
        self.metrics
            .iter()
            .find(|(key, _)| key == series)
            .map(|(_, value)| *value)
    }

    /// The worst partition this rule watches, if any of them has a position.
    fn worst_lag(&self, group_id: &str, topic: Option<&str>) -> Option<&LagSample> {
        self.samples
            .iter()
            .filter(|sample| {
                sample.group_id == group_id
                    && topic.is_none_or(|wanted| sample.topic == wanted)
                    && sample.lag.is_some()
            })
            .max_by_key(|sample| sample.lag.unwrap_or(0))
    }
}

/// What one rule makes of one observation.
struct Assessment {
    breaching: bool,
    /// The sentence for whichever event this produces — describing the state
    /// *now*, so a resolve says what cleared rather than repeating what broke.
    detail: String,
}

/// The hysteresis state of every rule, carried between ticks by the caller.
///
/// Deliberately not persisted. After a restart a rule's `for_ms` timer starts
/// again, and that is the honest behaviour: Kavka cannot claim a condition held
/// continuously across an interval when it was not running to see it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlertState {
    rules: BTreeMap<String, RuleState>,
}

impl AlertState {
    /// Whether a rule is currently firing — what the UI puts a ledger rule
    /// beside.
    pub fn is_firing(&self, rule_id: &str) -> bool {
        self.rules
            .get(rule_id)
            .is_some_and(|state| state.firing.is_some())
    }

    /// Every rule currently firing, in rule-id order, each with the instant it
    /// fired.
    ///
    /// The `fired_ms` is the half that matters to a caller shutting down. An
    /// incident is a fire and a resolve carrying the *same* `fired_ms` — that
    /// is what makes [`AlertStore::record_event`] close the row that is open
    /// rather than write a second one about the same thing — so anything
    /// synthesising a resolve for a rule that is still firing has to quote this
    /// back rather than stamp the moment it decided to give up.
    pub fn firing(&self) -> Vec<(&str, i64)> {
        self.rules
            .iter()
            .filter_map(|(id, state)| Some((id.as_str(), state.firing.as_ref()?.fired_ms)))
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct RuleState {
    /// When the condition started holding continuously; `None` when it is not.
    breaching_since: Option<i64>,
    firing: Option<Firing>,
    /// When this rule last resolved — the anchor [`COOLDOWN_MS`] measures from.
    last_resolved_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
struct Firing {
    fired_ms: i64,
}

/// Evaluates every rule against one observation.
///
/// Returns the next state and the events that happened *at this instant* —
/// which is at most one per rule, and usually none. The caller emits them,
/// records them ([`AlertStore::record_event`]) and sends them
/// ([`deliver`]); this function does none of those things, which is what keeps
/// it a function.
///
/// State for rules that no longer exist is dropped: deleting a rule that was
/// firing forgets it rather than leaving a resolve to emit for something the
/// user cannot see any more.
pub fn evaluate(
    rules: &[AlertRule],
    observation: &Observation,
    state: &AlertState,
    now_ms: i64,
) -> (AlertState, Vec<AlertEvent>) {
    let mut next = AlertState::default();
    let mut events = Vec::new();

    for rule in rules {
        let assessment = assess(rule, observation);
        let previous = state.rules.get(rule.id()).cloned().unwrap_or_default();
        let mut current = previous.clone();

        if assessment.breaching {
            let since = previous.breaching_since.unwrap_or(now_ms);
            current.breaching_since = Some(since);
            let held_long_enough = now_ms - since >= rule.for_ms();
            let cooling = previous
                .last_resolved_ms
                .is_some_and(|resolved| now_ms - resolved < COOLDOWN_MS);
            if previous.firing.is_none() && held_long_enough && !cooling {
                current.firing = Some(Firing { fired_ms: now_ms });
                events.push(AlertEvent {
                    rule_id: rule.id().to_string(),
                    rule_name: rule.name().to_string(),
                    fired_ms: now_ms,
                    resolved_ms: None,
                    detail: assessment.detail,
                });
            }
        } else {
            current.breaching_since = None;
            if let Some(firing) = previous.firing {
                current.firing = None;
                current.last_resolved_ms = Some(now_ms);
                events.push(AlertEvent {
                    rule_id: rule.id().to_string(),
                    rule_name: rule.name().to_string(),
                    fired_ms: firing.fired_ms,
                    resolved_ms: Some(now_ms),
                    detail: assessment.detail,
                });
            }
        }
        next.rules.insert(rule.id().to_string(), current);
    }
    (next, events)
}

fn assess(rule: &AlertRule, observation: &Observation) -> Assessment {
    match rule {
        AlertRule::LagThreshold {
            group_id,
            topic,
            threshold,
            ..
        } => match observation.worst_lag(group_id, topic.as_deref()) {
            Some(worst) => {
                let lag = worst.lag.unwrap_or(0);
                Assessment {
                    breaching: lag >= *threshold,
                    detail: if lag >= *threshold {
                        format!(
                            "{group_id} is {lag} messages behind on {} partition {}; \
                             the threshold is {threshold}",
                            worst.topic, worst.partition
                        )
                    } else {
                        format!(
                            "{group_id} is at most {lag} messages behind; \
                             the threshold is {threshold}"
                        )
                    },
                }
            }
            // Nothing measured: the group has never committed, has no
            // partitions assigned, or is not being sampled at all.
            None => Assessment {
                breaching: false,
                detail: format!("{group_id} has no committed offsets to measure right now"),
            },
        },
        AlertRule::UnderReplicated { .. } => {
            count_rule(observation, "under_replicated_partitions", |count| {
                (
                    format!("{count} partitions are under-replicated"),
                    "no partitions are under-replicated".to_string(),
                )
            })
        }
        AlertRule::OfflinePartitions { .. } => {
            count_rule(observation, "offline_partitions", |count| {
                (
                    format!("{count} partitions have no leader"),
                    "every partition has a leader".to_string(),
                )
            })
        }
        AlertRule::ThroughputFloor { series, below, .. } => match observation.metric(series) {
            Some(value) => Assessment {
                breaching: value < *below,
                detail: format!(
                    "{series} is {}; the floor is {}",
                    number(value),
                    number(*below)
                ),
            },
            None => Assessment {
                breaching: false,
                detail: format!(
                    "{series} isn't being measured — no reading from the metrics endpoint"
                ),
            },
        },
    }
}

/// The two count-shaped metric rules, which differ only in their wording.
fn count_rule(
    observation: &Observation,
    series: &str,
    sentences: impl Fn(i64) -> (String, String),
) -> Assessment {
    match observation.metric(series) {
        Some(value) if value > 0.0 => Assessment {
            breaching: true,
            detail: sentences(value.round() as i64).0,
        },
        Some(_) => Assessment {
            breaching: false,
            detail: sentences(0).1,
        },
        None => Assessment {
            breaching: false,
            detail: format!("{series} isn't being measured — no reading from the metrics endpoint"),
        },
    }
}

/// A throughput figure as prose. Whole numbers lose the `.0` a raw `{}` would
/// print — `bytes_in_per_sec is 41234` rather than `41234.0` — and everything
/// else keeps two decimals, which is as much precision as a rate deserves in a
/// sentence. Lag figures never come through here: they are integers and are
/// printed exactly (docs/DESIGN.md §7 — never round a lag figure someone is
/// about to act on).
fn number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

// ---------------------------------------------------------------------------
// Delivery
// ---------------------------------------------------------------------------

/// The sentence a human reads, in either channel.
///
/// Sentence case, no exclamation mark, the rule's own name first because that
/// is what the user wrote and what they will recognise at 3am
/// (docs/DESIGN.md §7).
pub fn headline(event: &AlertEvent) -> String {
    if event.is_resolved() {
        format!("{} cleared — {}", event.rule_name, event.detail)
    } else {
        format!("{} — {}", event.rule_name, event.detail)
    }
}

/// The body of the webhook POST.
///
/// Slack's incoming webhooks take `{"text": …}` and reject anything else, so
/// that mode sends exactly that and nothing more. The generic mode sends the
/// whole event **plus** the rendered sentence: a receiver that wants to route on
/// `rule_id` has the structure, and one that just wants to print something has
/// `text` without having to reimplement the wording.
pub fn webhook_payload(event: &AlertEvent, is_slack: bool) -> String {
    let text = headline(event);
    let body = if is_slack {
        serde_json::json!({ "text": text })
    } else {
        serde_json::json!({
            "status": if event.is_resolved() { "resolved" } else { "firing" },
            "rule_id": event.rule_id,
            "rule_name": event.rule_name,
            "fired_ms": event.fired_ms,
            "resolved_ms": event.resolved_ms,
            "detail": event.detail,
            "text": text,
        })
    };
    body.to_string()
}

/// Posts one event to a webhook.
///
/// **The URL never appears in the error.** A Slack incoming webhook is a
/// bearer credential in path form, and an error string ends up in a log, a
/// screenshot and a bug report; the host and the status say everything a user
/// can act on without any of that. (D5's rule, applied to the one credential
/// that is not in the keychain because it is not a password-shaped thing.)
pub fn send_webhook(url: &str, is_slack: bool, event: &AlertEvent) -> Result<()> {
    let host = host_of(url);
    let mut response = ureq::post(url)
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .header("Content-Type", "application/json")
        .send(webhook_payload(event, is_slack))
        .map_err(|e| Error::Other(format!("couldn't reach the alert webhook at {host}: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.body_mut().read_to_string().unwrap_or_default();
        let said = body
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        return Err(Error::Other(format!(
            "the alert webhook at {host} answered {}{}",
            status.as_u16(),
            if said.is_empty() {
                String::new()
            } else {
                format!(": {}", said.trim())
            }
        )));
    }
    Ok(())
}

/// Sends an event to whatever channels are configured, and reports a failure
/// rather than propagating one.
///
/// **A webhook that does not answer must not stop the alert existing.** The
/// event is already recorded and already on its way to an OS notification; a
/// dead Slack endpoint is a thing to show in the alerts panel, not a reason to
/// lose the alert.
pub fn deliver(channels: &AlertChannels, event: &AlertEvent) -> Option<String> {
    let url = channels.webhook_url.as_deref()?.trim();
    if url.is_empty() {
        return None;
    }
    send_webhook(url, channels.webhook_is_slack, event)
        .err()
        .map(|e| e.to_string())
}

/// `https://hooks.slack.com/services/T00/B00/XXXX` -> `hooks.slack.com`. Falls
/// back to a fixed phrase rather than to the URL, because the fallback is
/// exactly the case where the string is not shaped the way we assumed.
fn host_of(url: &str) -> String {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split(['/', '?'])
        .next()
        .filter(|host| !host.is_empty())
        .unwrap_or("the configured address")
        .to_string()
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProfileAlerts {
    #[serde(default)]
    rules: Vec<AlertRule>,
    #[serde(default)]
    channels: AlertChannels,
    /// Newest first, capped at [`HISTORY_LIMIT`].
    #[serde(default)]
    history: Vec<AlertEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    kavka_alerts: u32,
    #[serde(default)]
    profiles: BTreeMap<String, ProfileAlerts>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            kavka_alerts: ALERTS_VERSION,
            profiles: BTreeMap::new(),
        }
    }
}

/// Alert rules, channels and history, in `alerts.json` beside `profiles.json`.
///
/// Same discipline as [`crate::profiles::ProfileStore`], for the same reasons:
/// write-tmp-then-rename so a crash cannot leave a truncated file, and a write
/// lock so two IPC commands cannot lose each other's updates in a
/// read-modify-write. One file rather than one per profile because the whole
/// document is a few kilobytes and a rules editor reads all of it anyway.
pub struct AlertStore {
    dir: PathBuf,
    write_lock: std::sync::Mutex<()>,
}

impl AlertStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            write_lock: std::sync::Mutex::new(()),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("alerts.json")
    }

    pub fn rules(&self, profile_id: &str) -> Result<Vec<AlertRule>> {
        Ok(self
            .read()?
            .profiles
            .remove(profile_id)
            .unwrap_or_default()
            .rules)
    }

    /// Upserts a rule by id, so the editor's "save" is one call whether the
    /// rule is new or not.
    pub fn save_rule(&self, profile_id: &str, rule: AlertRule) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        let profile = document.profiles.entry(profile_id.to_string()).or_default();
        match profile.rules.iter_mut().find(|kept| kept.id() == rule.id()) {
            Some(slot) => *slot = rule,
            None => profile.rules.push(rule),
        }
        self.write(&document)
    }

    /// Idempotent: deleting a rule that is not there is not an error — the
    /// other window already did it.
    pub fn delete_rule(&self, profile_id: &str, rule_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        if let Some(profile) = document.profiles.get_mut(profile_id) {
            profile.rules.retain(|rule| rule.id() != rule_id);
        }
        self.write(&document)
    }

    pub fn channels(&self, profile_id: &str) -> Result<AlertChannels> {
        Ok(self
            .read()?
            .profiles
            .remove(profile_id)
            .unwrap_or_default()
            .channels)
    }

    pub fn set_channels(&self, profile_id: &str, channels: AlertChannels) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        document
            .profiles
            .entry(profile_id.to_string())
            .or_default()
            .channels = channels;
        self.write(&document)
    }

    /// Records an event.
    ///
    /// A resolve **updates the fire it belongs to** rather than appending a
    /// second row: an incident is one thing that started and ended, and a
    /// history that lists it twice makes "how many times did this happen last
    /// week" un-answerable by counting.
    pub fn record_event(&self, profile_id: &str, event: &AlertEvent) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        let profile = document.profiles.entry(profile_id.to_string()).or_default();
        let existing = profile
            .history
            .iter_mut()
            .find(|kept| kept.rule_id == event.rule_id && kept.fired_ms == event.fired_ms);
        match existing {
            Some(slot) => *slot = event.clone(),
            None => {
                profile.history.insert(0, event.clone());
                profile.history.truncate(HISTORY_LIMIT);
            }
        }
        self.write(&document)
    }

    /// The most recent events, newest first.
    pub fn history(&self, profile_id: &str, limit: u32) -> Result<Vec<AlertEvent>> {
        let mut history = self
            .read()?
            .profiles
            .remove(profile_id)
            .unwrap_or_default()
            .history;
        history.truncate(limit as usize);
        Ok(history)
    }

    /// Everything one profile owns here, dropped with the profile. Rules that
    /// outlive their connection are the `sr_password` regression in another
    /// costume — state referring to a cluster the user can no longer see.
    pub fn forget_profile(&self, profile_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        if document.profiles.remove(profile_id).is_none() {
            return Ok(());
        }
        self.write(&document)
    }

    fn read(&self) -> Result<Document> {
        let path = self.file();
        if !path.exists() {
            return Ok(Document::default());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| Error::Other(format!("reading {}: {e}", path.display())))?;
        let document: Document = serde_json::from_str(&raw)
            .map_err(|e| Error::Other(format!("parsing {}: {e}", path.display())))?;
        if document.kavka_alerts != ALERTS_VERSION {
            return Err(Error::Other(format!(
                "{} was written by a different Kavka (version {}); this build reads version \
                 {ALERTS_VERSION}",
                path.display(),
                document.kavka_alerts
            )));
        }
        Ok(document)
    }

    fn write(&self, document: &Document) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| Error::Other(format!("creating {}: {e}", self.dir.display())))?;
        let json = serde_json::to_vec_pretty(document)
            .map_err(|e| Error::Other(format!("serializing alert rules: {e}")))?;
        let tmp = self.dir.join("alerts.json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| Error::Other(format!("writing {}: {e}", tmp.display())))?;
        fs::rename(&tmp, self.file())
            .map_err(|e| Error::Other(format!("replacing alerts.json: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const MINUTE: i64 = 60 * 1000;

    fn lag_rule(for_ms: i64) -> AlertRule {
        AlertRule::LagThreshold {
            id: "r1".into(),
            name: "Checkout falling behind".into(),
            group_id: "checkout-service".into(),
            topic: None,
            threshold: 10_000,
            for_ms,
        }
    }

    fn lag(value: i64) -> Observation {
        Observation {
            samples: vec![LagSample {
                ts_ms: 0,
                group_id: "checkout-service".into(),
                topic: "orders.v2".into(),
                partition: 7,
                committed: Some(1),
                end_offset: 1 + value,
                lag: Some(value),
            }],
            metrics: Vec::new(),
        }
    }

    fn metric(series: &str, value: f64) -> Observation {
        Observation {
            samples: Vec::new(),
            metrics: vec![(series.to_string(), value)],
        }
    }

    /// Drives a timeline through the evaluator, one observation per step, and
    /// returns every event with the time it happened at.
    fn run(rules: &[AlertRule], timeline: &[(i64, Observation)]) -> Vec<(i64, AlertEvent)> {
        let mut state = AlertState::default();
        let mut seen = Vec::new();
        for (now, observation) in timeline {
            let (next, events) = evaluate(rules, observation, &state, *now);
            state = next;
            seen.extend(events.into_iter().map(|event| (*now, event)));
        }
        seen
    }

    #[test]
    fn every_rule_shape_survives_its_serde_tag() {
        let rules = vec![
            lag_rule(5 * MINUTE),
            AlertRule::UnderReplicated {
                id: "r2".into(),
                name: "URP".into(),
                for_ms: MINUTE,
            },
            AlertRule::OfflinePartitions {
                id: "r3".into(),
                name: "Offline".into(),
            },
            AlertRule::ThroughputFloor {
                id: "r4".into(),
                name: "Quiet".into(),
                series: "bytes_in_per_sec".into(),
                below: 1_000.5,
                for_ms: 2 * MINUTE,
            },
        ];
        let json = serde_json::to_value(&rules).unwrap();
        assert_eq!(json[0]["kind"], "lag_threshold");
        assert_eq!(json[0]["topic"], serde_json::Value::Null);
        assert_eq!(json[1]["kind"], "under_replicated");
        assert_eq!(json[2]["kind"], "offline_partitions");
        assert_eq!(json[3]["kind"], "throughput_floor");
        assert_eq!(json[3]["below"], 1_000.5);
        assert_eq!(
            serde_json::from_value::<Vec<AlertRule>>(json).unwrap(),
            rules
        );
    }

    #[test]
    fn offline_partitions_has_no_tolerance_field() {
        let json = serde_json::to_value(AlertRule::OfflinePartitions {
            id: "r".into(),
            name: "n".into(),
        })
        .unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3, "{json}");
        assert!(json.get("for_ms").is_none());
    }

    #[test]
    fn a_rule_fires_only_after_its_window_has_held() {
        let rules = [lag_rule(5 * MINUTE)];
        let events = run(
            &rules,
            &[
                (0, lag(50_000)),
                (MINUTE, lag(50_000)),
                (4 * MINUTE, lag(50_000)),
                // Five minutes of continuous breach: this is the tick.
                (5 * MINUTE, lag(50_000)),
                (6 * MINUTE, lag(50_000)),
            ],
        );
        assert_eq!(events.len(), 1, "{events:#?}");
        assert_eq!(events[0].0, 5 * MINUTE);
        assert_eq!(events[0].1.fired_ms, 5 * MINUTE);
        assert!(!events[0].1.is_resolved());
        assert!(
            events[0].1.detail.contains("50000"),
            "a lag figure is exact: {}",
            events[0].1.detail
        );
        assert!(events[0].1.detail.contains("orders.v2"));
        assert!(events[0].1.detail.contains("partition 7"));
    }

    #[test]
    fn a_gap_in_the_breach_restarts_the_window() {
        let rules = [lag_rule(5 * MINUTE)];
        let events = run(
            &rules,
            &[
                (0, lag(50_000)),
                (4 * MINUTE, lag(50_000)),
                // One clear tick, and the clock starts over.
                (5 * MINUTE, lag(10)),
                (6 * MINUTE, lag(50_000)),
                (10 * MINUTE, lag(50_000)),
            ],
        );
        assert!(events.is_empty(), "{events:#?}");
    }

    #[test]
    fn a_rule_resolves_when_the_condition_clears() {
        let rules = [lag_rule(0)];
        let events = run(
            &rules,
            &[
                (0, lag(50_000)),
                (MINUTE, lag(50_000)),
                (2 * MINUTE, lag(4)),
            ],
        );
        assert_eq!(events.len(), 2);
        let (at, resolve) = &events[1];
        assert_eq!(*at, 2 * MINUTE);
        assert_eq!(resolve.resolved_ms, Some(2 * MINUTE));
        // The pair is one incident: the resolve carries the fire's time.
        assert_eq!(resolve.fired_ms, 0);
        // ...and its sentence describes what cleared, not what broke.
        assert!(resolve.detail.contains("at most 4"), "{}", resolve.detail);
    }

    /// The flap: a rule sitting on its threshold would otherwise fire and
    /// resolve on every tick, forever.
    #[test]
    fn the_cooldown_suppresses_a_flapping_rule() {
        let rules = [lag_rule(0)];
        let mut timeline = Vec::new();
        // Alternating over/under every 30 seconds for ten minutes.
        for step in 0..20 {
            let at = step * 30_000;
            timeline.push((at, if step % 2 == 0 { lag(50_000) } else { lag(1) }));
        }
        let events = run(&rules, &timeline);

        let fires = events.iter().filter(|(_, e)| !e.is_resolved()).count();
        assert_eq!(
            fires, 2,
            "ten minutes of flapping should be two incidents, not ten: {events:#?}"
        );
        // The second fire is at least a cooldown after the first resolve.
        let resolved_first = events
            .iter()
            .find(|(_, e)| e.is_resolved())
            .map(|(at, _)| *at)
            .expect("a resolve");
        let second_fire = events
            .iter()
            .filter(|(_, e)| !e.is_resolved())
            .nth(1)
            .map(|(at, _)| *at)
            .expect("a second fire");
        assert!(
            second_fire - resolved_first >= COOLDOWN_MS,
            "re-fired {}ms after resolving",
            second_fire - resolved_first
        );
    }

    /// The cooldown must not lose a breach — only delay the notification.
    #[test]
    fn a_breach_that_outlasts_the_cooldown_still_fires() {
        let rules = [lag_rule(0)];
        let mut timeline = vec![(0, lag(50_000)), (MINUTE, lag(1))];
        // Breaching continuously from minute two onwards.
        for minute in 2..12 {
            timeline.push((minute * MINUTE, lag(50_000)));
        }
        let events = run(&rules, &timeline);

        let fires: Vec<i64> = events
            .iter()
            .filter(|(_, e)| !e.is_resolved())
            .map(|(at, _)| *at)
            .collect();
        assert_eq!(fires.len(), 2, "{events:#?}");
        // Resolved at minute 1, so the earliest re-fire is minute 6.
        assert_eq!(fires[1], MINUTE + COOLDOWN_MS);
    }

    #[test]
    fn a_topic_scoped_rule_ignores_the_groups_other_topics() {
        let rule = AlertRule::LagThreshold {
            id: "r1".into(),
            name: "Checkout falling behind".into(),
            group_id: "checkout-service".into(),
            topic: Some("orders.v2".into()),
            threshold: 10_000,
            for_ms: 0,
        };
        let mut observation = lag(5);
        observation.samples.push(LagSample {
            topic: "payments.v1".into(),
            lag: Some(9_000_000),
            ..observation.samples[0].clone()
        });

        let (_, events) = evaluate(&[rule], &observation, &AlertState::default(), 0);
        assert!(
            events.is_empty(),
            "a rule scoped to one topic fired on another: {events:#?}"
        );
    }

    #[test]
    fn a_lag_rule_with_no_samples_does_not_fire() {
        let rules = [lag_rule(0)];
        let (_, events) = evaluate(&rules, &Observation::default(), &AlertState::default(), 0);
        assert!(events.is_empty());
    }

    /// A group that stops committing (its application was stopped) resolves a
    /// firing rule rather than pinning it forever on the last number seen.
    #[test]
    fn losing_the_measurement_resolves_a_firing_rule() {
        let rules = [lag_rule(0)];
        let events = run(
            &rules,
            &[(0, lag(50_000)), (MINUTE, Observation::default())],
        );
        assert_eq!(events.len(), 2);
        assert!(events[1].1.is_resolved());
        assert!(
            events[1].1.detail.contains("no committed offsets"),
            "{}",
            events[1].1.detail
        );
    }

    #[test]
    fn under_replicated_reads_the_aggregated_series() {
        let rules = [AlertRule::UnderReplicated {
            id: "r".into(),
            name: "URP".into(),
            for_ms: MINUTE,
        }];
        let events = run(
            &rules,
            &[
                (0, metric("under_replicated_partitions", 3.0)),
                (MINUTE, metric("under_replicated_partitions", 3.0)),
                (2 * MINUTE, metric("under_replicated_partitions", 0.0)),
            ],
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].1.detail, "3 partitions are under-replicated");
        assert_eq!(events[1].1.detail, "no partitions are under-replicated");
    }

    #[test]
    fn offline_partitions_fires_on_the_first_observation() {
        let rules = [AlertRule::OfflinePartitions {
            id: "r".into(),
            name: "Offline".into(),
        }];
        let events = run(&rules, &[(0, metric("offline_partitions", 2.0))]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1.fired_ms, 0);
        assert_eq!(events[0].1.detail, "2 partitions have no leader");
    }

    #[test]
    fn a_throughput_floor_fires_below_its_bound_and_not_on_it() {
        let rules = [AlertRule::ThroughputFloor {
            id: "r".into(),
            name: "Quiet cluster".into(),
            series: "bytes_in_per_sec".into(),
            below: 1_000.0,
            for_ms: 0,
        }];
        let events = run(
            &rules,
            &[
                (0, metric("bytes_in_per_sec", 1_000.0)),
                (MINUTE, metric("bytes_in_per_sec", 999.5)),
            ],
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, MINUTE);
        assert_eq!(
            events[0].1.detail,
            "bytes_in_per_sec is 999.50; the floor is 1000"
        );
    }

    /// The rule that keeps an alerting system trustworthy: a missing
    /// measurement is not a breach.
    #[test]
    fn a_missing_series_never_fires_a_rule() {
        let rules = [
            AlertRule::ThroughputFloor {
                id: "r1".into(),
                name: "Quiet".into(),
                series: "bytes_in_per_sec".into(),
                below: 1_000.0,
                for_ms: 0,
            },
            AlertRule::UnderReplicated {
                id: "r2".into(),
                name: "URP".into(),
                for_ms: 0,
            },
            AlertRule::OfflinePartitions {
                id: "r3".into(),
                name: "Offline".into(),
            },
        ];
        let events = run(
            &rules,
            &[
                (0, Observation::default()),
                (MINUTE, Observation::default()),
                (10 * MINUTE, Observation::default()),
            ],
        );
        assert!(events.is_empty(), "{events:#?}");
    }

    #[test]
    fn losing_the_metrics_endpoint_resolves_a_firing_rule() {
        let rules = [AlertRule::UnderReplicated {
            id: "r".into(),
            name: "URP".into(),
            for_ms: 0,
        }];
        let events = run(
            &rules,
            &[
                (0, metric("under_replicated_partitions", 1.0)),
                (MINUTE, Observation::default()),
            ],
        );
        assert_eq!(events.len(), 2);
        assert!(events[1].1.is_resolved());
        assert!(events[1].1.detail.contains("isn't being measured"));
    }

    #[test]
    fn state_follows_the_rules_that_still_exist() {
        let rules = [lag_rule(0)];
        let (state, _) = evaluate(&rules, &lag(50_000), &AlertState::default(), 0);
        assert!(state.is_firing("r1"));
        // The instant it fired comes back with it: a caller closing this
        // incident out has to quote that back, not stamp its own clock.
        assert_eq!(state.firing(), vec![("r1", 0)]);

        // The rule is deleted while firing: no resolve, no lingering state.
        let (state, events) = evaluate(&[], &lag(50_000), &state, MINUTE);
        assert!(events.is_empty());
        assert!(!state.is_firing("r1"));
        assert!(state.firing().is_empty());
    }

    #[test]
    fn two_rules_keep_their_own_clocks() {
        let rules = [
            lag_rule(5 * MINUTE),
            AlertRule::UnderReplicated {
                id: "r2".into(),
                name: "URP".into(),
                for_ms: 0,
            },
        ];
        let mut observation = lag(50_000);
        observation
            .metrics
            .push(("under_replicated_partitions".into(), 1.0));

        let events = run(
            &rules,
            &[(0, observation.clone()), (5 * MINUTE, observation)],
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].1.rule_id, "r2");
        assert_eq!(events[0].0, 0);
        assert_eq!(events[1].1.rule_id, "r1");
        assert_eq!(events[1].0, 5 * MINUTE);
    }

    // -----------------------------------------------------------------------
    // Delivery
    // -----------------------------------------------------------------------

    fn event() -> AlertEvent {
        AlertEvent {
            rule_id: "r1".into(),
            rule_name: "Checkout falling behind".into(),
            fired_ms: 1_700_000_000_000,
            resolved_ms: None,
            detail: "checkout-service is 50000 messages behind on orders.v2 partition 7".into(),
        }
    }

    #[test]
    fn the_slack_payload_is_only_text() {
        let payload: serde_json::Value =
            serde_json::from_str(&webhook_payload(&event(), true)).unwrap();
        assert_eq!(payload.as_object().unwrap().len(), 1);
        assert_eq!(
            payload["text"],
            "Checkout falling behind — checkout-service is 50000 messages behind on orders.v2 \
             partition 7"
        );
    }

    #[test]
    fn the_generic_payload_carries_the_event_and_a_sentence() {
        let payload: serde_json::Value =
            serde_json::from_str(&webhook_payload(&event(), false)).unwrap();
        assert_eq!(payload["status"], "firing");
        assert_eq!(payload["rule_id"], "r1");
        assert_eq!(payload["fired_ms"], 1_700_000_000_000_i64);
        assert_eq!(payload["resolved_ms"], serde_json::Value::Null);
        assert!(payload["text"]
            .as_str()
            .unwrap()
            .starts_with("Checkout falling behind — "));

        let resolved = AlertEvent {
            resolved_ms: Some(1_700_000_060_000),
            detail: "checkout-service is at most 4 messages behind".into(),
            ..event()
        };
        let payload: serde_json::Value =
            serde_json::from_str(&webhook_payload(&resolved, false)).unwrap();
        assert_eq!(payload["status"], "resolved");
        assert!(payload["text"].as_str().unwrap().contains("cleared —"));
    }

    #[test]
    fn a_webhook_is_posted_as_json() {
        use crate::sr::canned::CannedRegistry;

        let server = CannedRegistry::start(vec![("POST /hook", 200, "ok")]);
        let channels = AlertChannels {
            os_notification: true,
            webhook_url: Some(format!("{}/hook", server.url())),
            webhook_is_slack: true,
        };
        assert_eq!(deliver(&channels, &event()), None);

        let seen = server.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].content_type.as_deref(), Some("application/json"));
        assert_eq!(seen[0].json()["text"], headline(&event()));
    }

    #[test]
    fn a_failing_webhook_is_reported_rather_than_fatal() {
        use crate::sr::canned::CannedRegistry;

        let server = CannedRegistry::start(vec![("POST /hook", 403, "invalid_token")]);
        let channels = AlertChannels {
            os_notification: true,
            webhook_url: Some(format!("{}/hook", server.url())),
            webhook_is_slack: true,
        };
        let failure = deliver(&channels, &event()).expect("a failure to report");
        assert!(failure.contains("403"), "{failure}");
        assert!(failure.contains("invalid_token"), "{failure}");
        // The host is named; the secret-bearing path is not.
        assert!(failure.contains("127.0.0.1"), "{failure}");
        assert!(!failure.contains("/hook"), "{failure}");
    }

    #[test]
    fn an_unreachable_webhook_is_reported_rather_than_fatal() {
        let channels = AlertChannels {
            os_notification: true,
            webhook_url: Some("http://127.0.0.1:1/services/T000/B000/xoxb-secret".into()),
            webhook_is_slack: true,
        };
        let failure = deliver(&channels, &event()).expect("a failure to report");
        assert!(failure.contains("127.0.0.1:1"), "{failure}");
        assert!(
            !failure.contains("xoxb-secret"),
            "the webhook's secret path reached an error string: {failure}"
        );
    }

    #[test]
    fn no_webhook_configured_is_not_a_failure() {
        assert_eq!(deliver(&AlertChannels::default(), &event()), None);
        assert_eq!(
            deliver(
                &AlertChannels {
                    webhook_url: Some("   ".into()),
                    ..AlertChannels::default()
                },
                &event()
            ),
            None
        );
    }

    #[test]
    fn a_webhook_host_is_extracted_without_its_path() {
        assert_eq!(
            host_of("https://hooks.slack.com/services/T00/B00/XXXX"),
            "hooks.slack.com"
        );
        assert_eq!(
            host_of("http://alerts.internal:8080/in?k=s"),
            "alerts.internal:8080"
        );
        assert_eq!(host_of("garbage"), "garbage");
        assert_eq!(host_of(""), "the configured address");
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kavka-alerts-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn store(&self) -> AlertStore {
            AlertStore::new(self.0.clone())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rules_round_trip_and_are_scoped_to_their_profile() {
        let dir = TempDir::new();
        let store = dir.store();
        assert!(store.rules("p1").unwrap().is_empty());

        store.save_rule("p1", lag_rule(MINUTE)).unwrap();
        store
            .save_rule(
                "p2",
                AlertRule::OfflinePartitions {
                    id: "other".into(),
                    name: "Offline".into(),
                },
            )
            .unwrap();

        assert_eq!(store.rules("p1").unwrap(), vec![lag_rule(MINUTE)]);
        assert_eq!(store.rules("p2").unwrap().len(), 1);

        // Saving the same id again edits rather than duplicates.
        store.save_rule("p1", lag_rule(9 * MINUTE)).unwrap();
        assert_eq!(store.rules("p1").unwrap(), vec![lag_rule(9 * MINUTE)]);

        store.delete_rule("p1", "r1").unwrap();
        assert!(store.rules("p1").unwrap().is_empty());
        // Idempotent, and it left the other profile alone.
        store.delete_rule("p1", "r1").unwrap();
        assert_eq!(store.rules("p2").unwrap().len(), 1);
    }

    #[test]
    fn channels_round_trip_and_default_to_notifications_only() {
        let dir = TempDir::new();
        let store = dir.store();
        assert_eq!(store.channels("p1").unwrap(), AlertChannels::default());
        assert!(store.channels("p1").unwrap().os_notification);

        let channels = AlertChannels {
            os_notification: false,
            webhook_url: Some("https://hooks.slack.com/services/x".into()),
            webhook_is_slack: true,
        };
        store.set_channels("p1", channels.clone()).unwrap();
        assert_eq!(store.channels("p1").unwrap(), channels);
        // Saving channels did not disturb the rules beside them.
        store.save_rule("p1", lag_rule(0)).unwrap();
        assert_eq!(store.channels("p1").unwrap(), channels);
        assert_eq!(store.rules("p1").unwrap().len(), 1);
    }

    #[test]
    fn a_resolve_updates_the_incident_it_belongs_to() {
        let dir = TempDir::new();
        let store = dir.store();
        store.record_event("p1", &event()).unwrap();
        let resolved = AlertEvent {
            resolved_ms: Some(event().fired_ms + MINUTE),
            detail: "cleared".into(),
            ..event()
        };
        store.record_event("p1", &resolved).unwrap();

        let history = store.history("p1", 10).unwrap();
        assert_eq!(history.len(), 1, "an incident is one row: {history:#?}");
        assert_eq!(history[0], resolved);
    }

    #[test]
    fn history_is_newest_first_bounded_and_limited() {
        let dir = TempDir::new();
        let store = dir.store();
        for i in 0..(HISTORY_LIMIT + 20) {
            store
                .record_event(
                    "p1",
                    &AlertEvent {
                        fired_ms: i as i64,
                        ..event()
                    },
                )
                .unwrap();
        }
        let history = store.history("p1", 1_000).unwrap();
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(history[0].fired_ms, (HISTORY_LIMIT + 19) as i64);
        assert_eq!(store.history("p1", 3).unwrap().len(), 3);
        assert!(store.history("p2", 10).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_profile_takes_its_rules_history_and_webhook_with_it() {
        let dir = TempDir::new();
        let store = dir.store();
        store.save_rule("p1", lag_rule(0)).unwrap();
        store
            .set_channels(
                "p1",
                AlertChannels {
                    webhook_url: Some("https://hooks.slack.com/services/x".into()),
                    ..AlertChannels::default()
                },
            )
            .unwrap();
        store.record_event("p1", &event()).unwrap();
        store.save_rule("p2", lag_rule(0)).unwrap();

        store.forget_profile("p1").unwrap();
        assert!(store.rules("p1").unwrap().is_empty());
        assert!(store.history("p1", 10).unwrap().is_empty());
        assert_eq!(store.channels("p1").unwrap(), AlertChannels::default());
        assert_eq!(store.rules("p2").unwrap().len(), 1);

        // Idempotent, like every other delete in the crate.
        store.forget_profile("p1").unwrap();
        store.forget_profile("never-existed").unwrap();

        // The stored webhook is gone from the file, not merely unreferenced.
        let raw = fs::read_to_string(dir.0.join("alerts.json")).unwrap();
        assert!(!raw.contains("hooks.slack.com"), "{raw}");
    }

    #[test]
    fn the_file_is_written_atomically_and_versioned() {
        let dir = TempDir::new();
        let store = dir.store();
        store.save_rule("p1", lag_rule(0)).unwrap();

        let raw = fs::read_to_string(dir.0.join("alerts.json")).unwrap();
        let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(document["kavka_alerts"], ALERTS_VERSION);
        assert_eq!(
            document["profiles"]["p1"]["rules"][0]["kind"],
            "lag_threshold"
        );
        assert!(
            !dir.0.join("alerts.json.tmp").exists(),
            "the temporary file outlived the rename"
        );
    }

    #[test]
    fn a_document_from_another_version_is_refused_not_half_read() {
        let dir = TempDir::new();
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(
            dir.0.join("alerts.json"),
            r#"{"kavka_alerts": 99, "profiles": {}}"#,
        )
        .unwrap();

        let error = dir.store().rules("p1").unwrap_err().to_string();
        assert!(error.contains("version 99"), "{error}");
        assert!(error.contains("alerts.json"), "{error}");
    }

    /// The store is read on every launch, so a document written before a field
    /// existed has to keep loading — the same rule the profile store lives by.
    #[test]
    fn a_document_missing_the_newer_fields_still_loads() {
        let dir = TempDir::new();
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(
            dir.0.join("alerts.json"),
            r#"{"kavka_alerts": 1, "profiles": {"p1": {"rules": [
                {"kind": "offline_partitions", "id": "r", "name": "Offline"}
            ]}}}"#,
        )
        .unwrap();

        let store = dir.store();
        assert_eq!(store.rules("p1").unwrap().len(), 1);
        assert_eq!(store.channels("p1").unwrap(), AlertChannels::default());
        assert!(store.history("p1", 10).unwrap().is_empty());
    }

    #[test]
    fn a_throughput_figure_reads_as_prose_and_a_lag_figure_exactly() {
        assert_eq!(number(41_234.0), "41234");
        assert_eq!(number(999.5), "999.50");
        assert_eq!(number(0.0), "0");
        // The lag path never rounds: it formats the integer it was given.
        let detail = assess(&lag_rule(0), &lag(4_218_907)).detail;
        assert!(detail.contains("4218907"), "{detail}");
    }
}
