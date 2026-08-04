//! The CLI end to end: the real binary, spawned as a child process, against the
//! local dev cluster.
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-cli
//!
//! Nothing here is mocked. The child is `target/<profile>/kavka`, its
//! `profiles.json` is written by kavka-core's own `ProfileStore` into a scratch
//! directory (so the format is the app's, not this test's idea of it), and every
//! command is a real argv. What that buys over the unit tests is the three
//! things a unit test cannot see: that the binary starts at all, that stdout
//! carries only the answer, and that the exit code a script would branch on is
//! the documented one.
//!
//! # The fixture
//!
//! Three profiles, because the gate has three answers: `it-cli` (dev,
//! writable), `it-cli-ro` (dev, read-only) and `it-cli-prod` (prod, writable).
//! All three point at the same dev broker — what differs is the document, which
//! is exactly what the gate reads.
//!
//! Reads use `orders`, seeded with 50 keyed JSON messages by the compose file.
//! Writes go to **`dead-letter`**, deliberately: `orders` is a counted fixture
//! (`crates/kavka-core/tests/search_local.rs` asserts all 50 of it), and a test
//! that quietly adds a record to another test's fixture is a failure somewhere
//! else next week.

use kavka_core::environments::{EnvironmentDef, EnvironmentStore};
use kavka_core::masking::{MaskRule, MaskStore, MaskTarget};
use kavka_core::profiles::{ConnectionProfile, ProfileStore};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const WRITABLE: &str = "it-cli";
const READ_ONLY: &str = "it-cli-ro";
const PROD: &str = "it-cli-prod";

fn integration() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

/// A scratch config dir holding the fixture profiles, removed on drop.
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
            "kavka-cli-it-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch config dir");
        // Written through the app's own store, so the file this program reads
        // is byte-for-byte the file the app writes.
        let store = ProfileStore::new(dir.clone());
        let bootstrap =
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into());
        for (id, environment, read_only) in profiles {
            // Built from JSON rather than as a struct literal: the profile type
            // gains optional fields between phases, and this fixture cares
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

    /// This machine's environment definitions, through the app's own store —
    /// so the `environments.json` this program reads is the file the app
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

    /// One enabled masking rule, through the app's own store — so the
    /// `masking.json` this program reads is the file the app writes.
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

    /// Runs `kavka` with these arguments against this fixture.
    fn run(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_kavka"))
            .env("KAVKA_CONFIG_DIR", &self.0)
            // Whatever the developer running the suite has exported must not
            // decide what these assertions prove.
            .env_remove("KAVKA_PROFILE")
            .args(args)
            .output()
            .expect("spawning target/debug/kavka");
        Run {
            code: output.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&output.stdout).into_owned(),
            err: String::from_utf8_lossy(&output.stderr).into_owned(),
            args: args.join(" "),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One completed command.
struct Run {
    code: i32,
    out: String,
    err: String,
    args: String,
}

impl Run {
    /// Must have answered. Returns stdout — owned, so a whole command can be
    /// asserted on in one expression.
    fn ok(&self) -> String {
        assert_eq!(
            self.code,
            0,
            "`kavka {args}` exited {code}\nstderr: {err}",
            args = self.args,
            code = self.code,
            err = self.err,
        );
        self.out.clone()
    }

    /// Stdout as NDJSON — one object per line, which is what a pipe gets by
    /// default. Also asserts the pipe carries nothing else.
    fn rows(&self) -> Vec<Value> {
        self.ok();
        self.out
            .lines()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("stdout line is not JSON: {line:?} ({e})"))
            })
            .collect()
    }

    /// Stdout as one JSON document (`--output json`).
    fn document(&self) -> Value {
        self.ok();
        serde_json::from_str(&self.out).expect("stdout is one JSON document")
    }

    /// Must have failed with this exact code, and must have written nothing to
    /// stdout while doing it. Returns stderr.
    fn refused(&self, code: i32) -> String {
        assert_eq!(
            self.code,
            code,
            "`kavka {args}` exited {actual}, expected {code}\nstdout: {out}\nstderr: {err}",
            args = self.args,
            actual = self.code,
            out = self.out,
            err = self.err,
        );
        assert!(
            self.out.is_empty(),
            "a failure wrote to stdout: {out}",
            out = self.out
        );
        self.err.clone()
    }
}

// ---------------------------------------------------------------------------
// No cluster needed
// ---------------------------------------------------------------------------

/// The exit codes a script branches on, and the promise that stdout stays clean
/// when something goes wrong. None of this needs a broker.
#[test]
fn the_documented_exit_codes() {
    let fixture = Fixture::new();

    // 4 — no such connection, and the message lists the ones there are.
    let refusal = fixture.run(&["-p", "nope", "topics", "list"]).refused(4);
    assert!(refusal.contains("it-cli"), "{refusal}");
    assert!(refusal.contains("profiles list"), "{refusal}");

    // 2 — the command line was wrong. clap's own code, for two seeks at once…
    let refusal = fixture
        .run(&[
            "-p",
            WRITABLE,
            "fetch",
            "orders",
            "--earliest",
            "--last",
            "5",
        ])
        .refused(2);
    assert!(refusal.contains("--last"), "{refusal}");
    // …and this program's, for a combination clap cannot express.
    let refusal = fixture
        .run(&["-p", WRITABLE, "fetch", "orders", "--offset", "10"])
        .refused(2);
    assert!(refusal.contains("--partition"), "{refusal}");
    // …and for a produce with no payload at all.
    let refusal = fixture
        .run(&["-p", WRITABLE, "produce", "dead-letter"])
        .refused(2);
    assert!(refusal.contains("--tombstone"), "{refusal}");

    // 3 — refused by a guardrail, before any connection is opened. A read-only
    // connection needs no broker to say no, which is the point of gating first.
    let refusal = fixture
        .run(&[
            "-p",
            READ_ONLY,
            "produce",
            "dead-letter",
            "--value",
            "never",
        ])
        .refused(3);
    assert!(refusal.contains("read-only"), "{refusal}");
    assert!(refusal.contains("does not lift it"), "{refusal}");

    // 2 — several connections and none named. Kavka does not guess which
    // cluster, which is the accident docs/DESIGN.md §6 is written against.
    let refusal = fixture.run(&["topics", "list"]).refused(2);
    assert!(refusal.contains("--profile"), "{refusal}");
    assert!(refusal.contains("KAVKA_PROFILE"), "{refusal}");
}

/// `profiles list` contacts nothing, so it answers with every broker down —
/// which is what makes it the command the help tells people to start with.
#[test]
fn profiles_are_listed_without_a_cluster_in_every_output_mode() {
    let fixture = Fixture::new();

    // A pipe gets NDJSON with no flag at all. This is the whole detection
    // contract: the test harness captures stdout, so stdout is not a terminal.
    let rows = fixture.run(&["profiles", "list"]).rows();
    assert_eq!(rows.len(), 3, "{rows:?}");
    let ids: Vec<&str> = rows
        .iter()
        .map(|row| row["id"].as_str().expect("an id"))
        .collect();
    for expected in [WRITABLE, READ_ONLY, PROD] {
        assert!(ids.contains(&expected), "{expected} missing from {ids:?}");
    }
    let read_only = rows
        .iter()
        .find(|row| row["id"] == READ_ONLY)
        .expect("the read-only fixture");
    assert_eq!(read_only["read_only"], json!(true));
    // No secret ever reaches an answer — the profile carries a reference.
    assert!(!fixture.run(&["profiles", "list"]).out.contains("password"));

    // `--output table` forces the human shape into a pipe: a head, one
    // hairline, three rows, and every state as a word.
    let table = fixture.run(&["-o", "table", "profiles", "list"]);
    table.ok();
    let lines: Vec<&str> = table.out.lines().collect();
    assert_eq!(lines.len(), 5, "{table:?}", table = table.out);
    assert!(lines[0].starts_with("ID"), "{}", lines[0]);
    assert!(
        lines[1].chars().all(|c| c == '─' || c == ' '),
        "{}",
        lines[1]
    );
    assert!(table.out.contains("read-only"), "{}", table.out);
    assert!(table.out.contains("writable"), "{}", table.out);
    assert!(table.out.contains("PROD"), "{}", table.out);

    // `--output json` is the document, with what the rows do not carry.
    let document = fixture.run(&["-o", "json", "profiles", "list"]).document();
    assert_eq!(document["count"], json!(3));
    assert!(document["profiles_file"]
        .as_str()
        .expect("the path Kavka read")
        .ends_with("profiles.json"));
}

/// An empty config directory is an answer, not a crash — and the answer says
/// where it looked.
#[test]
fn an_empty_config_directory_says_where_it_looked() {
    let fixture = Fixture::with(&[]);
    let run = fixture.run(&["profiles", "list"]);
    assert_eq!(run.code, 0, "an empty list is a successful answer");
    assert!(run.out.is_empty(), "nothing on stdout: {}", run.out);
    assert!(run.err.contains("No connections saved"), "{}", run.err);

    let refusal = fixture.run(&["topics", "list"]).refused(4);
    assert!(refusal.contains("profiles.json"), "{refusal}");
    assert!(refusal.contains("Kavka app"), "{refusal}");
}

// ---------------------------------------------------------------------------
// Against the dev cluster
// ---------------------------------------------------------------------------

/// The read path: topics, one topic's detail, and the 50 seeded records — in
/// both output shapes.
#[test]
fn browsing_the_dev_cluster() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();

    // --- topics -----------------------------------------------------------
    let rows = fixture.run(&["-p", WRITABLE, "topics", "list"]).rows();
    let names: Vec<&str> = rows
        .iter()
        .map(|row| row["name"].as_str().expect("a name"))
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

    // The filter and the internal switch are the two flags that change what
    // comes back rather than how much.
    let filtered = fixture
        .run(&["-p", WRITABLE, "topics", "list", "--contains", "ORDER"])
        .rows();
    assert_eq!(filtered.len(), 1, "{filtered:?}");
    assert_eq!(filtered[0]["name"], "orders");

    let internal = fixture.run(&["-p", WRITABLE, "topics", "list", "--internal"]);
    assert!(
        internal.ok().contains("__consumer_offsets"),
        "{}",
        internal.out
    );
    // …and the default said how many it was hiding, rather than hiding them
    // silently.
    let hidden = fixture.run(&["-p", WRITABLE, "topics", "list"]);
    assert!(hidden.err.contains("internal"), "{}", hidden.err);

    // --- one topic --------------------------------------------------------
    let detail = fixture
        .run(&["-o", "json", "-p", WRITABLE, "topics", "detail", "orders"])
        .document();
    assert_eq!(detail["partition_count"], json!(6));
    assert!(
        detail["approx_message_count"].as_i64().unwrap_or(0) >= 50,
        "the compose file seeds 50: {detail}"
    );
    assert_eq!(detail["under_replicated_partitions"], json!([]));

    // --- the records themselves -------------------------------------------
    let records = fixture
        .run(&[
            "-p",
            WRITABLE,
            "fetch",
            "orders",
            "--earliest",
            "--max",
            "50",
        ])
        .rows();
    assert_eq!(records.len(), 50, "the compose file seeds exactly 50");
    for record in &records {
        assert!(record["partition"].is_number(), "{record}");
        assert!(record["offset"].is_number(), "{record}");
        // The seeded key is `order-N` and the value is a JSON object — which is
        // the point of the compact shape: it arrives as JSON, not as a string
        // holding pretty-printed JSON.
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

    // The same fetch as a table: the ledger's offset column first, and a
    // timestamp a person can read.
    let table = fixture.run(&[
        "-o",
        "table",
        "-p",
        WRITABLE,
        "fetch",
        "orders",
        "--earliest",
        "--max",
        "3",
    ]);
    table.ok();
    let lines: Vec<&str> = table.out.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "head, hairline and three rows: {}",
        table.out
    );
    assert!(lines[0].starts_with("OFFSET"), "{}", lines[0]);
    assert!(
        lines[2].contains('T') && lines[2].contains('Z'),
        "{}",
        lines[2]
    );

    // Reading one partition from its first offset is the seek a script uses to
    // walk a topic, and the one most likely to be mis-parsed.
    let walked = fixture
        .run(&[
            "-p",
            WRITABLE,
            "fetch",
            "orders",
            "--partition",
            "0",
            "--offset",
            "0",
            "--max",
            "3",
        ])
        .rows();
    assert!(!walked.is_empty(), "partition 0 holds some of the 50");
    for record in &walked {
        assert_eq!(record["partition"], json!(0), "{record}");
    }

    // --- consumer groups ---------------------------------------------------
    let groups = fixture.run(&["-p", WRITABLE, "groups", "list"]);
    assert_eq!(groups.code, 0, "{}", groups.err);
}

/// Search: the substring, the CEL expression, and the reporting that makes
/// "never silently truncates" true on a terminal.
///
/// Both queries answer **11** on the seeded corpus, by two different routes:
/// the keys `order-1` and `order-10`…`order-19`, and the eleven orders with an
/// `orderId` of 40 or more. A single number arrived at two ways is worth more
/// than two numbers.
#[test]
fn searching_the_seeded_orders() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();

    let substring = fixture.run(&["-p", WRITABLE, "search", "orders", "order-1", "--earliest"]);
    let rows = substring.rows();
    assert_eq!(rows.len(), 11, "order-1 and order-10..19: {rows:?}");
    for row in &rows {
        let key = row["key"].as_str().expect("a key");
        assert!(key.starts_with("order-1"), "{key}");
    }
    // The summary is on stderr, so the pipe stayed clean — and it reports the
    // scan, not just the answer.
    assert!(
        substring.err.contains("Scanned 50 records"),
        "{}",
        substring.err
    );
    assert!(substring.err.contains("11 matched"), "{}", substring.err);

    // The same eleven, through the decoded record instead of the raw bytes.
    let cel = fixture.run(&[
        "-o",
        "json",
        "-p",
        WRITABLE,
        "search",
        "orders",
        "--cel",
        "value.orderId >= 40",
        "--earliest",
    ]);
    let document = cel.document();
    assert_eq!(document["matched"], json!(11), "orderId 40..50: {document}");
    assert_eq!(document["returned"], json!(11));
    assert_eq!(document["scanned"], json!(50));
    assert_eq!(document["matches_truncated"], json!(false));
    assert_eq!(document["unevaluated"], json!(0));
    assert_eq!(document["stopped_because"], "complete");

    // A cap that bites says so, in the sentence and in the document. The scan
    // runs on past it, which is what lets it say "the first 4 of 11".
    let capped = fixture.run(&[
        "-o",
        "json",
        "-p",
        WRITABLE,
        "search",
        "orders",
        "order-1",
        "--earliest",
        "--max-matches",
        "4",
    ]);
    let document = capped.document();
    assert_eq!(document["returned"], json!(4));
    assert_eq!(document["matched"], json!(11), "{document}");
    assert_eq!(document["matches_truncated"], json!(true));
    assert!(
        capped.err.contains("first 4 of 11 matches"),
        "{}",
        capped.err
    );

    // Nothing matched is an ANSWER: exit 0, an empty pipe, and a sentence
    // saying what was checked. Deliberately unlike grep.
    let nothing = fixture.run(&[
        "-p",
        WRITABLE,
        "search",
        "orders",
        "no-such-substring-anywhere",
        "--earliest",
    ]);
    assert_eq!(nothing.code, 0, "an empty result is not a failure");
    assert!(nothing.out.is_empty(), "{}", nothing.out);
    assert!(nothing.err.contains("Nothing matched"), "{}", nothing.err);
    assert!(nothing.err.contains("50 records"), "{}", nothing.err);

    // A search with nothing to look for is a usage error rather than a full
    // topic dump.
    let refusal = fixture
        .run(&["-p", WRITABLE, "search", "orders", "--earliest"])
        .refused(2);
    assert!(refusal.contains("--cel"), "{refusal}");

    // --- SQL over the same 50 ----------------------------------------------
    let counted = fixture.run(&[
        "-o",
        "json",
        "-p",
        WRITABLE,
        "sql",
        "orders",
        "select count(*) as n from messages",
        "--earliest",
    ]);
    let document = counted.document();
    assert_eq!(document["rows"][0][0], json!(50), "{document}");
    assert_eq!(document["capped"], json!(false));

    // …and as NDJSON a row is an object keyed by the query's own column names,
    // which is the shape `jq` can read. (`value_json` is compact JSON text in
    // this build — the SQL surface says so — so a payload is matched as text.)
    let rows = fixture
        .run(&[
            "-p",
            WRITABLE,
            "sql",
            "orders",
            "select partition as p, count(*) as n from messages \
             where value_json like '%\"status\":\"created\"%' group by partition",
            "--earliest",
        ])
        .rows();
    assert!(!rows.is_empty(), "{rows:?}");
    let total: i64 = rows
        .iter()
        .map(|row| {
            assert!(row["p"].is_number(), "a row is keyed by the query: {row}");
            row["n"].as_i64().expect("a count")
        })
        .sum();
    assert_eq!(total, 50, "every seeded record says created: {rows:?}");
}

/// The one write, and the two refusals that surround it.
///
/// The prod pair is the point: the same record, refused without `--yes-prod`
/// and delivered with it, with a read-back proving that "refused" means nothing
/// reached the topic.
#[test]
fn producing_and_the_prod_guardrail() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();
    let refused_marker = format!("kavka-cli-refused-{}", std::process::id());
    let sent_marker = format!("kavka-cli-sent-{}", std::process::id());

    // --- prod, without the flag: refused, and NOTHING was sent -------------
    let refusal = fixture
        .run(&[
            "-p",
            PROD,
            "produce",
            "dead-letter",
            "--value",
            &refused_marker,
        ])
        .refused(3);
    assert!(refusal.contains("--yes-prod"), "{refusal}");
    assert!(refusal.contains("nothing was sent"), "{refusal}");
    // The cluster, not just the connection name: right-action-wrong-cluster is
    // the accident this line exists to prevent.
    assert!(refusal.contains(":9092"), "{refusal}");

    let after = fixture.run(&[
        "-o",
        "json",
        "-p",
        WRITABLE,
        "search",
        "dead-letter",
        &refused_marker,
        "--earliest",
    ]);
    assert_eq!(
        after.document()["matched"],
        json!(0),
        "a refusal must not reach the topic"
    );

    // --- dev, plainly: delivered, and readable back ------------------------
    let delivered = fixture.run(&[
        "-o",
        "json",
        "-p",
        WRITABLE,
        "produce",
        "dead-letter",
        "--key",
        "A-102",
        "--json",
        r#"{"from":"kavka-cli integration"}"#,
        "-H",
        "trace-id=abc123",
    ]);
    let document = delivered.document();
    assert!(document["partition"].is_number(), "{document}");
    assert!(
        document["offset"].as_i64().unwrap_or(-1) >= 0,
        "the broker reported where it landed: {document}"
    );
    assert_eq!(document["tombstone"], json!(false));

    // A terminal gets the verb, not a one-row table (§7 rule 2).
    let sentence = fixture.run(&[
        "-o",
        "table",
        "-p",
        WRITABLE,
        "produce",
        "dead-letter",
        "--tombstone",
        "--key",
        "A-102",
    ]);
    assert!(
        sentence
            .ok()
            .starts_with("Sent a tombstone to dead-letter["),
        "{}",
        sentence.out
    );

    // --- prod, with the flag: delivered ------------------------------------
    let delivered = fixture.run(&[
        "-o",
        "json",
        "-p",
        PROD,
        "produce",
        "dead-letter",
        "--value",
        &sent_marker,
        "--yes-prod",
    ]);
    assert!(
        delivered.document()["offset"].as_i64().unwrap_or(-1) >= 0,
        "{}",
        delivered.err
    );
    // Every prod command announces the environment and the address, in words,
    // on stderr — docs/DESIGN.md §6 layers 2 and 3.
    assert!(delivered.err.contains("! PROD"), "{}", delivered.err);
    assert!(delivered.err.contains(":9092"), "{}", delivered.err);

    // …and the record is on the topic, which is what "delivered" has to mean.
    let back = fixture.run(&[
        "-o",
        "json",
        "-p",
        WRITABLE,
        "search",
        "dead-letter",
        &sent_marker,
        "--earliest",
    ]);
    assert!(
        back.document()["matched"].as_u64().unwrap_or(0) >= 1,
        "the produced record came back: {}",
        back.out
    );

    // The read-only connection refuses either way, and it refuses without
    // opening a connection at all — asserted in `the_documented_exit_codes`,
    // which needs no broker to prove it.
    let refusal = fixture
        .run(&[
            "-p",
            READ_ONLY,
            "produce",
            "dead-letter",
            "--value",
            "never",
            "--yes-prod",
        ])
        .refused(3);
    assert!(refusal.contains("read-only"), "{refusal}");
}

/// Masking, over a real read: the rules the app saved for a connection apply
/// here too, every answer they rewrote says so, and `--unmasked` is the way out.
#[test]
fn masking_rules_are_honoured_and_can_be_turned_off() {
    if !integration() {
        return;
    }
    let fixture = Fixture::new();
    // Matches the seeded key on every one of the 50 records, so "did masking
    // run" and "did the read work" cannot be confused for each other.
    fixture.mask(WRITABLE, r"order-\d+");

    let masked = fixture.run(&[
        "-p",
        WRITABLE,
        "fetch",
        "orders",
        "--earliest",
        "--max",
        "5",
    ]);
    for row in masked.rows() {
        assert_eq!(row["key"], json!("•••"), "the key came back raw: {row}");
    }
    assert!(
        !masked.out.contains("order-"),
        "an unmasked key survived somewhere in the answer: {}",
        masked.out
    );
    // And it SAYS so — a reader that quotes ••• as a value is a reader that was
    // not told.
    assert!(masked.err.contains("Masked by Kavka"), "{}", masked.err);
    assert!(masked.err.contains("--unmasked"), "{}", masked.err);

    let raw = fixture.run(&[
        "-p",
        WRITABLE,
        "--unmasked",
        "fetch",
        "orders",
        "--earliest",
        "--max",
        "5",
    ]);
    for row in raw.rows() {
        assert!(
            row["key"]
                .as_str()
                .unwrap_or_default()
                .starts_with("order-"),
            "the rule was applied despite the opt-out: {row}"
        );
    }
    assert!(
        !raw.err.contains("Masked by Kavka"),
        "an unmasked answer must carry no indicator: {}",
        raw.err
    );

    // A connection with no rules of its own is untouched — the rules are per
    // profile, exactly as they are in the app.
    let other = fixture.run(&["-p", PROD, "fetch", "orders", "--earliest", "--max", "3"]);
    for row in other.rows() {
        assert!(
            row["key"]
                .as_str()
                .unwrap_or_default()
                .starts_with("order-"),
            "{row}"
        );
    }
}

/// One saved connection needs no `--profile`, and stderr says which one it
/// used. Several do need one — asserted in `the_documented_exit_codes`.
#[test]
fn a_single_connection_needs_no_profile_flag() {
    if !integration() {
        return;
    }
    let fixture = Fixture::with(&[(WRITABLE, "dev", false)]);
    let run = fixture.run(&["topics", "list"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.err.contains("the only connection"), "{}", run.err);
    assert!(run.out.contains("orders"), "{}", run.out);
}

/// **The custom-environment guardrail, through the real config discovery.**
///
/// An enterprise runs more than three environments, so this fixture's
/// connections are on `Production` and `UAT` — names this build has never heard
/// of — and the definitions come from an `environments.json` written by
/// kavka-core's own store into the same directory as `profiles.json`. The
/// binary has to find it the way it finds the connections, or the CLI and the
/// app disagree about which clusters are dangerous.
///
/// No broker: the gate runs before anything is opened, which is the whole point
/// of checking it first.
#[test]
fn a_protected_custom_environment_gates_like_prod_did() {
    let fixture = Fixture::with(&[("ent-prod", "Production", false), ("ent-uat", "UAT", false)]);
    fixture.environments(&[
        // Violet, not red: colour is identity, `protected` is the guardrail.
        EnvironmentDef::new("Production", "violet", true),
        EnvironmentDef::new("UAT", "blue", false),
    ]);

    // Protected: refused with exit 3, and the refusal names the environment its
    // owner invented rather than a word Kavka chose.
    let refusal = fixture
        .run(&[
            "-p",
            "ent-prod",
            "produce",
            "dead-letter",
            "--value",
            "must-not-be-sent",
        ])
        .refused(3);
    assert!(refusal.contains("--yes-prod"), "{refusal}");
    assert!(refusal.contains("Production"), "{refusal}");
    assert!(refusal.contains("nothing was sent"), "{refusal}");

    // Unprotected, same file, same run: no flag needed. It gets as far as the
    // broker, which is what "the gate said yes" looks like from out here — and
    // the refusal above proves the gate is what stopped the other one.
    let uat = fixture.run(&[
        "-p",
        "ent-uat",
        "produce",
        "dead-letter",
        "--value",
        "gate-said-yes",
    ]);
    assert_ne!(
        uat.code, 3,
        "an unprotected environment must not gate: {}",
        uat.err
    );

    // The listing reports the flag beside the name, because the name stopped
    // answering "is this production".
    let listed = fixture.run(&["-o", "json", "profiles", "list"]).document();
    let rows = listed["profiles"].as_array().expect("profiles");
    let by_id = |id: &str| {
        rows.iter()
            .find(|row| row["id"] == json!(id))
            .expect("the profile")
            .clone()
    };
    assert_eq!(by_id("ent-prod")["environment"], json!("Production"));
    assert_eq!(by_id("ent-prod")["environment_protected"], json!(true));
    assert_eq!(by_id("ent-prod")["environment_color"], json!("violet"));
    assert_eq!(by_id("ent-uat")["environment_protected"], json!(false));
}

/// A connection tagged with an environment nothing defines still works: it is
/// neutral, unprotected, and says so once on stderr. Losing a cluster because a
/// label went missing is not an option.
#[test]
fn an_undefined_environment_is_a_hint_not_a_refusal() {
    let fixture = Fixture::with(&[("orphan", "QA", false)]);
    // No environments.json at all, so `QA` matches none of the shipped three.
    let listed = fixture.run(&["-o", "json", "profiles", "list"]).document();
    let row = &listed["profiles"][0];
    assert_eq!(row["environment"], json!("QA"));
    assert_eq!(row["environment_known"], json!(false));
    assert_eq!(row["environment_protected"], json!(false));
    assert_eq!(row["environment_color"], json!("slate"));

    // …and the write gate does not invent a guardrail for it.
    let run = fixture.run(&[
        "-p",
        "orphan",
        "produce",
        "dead-letter",
        "--value",
        "gate-said-yes",
    ]);
    assert_ne!(
        run.code, 3,
        "an undefined environment must not gate: {}",
        run.err
    );
    assert!(run.err.contains("Manage environments"), "{}", run.err);
}
