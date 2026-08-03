//! The server: MCP method dispatch, the tool implementations, and the shape of
//! what a tool answers with.
//!
//! **One request at a time, on one thread.** MCP allows a client to have
//! several calls in flight, and this serves them in order instead. That is a
//! deliberate trade and it is affordable for one reason: every tool here is
//! bounded — 500 records, 50,000 scanned, a 30-second wall clock — so the
//! longest anything can hold the pipe is a known number, and the alternative
//! (a runtime, a session table, cancellation plumbing) is machinery for a queue
//! that has one caller. kavka-core is blocking by design, so this also means no
//! `spawn_blocking` hop between the pipe and librdkafka.
//!
//! Connections are opened on first use and kept for the life of the process,
//! keyed by profile id **and a fingerprint of the profile**: edit a connection
//! in the app — new brokers, new credentials, read-only toggled — and the next
//! tool call reconnects rather than answering from a client built against the
//! old document.

use crate::config;
use crate::gate::{self, MaskPolicy, WritePolicy, ALLOW_PROD_ENV, ALLOW_WRITES_ENV, UNMASKED_ENV};
use crate::jsonrpc::{self, Incoming, Parsed};
use crate::tools::{self, Tool};
use kavka_core::admin::{self, OffsetResetSpec};
use kavka_core::connection::ClusterConnection;
use kavka_core::consume::{self, FetchSpec, SeekSpec};
use kavka_core::masking::{self, MaskSet, MaskStore};
use kavka_core::produce::{self, ProduceRecordSpec};
use kavka_core::profiles::{ConnectionProfile, ProfileStore};
use kavka_core::search::{SearchQuery, SearchSession, SearchSpec};
use kavka_core::serdes::{DecodedPayload, MessageRecord};
use kavka_core::sql::{SqlSession, SqlSpec};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// MCP revisions this server speaks, newest first.
///
/// Negotiation is the spec's: echo the client's version when it is one of
/// these, otherwise answer with the newest we support and let the client decide
/// whether it can live with that. Adding a revision is an entry in this list
/// plus whatever that revision actually changed — which is the drift the
/// official SDK would have handled for us, made explicit instead (see the crate
/// docs, "WHY NOT rmcp").
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The revision this server prefers.
pub const LATEST_PROTOCOL_VERSION: &str = SUPPORTED_PROTOCOL_VERSIONS[0];

/// How long a search or query reader waits for a batch before looking at the
/// clock and the caps again. Also the granularity of the deadline.
const DRAIN_POLL: Duration = Duration::from_millis(250);

/// One MCP server over one pair of pipes.
pub struct Server {
    store: ProfileStore,
    /// The app's `masking.json`, from the same directory as `profiles.json` —
    /// see [`Server::mask_set`].
    masks: MaskStore,
    /// Kept for messages: "no connections in <file>" is actionable, "no
    /// connections" is not.
    profiles_path: std::path::PathBuf,
    policy: WritePolicy,
    masking: MaskPolicy,
    /// profile id -> (fingerprint of the profile document, live connection).
    connections: HashMap<String, (String, Arc<ClusterConnection>)>,
}

impl Server {
    /// A server reading `profiles.json` and `masking.json` from `config_dir`,
    /// with both policies fixed for its lifetime.
    pub fn new(config_dir: std::path::PathBuf, policy: WritePolicy, masking: MaskPolicy) -> Self {
        Self {
            profiles_path: config::profiles_file(&config_dir),
            masks: MaskStore::new(config_dir.clone()),
            store: ProfileStore::new(config_dir),
            policy,
            masking,
            connections: HashMap::new(),
        }
    }

    /// Reads messages until the input ends, answering each on `output`.
    ///
    /// One JSON object per line, flushed after every response: a client that
    /// waits for an answer before sending the next request must not wait on a
    /// buffer. A blank line is skipped rather than answered — some clients send
    /// keep-alive newlines, and a parse error for one would look like a fault.
    pub fn run(&mut self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(response) = self.handle_line(&line) {
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    /// Handles one line, answering with the response line a call is owed.
    /// `None` means the message was a notification (or a response to nothing),
    /// which JSON-RPC forbids answering.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let response = match jsonrpc::parse(line) {
            Parsed::Call(message) => self.handle_call(&message),
            Parsed::Notification(message) => {
                self.handle_notification(&message);
                return None;
            }
            Parsed::Malformed { code, message } => jsonrpc::error(None, code, message),
        };
        // A response that cannot be serialized would desynchronize the stream,
        // so it is answered with one that can.
        Some(serde_json::to_string(&response).unwrap_or_else(|e| {
            format!(
                r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{code},"message":"the answer could not be serialized: {e}"}}}}"#,
                code = jsonrpc::INTERNAL_ERROR,
            )
        }))
    }

    fn handle_call(&mut self, message: &Incoming) -> Value {
        let id = message.id.as_ref().expect("a call carries an id");
        match message.method.as_str() {
            "initialize" => jsonrpc::result(id, self.initialize(message.args())),
            // The keep-alive. Answers an empty object, per spec.
            "ping" => jsonrpc::result(id, json!({})),
            "tools/list" => jsonrpc::result(id, tools::catalogue(self.policy)),
            "tools/call" => match self.call_tool(message.args()) {
                Ok(result) => jsonrpc::result(id, result),
                Err(protocol) => jsonrpc::error(Some(id), protocol.0, protocol.1),
            },
            other => jsonrpc::error(
                Some(id),
                jsonrpc::METHOD_NOT_FOUND,
                format!(
                    "this server implements initialize, ping, tools/list and tools/call. It \
                     declares no resources, prompts, sampling or completion capability, so \
                     {other} has no implementation here."
                ),
            ),
        }
    }

    /// Notifications are acknowledged by doing nothing, which is the protocol.
    ///
    /// `notifications/cancelled` is deliberately a no-op rather than a stub:
    /// this server is inside the tool call the client is trying to cancel — the
    /// message cannot be read until that call returns — and every call is
    /// bounded, so the honest behaviour is to finish and answer.
    fn handle_notification(&mut self, message: &Incoming) {
        match message.method.as_str() {
            "notifications/initialized" | "notifications/cancelled" => {}
            other => eprintln!("kavka-mcp: ignoring unknown notification {other}"),
        }
    }

    fn initialize(&mut self, args: &Value) -> Value {
        let requested = args["protocolVersion"].as_str();
        let agreed = match requested {
            Some(version) if SUPPORTED_PROTOCOL_VERSIONS.contains(&version) => version,
            _ => LATEST_PROTOCOL_VERSION,
        };
        json!({
            "protocolVersion": agreed,
            // Tools only. No resources, no prompts, no sampling — this server
            // answers questions about Kafka and does not ask the model for
            // anything.
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "kavka",
                "title": "Kavka",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": self.instructions(),
        })
    }

    /// The server's self-description, carried on `initialize` so a model knows
    /// the rules before its first call rather than from its first refusal.
    fn instructions(&self) -> String {
        format!(
            "Kavka is a desktop Kafka client; this server exposes the connections saved in it on \
             this machine. Start with kavka_list_profiles — every other tool takes a `profile` id \
             from it. Connections and credentials are read from the app's own profiles.json and \
             the OS keychain; nothing is configured here.\n\n\
             READS: cluster overview, topics, topic detail, messages (decoded through Kavka's \
             serde ladder), search (raw-byte substring and/or CEL over the decoded record), SQL \
             over a bounded scan, consumer groups and their lag. Every read is capped, and every \
             answer reports the cap it hit — a partial answer is never silently partial.\n\n\
             MASKING: {masking}\n\n\
             WRITES: exactly two, kavka_produce and kavka_reset_offsets. {gate}\n\n\
             THERE IS NO OTHER WRITE SURFACE. Creating or deleting topics, editing topic or broker \
             configuration, ACLs, quotas, partition reassignment, Kafka Connect and Schema \
             Registry writes are all things Kavka can do and this server deliberately cannot: the \
             blast radius of an agent holding those is larger than the convenience is worth. Do \
             them in the app.",
            masking = mask_state(self.masking),
            gate = tools::gate_state(self.policy),
        )
    }

    // -----------------------------------------------------------------------
    // tools/call
    // -----------------------------------------------------------------------

    /// Runs one tool. `Err` is a JSON-RPC-level fault (a malformed call, an
    /// unknown tool name); a tool that ran and failed is `Ok` carrying
    /// `isError: true`, because a model has to be able to read the failure and
    /// choose differently.
    fn call_tool(&mut self, args: &Value) -> Result<Value, ProtocolError> {
        let name = args["name"].as_str().ok_or_else(|| {
            ProtocolError(
                jsonrpc::INVALID_PARAMS,
                "tools/call needs a \"name\" string".into(),
            )
        })?;
        let tool = tools::find(name).ok_or_else(|| {
            ProtocolError(
                jsonrpc::INVALID_PARAMS,
                format!(
                    "no tool named {name:?}. This server has: {available}.",
                    available = tools::TOOLS
                        .iter()
                        .map(|tool| tool.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;

        let arguments = match &args["arguments"] {
            Value::Null => Value::Object(Map::new()),
            Value::Object(map) => Value::Object(map.clone()),
            _ => {
                return Err(ProtocolError(
                    jsonrpc::INVALID_PARAMS,
                    "\"arguments\" must be an object".into(),
                ))
            }
        };

        Ok(match self.dispatch(tool, &arguments) {
            Ok(structured) => tool_result(structured),
            Err(message) => tool_error(&message),
        })
    }

    fn dispatch(&mut self, tool: &Tool, args: &Value) -> Result<Value, String> {
        check_arguments(tool, args)?;
        match tool.name {
            "kavka_list_profiles" => self.list_profiles(),
            "kavka_cluster_overview" => self.cluster_overview(args),
            "kavka_list_topics" => self.list_topics(args),
            "kavka_topic_detail" => self.topic_detail(args),
            "kavka_fetch_messages" => self.fetch_messages(args),
            "kavka_search" => self.search(args),
            "kavka_sql" => self.sql(args),
            "kavka_groups" => self.groups(args),
            "kavka_group_detail" => self.group_detail(args),
            "kavka_produce" => self.produce(args),
            "kavka_reset_offsets" => self.reset_offsets(args),
            other => Err(format!("{other} is listed but not implemented")),
        }
    }

    // -----------------------------------------------------------------------
    // Read tools
    // -----------------------------------------------------------------------

    fn list_profiles(&mut self) -> Result<Value, String> {
        let profiles = self.profiles()?;
        let listed: Vec<Value> = profiles
            .iter()
            .map(|profile| {
                let refusal = gate::authorize_write(self.policy, profile, "a write tool").err();
                json!({
                    "id": profile.id,
                    "name": profile.name,
                    "environment": serde_json::to_value(profile.environment)
                        .unwrap_or(Value::Null),
                    "bootstrap_servers": profile.bootstrap_servers,
                    "auth": serde_json::to_value(&profile.auth)
                        .ok()
                        .and_then(|auth| auth.get("kind").cloned())
                        .unwrap_or(Value::Null),
                    "read_only": profile.read_only,
                    "schema_registry": profile.schema_registry.is_some(),
                    "writes_allowed": refusal.is_none(),
                    "writes_refused_because": refusal,
                })
            })
            .collect();
        Ok(json!({
            "profiles": listed,
            "count": listed.len(),
            "profiles_file": self.profiles_path.display().to_string(),
            "write_policy": {
                "writes_enabled": self.policy.writes_enabled,
                "prod_allowed": self.policy.prod_allowed,
                "allow_writes_env": ALLOW_WRITES_ENV,
                "allow_prod_env": ALLOW_PROD_ENV,
                "summary": tools::gate_state(self.policy),
            },
        }))
    }

    fn cluster_overview(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let conn = self.connection(&profile)?;
        let overview = conn.overview().map_err(|e| e.to_string())?;
        Ok(json!({
            "profile": profile.id,
            "name": profile.name,
            "environment": serde_json::to_value(profile.environment).unwrap_or(Value::Null),
            "read_only": profile.read_only,
            "bootstrap_servers": profile.bootstrap_servers,
            "cluster_id": overview.cluster_id,
            "brokers": serde_json::to_value(&overview.brokers).unwrap_or(Value::Null),
            "broker_count": overview.brokers.len(),
            "topic_count": overview.topic_count,
            "partition_count": overview.partition_count,
        }))
    }

    fn list_topics(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let include_internal = bool_arg(args, "include_internal", false)?;
        let needle = opt_str_arg(args, "name_contains")?.map(|s| s.to_lowercase());
        let limit = u32_arg(
            args,
            "limit",
            tools::DEFAULT_TOPIC_LIST,
            tools::MAX_TOPIC_LIST,
        )? as usize;

        let conn = self.connection(&profile)?;
        let all = conn.list_topics().map_err(|e| e.to_string())?;
        let internal_total = all.iter().filter(|topic| topic.internal).count();
        let matching: Vec<&kavka_core::admin::TopicInfo> = all
            .iter()
            .filter(|topic| include_internal || !topic.internal)
            .filter(|topic| {
                needle
                    .as_deref()
                    .is_none_or(|needle| topic.name.to_lowercase().contains(needle))
            })
            .collect();
        let total = matching.len();
        let topics: Vec<Value> = matching
            .into_iter()
            .take(limit)
            .map(|topic| {
                json!({
                    "name": topic.name,
                    "partitions": topic.partitions,
                    "replication_factor": topic.replication_factor,
                    "internal": topic.internal,
                })
            })
            .collect();
        Ok(json!({
            "profile": profile.id,
            "topics": topics,
            "returned": topics.len(),
            "total": total,
            "truncated": total > topics.len(),
            "internal_topics_hidden": if include_internal { 0 } else { internal_total },
            "limits": { "limit": limit },
        }))
    }

    fn topic_detail(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let topic = str_arg(args, "topic")?;
        let include_defaults = bool_arg(args, "include_default_configs", false)?;
        let conn = self.connection(&profile)?;
        let detail = admin::topic_detail(&conn, &topic).map_err(|e| e.to_string())?;

        let approx: i64 = detail
            .partitions
            .iter()
            .map(|p| p.latest_offset.saturating_sub(p.earliest_offset).max(0))
            .sum();
        let under_replicated: Vec<i32> = detail
            .partitions
            .iter()
            .filter(|p| p.isr.len() < p.replicas.len())
            .map(|p| p.partition)
            .collect();
        let configs: Vec<&kavka_core::admin::ConfigEntry> = detail
            .configs
            .iter()
            .filter(|entry| include_defaults || !entry.is_default)
            .collect();
        let omitted = detail.configs.len() - configs.len();

        Ok(json!({
            "profile": profile.id,
            "name": detail.name,
            "internal": detail.internal,
            "partition_count": detail.partitions.len(),
            "partitions": serde_json::to_value(&detail.partitions).unwrap_or(Value::Null),
            "approx_message_count": approx,
            "under_replicated_partitions": under_replicated,
            "configs": serde_json::to_value(&configs).unwrap_or(Value::Null),
            "default_configs_omitted": omitted,
        }))
    }

    fn fetch_messages(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let topic = str_arg(args, "topic")?;
        let max = u32_arg(args, "max", tools::DEFAULT_FETCH, tools::MAX_FETCH)?;
        let max_value_bytes = u32_arg(
            args,
            "max_value_bytes",
            tools::DEFAULT_VALUE_BYTES,
            tools::MAX_VALUE_BYTES,
        )?;
        let verbose = bool_arg(args, "verbose", false)?;
        let spec = FetchSpec {
            topic: topic.clone(),
            seek: seek_arg(args, max)?,
            partitions: partitions_arg(args, "partitions")?,
            max_messages: max,
            max_value_bytes: Some(max_value_bytes),
        };

        let conn = self.connection(&profile)?;
        let mut records =
            consume::fetch_messages(&conn, conn.profile().schema_registry.as_ref(), &spec, None)
                .map_err(|e| e.to_string())?;

        // BEFORE `render_records`, which is the only thing that reads a payload
        // — so there is no shape of this answer, compact or verbose, that can
        // carry text a rule was meant to hide.
        let rules = self.mask_set(&profile.id);
        let masked = mask_batch(&rules, &mut records);

        let mut answer = json!({
            "profile": profile.id,
            "topic": topic,
            "records": render_records(&records, verbose),
            "returned": records.len(),
            "limits": { "max": max, "max_value_bytes": max_value_bytes },
        });
        note_masking(&mut answer, &rules, masked);
        Ok(answer)
    }

    fn search(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let topic = str_arg(args, "topic")?;
        let query = search_query(args)?;
        let scan_cap = u32_arg(
            args,
            "scan_cap",
            tools::DEFAULT_SCAN_CAP,
            tools::MAX_SCAN_CAP,
        )?;
        let max_matches = u32_arg(
            args,
            "max_matches",
            tools::DEFAULT_SEARCH_MATCHES,
            tools::MAX_SEARCH_MATCHES,
        )?;
        let max_value_bytes = u32_arg(
            args,
            "max_value_bytes",
            tools::DEFAULT_VALUE_BYTES,
            tools::MAX_VALUE_BYTES,
        )?;
        let verbose = bool_arg(args, "verbose", false)?;
        let deadline = deadline_arg(args)?;

        let spec = SearchSpec {
            topic: topic.clone(),
            // The scan cap is a record count, so the window has to be at least
            // that big to be able to reach it.
            seek: seek_arg(args, scan_cap)?,
            partitions: partitions_arg(args, "partitions")?,
            query,
            max_buffered: max_matches,
            max_value_bytes: Some(max_value_bytes),
        };

        let conn = self.connection(&profile)?;
        let session = SearchSession::start(&conn, conn.profile().schema_registry.as_ref(), &spec)
            .map_err(|e| e.to_string())?;

        // The scan deliberately CONTINUES after the result buffer is full. The
        // core stops buffering at `max_buffered` and keeps counting, which is
        // what makes "the first 50 of 4,812 matches in 20,000 records" a thing
        // this tool can say — and stopping at the first 50 would make `matched`
        // a restatement of `returned`, which answers nothing.
        let mut matches = Vec::new();
        let mut stopped = "complete";
        while let Some(batch) = session.next_results(DRAIN_POLL) {
            matches.extend(batch);
            if session.progress().scanned >= u64::from(scan_cap) {
                stopped = "scan_cap";
                break;
            }
            if Instant::now() >= deadline {
                stopped = "deadline";
                break;
            }
        }
        session.stop();
        let progress = session.progress();
        // Dropping joins the workers, so the consumers are gone before the
        // answer is written rather than during the next call.
        drop(session);

        if let Some(error) = progress.error {
            return Err(format!(
                "the search failed after {scanned} records: {error}",
                scanned = progress.scanned,
            ));
        }
        matches.truncate(max_matches as usize);
        // The scan itself is unmasked — the CEL filter reads the record the
        // cluster holds, which is the only thing it CAN mean — and the answer
        // is masked on the way out. So `matched` counts what actually matched
        // and the records a caller reads are redacted; the alternative (mask,
        // then filter) would make a rule silently change what a query means.
        let rules = self.mask_set(&profile.id);
        let masked = mask_batch(&rules, &mut matches);
        let mut answer = json!({
            "profile": profile.id,
            "topic": topic,
            "matches": render_records(&matches, verbose),
            "returned": matches.len(),
            "matches_truncated": progress.matched > matches.len() as u64,
            "scanned": progress.scanned,
            "matched": progress.matched,
            "unevaluated": progress.unevaluated,
            "filter_error": progress.filter_error,
            "assumed_complete_partitions": progress.assumed_complete,
            "stopped_because": stopped,
            "limits": {
                "scan_cap": scan_cap,
                "max_matches": max_matches,
                "max_value_bytes": max_value_bytes,
            },
        });
        note_masking(&mut answer, &rules, masked);
        Ok(answer)
    }

    fn sql(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let topic = str_arg(args, "topic")?;
        let query = str_arg(args, "query")?;
        let scan_cap = u32_arg(
            args,
            "scan_cap",
            tools::DEFAULT_SCAN_CAP,
            tools::MAX_SCAN_CAP,
        )?;
        let max_rows = u32_arg(
            args,
            "max_rows",
            tools::DEFAULT_SQL_ROWS,
            tools::MAX_SQL_ROWS,
        )?;
        let deadline = deadline_arg(args)?;

        let spec = SqlSpec {
            topic: topic.clone(),
            query,
            seek: seek_arg(args, scan_cap)?,
            partitions: partitions_arg(args, "partitions")?,
            scan_cap,
            max_rows,
        };

        let conn = self.connection(&profile)?;
        let session = SqlSession::start(&conn, conn.profile().schema_registry.as_ref(), &spec)
            .map_err(|e| e.to_string())?;

        let mut rows = Vec::new();
        let mut stopped = "complete";
        while let Some(batch) = session.next_rows(DRAIN_POLL) {
            rows.extend(batch);
            if rows.len() >= max_rows as usize {
                stopped = "max_rows";
                break;
            }
            if Instant::now() >= deadline {
                stopped = "deadline";
                break;
            }
        }
        session.stop();
        let progress = session.progress();
        let columns = session.columns();
        drop(session);

        if let Some(error) = progress.error {
            return Err(format!(
                "the query failed after {scanned} records: {error}",
                scanned = progress.scanned,
            ));
        }
        rows.truncate(max_rows as usize);
        // A result set is projected columns rather than records, so the pass is
        // by COLUMN NAME — `kavka_core::masking::mask_sql_rows` owns which
        // column is masked as which part of a record, and what happens to a
        // computed one. The same function the desktop app's SQL view runs.
        let rules = self.mask_set(&profile.id);
        let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
        let masked = masking::mask_sql_rows(&rules, &names, &mut rows);
        // `stopped_because` says what ended the READ LOOP; `capped` is the
        // core's own flag and is the one that decides whether the ANSWER is
        // partial — it is also true when the scan stopped at `scan_cap` or a
        // partition went quiet, neither of which this loop can see. They are
        // reported separately rather than folded, because "the query finished"
        // and "the query saw everything" are different claims.
        let mut answer = json!({
            "profile": profile.id,
            "topic": topic,
            "columns": serde_json::to_value(&columns).unwrap_or(Value::Null),
            "rows": rows,
            "row_count": rows.len(),
            "scanned": progress.scanned,
            "capped": progress.capped,
            "stopped_because": stopped,
            "limits": { "scan_cap": scan_cap, "max_rows": max_rows },
        });
        note_masking(&mut answer, &rules, masked);
        Ok(answer)
    }

    fn groups(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let conn = self.connection(&profile)?;
        let groups = admin::groups_list(&conn).map_err(|e| e.to_string())?;
        Ok(json!({
            "profile": profile.id,
            "groups": serde_json::to_value(&groups).unwrap_or(Value::Null),
            "count": groups.len(),
        }))
    }

    fn group_detail(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        let group = str_arg(args, "group")?;
        let conn = self.connection(&profile)?;
        let detail = admin::group_detail(&conn, &group).map_err(|e| e.to_string())?;
        let total_lag: i64 = detail.offsets.iter().filter_map(|offset| offset.lag).sum();
        Ok(json!({
            "profile": profile.id,
            "group_id": detail.group_id,
            "state": detail.state,
            "member_count": detail.members.len(),
            "members": serde_json::to_value(&detail.members).unwrap_or(Value::Null),
            "offsets": serde_json::to_value(&detail.offsets).unwrap_or(Value::Null),
            "total_lag": total_lag,
        }))
    }

    // -----------------------------------------------------------------------
    // Write tools. Both gate before they connect.
    // -----------------------------------------------------------------------

    fn produce(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        gate::authorize_write(self.policy, &profile, "kavka_produce")?;
        let topic = str_arg(args, "topic")?;
        let record: ProduceRecordSpec = serde_json::from_value(args["record"].clone())
            .map_err(|e| format!("`record` is not a record this tool can send: {e}"))?;

        let conn = self.connection(&profile)?;
        let delivery = produce::send(
            &conn,
            conn.profile().schema_registry.as_ref(),
            &topic,
            &record,
        )
        .map_err(|e| e.to_string())?;
        Ok(json!({
            "profile": profile.id,
            "topic": topic,
            "partition": delivery.partition,
            "offset": delivery.offset,
            "tombstone": record.value.is_none(),
        }))
    }

    fn reset_offsets(&mut self, args: &Value) -> Result<Value, String> {
        let profile = self.profile(args)?;
        gate::authorize_write(self.policy, &profile, "kavka_reset_offsets")?;
        let spec: OffsetResetSpec = serde_json::from_value(args["spec"].clone())
            .map_err(|e| format!("`spec` is not a reset this tool can run: {e}"))?;

        let conn = self.connection(&profile)?;
        let offsets = admin::offsets_reset(&conn, &spec).map_err(|e| e.to_string())?;
        Ok(json!({
            "profile": profile.id,
            "group_id": spec.group_id,
            "topic": spec.topic,
            "offsets": serde_json::to_value(&offsets).unwrap_or(Value::Null),
            "partitions_moved": offsets.len(),
        }))
    }

    // -----------------------------------------------------------------------
    // Profiles and connections
    // -----------------------------------------------------------------------

    fn profiles(&self) -> Result<Vec<ConnectionProfile>, String> {
        self.store.list().map_err(|e| {
            format!(
                "couldn't read Kavka's connections from {file}: {e}",
                file = self.profiles_path.display(),
            )
        })
    }

    /// The profile named by the call's `profile` argument.
    ///
    /// Matched by id, then — only when exactly one matches — by display name,
    /// case-insensitively. A model that has seen `"name": "orders-prod"` in a
    /// listing will sometimes send it back as the profile, and answering that
    /// with a lecture costs a round trip for a question we could answer. An
    /// ambiguous name is refused rather than guessed, and every answer echoes
    /// the id it resolved to.
    fn profile(&self, args: &Value) -> Result<ConnectionProfile, String> {
        let wanted = str_arg(args, "profile")?;
        let profiles = self.profiles()?;
        if let Some(found) = profiles.iter().find(|profile| profile.id == wanted) {
            return Ok(found.clone());
        }
        let by_name: Vec<&ConnectionProfile> = profiles
            .iter()
            .filter(|profile| profile.name.eq_ignore_ascii_case(&wanted))
            .collect();
        if let [only] = by_name[..] {
            return Ok(only.clone());
        }
        if profiles.is_empty() {
            return Err(format!(
                "there are no connections saved in {file}. Add one in the Kavka app (it stores \
                 the credentials in this machine's keychain); this server has no way to create \
                 one.",
                file = self.profiles_path.display(),
            ));
        }
        Err(format!(
            "no connection matches {wanted:?}{ambiguity}. Saved connections (id — name): \
             {known}. Call kavka_list_profiles for the full list.",
            ambiguity = if by_name.len() > 1 {
                " — and more than one connection has that name, so it cannot be resolved"
            } else {
                ""
            },
            known = profiles
                .iter()
                .map(|profile| format!("{} — {}", profile.id, profile.name))
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }

    /// The masking rules in force for one profile, compiled.
    ///
    /// **Built per call, on purpose.** The desktop shell compiles a `MaskSet`
    /// once per session because it runs it over hundreds of thousands of
    /// records in a live tail; the volumes here are a tool call's worth — 500
    /// records at the very most — so reading a small JSON file and compiling
    /// two or three regexes costs less than the round trip that asked for them,
    /// and it buys the property a cached set would lose: a rule the person just
    /// switched on in the app applies to the agent's NEXT call, with no restart
    /// and no stale answer.
    ///
    /// A rules file that cannot be read is [`MaskSet::none`] with a line on
    /// stderr — the same shape as every other diagnostic here. That is the one
    /// place this differs from the shell, which stops the session instead:
    /// there, a session emits records continuously and un-masking mid-stream
    /// would be silent; here, every answer that was masked says so, and an
    /// answer that says nothing is one an agent can see said nothing.
    fn mask_set(&self, profile_id: &str) -> MaskSet {
        if !self.masking.honors_rules() {
            return MaskSet::none();
        }
        match self.masks.mask_set(profile_id) {
            Ok((rules, refused)) => {
                for problem in refused {
                    eprintln!("kavka-mcp: {problem}");
                }
                rules
            }
            Err(e) => {
                eprintln!(
                    "kavka-mcp: couldn't read the masking rules for {profile_id}: {e}; this \
                     answer is UNMASKED"
                );
                MaskSet::none()
            }
        }
    }

    /// This profile's connection, opening one on first use.
    ///
    /// The fingerprint is the serialized profile: a connection cached against
    /// an edited document would keep using the old brokers, the old credentials
    /// or — worst — the old `read_only` value, which is the one field a stale
    /// client must never be answered from.
    fn connection(
        &mut self,
        profile: &ConnectionProfile,
    ) -> Result<Arc<ClusterConnection>, String> {
        let fingerprint = serde_json::to_string(profile).map_err(|e| e.to_string())?;
        if let Some((cached, connection)) = self.connections.get(&profile.id) {
            if *cached == fingerprint {
                return Ok(Arc::clone(connection));
            }
        }
        let connection = Arc::new(ClusterConnection::connect(profile.clone()).map_err(|e| {
            format!(
                "couldn't connect to {name} ({servers}): {e}",
                name = profile.name,
                servers = profile.bootstrap_servers.join(", "),
            )
        })?);
        self.connections
            .insert(profile.id.clone(), (fingerprint, Arc::clone(&connection)));
        Ok(connection)
    }
}

/// A JSON-RPC-level fault: code and message.
struct ProtocolError(i64, String);

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// A successful tool result: the structured answer, plus the same JSON as text
/// for clients that read only `content`.
///
/// The text twin is COMPACT rather than pretty. It is a duplicate of
/// `structuredContent` either way, and the reader on the other end is paying
/// for both in context — indentation is the one part of it that carries no
/// information.
fn tool_result(structured: Value) -> Value {
    let text = serde_json::to_string(&structured)
        .unwrap_or_else(|e| format!("{{\"error\":\"could not render the answer: {e}\"}}"));
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": false,
    })
}

/// A tool that ran and failed. Not a JSON-RPC error: the model is meant to read
/// this and try something else.
fn tool_error(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

// ---------------------------------------------------------------------------
// Argument handling
// ---------------------------------------------------------------------------

/// Checks a call against the tool's advertised schema: no unknown top-level
/// arguments, and every required one present.
///
/// Driven by the schema the client was given rather than a second list, so
/// "closed schema" is one fact rather than two that can disagree. Unknown
/// arguments are refused rather than ignored — a model that sent `topic_name`
/// and got an answer about the wrong thing has no way to notice.
fn check_arguments(tool: &Tool, args: &Value) -> Result<(), String> {
    let schema = (tool.schema)();
    let properties = schema["properties"]
        .as_object()
        .expect("every tool schema declares properties");
    if let Some(given) = args.as_object() {
        for key in given.keys() {
            if !properties.contains_key(key) {
                return Err(format!(
                    "{name} has no argument {key:?}. It takes: {accepted}.",
                    name = tool.name,
                    accepted = properties.keys().cloned().collect::<Vec<_>>().join(", "),
                ));
            }
        }
    }
    for required in schema["required"].as_array().into_iter().flatten() {
        let required = required.as_str().unwrap_or_default();
        if args.get(required).is_none_or(Value::is_null) {
            return Err(format!("{name} needs {required:?}.", name = tool.name,));
        }
    }
    Ok(())
}

fn str_arg(args: &Value, key: &str) -> Result<String, String> {
    match &args[key] {
        Value::String(value) if !value.is_empty() => Ok(value.clone()),
        Value::String(_) => Err(format!("{key} is empty")),
        Value::Null => Err(format!("{key} is required")),
        other => Err(format!("{key} must be a string, got {other}")),
    }
}

fn opt_str_arg(args: &Value, key: &str) -> Result<Option<String>, String> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.clone())),
        other => Err(format!("{key} must be a string, got {other}")),
    }
}

fn bool_arg(args: &Value, key: &str, default: bool) -> Result<bool, String> {
    match &args[key] {
        Value::Null => Ok(default),
        Value::Bool(value) => Ok(*value),
        other => Err(format!("{key} must be true or false, got {other}")),
    }
}

/// A bounded count. Over the cap is clamped, not refused — the answer carries
/// `limits`, so a caller always learns what was applied. Zero and negatives are
/// refused, because they are a mistake rather than a preference.
fn u32_arg(args: &Value, key: &str, default: u32, cap: u32) -> Result<u32, String> {
    match &args[key] {
        Value::Null => Ok(default.min(cap)),
        Value::Number(number) => match number.as_u64() {
            Some(0) | None => Err(format!(
                "{key} must be a whole number of at least 1, got {number}"
            )),
            Some(value) => Ok(u32::try_from(value).unwrap_or(cap).min(cap)),
        },
        other => Err(format!("{key} must be a number, got {other}")),
    }
}

fn partitions_arg(args: &Value, key: &str) -> Result<Option<Vec<i32>>, String> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::Array(items) => {
            let mut partitions = Vec::with_capacity(items.len());
            for item in items {
                let partition = item
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| {
                        format!("{key} holds {item}, which is not a partition number")
                    })?;
                partitions.push(partition);
            }
            Ok(Some(partitions))
        }
        other => Err(format!("{key} must be an array of numbers, got {other}")),
    }
}

/// The `seek` argument, defaulting to the newest `default_last_n` records of
/// each partition.
///
/// Missing companion fields are defaulted rather than rejected where there is
/// an obvious answer (`latest` with no `last_n`), and named where there is not
/// (`offset` with no offset), because "kind=offset needs `partition` and
/// `offset`" is a fix and "missing field `offset`" is a puzzle.
fn seek_arg(args: &Value, default_last_n: u32) -> Result<SeekSpec, String> {
    let seek = &args["seek"];
    if seek.is_null() {
        return Ok(SeekSpec::Latest {
            last_n: default_last_n.max(1),
        });
    }
    let kind = seek["kind"].as_str().ok_or_else(|| {
        "seek needs a \"kind\" of earliest, latest, offset or timestamp".to_string()
    })?;
    match kind {
        "earliest" => Ok(SeekSpec::Earliest),
        "latest" => Ok(SeekSpec::Latest {
            last_n: u32_arg(seek, "last_n", default_last_n, u32::MAX)?.max(1),
        }),
        "offset" => {
            let partition = seek["partition"]
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| {
                    "seek kind=offset needs `partition` (which partition the offset is in) and \
                     `offset`"
                        .to_string()
                })?;
            let offset = seek["offset"].as_i64().ok_or_else(|| {
                "seek kind=offset needs `offset` — the first offset to read".to_string()
            })?;
            Ok(SeekSpec::Offset { partition, offset })
        }
        "timestamp" => {
            let timestamp_ms = seek["timestamp_ms"].as_i64().ok_or_else(|| {
                "seek kind=timestamp needs `timestamp_ms` — epoch milliseconds".to_string()
            })?;
            Ok(SeekSpec::Timestamp { timestamp_ms })
        }
        other => Err(format!(
            "seek kind {other:?} is not one of earliest, latest, offset or timestamp"
        )),
    }
}

fn search_query(args: &Value) -> Result<SearchQuery, String> {
    let query = &args["query"];
    if !query.is_object() {
        return Err("query must be an object with a `substring`, a `cel`, or both".into());
    }
    for key in query.as_object().into_iter().flatten().map(|(key, _)| key) {
        if key != "substring" && key != "cel" {
            return Err(format!(
                "query has no field {key:?} — it takes `substring` (raw bytes) and `cel` \
                 (the decoded record)"
            ));
        }
    }
    let substring = opt_str_arg(query, "substring")?.filter(|value| !value.is_empty());
    let cel = opt_str_arg(query, "cel")?.filter(|value| !value.is_empty());
    if substring.is_none() && cel.is_none() {
        return Err(
            "query needs `substring`, `cel`, or both — an empty query would return the whole \
             scan window, which kavka_fetch_messages does better"
                .into(),
        );
    }
    Ok(SearchQuery { substring, cel })
}

fn deadline_arg(args: &Value) -> Result<Instant, String> {
    let timeout = u32_arg(
        args,
        "timeout_ms",
        tools::DEFAULT_TIMEOUT_MS,
        tools::MAX_TIMEOUT_MS,
    )?;
    Ok(Instant::now() + Duration::from_millis(u64::from(timeout)))
}

// ---------------------------------------------------------------------------
// Record shaping
// ---------------------------------------------------------------------------

/// One sentence naming what this process does with a connection's masking
/// rules, for `initialize`'s instructions — the same shape as
/// [`tools::gate_state`], and there for the same reason: a model should learn
/// the rules before its first call, not from its first surprise.
fn mask_state(policy: MaskPolicy) -> String {
    match policy {
        MaskPolicy::Honor => format!(
            "this server HONOURS the display-masking rules saved for a connection in the Kavka \
             app. If a rule rewrites something in an answer, that answer carries a `masking` \
             object saying so — treat those values as redactions, not as data, and never produce \
             them back to a cluster. Answers with no `masking` object were not rewritten. The \
             person running this server can start it with {UNMASKED_ENV}=1 to receive raw \
             payloads; you cannot turn it off from here."
        ),
        MaskPolicy::Off => format!(
            "this server was started with {UNMASKED_ENV}=1, so payloads come back exactly as the \
             cluster holds them even where the Kavka app would redact them on screen. No answer \
             carries a `masking` object."
        ),
    }
}

/// Masks a batch of records in place and answers whether anything changed.
///
/// One boolean per batch when the connection has no enabled rules, which is
/// nearly every call.
fn mask_batch(rules: &MaskSet, records: &mut [MessageRecord]) -> bool {
    if rules.is_empty() {
        return false;
    }
    let mut masked = false;
    for record in records.iter_mut() {
        masked |= rules.mask_record(record);
    }
    masked
}

/// Adds the masked indicator to an answer — **only when something was actually
/// rewritten.**
///
/// That is the whole contract, and it is deliberately not "there are rules on
/// this connection": a rule can be enabled and match nothing in the range being
/// read, and telling a model its data was redacted when it was not is the same
/// class of lie as the reverse. So the presence of the key means "text in this
/// answer is not what is on the topic", the absence of it means "this is what
/// the cluster holds", and there is no third state to reason about.
///
/// It names [`UNMASKED_ENV`] because the reader is a model that has just been
/// handed `•••` and has to be able to tell its user what to do about it —
/// exactly as a write refusal names the variable that would lift it.
fn note_masking(answer: &mut Value, rules: &MaskSet, masked: bool) {
    if !masked {
        return;
    }
    let Some(object) = answer.as_object_mut() else {
        return;
    };
    object.insert(
        "masking".into(),
        json!({
            "applied": true,
            "rules": rules.len(),
            "note": format!(
                "{marker}: {rules} masking {word} configured on this connection in the Kavka \
                 desktop app rewrote text in this answer before it was returned. Those values are \
                 NOT the bytes on the topic — do not quote them as data, and do not produce them \
                 back to a cluster. Kavka masks in its core, so this applies to every tool here. \
                 The person running this server can start it with {UNMASKED_ENV}=1 to turn it off.",
                marker = masking::MASK_NOTICE_MARKER,
                rules = rules.len(),
                word = if rules.len() == 1 { "rule" } else { "rules" },
            ),
        }),
    );
}

fn render_records(records: &[MessageRecord], verbose: bool) -> Value {
    if verbose {
        return serde_json::to_value(records).unwrap_or(Value::Null);
    }
    Value::Array(records.iter().map(compact_record).collect())
}

/// One record, in the shape a model reads best.
///
/// Kavka's own `MessageRecord` carries each payload as
/// `{encoding, text, json, raw_len, truncated, schema}` — and for a JSON
/// payload `text` and `json` are the same content twice, pretty-printed and
/// parsed. That is right for a UI with an inspector and two tabs; here it is
/// double the context for one record. This keeps the value once, as JSON when
/// it decoded to JSON and as text otherwise, and keeps every fact that would
/// otherwise be lost: the encoding, the true byte length, whether the text was
/// cut, the schema, and the difference between a tombstone and an empty value.
/// `verbose: true` returns the full shape for anything this drops.
fn compact_record(record: &MessageRecord) -> Value {
    let mut out = Map::new();
    out.insert("partition".into(), json!(record.partition));
    out.insert("offset".into(), json!(record.offset));
    out.insert("timestamp_ms".into(), json!(record.timestamp_ms));
    out.insert(
        "key".into(),
        record.key.as_ref().map_or(Value::Null, payload_value),
    );
    match &record.value {
        Some(value) => {
            out.insert("value".into(), payload_value(value));
            out.insert(
                "value_encoding".into(),
                serde_json::to_value(value.encoding).unwrap_or(Value::Null),
            );
            out.insert("value_bytes".into(), json!(value.raw_len));
            if value.truncated {
                out.insert("value_truncated".into(), json!(true));
            }
            if let Some(schema) = &value.schema {
                out.insert(
                    "schema".into(),
                    serde_json::to_value(schema).unwrap_or(Value::Null),
                );
            }
        }
        None => {
            // A null value is not an empty one: on a compacted topic it is a
            // deletion, and the difference is the whole reason the field exists.
            out.insert("value".into(), Value::Null);
            out.insert("tombstone".into(), json!(true));
        }
    }
    if !record.headers.is_empty() {
        // An array, not an object: Kafka allows repeated header keys, and a map
        // would silently keep one of them.
        out.insert(
            "headers".into(),
            Value::Array(
                record
                    .headers
                    .iter()
                    .map(|header| json!({ "key": header.key, "value": header.value }))
                    .collect(),
            ),
        );
    }
    if let Some(dlq) = &record.dlq {
        out.insert(
            "dlq".into(),
            serde_json::to_value(dlq).unwrap_or(Value::Null),
        );
    }
    Value::Object(out)
}

/// The payload's content once: the parsed JSON when there is one, the display
/// text otherwise.
fn payload_value(payload: &DecodedPayload) -> Value {
    payload
        .json
        .clone()
        .unwrap_or_else(|| Value::String(payload.text.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server pointed at an empty scratch directory. Enough for everything
    /// that does not need a broker — which is the whole protocol surface.
    fn server(policy: WritePolicy) -> Server {
        server_with(policy, MaskPolicy::Honor)
    }

    fn server_with(policy: WritePolicy, masking: MaskPolicy) -> Server {
        let dir = std::env::temp_dir().join(format!(
            "kavka-mcp-unit-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Server::new(dir, policy, masking)
    }

    fn call(server: &mut Server, line: &str) -> Value {
        let response = server.handle_line(line).expect("a call is owed a response");
        serde_json::from_str(&response).expect("responses are JSON")
    }

    fn tool_call(server: &mut Server, name: &str, arguments: Value) -> Value {
        let line = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        }))
        .unwrap();
        call(server, &line)
    }

    #[test]
    fn initialize_agrees_the_clients_version_when_it_can() {
        let mut server = server(WritePolicy::read_only());
        for version in SUPPORTED_PROTOCOL_VERSIONS {
            let response = call(
                &mut server,
                &format!(
                    r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{version}","capabilities":{{}},"clientInfo":{{"name":"t","version":"1"}}}}}}"#
                ),
            );
            assert_eq!(response["result"]["protocolVersion"], json!(version));
        }
    }

    #[test]
    fn initialize_answers_an_unknown_version_with_the_latest_one_it_speaks() {
        let mut server = server(WritePolicy::read_only());
        let response = call(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        );
        assert_eq!(
            response["result"]["protocolVersion"],
            json!(LATEST_PROTOCOL_VERSION)
        );
        // And with no version at all, which is what a sloppy client sends.
        let response = call(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        assert_eq!(
            response["result"]["protocolVersion"],
            json!(LATEST_PROTOCOL_VERSION)
        );
    }

    #[test]
    fn initialize_declares_tools_only_and_states_the_write_policy() {
        let mut server = server(WritePolicy::read_only());
        let response = call(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        let result = &response["result"];
        assert!(result["capabilities"]["tools"].is_object());
        assert!(result["capabilities"]["resources"].is_null());
        assert!(result["capabilities"]["prompts"].is_null());
        assert_eq!(result["serverInfo"]["name"], "kavka");
        let instructions = result["instructions"].as_str().expect("instructions");
        assert!(instructions.contains(ALLOW_WRITES_ENV), "{instructions}");
        assert!(
            instructions.contains("NO OTHER WRITE SURFACE"),
            "{instructions}"
        );
        assert!(
            instructions.contains("kavka_list_profiles"),
            "{instructions}"
        );
    }

    #[test]
    fn a_notification_is_never_answered() {
        let mut server = server(WritePolicy::read_only());
        assert!(server
            .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none());
        assert!(server
            .handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#
            )
            .is_none());
    }

    #[test]
    fn ping_answers_an_empty_result() {
        let mut server = server(WritePolicy::read_only());
        let response = call(&mut server, r#"{"jsonrpc":"2.0","id":"p","method":"ping"}"#);
        assert_eq!(response["result"], json!({}));
        assert_eq!(response["id"], json!("p"));
    }

    #[test]
    fn tools_list_carries_every_tool_with_a_schema() {
        let mut server = server(WritePolicy::read_only());
        let response = call(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        let listed = response["result"]["tools"].as_array().expect("tools");
        assert_eq!(listed.len(), tools::TOOLS.len());
        assert!(listed
            .iter()
            .all(|tool| tool["inputSchema"]["type"] == "object"));
    }

    #[test]
    fn an_unknown_method_is_a_method_not_found_that_says_what_exists() {
        let mut server = server(WritePolicy::read_only());
        let response = call(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
        );
        assert_eq!(response["error"]["code"], json!(jsonrpc::METHOD_NOT_FOUND));
        let message = response["error"]["message"].as_str().unwrap();
        assert!(message.contains("tools/call"), "{message}");
    }

    #[test]
    fn an_unknown_tool_is_a_protocol_error_listing_the_real_ones() {
        let mut server = server(WritePolicy::read_only());
        let response = tool_call(&mut server, "kavka_delete_topic", json!({}));
        assert_eq!(response["error"]["code"], json!(jsonrpc::INVALID_PARAMS));
        let message = response["error"]["message"].as_str().unwrap();
        assert!(message.contains("kavka_list_topics"), "{message}");
    }

    /// A tool that ran and failed is a *successful* call carrying `isError`.
    /// The distinction is what lets a model read the failure and try again.
    #[test]
    fn a_failed_tool_is_a_result_not_an_error() {
        let mut server = server(WritePolicy::read_only());
        let response = tool_call(
            &mut server,
            "kavka_list_topics",
            json!({ "profile": "nope" }),
        );
        assert!(response["error"].is_null(), "{response}");
        assert_eq!(response["result"]["isError"], json!(true));
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no connections"), "{text}");
    }

    #[test]
    fn an_unknown_argument_is_refused_with_the_list_of_real_ones() {
        let mut server = server(WritePolicy::read_only());
        let response = tool_call(
            &mut server,
            "kavka_list_topics",
            json!({ "profile": "p", "topic_name": "orders" }),
        );
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(response["result"]["isError"], json!(true));
        assert!(text.contains("topic_name"), "{text}");
        assert!(text.contains("include_internal"), "{text}");
    }

    #[test]
    fn a_missing_required_argument_names_it() {
        let mut server = server(WritePolicy::read_only());
        let response = tool_call(&mut server, "kavka_topic_detail", json!({ "profile": "p" }));
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("topic"), "{text}");
    }

    #[test]
    fn list_profiles_reports_the_write_policy_and_the_file_it_read() {
        let mut server = server(WritePolicy::read_only());
        let response = tool_call(&mut server, "kavka_list_profiles", json!({}));
        let structured = &response["result"]["structuredContent"];
        assert_eq!(structured["count"], json!(0));
        assert_eq!(
            structured["write_policy"]["allow_writes_env"],
            json!(ALLOW_WRITES_ENV)
        );
        assert_eq!(structured["write_policy"]["writes_enabled"], json!(false));
        assert!(structured["profiles_file"]
            .as_str()
            .expect("a path")
            .ends_with("profiles.json"));
        // The text twin says the same thing, for clients that read only content.
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("write_policy"), "{text}");
        assert_eq!(response["result"]["isError"], json!(false));
    }

    /// The write gate runs before anything is opened, so a refusal is reachable
    /// with no cluster in sight — and that is exactly how it must behave when
    /// the cluster is unreachable too.
    #[test]
    fn a_write_is_refused_before_any_connection_is_attempted() {
        let mut server = server(WritePolicy::read_only());
        // A profile that exists: written straight into the scratch store. It
        // points at a port nothing is listening on — if the gate ran after the
        // connect rather than before it, this test would sit out the metadata
        // timeout instead of answering.
        let profile: ConnectionProfile = serde_json::from_value(json!({
            "id": "p1",
            "name": "orders",
            "environment": "dev",
            "bootstrap_servers": ["127.0.0.1:1"],
            "auth": { "kind": "plaintext" },
            "read_only": false,
        }))
        .expect("a ConnectionProfile");
        server.store.upsert(profile).expect("scratch store");

        let response = tool_call(
            &mut server,
            "kavka_produce",
            json!({
                "profile": "p1",
                "topic": "orders",
                "record": { "value": { "kind": "text", "text": "hi" } },
            }),
        );
        assert_eq!(response["result"]["isError"], json!(true));
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains(ALLOW_WRITES_ENV), "{text}");
    }

    #[test]
    fn seek_defaults_to_the_newest_records_and_reads_every_kind() {
        assert_eq!(
            seek_arg(&json!({}), 25).unwrap(),
            SeekSpec::Latest { last_n: 25 }
        );
        assert_eq!(
            seek_arg(&json!({"seek": {"kind": "earliest"}}), 25).unwrap(),
            SeekSpec::Earliest
        );
        assert_eq!(
            seek_arg(&json!({"seek": {"kind": "latest", "last_n": 7}}), 25).unwrap(),
            SeekSpec::Latest { last_n: 7 }
        );
        assert_eq!(
            seek_arg(
                &json!({"seek": {"kind": "offset", "partition": 2, "offset": 100}}),
                25
            )
            .unwrap(),
            SeekSpec::Offset {
                partition: 2,
                offset: 100
            }
        );
        assert_eq!(
            seek_arg(
                &json!({"seek": {"kind": "timestamp", "timestamp_ms": 42}}),
                25
            )
            .unwrap(),
            SeekSpec::Timestamp { timestamp_ms: 42 }
        );
    }

    #[test]
    fn a_seek_missing_its_companion_field_says_which_one() {
        let error = seek_arg(&json!({"seek": {"kind": "offset"}}), 25).unwrap_err();
        assert!(error.contains("partition"), "{error}");
        let error = seek_arg(&json!({"seek": {"kind": "timestamp"}}), 25).unwrap_err();
        assert!(error.contains("timestamp_ms"), "{error}");
        let error = seek_arg(&json!({"seek": {"kind": "somewhen"}}), 25).unwrap_err();
        assert!(error.contains("earliest"), "{error}");
        // `latest` is the one kind with an obvious default, so it gets one.
        assert_eq!(
            seek_arg(&json!({"seek": {"kind": "latest"}}), 25).unwrap(),
            SeekSpec::Latest { last_n: 25 }
        );
    }

    #[test]
    fn counts_clamp_at_their_cap_and_refuse_nonsense() {
        assert_eq!(u32_arg(&json!({}), "max", 50, 500).unwrap(), 50);
        assert_eq!(u32_arg(&json!({"max": 10}), "max", 50, 500).unwrap(), 10);
        // Over the cap is an answer, not an error.
        assert_eq!(
            u32_arg(&json!({"max": 99999}), "max", 50, 500).unwrap(),
            500
        );
        assert!(u32_arg(&json!({"max": 0}), "max", 50, 500).is_err());
        assert!(u32_arg(&json!({"max": -3}), "max", 50, 500).is_err());
        assert!(u32_arg(&json!({"max": "ten"}), "max", 50, 500).is_err());
    }

    #[test]
    fn a_search_query_needs_at_least_one_half_and_rejects_invented_fields() {
        assert_eq!(
            search_query(&json!({"query": {"substring": "failed"}})).unwrap(),
            SearchQuery {
                substring: Some("failed".into()),
                cel: None
            }
        );
        assert_eq!(
            search_query(&json!({"query": {"cel": "value.a == 1"}})).unwrap(),
            SearchQuery {
                substring: None,
                cel: Some("value.a == 1".into())
            }
        );
        let error = search_query(&json!({"query": {}})).unwrap_err();
        assert!(error.contains("substring"), "{error}");
        let error = search_query(&json!({"query": {"regex": "x"}})).unwrap_err();
        assert!(error.contains("regex"), "{error}");
        let error = search_query(&json!({"query": "failed"})).unwrap_err();
        assert!(error.contains("object"), "{error}");
    }

    fn payload(text: &str, json: Option<Value>) -> Value {
        json!({
            "encoding": if json.is_some() { "json" } else { "utf8" },
            "text": text,
            "json": json,
            "raw_len": text.len(),
            "truncated": false,
            "schema": Value::Null,
        })
    }

    /// Records are built from JSON rather than as struct literals,
    /// deliberately: `MessageRecord` grows fields between phases (`dlq`, and
    /// `masked` beside it), every one of them is additive and defaulted, and a
    /// test written this way keeps compiling — and keeps testing the same
    /// thing — instead of breaking on a field it never cared about.
    fn record(value: Value) -> MessageRecord {
        serde_json::from_value(value).expect("a MessageRecord")
    }

    #[test]
    fn a_compact_record_keeps_the_value_once_and_every_fact_about_it() {
        let record = record(json!({
            "partition": 3,
            "offset": 8412,
            "timestamp_ms": 1_700_000_000_000_i64,
            "key": payload("order-7", None),
            "value": payload("{\n  \"id\": 7\n}", Some(json!({"id": 7}))),
            "headers": [{ "key": "trace", "value": "abc", "is_text": true }],
        }));
        let compact = compact_record(&record);
        assert_eq!(compact["partition"], json!(3));
        assert_eq!(compact["offset"], json!(8412));
        // JSON arrives as JSON, not as a pretty-printed string.
        assert_eq!(compact["value"], json!({"id": 7}));
        assert_eq!(compact["value_encoding"], json!("json"));
        assert_eq!(compact["key"], json!("order-7"));
        assert_eq!(
            compact["headers"],
            json!([{"key": "trace", "value": "abc"}])
        );
        assert!(compact["tombstone"].is_null());
        assert!(compact.get("value_truncated").is_none(), "nothing was cut");
    }

    #[test]
    fn a_tombstone_is_distinguishable_from_an_empty_value() {
        let tombstone = record(json!({
            "partition": 0,
            "offset": 1,
            "timestamp_ms": Value::Null,
            "key": payload("k", None),
            "value": Value::Null,
            "headers": [],
        }));
        let empty = record(json!({
            "partition": 0,
            "offset": 1,
            "timestamp_ms": Value::Null,
            "key": payload("k", None),
            "value": payload("", None),
            "headers": [],
        }));
        assert_eq!(compact_record(&tombstone)["tombstone"], json!(true));
        assert_eq!(compact_record(&tombstone)["value"], Value::Null);
        assert!(compact_record(&empty)["tombstone"].is_null());
        assert_eq!(compact_record(&empty)["value"], json!(""));
    }

    #[test]
    fn a_truncated_payload_says_so_and_reports_its_true_length() {
        let mut cut = payload("the first 4 KB of it…", None);
        cut["truncated"] = json!(true);
        cut["raw_len"] = json!(900_000);
        let record = record(json!({
            "partition": 0,
            "offset": 1,
            "timestamp_ms": Value::Null,
            "key": Value::Null,
            "value": cut,
            "headers": [],
        }));
        let compact = compact_record(&record);
        assert_eq!(compact["value_truncated"], json!(true));
        assert_eq!(compact["value_bytes"], json!(900_000));
    }

    // ── Masking ────────────────────────────────────────────────────────────

    fn mask_rules(pattern: &str) -> MaskSet {
        let (set, refused) =
            MaskSet::compile(&[kavka_core::masking::MaskRule::new("r1", "cards", pattern)]);
        assert!(refused.is_empty(), "{refused:?}");
        set
    }

    /// The pass itself: every record in the batch, and the flag the core sets.
    #[test]
    fn a_batch_is_masked_record_by_record() {
        let rules = mask_rules(r"\d{4}");
        let mut batch = vec![
            record(json!({
                "partition": 0, "offset": 1, "timestamp_ms": Value::Null,
                "key": payload("card-4111", None),
                "value": payload("nothing here", None),
                "headers": [],
            })),
            record(json!({
                "partition": 0, "offset": 2, "timestamp_ms": Value::Null,
                "key": Value::Null,
                "value": payload("also nothing", None),
                "headers": [],
            })),
        ];
        assert!(mask_batch(&rules, &mut batch));
        assert_eq!(compact_record(&batch[0])["key"], json!("card-•••"));
        assert!(batch[0].masked);
        // A record nothing matched is not flagged, or the indicator means
        // nothing.
        assert!(!batch[1].masked);
        // …and no rules is one boolean per batch and no walk at all.
        assert!(!mask_batch(&MaskSet::none(), &mut batch));
    }

    /// THE INDICATOR TRACKS WHAT CHANGED, not what rules exist. An answer that
    /// says "your data was redacted" about data that was not is the same class
    /// of lie as the reverse — so it is keyed on the pass, and it names the way
    /// out because the reader is a model that has to explain the `•••`.
    #[test]
    fn the_masked_indicator_is_present_only_when_something_was_rewritten() {
        let rules = mask_rules(r"\d{4}");

        let mut answered = json!({ "records": [] });
        note_masking(&mut answered, &rules, true);
        assert_eq!(answered["masking"]["applied"], json!(true));
        assert_eq!(answered["masking"]["rules"], json!(1));
        let note = answered["masking"]["note"].as_str().expect("a sentence");
        assert!(note.contains(UNMASKED_ENV), "{note}");
        assert!(
            note.contains(kavka_core::masking::MASK_NOTICE_MARKER),
            "{note}"
        );

        // Rules on, nothing matched: no indicator.
        let mut untouched = json!({ "records": [] });
        note_masking(&mut untouched, &rules, false);
        assert!(untouched.get("masking").is_none(), "{untouched}");
    }

    /// The opt-out is read at startup and nothing else can reach it: a server
    /// started with it compiles no rules at all, so the pass is not merely
    /// skipped — there is nothing to skip.
    #[test]
    fn the_unmasked_policy_compiles_no_rules_and_the_default_is_to_honour_them() {
        let unmasked = server_with(WritePolicy::read_only(), MaskPolicy::Off);
        assert!(unmasked.mask_set("p1").is_empty());
        // The default server has no rules either — its scratch directory is
        // empty — but it got there by READING the file, which is the difference
        // the integration test drives with a real masking.json.
        let honoured = server(WritePolicy::read_only());
        assert!(honoured.mask_set("p1").is_empty());
        assert!(honoured.masking.honors_rules());
    }

    /// `initialize` states the policy, both ways round, so a model knows
    /// whether what it is reading is the topic's own bytes.
    #[test]
    fn initialize_states_the_masking_policy() {
        let honoured = call(
            &mut server(WritePolicy::read_only()),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        let text = honoured["result"]["instructions"]
            .as_str()
            .expect("instructions");
        assert!(text.contains("HONOURS"), "{text}");
        assert!(text.contains(UNMASKED_ENV), "{text}");

        let raw = call(
            &mut server_with(WritePolicy::read_only(), MaskPolicy::Off),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        let text = raw["result"]["instructions"]
            .as_str()
            .expect("instructions");
        assert!(text.contains("exactly as the cluster holds them"), "{text}");
    }

    /// The verbose escape hatch has to be a superset — otherwise "call it again
    /// with verbose" is not an answer to anything the compact shape dropped.
    #[test]
    fn verbose_returns_kavkas_own_record_shape() {
        let record = record(json!({
            "partition": 1,
            "offset": 2,
            "timestamp_ms": 3,
            "key": Value::Null,
            "value": payload("hi", None),
            "headers": [],
        }));
        let verbose = render_records(std::slice::from_ref(&record), true);
        assert_eq!(verbose[0]["value"]["encoding"], json!("utf8"));
        assert_eq!(verbose[0]["value"]["raw_len"], json!(2));
        assert_eq!(verbose[0]["value"]["text"], json!("hi"));
    }
}
