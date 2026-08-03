//! The WASM decoder plugin, end to end through the read paths, against the
//! local dev cluster (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl --test wasm_pipeline_local
//!
//! # What this proves that the unit tests cannot
//!
//! `src/wasm_serde.rs` tests the engine — the ABI, the sandbox, the fuel — and
//! `src/serdes.rs` tests that `decode_with` gives a `CustomDecoder` first
//! refusal over the built-in ladder. Neither of them can prove the thing a user
//! actually depends on: that a plugin named on a PROFILE reaches the decode of
//! a record read from a BROKER.
//!
//! That path runs through `ClusterConnection::decoder_for`, which is the one
//! seam every read path uses (`consume`, `search`, `sql`, and the CEL filter of
//! an `xcluster` copy). This walks it for real: a profile with a plugin, a
//! connection opened from it, and the plugin's fingerprint — `decoded_by`, and
//! the JSON it invented — on records that came off the wire.
//!
//! The plugin is a WebAssembly TEXT module written to a temp file rather than
//! the committed example in `docs/examples/wasm-serde/`: that one needs
//! `cargo build --target wasm32-unknown-unknown` to exist, and a test that
//! silently skips unless somebody installed a rustup target is a test that
//! never runs. `wat` is in wasmtime's feature list for exactly this.
#![cfg(all(feature = "kafka", feature = "wasm-serdes"))]

use kavka_core::connection::ClusterConnection;
use kavka_core::consume::{fetch_messages, FetchSpec, SeekSpec};
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment, WasmSerdeConfig};
use kavka_core::search::{SearchQuery, SearchSession, SearchSpec};
use kavka_core::sql::{SqlSession, SqlSpec};
use std::time::{Duration, Instant};

/// Seeded by dev/docker-compose.yml: 50 keyed JSON messages over 6 partitions.
const SEEDED: usize = 50;

/// A complete ABI v1 plugin that claims EVERY record and answers one fixed
/// document.
///
/// Claiming everything is what makes it a probe: the `orders` topic is JSON, so
/// the built-in ladder would decode it perfectly well, and a payload that comes
/// back as `{"decoded_by_the_plugin":true}` can only have come from here. That
/// is the assertion — the plugin ran BEFORE the ladder, which is the ordering
/// the whole feature depends on.
const PROBE: &str = r#"
(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 4096))

  ;; The result block at address 64: status 0 (decoded), a little-endian u32
  ;; length of 30 (0x1e — the JSON below is exactly that long), then the bytes.
  (data (i32.const 64) "\00\1e\00\00\00{\22decoded_by_the_plugin\22:true}")

  (func (export "kavka_abi_version") (result i32)
    (i32.const 1))

  (func (export "kavka_alloc") (param $len i32) (result i32)
    (local $at i32)
    (local.set $at (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $len)))
    (local.get $at))

  (func (export "kavka_free") (param i32) (param i32))

  (func (export "kavka_decode") (param $ptr i32) (param $len i32) (result i32)
    (i32.const 64))
)
"#;

fn integration() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

/// Writes the probe beside the test binary and answers its path.
///
/// One file per test, named after the test, because these run in parallel and
/// a shared name is a race that fails on someone else's machine.
fn probe_path(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("kavka-probe-{name}.wat"));
    std::fs::write(&path, PROBE).expect("the temp dir is writable");
    path
}

fn connection(name: &str, topics: &[&str]) -> ClusterConnection {
    let conn = ClusterConnection::connect(ConnectionProfile {
        id: format!("it-wasm-{name}"),
        name: "local docker".into(),
        environment: Environment::Dev,
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        read_only: true,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
        wasm_serdes: vec![WasmSerdeConfig {
            name: "probe".into(),
            path: probe_path(name).display().to_string(),
            applies_to_topics: topics.iter().map(|glob| (*glob).to_string()).collect(),
        }],
    })
    .expect("connect");
    assert!(
        conn.serde_problems().is_empty(),
        "the probe must load: {:?}",
        conn.serde_problems()
    );
    conn
}

/// A bounded browse is the path most people meet a plugin on.
#[test]
fn a_plugin_decodes_the_records_a_fetch_returns() {
    if !integration() {
        return;
    }
    let conn = connection("fetch", &["orders"]);
    let records = fetch_messages(
        &conn,
        None,
        &FetchSpec {
            topic: "orders".into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            max_messages: 10,
            max_value_bytes: None,
        },
        None,
    )
    .expect("fetch");
    assert!(!records.is_empty(), "the seeded topic has records");
    for record in &records {
        let value = record.value.as_ref().expect("orders has no tombstones");
        assert_eq!(
            value.decoded_by.as_deref(),
            Some("probe"),
            "the provenance names the plugin"
        );
        assert_eq!(
            value.json,
            Some(serde_json::json!({"decoded_by_the_plugin": true})),
            "the plugin ran BEFORE the built-in JSON decoder"
        );
    }
}

/// THE GLOB IS THE SWITCH. A plugin that does not claim the topic must not be
/// consulted for it — the built-in ladder decodes `orders` as the JSON it is.
#[test]
fn a_plugin_that_does_not_claim_the_topic_is_not_consulted() {
    if !integration() {
        return;
    }
    let conn = connection("glob", &["payments.*"]);
    let records = fetch_messages(
        &conn,
        None,
        &FetchSpec {
            topic: "orders".into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            max_messages: 5,
            max_value_bytes: None,
        },
        None,
    )
    .expect("fetch");
    assert!(!records.is_empty());
    for record in &records {
        let value = record.value.as_ref().expect("orders has no tombstones");
        assert_eq!(value.decoded_by, None);
        assert!(
            value.text.contains("orderId"),
            "the built-in ladder decoded it: {}",
            value.text
        );
    }
}

/// A search runs on eight worker threads, each with its own sandbox — so this
/// is also the proof that one compiled module serves them all.
///
/// The CEL expression reads a field ONLY the plugin produces, which is what
/// makes it an assertion about the pipeline rather than about the topic.
#[test]
fn a_search_filters_on_what_the_plugin_decoded() {
    if !integration() {
        return;
    }
    let conn = connection("search", &["orders"]);
    let session = SearchSession::start(
        &conn,
        None,
        &SearchSpec {
            topic: "orders".into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            query: SearchQuery {
                substring: None,
                cel: Some("value.decoded_by_the_plugin == true".into()),
            },
            max_buffered: 200,
            max_value_bytes: None,
        },
    )
    .expect("search starts");

    let mut matched = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while let Some(batch) = session.next_results(Duration::from_millis(250)) {
        matched.extend(batch);
        if Instant::now() >= deadline {
            break;
        }
    }
    assert_eq!(
        matched.len(),
        SEEDED,
        "every seeded record decodes through the plugin"
    );
    for record in &matched {
        assert_eq!(
            record
                .value
                .as_ref()
                .and_then(|value| value.decoded_by.as_deref()),
            Some("probe")
        );
    }
}

/// The `messages` table's `value_json` column is built from the decoded
/// payload, so a query over it is a query over the plugin's answer.
#[test]
fn sql_reads_the_columns_the_plugin_produced() {
    if !integration() {
        return;
    }
    let conn = connection("sql", &["orders"]);
    let session = SqlSession::start(
        &conn,
        None,
        &SqlSpec {
            topic: "orders".into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            query: "SELECT count(*) AS n FROM messages \
                    WHERE value_json LIKE '%decoded_by_the_plugin%'"
                .into(),
            scan_cap: 1000,
            max_rows: 10,
        },
    )
    .expect("the query plans and starts");

    let mut rows = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while let Some(batch) = session.next_rows(Duration::from_millis(250)) {
        rows.extend(batch);
        if Instant::now() >= deadline {
            break;
        }
    }
    let progress = session.progress();
    assert_eq!(progress.error, None, "the query finished cleanly");
    assert_eq!(rows.len(), 1, "count(*) is one row");
    assert_eq!(rows[0][0], serde_json::json!(SEEDED));
}
