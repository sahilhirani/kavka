//! The tool catalogue: names, JSON Schemas, and descriptions written for the
//! thing that reads them — a model choosing what to call next.
//!
//! Three rules hold across every entry here, and the tests at the bottom of the
//! file enforce all three:
//!
//! - **Every description states its unit, its cap and what its errors mean.**
//!   "max: how many records" is useless; "how many records to return, 1–500,
//!   default 50 — the newest first" is a decision a model can make.
//! - **Every schema is closed** (`additionalProperties: false`). A
//!   misremembered argument name comes back as "unknown argument `topic_name`;
//!   this tool takes `topic`" instead of being silently ignored, which is the
//!   difference between one wasted call and a confidently wrong answer.
//! - **Caps clamp, they do not fail.** A request for 10,000 records answers
//!   with 500 and says so in `limits`, because a bounded answer is useful and
//!   an error is not.
//!
//! The caps themselves are the ones the IPC contract fixes: 500 records a
//! fetch, 50,000 records a scan, 200 search matches, 500 SQL rows.

use crate::gate::{WritePolicy, ALLOW_PROD_ENV, ALLOW_WRITES_ENV};
use serde_json::{json, Value};

/// Records one [`kavka_fetch_messages`](TOOLS) call may return.
pub const MAX_FETCH: u32 = 500;
pub const DEFAULT_FETCH: u32 = 50;

/// Records one search or SQL scan may read off the topic.
pub const MAX_SCAN_CAP: u32 = 50_000;
pub const DEFAULT_SCAN_CAP: u32 = 20_000;

/// Matches one search may return. It keeps counting past this — the count is
/// in the answer, which is the difference between "200 matches" and "the first
/// 200 of 4,812".
pub const MAX_SEARCH_MATCHES: u32 = 200;
pub const DEFAULT_SEARCH_MATCHES: u32 = 50;

/// Rows one SQL answer may hold.
pub const MAX_SQL_ROWS: u32 = 500;
pub const DEFAULT_SQL_ROWS: u32 = 100;

/// Bytes of decoded payload text carried per key and per value.
///
/// Far below the app's own 256 KB display cap, and deliberately: the reader
/// here has a context window, and one 256 KB record can cost more of it than
/// the whole rest of an investigation. The record still reports its true byte
/// length and whether it was cut.
pub const DEFAULT_VALUE_BYTES: u32 = 4_096;
pub const MAX_VALUE_BYTES: u32 = 65_536;

/// Wall-clock ceiling on one search or one query, so a tool call cannot outlive
/// the patience of whatever is waiting on the other end of the pipe.
pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;
pub const MAX_TIMEOUT_MS: u32 = 120_000;

/// Topics one listing may name. A cluster with 20,000 topics is a real thing,
/// and all of them in one answer is not an answer.
pub const MAX_TOPIC_LIST: u32 = 5_000;
pub const DEFAULT_TOPIC_LIST: u32 = 500;

/// Whether a tool can change anything on a cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// One entry in the catalogue.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub access: Access,
    /// MCP's `destructiveHint`: whether a call can undo work that already
    /// happened. Producing a record adds one; resetting offsets moves a
    /// running application's position, which is the one thing here that can
    /// make a consumer reprocess (or skip) messages.
    pub destructive: bool,
    pub description: &'static str,
    /// A function rather than a string so the schema is built with `json!` and
    /// checked by the tests as a `Value`, not as text.
    pub schema: fn() -> Value,
}

/// Every tool, in the order a first-time caller should meet them: what
/// connections exist, then what is in one, then what is in a topic, then the
/// two that write.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "kavka_list_profiles",
        title: "List Kafka connections",
        access: Access::Read,
        destructive: false,
        description: "\
Lists the Kafka connections saved in the Kavka desktop app on this machine — id, display name, \
environment tag (dev/staging/prod), bootstrap servers, and whether the connection is read-only. \
START HERE: every other tool takes the `profile` id from this list. Nothing is contacted; this \
reads the app's profiles.json only, so it works even when no broker is reachable. The answer also \
reports this server's write policy and, per connection, whether a write would currently be \
allowed and why not if it would not.",
        schema: schema_list_profiles,
    },
    Tool {
        name: "kavka_cluster_overview",
        title: "Cluster overview",
        access: Access::Read,
        destructive: false,
        description: "\
Connects to one cluster and reports its identity and size: cluster id, every broker (id, host, \
port), the number of non-internal topics and the total number of partitions across them. Use it \
to confirm a connection works before a longer investigation — an authentication or network \
failure surfaces here, in one round trip, rather than halfway through a scan. Errors name the \
host and the cause (DNS, refused, TLS, rejected credentials).",
        schema: schema_profile_only,
    },
    Tool {
        name: "kavka_list_topics",
        title: "List topics",
        access: Access::Read,
        destructive: false,
        description: "\
Lists the cluster's topics with their partition count and replication factor. Kafka's own \
internal topics (names starting `__`, e.g. `__consumer_offsets`) are excluded unless \
`include_internal` is true. Filter with `name_contains` (case-insensitive substring) before \
raising `limit`: the listing is capped and reports `total` and `truncated` so a partial answer is \
never mistaken for the whole cluster. Partition counts are metadata, not sizes — for offsets and \
an approximate message count, call kavka_topic_detail.",
        schema: schema_list_topics,
    },
    Tool {
        name: "kavka_topic_detail",
        title: "Topic detail",
        access: Access::Read,
        destructive: false,
        description: "\
One topic in full: every partition with its leader, replicas, in-sync replicas (ISR) and its \
earliest/latest offsets, plus the topic's configuration. `approx_message_count` sums \
(latest − earliest) across partitions — it is an upper bound, not a count: compaction, retention \
deletes and transaction markers all consume offsets that hold no readable record. \
`under_replicated_partitions` is the field to look at first when something is wrong. Only \
non-default configuration entries are returned unless `include_default_configs` is true (a broker \
returns roughly 30, most of them defaults nobody set).",
        schema: schema_topic_detail,
    },
    Tool {
        name: "kavka_fetch_messages",
        title: "Fetch messages",
        access: Access::Read,
        destructive: false,
        description: "\
Reads a bounded window of records from a topic and returns them decoded, sorted by (timestamp, \
partition, offset). `seek` decides where the read starts and defaults to the newest records. \
Payloads are decoded through Kavka's ladder — Schema Registry framing (Avro), JSON, UTF-8 text, \
MessagePack, CBOR, hex — and a value that decoded to JSON arrives as JSON, not as a quoted \
string. Key and value text are truncated to `max_value_bytes` (default 4096) to protect your \
context; `value_truncated` and `value_bytes` report the truth about what was cut. A record with \
no value is a tombstone and says so. This never joins a consumer group and never commits an \
offset, so browsing cannot disturb a running application.",
        schema: schema_fetch_messages,
    },
    Tool {
        name: "kavka_search",
        title: "Search a topic",
        access: Access::Read,
        destructive: false,
        description: "\
Scans a topic in parallel and returns the records that match. Two filters, ANDed when both are \
given: `substring` (case-insensitive, matched against the RAW key, value and header bytes before \
anything is decoded — fast, and the right choice when you do not know the payload format), and \
`cel` (a CEL expression over the DECODED record). The CEL activation binds: key, value \
(a map for JSON payloads, a string otherwise), key_text, value_text (always a string, always the \
same text this tool returns), headers (map), partition, offset, timestamp_ms. Example: \
`value.status == \"failed\" && value.amount > 100`. Note `string(value)` is an ERROR on a JSON \
payload — use value_text. THE COUNTS ARE THE POINT: the answer holds at most `max_matches` \
records, but THE SCAN DOES NOT STOP THERE — it runs on to `scan_cap` and `matched` keeps \
counting, so \"the first 50 of 4,812 matches in 20,000 records scanned\" is expressible and a \
truncated answer is never silent (`matches_truncated` says so outright). `stopped_because` says \
what ended the scan: complete, scan_cap or deadline. `unevaluated` counts records the CEL \
expression could not judge (a field it asked for was missing) — those are not matches and not \
failures.",
        schema: schema_search,
    },
    Tool {
        name: "kavka_sql",
        title: "SQL over a topic",
        access: Access::Read,
        destructive: false,
        description: "\
Runs read-only SQL over a bounded scan window of one topic. There is exactly one table, \
`messages`, and the topic is the one in `topic` — not named in FROM. Columns: partition BIGINT, \
\"offset\" BIGINT (quote it — OFFSET is a keyword), timestamp_ms BIGINT (epoch ms, null before \
KIP-32), key_text VARCHAR, value_text VARCHAR, value_json VARCHAR (COMPACT JSON text when the \
payload decoded to JSON, else null), headers_json VARCHAR. SELECT/WHERE/GROUP BY/HAVING/ORDER \
BY/LIMIT, CTEs, sub-queries, UNION, window functions and the usual string, regex, time and math \
functions are available; CREATE/INSERT/UPDATE/DELETE/COPY/SET are refused before anything runs. \
THERE ARE NO JSON FUNCTIONS in this build — match value_json as text, e.g. \
`WHERE value_json LIKE '%\"status\":\"failed\"%'` (it is compact, so there is no space after `:` \
or `,`). An aggregate over a capped scan is a confidently wrong number, so `capped` is reported \
and is true whenever the scan hit `scan_cap`, the answer hit `max_rows`, or a partition went \
quiet before its end offset — READ IT BEFORE QUOTING ANY TOTAL. `stopped_because` is the separate, \
smaller fact of what ended the read: complete, max_rows or deadline.",
        schema: schema_sql,
    },
    Tool {
        name: "kavka_groups",
        title: "List consumer groups",
        access: Access::Read,
        destructive: false,
        description: "\
Lists the cluster's consumer groups with their state (Stable, Empty, PreparingRebalance, Dead, …), \
protocol type and member count. A group in `Empty` has committed offsets but nothing running; a \
group that has only ever produced never appears at all. For lag, members and per-partition \
offsets, call kavka_group_detail.",
        schema: schema_profile_only,
    },
    Tool {
        name: "kavka_group_detail",
        title: "Consumer group detail",
        access: Access::Read,
        destructive: false,
        description: "\
One consumer group in full: its state, its live members (member id, client id, host, and the \
partitions each is assigned) and, per topic-partition, the committed offset, the partition's end \
offset and the lag between them. `total_lag` sums the lag across every partition. Lag is a \
number of records, not a duration — a lag of 40,000 on a topic doing 10 records a second is \
different from the same number on one doing 10,000. A committed offset of null means the group \
has no position on that partition yet.",
        schema: schema_group_detail,
    },
    Tool {
        name: "kavka_produce",
        title: "Produce one record",
        access: Access::Write,
        destructive: false,
        description: "\
WRITES TO KAFKA. Sends one record to a topic and returns the partition and offset the broker \
assigned it. The value may be plain text, JSON, or Avro encoded against the latest schema \
registered for a subject in the connection's Schema Registry; omitting `value` entirely produces \
a TOMBSTONE (a null payload), which permanently deletes the key's history on a compacted topic — \
that is a different act from sending an empty string. Delivery is confirmed by the full in-sync \
replica set (acks=all), so a returned offset is durable. Omit `partition` unless you mean it: \
Kafka partitions by key hash when there is a key and round-robin when there is not.",
        schema: schema_produce,
    },
    Tool {
        name: "kavka_reset_offsets",
        title: "Reset a consumer group's offsets",
        access: Access::Write,
        destructive: true,
        description: "\
WRITES TO KAFKA, AND CHANGES WHAT A RUNNING APPLICATION WILL DO. Moves a consumer group's \
committed offsets for one topic and returns the offsets after the move. Targets: `earliest` \
(reprocess everything retained — every side effect the application has per message happens \
again), `latest` (skip everything unread), `offset`, `timestamp_ms` (the first record at or after \
that time; the end of the partition when there is none), `shift_by` (relative to the current \
committed offset, or to `earliest` when the group has never committed). Every resolved offset is \
clamped into [earliest, end] so the group cannot land out of range and have auto.offset.reset \
silently decide for it. Kafka REJECTS a reset on a group with live members: stop the consumers \
first. `force` skips Kavka's own check of that, not Kafka's — it buys a clearer broker error, not \
a different outcome. Call kavka_group_detail first and state the blast radius (how many records \
will be reprocessed) before calling this.",
        schema: schema_reset_offsets,
    },
];

/// The `tools/list` payload, with the write tools' current gate state appended
/// to their descriptions.
///
/// The state is in the description rather than only in the refusal because a
/// model that can read "currently DISABLED" before it calls does not have to
/// spend a call learning it — and because a model that reads "currently
/// ENABLED for non-prod connections" is being told, at the moment it matters,
/// that the next call is real.
pub fn catalogue(policy: WritePolicy) -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .map(|tool| {
            let mut description = tool.description.to_string();
            if tool.access == Access::Write {
                description.push_str("\n\n");
                description.push_str(&gate_state(policy));
            }
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": description,
                "inputSchema": (tool.schema)(),
                "annotations": {
                    "title": tool.title,
                    "readOnlyHint": tool.access == Access::Read,
                    "destructiveHint": tool.destructive,
                    "idempotentHint": false,
                    // Every one of these reaches a Kafka cluster, whose state
                    // nothing here controls.
                    "openWorldHint": tool.access == Access::Read,
                },
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// One sentence naming what this process was started with. Appended to both
/// write tools, and quoted in the server's `instructions`.
pub fn gate_state(policy: WritePolicy) -> String {
    match (policy.writes_enabled, policy.prod_allowed) {
        (false, _) => format!(
            "WRITE GATE — currently DISABLED: this server was started without \
             {ALLOW_WRITES_ENV}=1, so this tool refuses every call. The variable is read once, at \
             startup; set it beside the command in your MCP client's config and restart the \
             server."
        ),
        (true, false) => format!(
            "WRITE GATE — currently ENABLED for non-production connections ({ALLOW_WRITES_ENV}=1). \
             Connections tagged `prod` still refuse, because {ALLOW_PROD_ENV}=1 was not set. \
             Connections marked read-only refuse regardless."
        ),
        (true, true) => format!(
            "WRITE GATE — currently ENABLED, INCLUDING PRODUCTION ({ALLOW_WRITES_ENV}=1 and \
             {ALLOW_PROD_ENV}=1). Connections marked read-only still refuse — that flag is a \
             property of the connection and no variable lifts it."
        ),
    }
}

/// The tool by that name, or `None`.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

// ---------------------------------------------------------------------------
// Schemas. One function per tool, plus the fragments they share.
// ---------------------------------------------------------------------------

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn profile_property() -> Value {
    json!({
        "type": "string",
        "description": "The connection's `id` from kavka_list_profiles — the id, not the display \
                        name.",
    })
}

fn topic_property() -> Value {
    json!({ "type": "string", "description": "Topic name, exactly as kavka_list_topics reports it." })
}

fn partitions_property() -> Value {
    json!({
        "type": "array",
        "items": { "type": "integer" },
        "description": "Partition numbers to read. Omit for every partition of the topic.",
    })
}

/// The seek shape, shared by fetch, search and SQL. Mirrors kavka-core's
/// `SeekSpec` exactly, so the parsed object is handed to the core unchanged.
fn seek_property(default_note: &str) -> Value {
    json!({
        "type": "object",
        "description": format!(
            "Where the read starts. {default_note} `kind` chooses; the other fields belong to one \
             kind each."
        ),
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["earliest", "latest", "offset", "timestamp"],
                "description": "earliest: from the oldest retained record. latest: the newest \
                                `last_n` records of EACH partition. offset: one partition from one \
                                offset. timestamp: every partition from the first record at or \
                                after a time.",
            },
            "last_n": {
                "type": "integer",
                "minimum": 1,
                "description": "kind=latest only: how many records back to start, PER PARTITION.",
            },
            "partition": {
                "type": "integer",
                "description": "kind=offset only: which partition the offset belongs to.",
            },
            "offset": {
                "type": "integer",
                "description": "kind=offset only: the first offset to read.",
            },
            "timestamp_ms": {
                "type": "integer",
                "description": "kind=timestamp only: epoch milliseconds (UTC).",
            },
        },
        "required": ["kind"],
        "additionalProperties": false,
    })
}

fn max_value_bytes_property() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": MAX_VALUE_BYTES,
        "description": format!(
            "Bytes of decoded text kept per key and per value, {DEFAULT_VALUE_BYTES} by default, \
             {MAX_VALUE_BYTES} at most. Records report their true byte length and whether they \
             were cut."
        ),
    })
}

fn verbose_property() -> Value {
    json!({
        "type": "boolean",
        "description": "false (the default) returns the compact record shape described above. \
                        true returns Kavka's full internal record — every payload as \
                        {encoding, text, json, raw_len, truncated, schema} — which is roughly \
                        twice the size and rarely worth it.",
    })
}

fn timeout_property() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": MAX_TIMEOUT_MS,
        "description": format!(
            "Wall-clock ceiling in milliseconds, {DEFAULT_TIMEOUT_MS} by default, {MAX_TIMEOUT_MS} \
             at most. Hitting it is not an error: the partial answer comes back with \
             stopped_because = \"deadline\" and the counts so far."
        ),
    })
}

fn schema_list_profiles() -> Value {
    object(json!({}), &[])
}

fn schema_profile_only() -> Value {
    object(json!({ "profile": profile_property() }), &["profile"])
}

fn schema_list_topics() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "name_contains": {
                "type": "string",
                "description": "Case-insensitive substring filter on the topic name. Applied \
                                locally, after the cluster's full list arrives.",
            },
            "include_internal": {
                "type": "boolean",
                "description": "Include Kafka's internal topics (names starting `__`). Default \
                                false.",
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_TOPIC_LIST,
                "description": format!(
                    "How many topics to name, {DEFAULT_TOPIC_LIST} by default, {MAX_TOPIC_LIST} at \
                     most. `total` and `truncated` always report the whole picture."
                ),
            },
        }),
        &["profile"],
    )
}

fn schema_topic_detail() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "topic": topic_property(),
            "include_default_configs": {
                "type": "boolean",
                "description": "Include configuration entries the broker reports as defaults. \
                                Default false — only entries somebody set are returned, and the \
                                number omitted is reported.",
            },
        }),
        &["profile", "topic"],
    )
}

fn schema_fetch_messages() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "topic": topic_property(),
            "seek": seek_property(
                "Defaults to the newest `max` records of each partition, of which the newest \
                 `max` overall are returned.",
            ),
            "partitions": partitions_property(),
            "max": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_FETCH,
                "description": format!(
                    "How many records to return, {DEFAULT_FETCH} by default, {MAX_FETCH} at most. \
                     A larger number is clamped, not refused, and `limits.max` in the answer says \
                     what was applied."
                ),
            },
            "max_value_bytes": max_value_bytes_property(),
            "verbose": verbose_property(),
        }),
        &["profile", "topic"],
    )
}

fn schema_search() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "topic": topic_property(),
            "query": {
                "type": "object",
                "description": "What makes a record a match. Give at least one of the two; both \
                                is an AND, with the cheap raw-byte half running first.",
                "properties": {
                    "substring": {
                        "type": "string",
                        "description": "Case-insensitive (ASCII) substring, matched against the \
                                        raw key, value and header BYTES before any decoding.",
                    },
                    "cel": {
                        "type": "string",
                        "description": "CEL expression over the decoded record. Bindings: key, \
                                        value, key_text, value_text, headers, partition, offset, \
                                        timestamp_ms. Example: value.status == \"failed\".",
                    },
                },
                "additionalProperties": false,
            },
            "seek": seek_property(
                "Defaults to the newest `scan_cap` records of each partition; pass \
                 {\"kind\":\"earliest\"} to sweep from the start of retention instead.",
            ),
            "partitions": partitions_property(),
            "scan_cap": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_SCAN_CAP,
                "description": format!(
                    "Records the scan may read before it stops, {DEFAULT_SCAN_CAP} by default, \
                     {MAX_SCAN_CAP} at most. Stopping here is reported as \
                     stopped_because = \"scan_cap\", never as \"no matches\"."
                ),
            },
            "max_matches": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_SEARCH_MATCHES,
                "description": format!(
                    "Matching records to RETURN, {DEFAULT_SEARCH_MATCHES} by default, \
                     {MAX_SEARCH_MATCHES} at most. It does not stop the scan: `matched` counts \
                     every match in the window and `matches_truncated` says whether the list is \
                     the first N of them."
                ),
            },
            "max_value_bytes": max_value_bytes_property(),
            "timeout_ms": timeout_property(),
            "verbose": verbose_property(),
        }),
        &["profile", "topic", "query"],
    )
}

fn schema_sql() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "topic": topic_property(),
            "query": {
                "type": "string",
                "description": "The SQL. One table, `messages`; the topic is the `topic` argument. \
                                Parsed and planned before any broker call, so a syntax error costs \
                                nothing.",
            },
            "seek": seek_property(
                "Defaults to the newest `scan_cap` records of each partition; pass \
                 {\"kind\":\"earliest\"} to query from the start of retention instead.",
            ),
            "partitions": partitions_property(),
            "scan_cap": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_SCAN_CAP,
                "description": format!(
                    "Records the scan may read into memory for the query, {DEFAULT_SCAN_CAP} by \
                     default, {MAX_SCAN_CAP} at most. Hitting it sets `capped`, which makes every \
                     aggregate in the answer a partial one."
                ),
            },
            "max_rows": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_SQL_ROWS,
                "description": format!(
                    "Rows the answer may hold, {DEFAULT_SQL_ROWS} by default, {MAX_SQL_ROWS} at \
                     most. Truncation here also sets `capped`."
                ),
            },
            "timeout_ms": timeout_property(),
        }),
        &["profile", "topic", "query"],
    )
}

fn schema_group_detail() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "group": {
                "type": "string",
                "description": "Consumer group id, exactly as kavka_groups reports it.",
            },
        }),
        &["profile", "group"],
    )
}

fn schema_produce() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "topic": topic_property(),
            "record": {
                "type": "object",
                "description": "The record to send.",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Record key, sent as UTF-8. Omit for a keyless record. The \
                                        key is what decides the partition when `partition` is not \
                                        given.",
                    },
                    "value": {
                        "type": "object",
                        "description": "The payload. OMIT THE WHOLE FIELD to produce a tombstone \
                                        (a null value).",
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": ["text", "json", "avro"],
                                "description": "text: send `text` as UTF-8. json: serialize `json` \
                                                compactly. avro: encode `json` against the LATEST \
                                                schema registered for `subject` and frame it the \
                                                Confluent way (magic byte + schema id).",
                            },
                            "text": { "type": "string", "description": "kind=text only." },
                            "json": {
                                "description": "kind=json and kind=avro: any JSON value.",
                            },
                            "subject": {
                                "type": "string",
                                "description": "kind=avro only: the Schema Registry subject, e.g. \
                                                `orders-value`. The connection must have a \
                                                registry configured.",
                            },
                        },
                        "required": ["kind"],
                        "additionalProperties": false,
                    },
                    "headers": {
                        "type": "array",
                        "description": "Record headers, in order. Values are sent as UTF-8.",
                        "items": {
                            "type": "object",
                            "description": "One header.",
                            "properties": {
                                "key": { "type": "string", "description": "Header name." },
                                "value": {
                                    "type": "string",
                                    "description": "Header value, sent as UTF-8.",
                                },
                            },
                            "required": ["key", "value"],
                            "additionalProperties": false,
                        },
                    },
                    "partition": {
                        "type": "integer",
                        "description": "Force a partition. Omit unless you mean it — Kafka's own \
                                        partitioner keeps a key's records in order on one \
                                        partition, and pinning breaks that guarantee for other \
                                        producers of the same key.",
                    },
                },
                "additionalProperties": false,
            },
        }),
        &["profile", "topic", "record"],
    )
}

fn schema_reset_offsets() -> Value {
    object(
        json!({
            "profile": profile_property(),
            "spec": {
                "type": "object",
                "description": "Which group, which topic, and where its offsets should land.",
                "properties": {
                    "group_id": {
                        "type": "string",
                        "description": "The consumer group to move.",
                    },
                    "topic": topic_property(),
                    "partitions": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Partitions to move. Omit for every partition of the topic.",
                    },
                    "target": {
                        "type": "object",
                        "description": "Where the offsets land, before clamping into \
                                        [earliest, end].",
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": ["earliest", "latest", "offset", "timestamp_ms", "shift_by"],
                                "description": "earliest/latest: the partition's watermarks. \
                                                offset: an absolute offset. timestamp_ms: the \
                                                first record at or after a time. shift_by: \
                                                relative to the current committed offset.",
                            },
                            "offset": {
                                "type": "integer",
                                "description": "kind=offset only.",
                            },
                            "timestamp_ms": {
                                "type": "integer",
                                "description": "kind=timestamp_ms only: epoch milliseconds (UTC).",
                            },
                            "shift_by": {
                                "type": "integer",
                                "description": "kind=shift_by only: signed record count. Negative \
                                                replays, positive skips.",
                            },
                        },
                        "required": ["kind"],
                        "additionalProperties": false,
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Skip Kavka's own active-group check. Kafka still refuses a \
                                        reset while members are running, so this only changes \
                                        which error you get. Default false.",
                    },
                },
                "required": ["group_id", "topic", "target"],
                "additionalProperties": false,
            },
        }),
        &["profile", "spec"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schemas() -> impl Iterator<Item = (&'static str, Value)> {
        TOOLS.iter().map(|tool| (tool.name, (tool.schema)()))
    }

    #[test]
    fn the_catalogue_is_the_eleven_tools_the_contract_fixes() {
        let names: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
        assert_eq!(
            names,
            vec![
                "kavka_list_profiles",
                "kavka_cluster_overview",
                "kavka_list_topics",
                "kavka_topic_detail",
                "kavka_fetch_messages",
                "kavka_search",
                "kavka_sql",
                "kavka_groups",
                "kavka_group_detail",
                "kavka_produce",
                "kavka_reset_offsets",
            ]
        );
        assert_eq!(
            TOOLS
                .iter()
                .filter(|tool| tool.access == Access::Write)
                .map(|tool| tool.name)
                .collect::<Vec<_>>(),
            vec!["kavka_produce", "kavka_reset_offsets"],
            "no third write tool without a decision about its blast radius"
        );
    }

    #[test]
    fn every_name_is_unique_and_namespaced() {
        let mut seen = std::collections::BTreeSet::new();
        for tool in TOOLS {
            assert!(tool.name.starts_with("kavka_"), "{}", tool.name);
            assert!(seen.insert(tool.name), "duplicate tool {}", tool.name);
            assert!(!tool.title.is_empty(), "{}", tool.name);
        }
    }

    /// Every schema is a closed object whose `required` names real properties,
    /// and every property carries a description. A model reading an
    /// undocumented property guesses, and a guess costs a round trip.
    #[test]
    fn every_schema_is_a_closed_documented_object() {
        for (name, schema) in schemas() {
            assert_eq!(schema["type"], "object", "{name}");
            assert_eq!(schema["additionalProperties"], false, "{name}");
            let properties = schema["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} has no properties object"));
            for (property, spec) in properties {
                assert!(
                    spec["description"].is_string(),
                    "{name}.{property} has no description"
                );
            }
            for required in schema["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{name} has no required array"))
            {
                let required = required.as_str().expect("required names are strings");
                assert!(
                    properties.contains_key(required),
                    "{name} requires {required}, which it does not declare"
                );
            }
        }
    }

    /// Nested objects are held to the same standard — a half-documented
    /// `record` or `spec` is where a write call goes wrong.
    #[test]
    fn nested_objects_are_closed_and_documented_too() {
        fn walk(name: &str, path: &str, value: &Value) {
            let Some(object) = value.as_object() else {
                return;
            };
            if object.get("type").and_then(Value::as_str) == Some("object") {
                assert_eq!(
                    object.get("additionalProperties"),
                    Some(&json!(false)),
                    "{name}{path} is an open object"
                );
                if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                    for (key, child) in properties {
                        assert!(
                            child["description"].is_string() || child["items"].is_object(),
                            "{name}{path}.{key} has no description"
                        );
                        walk(name, &format!("{path}.{key}"), child);
                    }
                }
            }
            if let Some(items) = object.get("items") {
                walk(name, &format!("{path}[]"), items);
            }
        }
        for (name, schema) in schemas() {
            walk(name, "", &schema);
        }
    }

    /// The caps are contractual, so they are asserted where a caller reads
    /// them: in the text, not only in the constants.
    #[test]
    fn every_bounded_tool_states_its_cap_in_its_description() {
        let text = |name: &str| find(name).expect(name).description;
        assert!(text("kavka_fetch_messages").contains("max_value_bytes"));
        assert!(text("kavka_search").contains("scan_cap"));
        assert!(text("kavka_search").contains("max_matches"));
        assert!(text("kavka_sql").contains("max_rows"));
        assert!(text("kavka_list_topics").contains("truncated"));
        // And the schema carries the number itself, so a client that validates
        // rejects an over-cap call before it is sent.
        let fetch = schema_fetch_messages();
        assert_eq!(fetch["properties"]["max"]["maximum"], json!(MAX_FETCH));
        let search = schema_search();
        assert_eq!(
            search["properties"]["scan_cap"]["maximum"],
            json!(MAX_SCAN_CAP)
        );
        assert_eq!(
            search["properties"]["max_matches"]["maximum"],
            json!(MAX_SEARCH_MATCHES)
        );
        let sql = schema_sql();
        assert_eq!(
            sql["properties"]["scan_cap"]["maximum"],
            json!(MAX_SCAN_CAP)
        );
        assert_eq!(
            sql["properties"]["max_rows"]["maximum"],
            json!(MAX_SQL_ROWS)
        );
    }

    /// The contract's numbers, asserted as numbers: 500 records a fetch,
    /// 50,000 a scan, 200 matches, 500 rows.
    #[test]
    fn the_caps_are_the_ones_the_contract_fixes() {
        assert_eq!(MAX_FETCH, 500);
        assert_eq!(MAX_SCAN_CAP, 50_000);
        assert_eq!(MAX_SEARCH_MATCHES, 200);
        assert_eq!(MAX_SQL_ROWS, 500);
        // Defaults sit under their ceilings, or the default would be the cap.
        // Const blocks, so a bad edit is a build failure rather than a test
        // failure — the numbers are known at compile time and nothing here has
        // to run to know they are wrong.
        const {
            assert!(DEFAULT_FETCH < MAX_FETCH);
            assert!(DEFAULT_SCAN_CAP < MAX_SCAN_CAP);
            assert!(DEFAULT_SEARCH_MATCHES < MAX_SEARCH_MATCHES);
            assert!(DEFAULT_SQL_ROWS < MAX_SQL_ROWS);
            assert!(DEFAULT_VALUE_BYTES < MAX_VALUE_BYTES);
            assert!(DEFAULT_TIMEOUT_MS < MAX_TIMEOUT_MS);
        }
    }

    #[test]
    fn a_write_tool_announces_the_gate_and_names_both_variables() {
        for policy in [
            WritePolicy::read_only(),
            WritePolicy {
                writes_enabled: true,
                prod_allowed: false,
            },
            WritePolicy {
                writes_enabled: true,
                prod_allowed: true,
            },
        ] {
            let listed = catalogue(policy);
            let tools = listed["tools"].as_array().expect("tools array");
            assert_eq!(tools.len(), TOOLS.len());
            for tool in tools {
                let name = tool["name"].as_str().expect("name");
                let description = tool["description"].as_str().expect("description");
                let is_write = matches!(name, "kavka_produce" | "kavka_reset_offsets");
                assert_eq!(
                    tool["annotations"]["readOnlyHint"],
                    json!(!is_write),
                    "{name}"
                );
                assert_eq!(description.contains("WRITE GATE"), is_write, "{name}");
                if is_write {
                    assert!(description.contains(ALLOW_WRITES_ENV), "{name}");
                    assert!(description.contains("WRITES TO KAFKA"), "{name}");
                }
                assert!(tool["inputSchema"].is_object(), "{name}");
                assert!(tool["title"].is_string(), "{name}");
            }
        }
        // The prod variable appears whenever it is the thing standing in the
        // way, and when it is the thing that was lifted.
        assert!(gate_state(WritePolicy {
            writes_enabled: true,
            prod_allowed: false
        })
        .contains(ALLOW_PROD_ENV));
        assert!(gate_state(WritePolicy {
            writes_enabled: true,
            prod_allowed: true
        })
        .contains("INCLUDING PRODUCTION"));
        assert!(gate_state(WritePolicy::read_only()).contains("DISABLED"));
    }

    #[test]
    fn the_destructive_hint_is_set_only_on_the_one_that_moves_a_consumer() {
        assert!(!find("kavka_produce").unwrap().destructive);
        assert!(find("kavka_reset_offsets").unwrap().destructive);
    }

    #[test]
    fn lookups_answer_by_name() {
        assert_eq!(find("kavka_sql").map(|tool| tool.name), Some("kavka_sql"));
        assert!(
            find("kavka_delete_topic").is_none(),
            "no such write surface"
        );
        assert!(find("").is_none());
    }

    /// The seek fragment is the one shape three tools share, and the one a
    /// model gets wrong by inventing a field name.
    #[test]
    fn the_seek_fragment_names_every_kind_and_its_fields() {
        let seek = seek_property("Defaults to the newest records.");
        let kinds = seek["properties"]["kind"]["enum"].clone();
        assert_eq!(kinds, json!(["earliest", "latest", "offset", "timestamp"]));
        for field in ["last_n", "partition", "offset", "timestamp_ms"] {
            assert!(
                seek["properties"][field]["description"].is_string(),
                "{field}"
            );
        }
        assert_eq!(seek["required"], json!(["kind"]));
        assert_eq!(seek["additionalProperties"], json!(false));
    }
}
