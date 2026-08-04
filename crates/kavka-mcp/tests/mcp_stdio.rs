//! The MCP server end to end: the real binary, spawned as a child process,
//! driven over real pipes, against the local dev cluster.
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-mcp
//!
//! Nothing here is mocked. The child is `target/<profile>/kavka-mcp`, its
//! `profiles.json` is written by kavka-core's own `ProfileStore` into a scratch
//! directory (so the format is the app's, not this test's idea of it), and every
//! request goes down its stdin as a line of JSON. What that buys over the unit
//! tests is the two things a unit test cannot see: that the binary starts at
//! all, and that the write gate is read from the *process environment* at
//! startup rather than from something a test could reach around.
//!
//! # The fixture
//!
//! Three profiles, because the gate has three answers: `it-mcp` (dev,
//! writable), `it-mcp-ro` (dev, read-only) and `it-mcp-prod` (prod, writable).
//! All three point at the same dev broker — what differs is the document, which
//! is exactly what the gate reads.
//!
//! Reads use `orders`, seeded with 50 keyed JSON messages by the compose file.
//! The one write test produces to **`dead-letter`**, deliberately: `orders` is a
//! counted fixture (`crates/kavka-core/tests/search_local.rs` asserts all 50 of
//! it), and a test that quietly adds a record to another test's fixture is a
//! failure somewhere else next week.

use kavka_core::environments::{EnvironmentDef, EnvironmentStore};
use kavka_core::masking::{MaskRule, MaskStore, MaskTarget};
use kavka_core::profiles::{ConnectionProfile, ProfileStore};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const WRITABLE: &str = "it-mcp";
const READ_ONLY: &str = "it-mcp-ro";
const PROD: &str = "it-mcp-prod";

fn integration() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

/// A scratch config dir holding the three fixture profiles, removed on drop.
struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        Self::with(&[
            (WRITABLE, "dev", false),
            (READ_ONLY, "dev", true),
            (PROD, "prod", false),
        ])
    }

    fn with(profiles: &[(&str, &str, bool)]) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kavka-mcp-it-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        // Written through the app's own store, so the file this server reads is
        // byte-for-byte the file the app writes.
        let store = ProfileStore::new(dir.clone());
        let bootstrap =
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into());
        for (id, environment, read_only) in profiles {
            // Built from JSON rather than as a struct literal: the profile type
            // grows optional fields between phases, and this fixture cares
            // about exactly three of them.
            let profile: ConnectionProfile = serde_json::from_value(json!({
                "id": id,
                "name": format!("{id} cluster"),
                "environment": environment,
                "bootstrap_servers": [bootstrap],
                "auth": { "kind": "plaintext" },
                "read_only": read_only,
            }))
            .expect("a ConnectionProfile");
            store.upsert(profile).expect("writing the fixture profiles");
        }
        Self(dir)
    }

    fn dir(&self) -> &Path {
        &self.0
    }

    /// This machine's environment definitions, through the app's own store —
    /// so the `environments.json` this server reads is the file the app
    /// writes, discovered the same way `profiles.json` is.
    ///
    /// A fixture that does NOT call this has no such file, which is the state
    /// every machine upgrading into this build is in: the shipped `dev`,
    /// `staging` and `prod` apply, and `prod` is protected.
    fn environments(&self, defs: &[EnvironmentDef]) {
        let store = EnvironmentStore::new(self.0.clone());
        for def in defs {
            store
                .save(def.clone())
                .expect("writing the fixture environments");
        }
    }

    /// Writes one enabled masking rule for a profile, through the app's own
    /// store — so the `masking.json` this server reads is byte-for-byte the
    /// file the app writes, exactly as the profiles are.
    fn mask(&self, profile_id: &str, pattern: &str) {
        MaskStore::new(self.0.clone())
            .save_rule(
                profile_id,
                MaskRule {
                    applies_to: MaskTarget::All,
                    ..MaskRule::new("it-mask", "order ids", pattern)
                },
            )
            .expect("writing the fixture masking rule");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The server as a client sees it: a child process and two pipes.
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
}

impl Mcp {
    /// Spawns the built binary. `gate` is the environment the write policy is
    /// read from — passed to the PROCESS, which is the only way to set it.
    fn start(dir: &Path, gate: &[(&str, &str)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kavka-mcp"));
        command
            .env("KAVKA_MCP_CONFIG_DIR", dir)
            // Whatever the developer running the suite has exported must not
            // decide what this test proves.
            .env_remove("KAVKA_MCP_ALLOW_WRITES")
            .env_remove("KAVKA_MCP_ALLOW_PROD")
            .env_remove("KAVKA_MCP_UNMASKED")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Inherited: the server's startup line and any diagnostic land in
            // the test output, where a failure is being read anyway.
            .stderr(Stdio::inherit());
        for (key, value) in gate {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("spawning target/debug/kavka-mcp");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    /// Sends a request and reads the one response it is owed.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        let response = self.read();
        assert_eq!(response["jsonrpc"], "2.0", "{response}");
        assert_eq!(response["id"], json!(id), "answers are matched by id");
        response
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn send(&mut self, message: Value) {
        let line = serde_json::to_string(&message).expect("a request serializes");
        writeln!(self.stdin, "{line}").expect("writing to the server");
        self.stdin.flush().expect("flushing to the server");
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .expect("reading from the server");
        assert!(read > 0, "the server closed its stdout without answering");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON: {line:?} ({e})"))
    }

    /// The MCP handshake, exactly as a client performs it.
    fn handshake(&mut self) -> Value {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "kavka-mcp integration test", "version": "1" },
            }),
        );
        self.notify("notifications/initialized", json!({}));
        response["result"].clone()
    }

    /// A tool call that must succeed, answering with its structured content.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        assert!(
            response["error"].is_null(),
            "{name} faulted at the protocol level: {response}"
        );
        let result = &response["result"];
        assert_eq!(
            result["isError"],
            json!(false),
            "{name} failed: {}",
            result["content"][0]["text"]
        );
        // Every answer carries both halves of the contract.
        assert!(result["structuredContent"].is_object(), "{name}: {result}");
        assert_eq!(result["content"][0]["type"], "text", "{name}");
        result["structuredContent"].clone()
    }

    /// A tool call that must be refused, answering with the refusal text.
    fn refusal(&mut self, name: &str, arguments: Value) -> String {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        assert!(response["error"].is_null(), "{name}: {response}");
        let result = &response["result"];
        assert_eq!(result["isError"], json!(true), "{name} was not refused");
        result["content"][0]["text"]
            .as_str()
            .expect("a refusal is text")
            .to_string()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        // Closing stdin is how a client says goodbye: the server's read loop
        // ends and the process exits on its own. Killing is the backstop.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_handshake_the_catalogue_and_a_browse_over_real_stdio() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();
    let mut mcp = Mcp::start(fixture.dir(), &[]);

    // --- initialize ------------------------------------------------------
    let initialized = mcp.handshake();
    assert_eq!(initialized["protocolVersion"], "2025-06-18");
    assert_eq!(initialized["serverInfo"]["name"], "kavka");
    assert!(initialized["capabilities"]["tools"].is_object());
    let instructions = initialized["instructions"].as_str().expect("instructions");
    assert!(
        instructions.contains("KAVKA_MCP_ALLOW_WRITES"),
        "{instructions}"
    );
    assert!(
        instructions.contains("NO OTHER WRITE SURFACE"),
        "{instructions}"
    );

    // A notification is answered with silence, so the next response must be
    // the next request's — if the server had answered it, every id from here
    // on would be off by one, and `request` asserts the id.
    assert_eq!(mcp.request("ping", json!({}))["result"], json!({}));

    // --- tools/list ------------------------------------------------------
    let listed = mcp.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names.len(), 11, "{names:?}");
    for expected in [
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
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }

    // --- the profiles this server can see --------------------------------
    let profiles = mcp.call("kavka_list_profiles", json!({}));
    assert_eq!(profiles["count"], json!(3));
    assert_eq!(profiles["write_policy"]["writes_enabled"], json!(false));
    for profile in profiles["profiles"].as_array().expect("profiles") {
        assert_eq!(
            profile["writes_allowed"],
            json!(false),
            "no variable was set, so nothing may write: {profile}"
        );
    }

    // --- the cluster -----------------------------------------------------
    let overview = mcp.call("kavka_cluster_overview", json!({ "profile": WRITABLE }));
    assert_eq!(
        overview["broker_count"],
        json!(1),
        "single-node dev cluster"
    );
    assert!(overview["cluster_id"].is_string(), "KRaft cluster id");

    let topics = mcp.call("kavka_list_topics", json!({ "profile": WRITABLE }));
    let names: Vec<&str> = topics["topics"]
        .as_array()
        .expect("topics")
        .iter()
        .map(|topic| topic["name"].as_str().expect("name"))
        .collect();
    for expected in [
        "orders",
        "payments",
        "customers",
        "inventory",
        "dead-letter",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
    assert!(
        !names.iter().any(|name| name.starts_with("__")),
        "internal topics are hidden by default: {names:?}"
    );
    assert_eq!(topics["truncated"], json!(false));

    // The filter and the internal switch are the two arguments that change
    // what comes back rather than how much.
    let filtered = mcp.call(
        "kavka_list_topics",
        json!({ "profile": WRITABLE, "name_contains": "ORDER" }),
    );
    assert_eq!(filtered["topics"][0]["name"], "orders");
    assert_eq!(filtered["returned"], json!(1));

    // --- one topic -------------------------------------------------------
    let detail = mcp.call(
        "kavka_topic_detail",
        json!({ "profile": WRITABLE, "topic": "orders" }),
    );
    assert_eq!(detail["partition_count"], json!(6));
    assert!(
        detail["approx_message_count"].as_i64().unwrap_or(0) >= 50,
        "the compose file seeds 50: {detail}"
    );
    assert_eq!(detail["under_replicated_partitions"], json!([]));

    // --- the records themselves -------------------------------------------
    let fetched = mcp.call(
        "kavka_fetch_messages",
        json!({ "profile": WRITABLE, "topic": "orders", "max": 5 }),
    );
    let records = fetched["records"].as_array().expect("records");
    assert_eq!(records.len(), 5, "{fetched}");
    assert_eq!(fetched["limits"]["max"], json!(5));
    for record in records {
        assert!(record["partition"].is_number(), "{record}");
        assert!(record["offset"].is_number(), "{record}");
        // The seeded key is `order-N` and the value is a JSON object — which is
        // the whole point of the compact shape: it arrives as JSON, not as a
        // string holding pretty-printed JSON.
        assert!(
            record["key"]
                .as_str()
                .unwrap_or_default()
                .starts_with("order-"),
            "{record}"
        );
        assert!(record["value"]["orderId"].is_number(), "{record}");
        assert_eq!(record["value_encoding"], "json", "{record}");
        assert!(record["tombstone"].is_null(), "{record}");
    }

    // Reading from the earliest offset of one partition is the seek a caller
    // uses to walk a topic, and the one most likely to be mis-parsed.
    let from_start = mcp.call(
        "kavka_fetch_messages",
        json!({
            "profile": WRITABLE,
            "topic": "orders",
            "seek": { "kind": "offset", "partition": 0, "offset": 0 },
            "partitions": [0],
            "max": 3,
        }),
    );
    for record in from_start["records"].as_array().expect("records") {
        assert_eq!(record["partition"], json!(0), "{record}");
    }

    // --- search -----------------------------------------------------------
    let found = mcp.call(
        "kavka_search",
        json!({
            "profile": WRITABLE,
            "topic": "orders",
            "query": { "substring": "created" },
            "seek": { "kind": "earliest" },
            "max_matches": 10,
            "timeout_ms": 20_000,
        }),
    );
    // The list is capped at 10 — and the scan ran on regardless, which is why
    // `matched` can say 50. That gap is the feature.
    assert_eq!(found["returned"], json!(10), "capped by max_matches");
    assert_eq!(
        found["matched"],
        json!(50),
        "every seeded record says created"
    );
    assert_eq!(found["matches_truncated"], json!(true));
    assert_eq!(found["scanned"], json!(50), "{found}");
    assert_eq!(found["stopped_because"], "complete");

    // A CEL filter over the decoded value — the half a substring cannot do.
    let expensive = mcp.call(
        "kavka_search",
        json!({
            "profile": WRITABLE,
            "topic": "orders",
            "query": { "cel": "value.orderId > 45" },
            "seek": { "kind": "earliest" },
            "timeout_ms": 20_000,
        }),
    );
    assert_eq!(
        expensive["matched"],
        json!(5),
        "orderId 46..50: {expensive}"
    );
    assert_eq!(expensive["returned"], json!(5));
    assert_eq!(expensive["matches_truncated"], json!(false));
    assert_eq!(expensive["stopped_because"], "complete");
    assert_eq!(expensive["unevaluated"], json!(0));

    // --- SQL --------------------------------------------------------------
    let counted = mcp.call(
        "kavka_sql",
        json!({
            "profile": WRITABLE,
            "topic": "orders",
            "query": "select count(*) as n from messages",
            "seek": { "kind": "earliest" },
            "timeout_ms": 20_000,
        }),
    );
    assert_eq!(counted["columns"][0]["name"], "n");
    assert_eq!(counted["rows"][0][0], json!(50), "{counted}");
    assert_eq!(counted["capped"], json!(false));

    // --- groups -----------------------------------------------------------
    let groups = mcp.call("kavka_groups", json!({ "profile": WRITABLE }));
    assert!(groups["groups"].is_array(), "{groups}");

    // --- and the write, refused, on the wire ------------------------------
    let refusal = mcp.refusal(
        "kavka_produce",
        json!({
            "profile": WRITABLE,
            "topic": "dead-letter",
            "record": { "value": { "kind": "text", "text": "should never be sent" } },
        }),
    );
    assert!(refusal.contains("KAVKA_MCP_ALLOW_WRITES"), "{refusal}");
}

/// The gate, through the process boundary: the same three refusals the unit
/// matrix asserts, plus the one that must actually go through.
#[test]
fn the_write_gate_over_real_stdio() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();

    // Writes on, prod off — the configuration the app's snippet suggests for
    // anyone who wants to produce.
    let mut mcp = Mcp::start(fixture.dir(), &[("KAVKA_MCP_ALLOW_WRITES", "1")]);
    mcp.handshake();

    let policy = mcp.call("kavka_list_profiles", json!({}));
    assert_eq!(policy["write_policy"]["writes_enabled"], json!(true));
    assert_eq!(policy["write_policy"]["prod_allowed"], json!(false));

    let record =
        json!({ "value": { "kind": "json", "json": { "from": "kavka-mcp integration" } } });

    // read-only: refused, and told no variable lifts it.
    let refusal = mcp.refusal(
        "kavka_produce",
        json!({ "profile": READ_ONLY, "topic": "dead-letter", "record": record }),
    );
    assert!(refusal.contains("read-only"), "{refusal}");
    assert!(refusal.contains("does not lift it"), "{refusal}");

    // prod: refused, and told which variable is missing.
    let refusal = mcp.refusal(
        "kavka_produce",
        json!({ "profile": PROD, "topic": "dead-letter", "record": record }),
    );
    assert!(refusal.contains("KAVKA_MCP_ALLOW_PROD"), "{refusal}");

    // The other write tool is gated by the same three checks, so it refuses the
    // same two profiles — a gate wired into one tool and not the other is
    // exactly what this asserts against.
    let spec = json!({
        "group_id": "kavka-mcp-it",
        "topic": "dead-letter",
        "target": { "kind": "earliest" },
    });
    let refusal = mcp.refusal(
        "kavka_reset_offsets",
        json!({ "profile": READ_ONLY, "spec": spec }),
    );
    assert!(refusal.contains("read-only"), "{refusal}");
    let refusal = mcp.refusal(
        "kavka_reset_offsets",
        json!({ "profile": PROD, "spec": spec }),
    );
    assert!(refusal.contains("KAVKA_MCP_ALLOW_PROD"), "{refusal}");

    // …and the dev connection writes. `dead-letter` rather than `orders`: see
    // the module docs.
    let delivered = mcp.call(
        "kavka_produce",
        json!({ "profile": WRITABLE, "topic": "dead-letter", "record": record }),
    );
    assert!(delivered["partition"].is_number(), "{delivered}");
    assert!(
        delivered["offset"].as_i64().unwrap_or(-1) >= 0,
        "the broker reported where it landed: {delivered}"
    );
    assert_eq!(delivered["tombstone"], json!(false));

    // The record is readable back, which is what "delivered" has to mean.
    let back = mcp.call(
        "kavka_search",
        json!({
            "profile": WRITABLE,
            "topic": "dead-letter",
            "query": { "substring": "kavka-mcp integration" },
            "seek": { "kind": "earliest" },
            "timeout_ms": 20_000,
        }),
    );
    assert!(
        back["matched"].as_u64().unwrap_or(0) >= 1,
        "the produced record came back: {back}"
    );
}

/// Prod stays refused until BOTH variables are set — and then goes through.
/// Separate server, because the policy is read once at startup, which is the
/// property being asserted.
#[test]
fn prod_needs_both_variables_and_read_only_still_wins() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();
    let mut mcp = Mcp::start(
        fixture.dir(),
        &[
            ("KAVKA_MCP_ALLOW_WRITES", "1"),
            ("KAVKA_MCP_ALLOW_PROD", "1"),
        ],
    );
    mcp.handshake();

    let policy = mcp.call("kavka_list_profiles", json!({}));
    assert_eq!(policy["write_policy"]["prod_allowed"], json!(true));
    let by_id = |id: &str| -> Value {
        policy["profiles"]
            .as_array()
            .expect("profiles")
            .iter()
            .find(|profile| profile["id"] == id)
            .cloned()
            .expect("the fixture profile")
    };
    // The prod connection is now writable; the read-only one is not, and says
    // why in the listing rather than only on refusal.
    assert_eq!(by_id(PROD)["writes_allowed"], json!(true));
    assert_eq!(by_id(READ_ONLY)["writes_allowed"], json!(false));
    assert!(by_id(READ_ONLY)["writes_refused_because"]
        .as_str()
        .expect("a reason")
        .contains("read-only"));

    // And the refusal still happens at call time, not only in the listing.
    let refusal = mcp.refusal(
        "kavka_produce",
        json!({
            "profile": READ_ONLY,
            "topic": "dead-letter",
            "record": { "value": { "kind": "text", "text": "never" } },
        }),
    );
    assert!(refusal.contains("read-only"), "{refusal}");
}

/// THE MASKING POLICY, over the wire.
///
/// A rule saved for a connection in the app's own `masking.json` has to reach
/// an agent reading that connection through this server, in every tool that
/// returns a record — and the answer has to say it was rewritten, because a
/// model that quotes `•••` as a value is a model that was not told.
///
/// The rule matches the seeded key `order-N`, which is on every one of the 50
/// records the compose file writes to `orders`, so "did masking run" and "did
/// the read work" cannot be confused for each other.
#[test]
fn masking_rules_are_honoured_over_real_stdio() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();
    fixture.mask(WRITABLE, r"order-\d+");

    let mut mcp = Mcp::start(fixture.dir(), &[]);
    let instructions = mcp.handshake()["instructions"]
        .as_str()
        .expect("instructions")
        .to_string();
    assert!(instructions.contains("HONOURS"), "{instructions}");
    assert!(
        instructions.contains("KAVKA_MCP_UNMASKED"),
        "{instructions}"
    );

    // --- fetch -------------------------------------------------------------
    let fetched = mcp.call(
        "kavka_fetch_messages",
        json!({ "profile": WRITABLE, "topic": "orders", "max": 5 }),
    );
    let records = fetched["records"].as_array().expect("records");
    assert_eq!(records.len(), 5, "{fetched}");
    for record in records {
        assert_eq!(
            record["key"],
            json!("•••"),
            "the key came back raw: {record}"
        );
    }
    assert_eq!(fetched["masking"]["applied"], json!(true), "{fetched}");
    assert_eq!(fetched["masking"]["rules"], json!(1));
    let note = fetched["masking"]["note"].as_str().expect("a sentence");
    assert!(note.contains("KAVKA_MCP_UNMASKED"), "{note}");
    // The text twin carries it too, for a client that reads only `content`.
    assert!(
        !serde_json::to_string(&fetched).unwrap().contains("order-"),
        "an unmasked key survived somewhere in the answer: {fetched}"
    );

    // --- search: the SCAN is unmasked, the ANSWER is masked ----------------
    let found = mcp.call(
        "kavka_search",
        json!({
            "profile": WRITABLE,
            "topic": "orders",
            "query": { "substring": "created" },
            "seek": { "kind": "earliest" },
            "max_matches": 5,
            "timeout_ms": 20_000,
        }),
    );
    assert_eq!(found["matched"], json!(50), "the scan saw the real records");
    for record in found["matches"].as_array().expect("matches") {
        assert_eq!(record["key"], json!("•••"), "{record}");
    }
    assert_eq!(found["masking"]["applied"], json!(true), "{found}");

    // --- SQL: masked by column ---------------------------------------------
    let queried = mcp.call(
        "kavka_sql",
        json!({
            "profile": WRITABLE,
            "topic": "orders",
            "query": "select key_text from messages order by key_text limit 5",
            "seek": { "kind": "earliest" },
            "timeout_ms": 20_000,
        }),
    );
    for row in queried["rows"].as_array().expect("rows") {
        assert_eq!(row[0], json!("•••"), "{queried}");
    }
    assert_eq!(queried["masking"]["applied"], json!(true), "{queried}");

    // A connection with no rules of its own is untouched, and says nothing —
    // the rules are per profile, exactly as they are in the app.
    let other = mcp.call(
        "kavka_fetch_messages",
        json!({ "profile": PROD, "topic": "orders", "max": 3 }),
    );
    assert!(other.get("masking").is_none(), "{other}");
    assert!(
        other["records"][0]["key"]
            .as_str()
            .unwrap_or_default()
            .starts_with("order-"),
        "{other}"
    );
}

/// The opt-out, and the proof that a write is not touched by any of it.
///
/// A separate process, because the policy is read once at startup — which is
/// the property being asserted as much as the raw payloads are.
#[test]
fn the_unmasked_opt_out_returns_raw_payloads_and_writes_are_unaffected() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();
    fixture.mask(WRITABLE, r"order-\d+");

    // --- the write, through a server that IS masking -----------------------
    //
    // Masking is a read-side transform: the record that reaches the broker must
    // be the bytes the caller sent, or this server would corrupt a topic while
    // redacting nothing.
    let mut masking = Mcp::start(fixture.dir(), &[("KAVKA_MCP_ALLOW_WRITES", "1")]);
    masking.handshake();
    let marker = format!("order-9{}", std::process::id());
    let delivered = masking.call(
        "kavka_produce",
        json!({
            "profile": WRITABLE,
            "topic": "dead-letter",
            "record": { "key": marker, "value": { "kind": "text", "text": marker } },
        }),
    );
    assert!(
        delivered["offset"].as_i64().unwrap_or(-1) >= 0,
        "{delivered}"
    );
    assert!(
        delivered.get("masking").is_none(),
        "a write answer is not a record: {delivered}"
    );
    drop(masking);

    // --- the same fixture, read raw ----------------------------------------
    let mut raw = Mcp::start(fixture.dir(), &[("KAVKA_MCP_UNMASKED", "1")]);
    let instructions = raw.handshake()["instructions"]
        .as_str()
        .expect("instructions")
        .to_string();
    assert!(
        instructions.contains("exactly as the cluster holds them"),
        "{instructions}"
    );

    let fetched = raw.call(
        "kavka_fetch_messages",
        json!({ "profile": WRITABLE, "topic": "orders", "max": 5 }),
    );
    for record in fetched["records"].as_array().expect("records") {
        assert!(
            record["key"]
                .as_str()
                .unwrap_or_default()
                .starts_with("order-"),
            "the rule was applied despite the opt-out: {record}"
        );
    }
    assert!(
        fetched.get("masking").is_none(),
        "an unmasked answer must carry no indicator: {fetched}"
    );

    // …and the produced record is on the topic in full, which is the half a
    // masked read could not have told us.
    let back = raw.call(
        "kavka_search",
        json!({
            "profile": WRITABLE,
            "topic": "dead-letter",
            "query": { "substring": &marker },
            "seek": { "kind": "earliest" },
            "timeout_ms": 20_000,
        }),
    );
    assert!(
        back["matched"].as_u64().unwrap_or(0) >= 1,
        "the broker holds the real bytes, not the replacement: {back}"
    );
}

/// A malformed line must not desynchronize the stream: the server answers it
/// and keeps serving. Needs no cluster, so it runs on every `cargo test`.
#[test]
fn a_broken_line_is_answered_and_the_session_survives() {
    let fixture = Fixture::new();
    let mut mcp = Mcp::start(fixture.dir(), &[]);
    mcp.handshake();

    mcp.send(json!("this is not a request"));
    let response = mcp.read();
    assert_eq!(response["error"]["code"], json!(-32600));
    assert_eq!(
        response["id"],
        Value::Null,
        "a fault answers with a null id"
    );

    // Not JSON at all, sent as a raw line.
    writeln!(mcp.stdin, "{{not json").expect("writing to the server");
    mcp.stdin.flush().expect("flush");
    let response = mcp.read();
    assert_eq!(response["error"]["code"], json!(-32700));

    // Still serving, and still matching ids.
    assert_eq!(mcp.request("ping", json!({}))["result"], json!({}));
    let listed = mcp.request("tools/list", json!({}));
    assert_eq!(
        listed["result"]["tools"].as_array().expect("tools").len(),
        11
    );
}

/// The server reads `profiles.json` from the directory it was pointed at, and
/// an empty one is an answer rather than a crash. No cluster needed.
#[test]
fn an_empty_config_directory_answers_with_no_connections() {
    let dir = std::env::temp_dir().join(format!("kavka-mcp-it-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let mut mcp = Mcp::start(&dir, &[]);
    mcp.handshake();

    let profiles = mcp.call("kavka_list_profiles", json!({}));
    assert_eq!(profiles["count"], json!(0));
    assert!(profiles["profiles_file"]
        .as_str()
        .expect("a path")
        .ends_with("profiles.json"));

    // And a tool that needs one says where it looked.
    let refusal = mcp.refusal("kavka_list_topics", json!({ "profile": "anything" }));
    assert!(refusal.contains("profiles.json"), "{refusal}");
    assert!(refusal.contains("Kavka app"), "{refusal}");

    drop(mcp);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The custom-environment guardrail, through the real config discovery.**
///
/// An enterprise runs more than three environments, so this fixture's
/// connections are on `Production` and `UAT` — names this build has never heard
/// of — and the definitions come from an `environments.json` written by
/// kavka-core's own store into the same directory as `profiles.json`. The
/// server has to find it the way it finds the connections, or the MCP surface
/// and the app disagree about which clusters an agent may write to.
///
/// `KAVKA_MCP_ALLOW_WRITES=1` without `KAVKA_MCP_ALLOW_PROD=1` is the
/// configuration the whole assertion turns on: `Production` must refuse and
/// `UAT` must not, and nothing about the words decides it.
///
/// The third connection is the state importing a colleague's export leaves you
/// in: it names `Legacy`, which this machine defines nothing for. That is not a
/// failure — it resolves neutral and unprotected — but it is not the same
/// answer as `UAT`'s unprotected, and the listing has to let a model tell them
/// apart.
///
/// No broker: the gate runs before any connection is attempted, which is the
/// whole point of checking it first.
#[test]
fn a_protected_custom_environment_needs_the_prod_variable() {
    let fixture = Fixture::with(&[
        ("ent-prod", "Production", false),
        ("ent-uat", "UAT", false),
        ("ent-legacy", "Legacy", false),
    ]);
    fixture.environments(&[
        // Violet, not red: colour is identity, `protected` is the guardrail.
        EnvironmentDef::new("Production", "violet", true),
        EnvironmentDef::new("UAT", "blue", false),
    ]);
    let mut mcp = Mcp::start(fixture.dir(), &[("KAVKA_MCP_ALLOW_WRITES", "1")]);
    mcp.handshake();

    let listed = mcp.call("kavka_list_profiles", json!({}));
    let by_id = |id: &str| -> Value {
        listed["profiles"]
            .as_array()
            .expect("profiles")
            .iter()
            .find(|profile| profile["id"] == id)
            .cloned()
            .expect("the fixture profile")
    };
    // The listing carries the flag beside the name, because the name stopped
    // answering "is this production" — this is the field a model reads.
    assert_eq!(by_id("ent-prod")["environment"], json!("Production"));
    assert_eq!(by_id("ent-prod")["environment_protected"], json!(true));
    assert_eq!(by_id("ent-prod")["writes_allowed"], json!(false));
    assert_eq!(by_id("ent-uat")["environment_protected"], json!(false));
    assert_eq!(by_id("ent-uat")["writes_allowed"], json!(true));

    // Both defined environments say so, and neither carries a hint: there is
    // nothing to tell the model about an environment its owner defined.
    assert_eq!(by_id("ent-prod")["environment_known"], json!(true));
    assert_eq!(by_id("ent-uat")["environment_known"], json!(true));
    assert_eq!(by_id("ent-uat")["environment_hint"], json!(null));

    // `Legacy` is the third answer, and it is NOT "UAT again". Unprotected
    // because nothing defines it, not because somebody decided it was safe —
    // so the flag that separates the two is on the row, with the sentence that
    // says what to do about it.
    let legacy = by_id("ent-legacy");
    assert_eq!(legacy["environment"], json!("Legacy"));
    assert_eq!(legacy["environment_known"], json!(false));
    assert_eq!(legacy["environment_protected"], json!(false));
    let hint = legacy["environment_hint"]
        .as_str()
        .expect("an unknown environment carries its hint");
    assert!(hint.contains("Legacy"), "{hint}");
    // Writes are allowed — the gate reads `protected`, and nothing marked it.
    assert_eq!(legacy["writes_allowed"], json!(true));

    // And the refusal happens at call time too, naming the environment its
    // owner invented and the variable that lifts it.
    let refusal = mcp.refusal(
        "kavka_produce",
        json!({
            "profile": "ent-prod",
            "topic": "dead-letter",
            "record": { "value": { "kind": "text", "text": "never" } },
        }),
    );
    assert!(refusal.contains("Production"), "{refusal}");
    assert!(refusal.contains("KAVKA_MCP_ALLOW_PROD"), "{refusal}");
}
