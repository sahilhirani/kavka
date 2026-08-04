//! Kafka Connect REST clients — one per Connect cluster a profile names
//! (docs/ROADMAP.md Phase 3, "Kafka Connect: multi-cluster, connector CRUD with
//! config-def validation, task restart/pause/resume, failure traces").
//!
//! Shaped after [`crate::sr`], and for the same reasons: blocking `ureq` rather
//! than an async client (the Tauri shell already wraps every core call in
//! `spawn_blocking`, so an async client would buy a runtime and no latency),
//! rustls with the platform verifier (a Connect worker behind a corporate CA is
//! the normal enterprise case), and a per-call global timeout so a wedged
//! worker cannot hold a command open.
//!
//! # Three things this module refuses to smooth over
//!
//! 1. **A trace is printed verbatim.** [`TaskStatus::trace`] is the Java stack
//!    trace the worker recorded when the task died, complete with `Caused by:`
//!    chains. It is the single most useful string in the whole Connect API and
//!    the one a "friendly" client is most tempted to summarise. Kavka carries
//!    it whole and lets the inspector decide how much to show.
//! 2. **A 409 is not a failure, it is a "not yet".** Connect answers 409 while
//!    the worker group is rebalancing — a state every connector create, delete
//!    and restart *causes*. Reported as a generic error it reads as "your
//!    change was rejected"; it means the opposite, so it gets its own sentence
//!    saying to try again in a moment.
//! 3. **A validation response is data, not an error.** `error_count > 0` comes
//!    back as a 200, and it is the whole point of the endpoint: per-field
//!    messages the form renders next to the field that caused them
//!    (docs/DESIGN.md §5.3, "validation never goes to the global banner").
//!
//! Secret discipline (D5): the worker password is resolved from the OS keychain
//! here, folded immediately into an `Authorization` header value, never appears
//! in an error message (the cluster URL carries no credentials), and is
//! redacted from `Debug`.
//!
//! Read-only (D5) is enforced in this file, not in the UI: every mutating entry
//! point is a free function taking a [`ClusterConnection`] whose first line is
//! [`ClusterConnection::ensure_writable`]. The client is built *after* the gate,
//! so a read-only profile costs no keychain read and no socket.

use crate::connection::ClusterConnection;
use crate::profiles::ConnectClusterConfig;
use crate::secrets;
use crate::Error;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};

/// Budget for one Connect round trip. Longer than the Schema Registry's ten
/// seconds because a worker answers `PUT /connectors/{name}/config` only after
/// the config has been written to the config topic and read back by the herder
/// — a real cluster under load spends seconds there routinely, and reporting a
/// timeout for a change the cluster went on to apply is the one outcome a user
/// cannot act on.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

const ACCEPT: &str = "application/json";
const CONTENT_TYPE: &str = "application/json";

/// Cap on how much of an unstructured error body is quoted back. A Connect
/// worker fronted by a proxy answers HTML, and a wall of it is not a message.
/// **Traces are not subject to this** — see the module docs.
const ERROR_BODY_LIMIT: usize = 300;

// ---------------------------------------------------------------------------
// Wire types. Field names are the IPC contract — the TypeScript in
// apps/desktop/src mirrors them exactly, so renaming one is a breaking change
// on both sides of the bridge.
// ---------------------------------------------------------------------------

/// One task of a connector, as its worker last reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatus {
    pub id: i32,
    /// `RUNNING`, `FAILED`, `PAUSED`, `UNASSIGNED`, `RESTARTING` — Connect's
    /// own vocabulary, uppercased by the worker and passed through unchanged.
    pub state: String,
    /// The worker holding this task, as `host:port`. The answer to "which
    /// machine do I look at", so it is never dropped even when it repeats.
    pub worker_id: String,
    /// The failure trace, verbatim. `None` unless the task failed.
    pub trace: Option<String>,
}

/// A connector and every task under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorSummary {
    pub name: String,
    /// The connector's own state, which is **not** a summary of its tasks: a
    /// `RUNNING` connector with three `FAILED` tasks is the most common
    /// production incident there is, and collapsing the two into one field is
    /// exactly how it gets missed.
    pub connector_state: String,
    pub worker_id: String,
    /// `source` or `sink`, lowercase as Connect reports it. `unknown` on a
    /// worker too old to say — named rather than guessed.
    pub connector_type: String,
    pub tasks: Vec<TaskStatus>,
}

/// One field of a connector's config definition, as the worker validated it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigValue {
    pub name: String,
    /// `None` for a field with no value — and for a `PASSWORD`-typed field,
    /// which Connect masks. Kavka never invents one (D5).
    pub value: Option<String>,
    /// The worker's own messages for this field, verbatim. Empty is the normal
    /// state.
    pub errors: Vec<String>,
    pub required: bool,
    /// The plugin author's documentation for the field, which is the only help
    /// text that can exist for a third-party connector.
    pub documentation: Option<String>,
}

/// The worker's verdict on a candidate connector config.
///
/// `error_count` is Connect's own total and is **not** recomputed from
/// `configs`: the two can legitimately differ (a group-level error belongs to
/// no single field), and a client that derived it would quietly report zero
/// errors on a config the worker will refuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigValidation {
    pub error_count: i32,
    pub configs: Vec<ConfigValue>,
}

// ---------------------------------------------------------------------------
// The IPC entry points. See the module docs on where the read-only gate sits.
// ---------------------------------------------------------------------------

/// Every connector on the cluster, with its status and its tasks.
pub fn list(conn: &ClusterConnection, cluster: &str) -> crate::Result<Vec<ConnectorSummary>> {
    client(conn, cluster)?.list()
}

/// A connector's current config, as the worker holds it.
pub fn config(
    conn: &ClusterConnection,
    cluster: &str,
    name: &str,
) -> crate::Result<BTreeMap<String, String>> {
    client(conn, cluster)?.config(name)
}

/// Runs a candidate config through the plugin's own config definition. Not a
/// mutation — nothing is created and nothing is changed, which is the whole
/// point: it is what lets the form tell the user what is wrong *before* they
/// commit to a connector.
pub fn validate(
    conn: &ClusterConnection,
    cluster: &str,
    connector_class: &str,
    config: &BTreeMap<String, String>,
) -> crate::Result<ConfigValidation> {
    client(conn, cluster)?.validate(connector_class, config)
}

/// Creates the connector, or replaces its config if it already exists —
/// Connect's `PUT /connectors/{name}/config` is one verb for both, and so is
/// this. Read-only-checked first (D5).
pub fn apply(
    conn: &ClusterConnection,
    cluster: &str,
    name: &str,
    config: &BTreeMap<String, String>,
) -> crate::Result<()> {
    conn.ensure_writable("apply a connector config")?;
    client(conn, cluster)?.apply(name, config)
}

/// Deletes the connector and stops its tasks. Read-only-checked first (D5).
pub fn delete(conn: &ClusterConnection, cluster: &str, name: &str) -> crate::Result<()> {
    conn.ensure_writable("delete a connector")?;
    client(conn, cluster)?.delete(name)
}

/// Restarts a connector, one of its tasks, or the connector and every task.
///
/// `task` picks a single task; with `None`, `include_tasks` decides whether the
/// restart is the connector instance alone (Connect's historical behaviour, and
/// almost never what a user means) or the connector and all of its tasks.
/// Read-only-checked first (D5).
pub fn restart(
    conn: &ClusterConnection,
    cluster: &str,
    name: &str,
    task: Option<i32>,
    include_tasks: bool,
) -> crate::Result<()> {
    conn.ensure_writable("restart a connector")?;
    client(conn, cluster)?.restart(name, task, include_tasks)
}

/// Pauses the connector and its tasks. Read-only-checked first (D5).
pub fn pause(conn: &ClusterConnection, cluster: &str, name: &str) -> crate::Result<()> {
    conn.ensure_writable("pause a connector")?;
    client(conn, cluster)?.pause(name)
}

/// Resumes a paused connector. Read-only-checked first (D5).
pub fn resume(conn: &ClusterConnection, cluster: &str, name: &str) -> crate::Result<()> {
    conn.ensure_writable("resume a connector")?;
    client(conn, cluster)?.resume(name)
}

/// The client for one of a profile's Connect clusters.
///
/// The "no such cluster" message lists the ones the profile *does* have: the
/// name arrives from IPC as a string, so the failure mode is a stale UI or a
/// renamed cluster, and both are answered by seeing the real list.
fn client(conn: &ClusterConnection, cluster: &str) -> crate::Result<ConnectClient> {
    let profile = conn.profile();
    match profile.connect_cluster(cluster) {
        Some(config) => ConnectClient::new(config),
        None if profile.connect_clusters.is_empty() => Err(Error::Other(format!(
            "{} has no Kafka Connect clusters — add one in the connection's settings",
            profile.name
        ))),
        None => Err(Error::Other(format!(
            "{} has no Kafka Connect cluster named {cluster:?} — it has {}",
            profile.name,
            profile
                .connect_clusters
                .iter()
                .map(|c| format!("{:?}", c.name))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

// ---------------------------------------------------------------------------
// The client.
// ---------------------------------------------------------------------------

/// A Kafka Connect worker group's REST endpoint.
///
/// Stateless and cheap: no cache, no pooling, one per call. Connect's answers
/// are *status* — the thing a user pressed refresh to see — so a cached one is
/// worse than no answer at all. (This is the opposite of [`crate::sr`]'s decode
/// path, which caches hard because schemas are immutable and it asks the same
/// question two thousand times a fetch.)
pub struct ConnectClient {
    /// The cluster's name in the profile, for messages. Never the URL alone: a
    /// user with a source and a sink cluster on adjacent ports needs the name.
    cluster: String,
    base_url: String,
    /// The complete `Authorization` header value, built once. Redacted in
    /// `Debug` — it embeds the worker password.
    authorization: Option<String>,
}

impl ConnectClient {
    /// Builds a client for one configured cluster, resolving its password from
    /// the keychain.
    ///
    /// **Fails when the keychain does**, unlike [`crate::sr::SchemaRegistry::new`].
    /// The registry degrades because a schema it cannot resolve still leaves a
    /// readable message on screen; there is no equivalent half-answer here.
    /// Every call is a thing a person just asked for, and "the worker rejected
    /// these credentials" would send them auditing Connect's own ACLs for a
    /// failure that happened on their laptop.
    pub fn new(config: &ConnectClusterConfig) -> crate::Result<Self> {
        let mut client = Self {
            cluster: config.name.clone(),
            base_url: config.url.trim_end_matches('/').to_string(),
            authorization: None,
        };
        let Some(username) = config.username.as_deref() else {
            return Ok(client);
        };
        let password = match &config.password {
            Some(secret) => secrets::resolve(secret).map_err(|e| {
                Error::Other(format!(
                    "credentials for the Connect cluster {:?}: {e}",
                    config.name
                ))
            })?,
            // A username with no stored password is legal — some workers front
            // a proxy that only reads the user half.
            None => String::new(),
        };
        client.authorization = Some(basic_auth(username, &password));
        Ok(client)
    }

    /// `GET /connectors?expand=info&expand=status`.
    ///
    /// The parameter is repeated rather than comma-joined: Connect binds it to
    /// a JAX-RS `List<String>`, so `expand=status,info` is one unmatched value
    /// and the worker silently answers the *unexpanded* form — a plain array of
    /// names. Which is also what a worker older than Connect 2.3 answers to the
    /// correct request, so both shapes are handled: the array falls back to one
    /// `GET /connectors/{name}/status` per connector rather than showing an
    /// inventory with every state blank.
    pub fn list(&self) -> crate::Result<Vec<ConnectorSummary>> {
        let body = self.call(
            Method::Get,
            "/connectors?expand=info&expand=status",
            None,
            "list the connectors",
        )?;

        // Try the expanded map first; a bare array is the older shape.
        if let Ok(expanded) = serde_json::from_str::<BTreeMap<String, ExpandedConnector>>(&body) {
            let mut summaries: Vec<ConnectorSummary> = expanded
                .into_iter()
                .map(|(name, entry)| entry.into_summary(&name))
                .collect();
            summaries.sort_by(|a, b| a.name.cmp(&b.name));
            return Ok(summaries);
        }

        let names: Vec<String> = serde_json::from_str(&body).map_err(|e| {
            Error::Other(format!(
                "{} returned a connector listing Kavka could not read: {e}",
                self.describe()
            ))
        })?;
        let mut summaries = Vec::with_capacity(names.len());
        for name in names {
            let status = self.call(
                Method::Get,
                &format!("/connectors/{}/status", path_segment(&name)),
                None,
                &format!("read the status of {name}"),
            )?;
            let status: ConnectorStatus = serde_json::from_str(&status).map_err(|e| {
                Error::Other(format!(
                    "{} returned a status for {name} Kavka could not read: {e}",
                    self.describe()
                ))
            })?;
            summaries.push(status.into_summary(&name));
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    /// `GET /connectors/{name}/config`.
    pub fn config(&self, name: &str) -> crate::Result<BTreeMap<String, String>> {
        let body = self.call(
            Method::Get,
            &format!("/connectors/{}/config", path_segment(name)),
            None,
            &format!("read the config of {name}"),
        )?;
        serde_json::from_str(&body).map_err(|e| {
            Error::Other(format!(
                "{} returned a config for {name} Kavka could not read: {e}",
                self.describe()
            ))
        })
    }

    /// `PUT /connector-plugins/{class}/config/validate`.
    ///
    /// `connector.class` is filled in from the class named in the URL when the
    /// caller left it out: the worker refuses a config without it, and making
    /// the user type the plugin's fully-qualified Java class name into a field
    /// *and* pick it from a list is asking the same question twice. A class
    /// that contradicts the URL is an error rather than a silent overwrite —
    /// the two disagreeing means the form is out of sync with itself.
    pub fn validate(
        &self,
        connector_class: &str,
        config: &BTreeMap<String, String>,
    ) -> crate::Result<ConfigValidation> {
        let config = self.with_field(config, "connector.class", connector_class)?;
        let body = self.call(
            Method::Put,
            &format!(
                "/connector-plugins/{}/config/validate",
                path_segment(connector_class)
            ),
            Some(&serialize(&config)),
            &format!("validate a config for {connector_class}"),
        )?;

        let response: ValidationResponse = serde_json::from_str(&body).map_err(|e| {
            Error::Other(format!(
                "{} returned a validation result Kavka could not read: {e}",
                self.describe()
            ))
        })?;
        Ok(ConfigValidation {
            error_count: response.error_count,
            configs: response.configs.into_iter().map(Into::into).collect(),
        })
    }

    /// `PUT /connectors/{name}/config` — create-or-update, one verb.
    fn apply(&self, name: &str, config: &BTreeMap<String, String>) -> crate::Result<()> {
        let config = self.with_field(config, "name", name)?;
        self.call(
            Method::Put,
            &format!("/connectors/{}/config", path_segment(name)),
            Some(&serialize(&config)),
            &format!("apply the config for {name}"),
        )?;
        Ok(())
    }

    /// `DELETE /connectors/{name}`.
    fn delete(&self, name: &str) -> crate::Result<()> {
        self.call(
            Method::Delete,
            &format!("/connectors/{}", path_segment(name)),
            None,
            &format!("delete {name}"),
        )?;
        Ok(())
    }

    /// `POST /connectors/{name}/restart` or
    /// `POST /connectors/{name}/tasks/{task}/restart`.
    fn restart(&self, name: &str, task: Option<i32>, include_tasks: bool) -> crate::Result<()> {
        let (path, what) = match task {
            Some(task) => (
                format!("/connectors/{}/tasks/{task}/restart", path_segment(name)),
                format!("restart task {task} of {name}"),
            ),
            // `onlyFailed=false` is sent explicitly: it is the default, but the
            // two parameters are read together by the worker and a restart that
            // silently skipped the healthy tasks would be indistinguishable
            // from one that did nothing.
            None => (
                format!(
                    "/connectors/{}/restart?includeTasks={include_tasks}&onlyFailed=false",
                    path_segment(name)
                ),
                match include_tasks {
                    true => format!("restart {name} and its tasks"),
                    false => format!("restart {name}"),
                },
            ),
        };
        self.call(Method::Post, &path, None, &what)?;
        Ok(())
    }

    /// `PUT /connectors/{name}/pause`.
    fn pause(&self, name: &str) -> crate::Result<()> {
        self.call(
            Method::Put,
            &format!("/connectors/{}/pause", path_segment(name)),
            None,
            &format!("pause {name}"),
        )?;
        Ok(())
    }

    /// `PUT /connectors/{name}/resume`.
    fn resume(&self, name: &str) -> crate::Result<()> {
        self.call(
            Method::Put,
            &format!("/connectors/{}/resume", path_segment(name)),
            None,
            &format!("resume {name}"),
        )?;
        Ok(())
    }

    /// A config with `field` set to `expected`, or an error if it is already
    /// set to something else. See [`validate`](Self::validate).
    fn with_field(
        &self,
        config: &BTreeMap<String, String>,
        field: &str,
        expected: &str,
    ) -> crate::Result<BTreeMap<String, String>> {
        match config.get(field) {
            Some(present) if present != expected => Err(Error::Other(format!(
                "this config says {field}={present:?} but the request is for {expected:?} — \
                 they have to match"
            ))),
            _ => {
                let mut config = config.clone();
                config.insert(field.to_string(), expected.to_string());
                Ok(config)
            }
        }
    }

    /// One Connect round trip, with every failure already classified.
    ///
    /// `what` is the operation as the user would name it, and becomes the head
    /// of any error message.
    fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&str>,
        what: &str,
    ) -> crate::Result<String> {
        let url = format!("{}{path}", self.base_url);
        let mut response = match method {
            Method::Get => self.tune(ureq::get(&url), false).call(),
            Method::Delete => self.tune(ureq::delete(&url), false).call(),
            Method::Post => match body {
                Some(payload) => self.tune(ureq::post(&url), true).send(payload),
                None => self.tune(ureq::post(&url), false).send_empty(),
            },
            Method::Put => match body {
                Some(payload) => self.tune(ureq::put(&url), true).send(payload),
                None => self.tune(ureq::put(&url), false).send_empty(),
            },
        }
        .map_err(|e| {
            // The one message that has to name the URL: a Connect cluster is
            // reached at an address the user typed into a field they may not
            // remember filling in, and "connection refused" without it is
            // unactionable.
            Error::Other(format!(
                "Kavka could not reach the Kafka Connect cluster {:?} at {} to {what}: {e}",
                self.cluster, self.base_url
            ))
        })?;

        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|e| {
            Error::Other(format!(
                "reading {}'s answer while trying to {what}: {e}",
                self.describe()
            ))
        })?;
        if (200..300).contains(&status) {
            return Ok(body);
        }
        Err(self.classify(status, &body, what))
    }

    /// Turns a non-2xx into the sentence the user reads.
    ///
    /// Every branch quotes the worker's own `message` — it names the connector,
    /// the field or the config topic, and no wording invented here could be
    /// more specific than that.
    fn classify(&self, status: u16, body: &str, what: &str) -> Error {
        let detail = describe_error(body);
        Error::Other(match status {
            // The worker group is mid-rebalance. Not a rejection — see the
            // module docs.
            409 => format!(
                "the Connect cluster {:?} is rebalancing and could not {what} yet — \
                 try again in a few seconds. It said: {detail}",
                self.cluster
            ),
            404 => format!(
                "{} has no such connector or plugin: {detail}",
                self.describe()
            ),
            400 => format!("{} refused this request: {detail}", self.describe()),
            401 | 403 => format!(
                "{} rejected these credentials — check the username, then re-enter the \
                 password: {detail}",
                self.describe()
            ),
            500..=599 => format!(
                "{} failed while trying to {what}: {detail}",
                self.describe()
            ),
            other => format!(
                "{} answered HTTP {other} while trying to {what}: {detail}",
                self.describe()
            ),
        })
    }

    /// The parts of a request that never vary. Generic over ureq's body
    /// typestate so the four verbs share one place where the TLS config, the
    /// timeout and the credentials are decided.
    fn tune<Any>(
        &self,
        builder: ureq::RequestBuilder<Any>,
        has_body: bool,
    ) -> ureq::RequestBuilder<Any> {
        let mut builder = builder
            .config()
            // Same reasoning as src/sr.rs and src/auth/oidc.rs: a self-hosted
            // worker is routinely fronted by a private CA that lives in the OS
            // trust store and nowhere else.
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            // Read the body ourselves: Connect puts its own message in the
            // 4xx/5xx body, and that is the useful diagnosis.
            .http_status_as_error(false)
            .timeout_global(Some(HTTP_TIMEOUT))
            .build()
            .header("Accept", ACCEPT);
        if has_body {
            builder = builder.header("Content-Type", CONTENT_TYPE);
        }
        if let Some(authorization) = &self.authorization {
            builder = builder.header("Authorization", authorization);
        }
        builder
    }

    /// How the cluster is named in a message: the name the user gave it and
    /// the address it lives at, because a profile with two workers has two of
    /// each and only both together identify one.
    fn describe(&self) -> String {
        format!(
            "the Connect cluster {:?} at {}",
            self.cluster, self.base_url
        )
    }
}

/// Hand-written so the `Authorization` header — which carries the worker
/// password — can never reach a log line or a panic message (D5).
impl fmt::Debug for ConnectClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectClient")
            .field("cluster", &self.cluster)
            .field("base_url", &self.base_url)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Wire shapes. Connect's REST API mixes camelCase and snake_case in the same
// document (`error_count` beside `recommended_values` beside `display_name`),
// so every field is spelled out rather than renamed wholesale.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// One entry of `GET /connectors?expand=info&expand=status`.
#[derive(Deserialize)]
struct ExpandedConnector {
    status: Option<ConnectorStatus>,
    info: Option<ConnectorInfo>,
}

impl ExpandedConnector {
    /// A worker that expanded `info` but not `status` still yields a row: the
    /// connector exists, and "it is there but its state is unknown" is a
    /// truthful answer where dropping it from the list is not.
    fn into_summary(self, name: &str) -> ConnectorSummary {
        let connector_type = self
            .info
            .and_then(|info| info.connector_type)
            .or_else(|| self.status.as_ref().and_then(|s| s.connector_type.clone()));
        let mut summary = match self.status {
            Some(status) => status.into_summary(name),
            None => ConnectorSummary {
                name: name.to_string(),
                connector_state: UNKNOWN.to_string(),
                worker_id: String::new(),
                connector_type: UNKNOWN.to_string(),
                tasks: Vec::new(),
            },
        };
        if let Some(connector_type) = connector_type {
            summary.connector_type = connector_type;
        }
        summary
    }
}

/// `GET /connectors/{name}/status`, and the `status` half of the expanded
/// listing.
#[derive(Deserialize)]
struct ConnectorStatus {
    connector: Option<ConnectorState>,
    #[serde(default)]
    tasks: Vec<TaskState>,
    #[serde(rename = "type")]
    connector_type: Option<String>,
}

impl ConnectorStatus {
    fn into_summary(self, name: &str) -> ConnectorSummary {
        let connector = self.connector.unwrap_or_default();
        ConnectorSummary {
            name: name.to_string(),
            connector_state: state_or_unknown(connector.state),
            worker_id: connector.worker_id.unwrap_or_default(),
            connector_type: state_or_unknown(self.connector_type),
            tasks: self
                .tasks
                .into_iter()
                .map(|task| TaskStatus {
                    id: task.id,
                    state: state_or_unknown(task.state),
                    worker_id: task.worker_id.unwrap_or_default(),
                    // Verbatim, uncapped — see the module docs.
                    trace: task.trace,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize, Default)]
struct ConnectorState {
    state: Option<String>,
    worker_id: Option<String>,
}

#[derive(Deserialize)]
struct TaskState {
    id: i32,
    state: Option<String>,
    worker_id: Option<String>,
    trace: Option<String>,
}

/// The `info` half of the expanded listing. Only `type` is read from it; the
/// config it also carries is fetched deliberately by [`ConnectClient::config`],
/// because a listing is not where a user expects to be handed every
/// connector's credentials-bearing config.
#[derive(Deserialize)]
struct ConnectorInfo {
    #[serde(rename = "type")]
    connector_type: Option<String>,
}

/// `PUT /connector-plugins/{class}/config/validate`.
#[derive(Deserialize)]
struct ValidationResponse {
    error_count: i32,
    #[serde(default)]
    configs: Vec<ValidationEntry>,
}

/// Connect splits each field into the plugin's static `definition` and the
/// worker's `value` for this particular config. Kavka flattens them, because
/// no caller has ever wanted one without the other.
#[derive(Deserialize)]
struct ValidationEntry {
    definition: Option<ConfigDefinition>,
    value: Option<ConfigValueEntry>,
}

impl From<ValidationEntry> for ConfigValue {
    fn from(entry: ValidationEntry) -> Self {
        let definition = entry.definition.unwrap_or_default();
        let value = entry.value.unwrap_or_default();
        ConfigValue {
            // The definition is the authority on the name; `value.name` is the
            // fallback for a field the plugin does not define at all, which is
            // exactly the "unknown configuration" case a user needs to see.
            name: definition.name.or(value.name).unwrap_or_default(),
            value: value.value,
            errors: value.errors,
            required: definition.required,
            documentation: definition.documentation,
        }
    }
}

#[derive(Deserialize, Default)]
struct ConfigDefinition {
    name: Option<String>,
    #[serde(default)]
    required: bool,
    documentation: Option<String>,
}

#[derive(Deserialize, Default)]
struct ConfigValueEntry {
    name: Option<String>,
    value: Option<String>,
    #[serde(default)]
    errors: Vec<String>,
}

/// Connect's error envelope: `{"error_code": 409, "message": "..."}`.
#[derive(Deserialize)]
struct ErrorResponse {
    message: Option<String>,
}

/// What a worker reports when it will not name a state or a type. A word, not
/// an empty cell — docs/DESIGN.md Law 2, "every dot has a word".
const UNKNOWN: &str = "unknown";

fn state_or_unknown(value: Option<String>) -> String {
    match value {
        Some(value) if !value.is_empty() => value,
        _ => UNKNOWN.to_string(),
    }
}

fn serialize(config: &BTreeMap<String, String>) -> String {
    serde_json::to_string(config).expect("a string map is infallibly serializable")
}

/// RFC 7617 basic credentials. Built here rather than by putting the
/// credentials in the URL, which would leak them into every ureq error string.
fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

/// Percent-encodes one path segment (RFC 3986 unreserved set).
///
/// A connector name is user data and Connect allows very nearly anything in
/// one, including `/` and spaces — the same reasoning as
/// [`crate::sr`]'s subject names, with a worse failure mode: an unencoded `/`
/// in `DELETE /connectors/{name}` addresses a *different* connector's
/// sub-resource.
fn path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

fn describe_error(body: &str) -> String {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(ErrorResponse {
            message: Some(message),
        }) => message,
        _ => truncate(body.trim(), ERROR_BODY_LIMIT),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::SecretRef;
    use crate::sr::canned::CannedRegistry;

    const EXPANDED: &str = r#"{
      "orders-sink": {
        "info": {"name":"orders-sink","type":"sink","config":{"connector.class":"io.example.Sink"}},
        "status": {
          "name": "orders-sink",
          "connector": {"state":"RUNNING","worker_id":"10.0.4.19:8083"},
          "tasks": [
            {"id":0,"state":"RUNNING","worker_id":"10.0.4.19:8083"},
            {"id":1,"state":"FAILED","worker_id":"10.0.4.20:8083",
             "trace":"org.apache.kafka.connect.errors.ConnectException: Tolerance exceeded\n\tat org.apache.kafka.connect.runtime.errors.RetryWithToleranceOperator.execAndHandleError(RetryWithToleranceOperator.java:220)\nCaused by: java.sql.SQLException: connection refused"}
          ],
          "type": "sink"
        }
      },
      "audit-source": {
        "info": {"name":"audit-source","type":"source","config":{}},
        "status": {
          "name": "audit-source",
          "connector": {"state":"PAUSED","worker_id":"10.0.4.19:8083"},
          "tasks": [],
          "type": "source"
        }
      }
    }"#;

    fn cluster(url: &str) -> ConnectClusterConfig {
        ConnectClusterConfig {
            name: "sources".into(),
            url: url.to_string(),
            username: None,
            password: None,
        }
    }

    fn client(url: &str) -> ConnectClient {
        ConnectClient::new(&cluster(url)).expect("no credentials to resolve")
    }

    fn config_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    // -----------------------------------------------------------------------
    // Listing.
    // -----------------------------------------------------------------------

    #[test]
    fn the_listing_expands_status_and_info_in_one_request() {
        let server = CannedRegistry::start(vec![(
            "/connectors?expand=info&expand=status",
            200,
            EXPANDED,
        )]);
        let connectors = client(server.url()).list().expect("lists");

        assert_eq!(connectors.len(), 2);
        // Sorted by name, so the table does not reshuffle between refreshes.
        assert_eq!(connectors[0].name, "audit-source");
        assert_eq!(connectors[0].connector_state, "PAUSED");
        assert_eq!(connectors[0].connector_type, "source");
        assert!(connectors[0].tasks.is_empty());

        let sink = &connectors[1];
        assert_eq!(sink.name, "orders-sink");
        assert_eq!(sink.connector_state, "RUNNING");
        assert_eq!(sink.connector_type, "sink");
        assert_eq!(sink.worker_id, "10.0.4.19:8083");
        assert_eq!(sink.tasks.len(), 2);
        assert_eq!(sink.tasks[0].state, "RUNNING");
        assert_eq!(sink.tasks[0].trace, None);
        assert_eq!(sink.tasks[1].id, 1);
        assert_eq!(sink.tasks[1].worker_id, "10.0.4.20:8083");

        let seen = server.seen();
        assert_eq!(
            seen.len(),
            1,
            "one request, not one per connector: {seen:?}"
        );
        assert_eq!(seen[0].method, "GET");
        // Repeated parameter, not comma-joined — Connect binds a List<String>.
        assert_eq!(seen[0].path, "/connectors?expand=info&expand=status");
    }

    /// The single most useful string Connect produces, and the one a "helpful"
    /// client is most tempted to trim.
    #[test]
    fn a_failure_trace_survives_verbatim_including_its_caused_by_chain() {
        let server = CannedRegistry::start(vec![(
            "/connectors?expand=info&expand=status",
            200,
            EXPANDED,
        )]);
        let connectors = client(server.url()).list().expect("lists");
        let trace = connectors[1].tasks[1]
            .trace
            .as_deref()
            .expect("the failed task carries one");

        assert!(trace.starts_with("org.apache.kafka.connect.errors.ConnectException"));
        assert!(trace.contains("Caused by: java.sql.SQLException: connection refused"));
        assert!(trace.contains("RetryWithToleranceOperator.java:220"));
        assert!(!trace.contains('…'), "not truncated: {trace}");
    }

    /// A `RUNNING` connector whose tasks are dead is the commonest Connect
    /// incident there is; the two states must not be collapsed.
    #[test]
    fn a_running_connector_with_failed_tasks_reports_both() {
        let server = CannedRegistry::start(vec![(
            "/connectors?expand=info&expand=status",
            200,
            EXPANDED,
        )]);
        let connectors = client(server.url()).list().expect("lists");
        let sink = &connectors[1];
        assert_eq!(sink.connector_state, "RUNNING");
        assert_eq!(sink.tasks[1].state, "FAILED");
    }

    /// Workers older than Connect 2.3 ignore `expand` and answer a plain array.
    /// Reading that as "no connectors" would be a silently wrong inventory.
    #[test]
    fn a_worker_that_ignores_expand_still_produces_states() {
        let server = CannedRegistry::start(vec![
            (
                "/connectors?expand=info&expand=status",
                200,
                r#"["orders-sink"]"#,
            ),
            (
                "/connectors/orders-sink/status",
                200,
                r#"{"name":"orders-sink","connector":{"state":"RUNNING","worker_id":"w1:8083"},"tasks":[{"id":0,"state":"UNASSIGNED","worker_id":"w1:8083"}],"type":"sink"}"#,
            ),
        ]);
        let connectors = client(server.url()).list().expect("lists");

        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].connector_state, "RUNNING");
        assert_eq!(connectors[0].connector_type, "sink");
        assert_eq!(connectors[0].tasks[0].state, "UNASSIGNED");
        assert_eq!(server.seen().len(), 2, "listing then one status");
    }

    /// Law 2: every state produces a word. A worker that answers without one
    /// yields "unknown", never an empty cell.
    #[test]
    fn a_status_with_no_state_reads_as_unknown_rather_than_blank() {
        let server = CannedRegistry::start(vec![(
            "/connectors?expand=info&expand=status",
            200,
            r#"{"ghost":{"info":null,"status":{"name":"ghost","connector":{},"tasks":[{"id":0}]}}}"#,
        )]);
        let connectors = client(server.url()).list().expect("lists");

        assert_eq!(connectors[0].connector_state, "unknown");
        assert_eq!(connectors[0].connector_type, "unknown");
        assert_eq!(connectors[0].tasks[0].state, "unknown");
    }

    // -----------------------------------------------------------------------
    // Config and validation.
    // -----------------------------------------------------------------------

    #[test]
    fn a_connectors_config_comes_back_as_a_plain_map() {
        let server = CannedRegistry::start(vec![(
            "/connectors/orders-sink/config",
            200,
            r#"{"connector.class":"io.example.Sink","tasks.max":"3","topics":"orders"}"#,
        )]);
        let config = client(server.url()).config("orders-sink").expect("reads");

        assert_eq!(config["tasks.max"], "3");
        assert_eq!(config["topics"], "orders");
        assert_eq!(server.seen()[0].method, "GET");
    }

    #[test]
    fn validation_carries_per_field_errors_and_the_workers_own_count() {
        let server = CannedRegistry::start(vec![(
            "/connector-plugins/io.example.Sink/config/validate",
            200,
            r#"{
              "name": "io.example.Sink",
              "error_count": 2,
              "groups": ["Common"],
              "configs": [
                {
                  "definition": {"name":"topics","type":"LIST","required":true,"documentation":"Topics to consume."},
                  "value": {"name":"topics","value":null,"recommended_values":[],"errors":["Missing required configuration \"topics\" which has no default value."],"visible":true}
                },
                {
                  "definition": {"name":"tasks.max","type":"INT","required":false,"documentation":"Maximum tasks."},
                  "value": {"name":"tasks.max","value":"0","recommended_values":[],"errors":["Value must be at least 1"],"visible":true}
                },
                {
                  "definition": {"name":"connector.class","type":"STRING","required":true},
                  "value": {"name":"connector.class","value":"io.example.Sink","recommended_values":[],"errors":[],"visible":true}
                }
              ]
            }"#,
        )]);

        let result = client(server.url())
            .validate("io.example.Sink", &config_of(&[("tasks.max", "0")]))
            .expect("validates");

        assert_eq!(result.error_count, 2);
        assert_eq!(result.configs.len(), 3);

        let topics = &result.configs[0];
        assert_eq!(topics.name, "topics");
        assert_eq!(topics.value, None);
        assert!(topics.required);
        assert_eq!(topics.documentation.as_deref(), Some("Topics to consume."));
        assert_eq!(topics.errors.len(), 1);
        assert!(topics.errors[0].contains("Missing required configuration"));

        assert_eq!(result.configs[1].errors, vec!["Value must be at least 1"]);
        assert!(result.configs[2].errors.is_empty());
        assert!(!result.configs[1].required);

        let seen = server.seen();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(
            seen[0].content_type.as_deref(),
            Some("application/json"),
            "{seen:?}"
        );
        // `connector.class` was filled in from the URL rather than demanded
        // twice.
        assert_eq!(
            seen[0].json(),
            serde_json::json!({"connector.class": "io.example.Sink", "tasks.max": "0"})
        );
    }

    /// `error_count` is the worker's, not a count of the fields it listed: a
    /// group-level error belongs to no field, and deriving the number would
    /// report a clean config the worker will refuse.
    #[test]
    fn the_error_count_is_the_workers_own_not_a_recount() {
        let server = CannedRegistry::start(vec![(
            "/connector-plugins/io.example.Sink/config/validate",
            200,
            r#"{"name":"io.example.Sink","error_count":1,"groups":[],"configs":[]}"#,
        )]);
        let result = client(server.url())
            .validate("io.example.Sink", &config_of(&[]))
            .expect("validates");

        assert_eq!(result.error_count, 1);
        assert!(result.configs.is_empty());
    }

    #[test]
    fn a_config_that_contradicts_the_class_in_the_url_is_refused_before_the_request() {
        let server = CannedRegistry::start(vec![]);
        let err = client(server.url())
            .validate(
                "io.example.Sink",
                &config_of(&[("connector.class", "io.example.Other")]),
            )
            .expect_err("the form disagrees with itself");

        assert!(err.to_string().contains("io.example.Other"), "got {err}");
        assert!(err.to_string().contains("have to match"), "got {err}");
        assert!(server.seen().is_empty(), "nothing was sent");
    }

    // -----------------------------------------------------------------------
    // Mutations.
    // -----------------------------------------------------------------------

    #[test]
    fn applying_a_config_puts_it_with_the_name_filled_in() {
        let server = CannedRegistry::start(vec![(
            "PUT /connectors/orders-sink/config",
            201,
            r#"{"name":"orders-sink","config":{},"tasks":[],"type":"sink"}"#,
        )]);
        client(server.url())
            .apply(
                "orders-sink",
                &config_of(&[("connector.class", "io.example.Sink"), ("tasks.max", "3")]),
            )
            .expect("applies");

        let seen = server.seen();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(seen[0].path, "/connectors/orders-sink/config");
        assert_eq!(
            seen[0].json(),
            serde_json::json!({
                "name": "orders-sink",
                "connector.class": "io.example.Sink",
                "tasks.max": "3"
            })
        );
    }

    #[test]
    fn deleting_a_connector_accepts_the_workers_204() {
        let server = CannedRegistry::start(vec![("DELETE /connectors/orders-sink", 204, "")]);
        client(server.url()).delete("orders-sink").expect("deletes");

        assert_eq!(server.seen()[0].method, "DELETE");
        assert_eq!(server.seen()[0].path, "/connectors/orders-sink");
    }

    #[test]
    fn restart_sends_the_verb_and_the_scope_the_caller_asked_for() {
        let server = CannedRegistry::start(vec![
            (
                "/connectors/orders-sink/restart?includeTasks=true&onlyFailed=false",
                204,
                "",
            ),
            (
                "/connectors/orders-sink/restart?includeTasks=false&onlyFailed=false",
                204,
                "",
            ),
            ("/connectors/orders-sink/tasks/2/restart", 204, ""),
        ]);
        let client = client(server.url());

        client.restart("orders-sink", None, true).expect("all");
        client
            .restart("orders-sink", None, false)
            .expect("connector only");
        client
            .restart("orders-sink", Some(2), true)
            .expect("one task");

        let seen = server.seen();
        assert!(seen.iter().all(|s| s.method == "POST"), "{seen:?}");
        assert_eq!(
            seen[0].path,
            "/connectors/orders-sink/restart?includeTasks=true&onlyFailed=false"
        );
        assert_eq!(
            seen[1].path,
            "/connectors/orders-sink/restart?includeTasks=false&onlyFailed=false"
        );
        // A task-level restart ignores includeTasks entirely — there is one
        // task to restart and it is named in the path.
        assert_eq!(seen[2].path, "/connectors/orders-sink/tasks/2/restart");
    }

    #[test]
    fn pause_and_resume_are_puts_and_accept_the_workers_202() {
        let server = CannedRegistry::start(vec![
            ("/connectors/orders-sink/pause", 202, ""),
            ("/connectors/orders-sink/resume", 202, ""),
        ]);
        let client = client(server.url());
        client.pause("orders-sink").expect("pauses");
        client.resume("orders-sink").expect("resumes");

        let seen = server.seen();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(seen[0].path, "/connectors/orders-sink/pause");
        assert_eq!(seen[1].method, "PUT");
        assert_eq!(seen[1].path, "/connectors/orders-sink/resume");
    }

    /// A connector name is user data, and an unencoded `/` in it addresses a
    /// different connector's sub-resource.
    #[test]
    fn connector_names_are_percent_encoded_into_the_path() {
        assert_eq!(path_segment("orders-sink"), "orders-sink");
        assert_eq!(path_segment("a/b"), "a%2Fb");
        assert_eq!(path_segment("with space"), "with%20space");
        assert_eq!(path_segment("io.example.Sink"), "io.example.Sink");

        let server = CannedRegistry::start(vec![("DELETE /connectors/a%2Fb", 204, "")]);
        client(server.url()).delete("a/b").expect("deletes");
        assert_eq!(server.seen()[0].path, "/connectors/a%2Fb");
    }

    // -----------------------------------------------------------------------
    // Error mapping.
    // -----------------------------------------------------------------------

    /// The one message that has to name the address: the user typed it into a
    /// field they may not remember filling in.
    #[test]
    fn an_unreachable_cluster_names_the_url_and_the_cluster() {
        // Port 1 is never listening; the connection is refused immediately.
        let err = client("http://127.0.0.1:1")
            .list()
            .expect_err("nothing is listening")
            .to_string();

        assert!(err.contains("http://127.0.0.1:1"), "got {err}");
        assert!(err.contains("\"sources\""), "got {err}");
        assert!(err.contains("list the connectors"), "got {err}");
    }

    /// 409 means "the group is rebalancing", which is a state the user's own
    /// action causes. Reported as a rejection it reads as the opposite of what
    /// happened.
    #[test]
    fn a_rebalancing_worker_says_to_try_again_rather_than_reporting_a_rejection() {
        let server = CannedRegistry::start(vec![(
            "PUT /connectors/orders-sink/config",
            409,
            r#"{"error_code":409,"message":"Cannot complete request momentarily due to stale configuration (typically caused by a concurrent config change)"}"#,
        )]);
        let err = client(server.url())
            .apply("orders-sink", &config_of(&[]))
            .expect_err("409")
            .to_string();

        assert!(err.contains("rebalancing"), "got {err}");
        assert!(err.contains("try again"), "got {err}");
        // The worker's own reason is quoted, not replaced.
        assert!(err.contains("stale configuration"), "got {err}");
        assert!(!err.contains("rejected"), "409 is not a rejection: {err}");
    }

    #[test]
    fn the_status_code_decides_the_sentence_and_the_worker_supplies_the_reason() {
        for (status, body, expected) in [
            (
                404,
                r#"{"error_code":404,"message":"Connector orders-sink not found"}"#,
                "no such connector",
            ),
            (
                400,
                r#"{"error_code":400,"message":"Connector config {} contains no connector type"}"#,
                "refused this request",
            ),
            (
                401,
                r#"{"error_code":401,"message":"User cannot access the resource."}"#,
                "rejected these credentials",
            ),
            (
                403,
                r#"{"error_code":403,"message":"User cannot access the resource."}"#,
                "rejected these credentials",
            ),
            (
                500,
                r#"{"error_code":500,"message":"Request timed out"}"#,
                "failed while trying to",
            ),
        ] {
            let server =
                CannedRegistry::start(vec![("/connectors/orders-sink/config", status, body)]);
            let err = client(server.url())
                .config("orders-sink")
                .expect_err("not a 2xx")
                .to_string();

            assert!(err.contains(expected), "HTTP {status} -> {err}");
            // Every branch quotes the worker verbatim.
            let message: serde_json::Value = serde_json::from_str(body).unwrap();
            let message = message["message"].as_str().unwrap();
            assert!(
                err.contains(message),
                "HTTP {status} dropped its reason: {err}"
            );
            assert!(
                err.contains("the Connect cluster"),
                "HTTP {status} -> {err}"
            );
        }
    }

    /// A worker behind a proxy answers HTML, and a wall of it is not a message.
    #[test]
    fn an_unstructured_error_body_is_quoted_but_capped() {
        let server = CannedRegistry::start(vec![(
            "/connectors/orders-sink/config",
            502,
            "<html><head><title>502 Bad Gateway</title></head></html>",
        )]);
        let err = client(server.url())
            .config("orders-sink")
            .expect_err("502")
            .to_string();

        assert!(err.contains("502 Bad Gateway"), "got {err}");
        let capped = truncate(&"x".repeat(400), ERROR_BODY_LIMIT);
        assert_eq!(
            capped.chars().count(),
            ERROR_BODY_LIMIT + 1,
            "plus the ellipsis"
        );
        assert!(capped.ends_with('…'));
    }

    // -----------------------------------------------------------------------
    // Credentials (D5).
    // -----------------------------------------------------------------------

    #[test]
    fn basic_auth_is_sent_when_the_cluster_carries_credentials() {
        let server =
            CannedRegistry::start(vec![("/connectors?expand=info&expand=status", 200, "{}")]);
        let mut client = client(server.url());
        client.authorization = Some(basic_auth("alice", "s3cr3t"));
        client.list().expect("lists");

        let seen = server.seen();
        // base64("alice:s3cr3t")
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some("Basic YWxpY2U6czNjcjN0")
        );
        assert_eq!(seen[0].accept.as_deref(), Some("application/json"));
    }

    #[test]
    fn no_credentials_means_no_header_at_all_not_an_empty_one() {
        let server =
            CannedRegistry::start(vec![("/connectors?expand=info&expand=status", 200, "{}")]);
        client(server.url()).list().expect("lists");
        assert_eq!(server.seen()[0].authorization, None);
    }

    #[test]
    fn the_authorization_header_never_reaches_debug() {
        let mut client = client("http://connect.example:8083");
        client.authorization = Some(basic_auth("alice", "s3cr3t"));
        let rendered = format!("{client:?}");

        assert!(!rendered.contains("s3cr3t"), "leaked: {rendered}");
        assert!(!rendered.contains("YWxpY2U6czNjcjN0"), "leaked: {rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }

    /// Unlike the Schema Registry's decode path, there is no half-answer to
    /// degrade to here — so a keychain failure is reported instead of becoming
    /// an unauthenticated request that the worker rejects for the wrong reason.
    #[test]
    fn a_missing_keychain_secret_fails_the_client_rather_than_going_anonymous() {
        let err = ConnectClient::new(&ConnectClusterConfig {
            name: "sources".into(),
            url: "http://connect.example:8083".into(),
            username: Some("alice".into()),
            password: Some(SecretRef {
                entry: format!("kavka-unit-test/absent-connect/{}", std::process::id()),
            }),
        })
        .expect_err("nothing in the keychain")
        .to_string();

        assert!(err.contains("Connect cluster \"sources\""), "got {err}");
        assert!(!err.contains("http://connect.example"), "no need: {err}");
    }

    #[test]
    fn trailing_slashes_in_the_configured_url_do_not_double_up() {
        assert_eq!(
            client("http://connect.example:8083/").base_url,
            "http://connect.example:8083"
        );
    }

    // -----------------------------------------------------------------------
    // Cluster resolution and the read-only gate (D5).
    // -----------------------------------------------------------------------

    fn bootstrap() -> String {
        std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
    }

    fn integration() -> bool {
        if std::env::var("KAVKA_IT").is_err() {
            eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
            return false;
        }
        true
    }

    /// A connection whose only Connect cluster would hang or fail loudly if it
    /// were ever consulted — so a passing read-only assertion proves the gate
    /// ran before the client was built.
    fn connection(read_only: bool) -> ClusterConnection {
        use crate::profiles::{AuthConfig, ConnectionProfile};
        ClusterConnection::connect(ConnectionProfile {
            id: "it-connect".into(),
            name: "local docker".into(),
            environment: "dev".into(),
            bootstrap_servers: vec![bootstrap()],
            auth: AuthConfig::Plaintext,
            read_only,
            schema_registry: None,
            connect_clusters: vec![ConnectClusterConfig {
                name: "sources".into(),
                url: "http://127.0.0.1:1".into(),
                username: None,
                password: None,
            }],
            metrics_endpoint: None,
            sampler_interval_ms: None,
            wasm_serdes: Vec::new(),
        })
        .expect("connect")
    }

    /// D5: read-only is enforced in core, and enforced *first* — before a
    /// keychain read, before a socket.
    #[test]
    fn a_read_only_connection_refuses_every_mutating_connect_call() {
        if !integration() {
            return;
        }
        let conn = connection(true);
        let config = config_of(&[("connector.class", "io.example.Sink")]);
        let started = std::time::Instant::now();

        let refusals: Vec<Error> = vec![
            apply(&conn, "sources", "orders-sink", &config).expect_err("apply"),
            delete(&conn, "sources", "orders-sink").expect_err("delete"),
            restart(&conn, "sources", "orders-sink", None, true).expect_err("restart"),
            pause(&conn, "sources", "orders-sink").expect_err("pause"),
            resume(&conn, "sources", "orders-sink").expect_err("resume"),
        ];
        for err in &refusals {
            assert!(matches!(err, Error::ReadOnly(_)), "got {err}");
            assert!(err.to_string().contains("read-only"), "got {err}");
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "nothing was attempted over the network"
        );
    }

    /// The read side is not gated: a read-only connection is for reading.
    #[test]
    fn a_read_only_connection_still_reaches_the_read_side() {
        if !integration() {
            return;
        }
        let err = list(&connection(true), "sources").expect_err("nothing is listening on port 1");
        assert!(!matches!(err, Error::ReadOnly(_)), "got {err}");
        assert!(err.to_string().contains("127.0.0.1:1"), "got {err}");
    }

    #[test]
    fn naming_a_cluster_the_profile_does_not_have_lists_the_ones_it_does() {
        if !integration() {
            return;
        }
        let conn = connection(false);
        let err = list(&conn, "sinks")
            .expect_err("no such cluster")
            .to_string();
        assert!(err.contains("\"sinks\""), "got {err}");
        assert!(err.contains("\"sources\""), "got {err}");
    }
}
