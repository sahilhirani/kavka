//! Streaming search against the local dev cluster (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl --test search_local
//!
//! These read only. The tests that need a producer — cancellation mid-scan and
//! the throughput gate — live in `src/search.rs`'s own test module, where
//! rdkafka is reachable without making it a dev-dependency of the whole crate
//! (see the note there).
//!
//! # The fixture
//!
//! `orders` holds 50 keyed JSON messages seeded by the compose file:
//! `order-N` → `{"orderId":N,"status":"created","amount":N*7.50}` for N in
//! 1..=50, spread over 6 partitions. Every expected count below is derived from
//! that and is stated in the test that uses it.
#![cfg(feature = "kafka")]

use kavka_core::connection::ClusterConnection;
use kavka_core::consume::SeekSpec;
use kavka_core::profiles::{AuthConfig, ConnectionProfile};
use kavka_core::search::{SearchQuery, SearchSession, SearchSpec, MAX_BUFFERED};
use kavka_core::serdes::MessageRecord;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// Seeded by dev/docker-compose.yml.
const SEEDED: u64 = 50;
const PARTITIONS: usize = 6;

fn integration() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

fn local_connection() -> ClusterConnection {
    ClusterConnection::connect(ConnectionProfile {
        id: "it-search".into(),
        name: "local docker".into(),
        environment: "dev".into(),
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        // Searching must not need a writable connection.
        read_only: true,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
        wasm_serdes: Vec::new(),
    })
    .expect("connect")
}

fn query(substring: Option<&str>, cel: Option<&str>) -> SearchQuery {
    SearchQuery {
        substring: substring.map(str::to_string),
        cel: cel.map(str::to_string),
    }
}

fn spec(query: SearchQuery) -> SearchSpec {
    SearchSpec {
        topic: "orders".into(),
        seek: SeekSpec::Earliest,
        partitions: None,
        query,
        max_buffered: MAX_BUFFERED,
        max_value_bytes: None,
    }
}

/// What one finished search produced: everything it buffered, and its final
/// progress. Runs the session to completion, which is what `None` from
/// `next_results` means.
struct Outcome {
    records: Vec<MessageRecord>,
    progress: kavka_core::search::SearchProgress,
}

fn run(conn: &ClusterConnection, spec: &SearchSpec) -> Outcome {
    let session = SearchSession::start(conn, None, spec).expect("start search");
    let mut records = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while let Some(batch) = session.next_results(Duration::from_millis(250)) {
        records.extend(batch);
        assert!(Instant::now() < deadline, "the search never finished");
    }
    let progress = session.progress();
    assert!(progress.done, "the buffer only ends when the search does");
    assert_eq!(progress.error, None, "a clean search reports no error");
    Outcome { records, progress }
}

fn keys(records: &[MessageRecord]) -> BTreeSet<String> {
    records
        .iter()
        .map(|r| {
            r.key
                .as_ref()
                .expect("seeded records are keyed")
                .text
                .clone()
        })
        .collect()
}

/// THE SUBSTRING SEARCH. `order-4` is in the key of `order-4` and of
/// `order-40` … `order-49` — 11 of the 50 — and in no others. The engine reads
/// raw bytes, so it finds them without deserializing anything.
#[test]
fn a_substring_matches_every_record_whose_bytes_contain_it() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(&conn, &spec(query(Some("order-4"), None)));

    let expected: BTreeSet<String> = std::iter::once("order-4".to_string())
        .chain((40..=49).map(|i| format!("order-{i}")))
        .collect();
    assert_eq!(keys(&found.records), expected);
    assert_eq!(found.progress.matched, 11);
    assert_eq!(found.progress.buffered, 11);
    assert_eq!(
        found.progress.scanned, SEEDED,
        "the whole topic is scanned, not just the matches"
    );
}

/// Case folding is part of the contract: a support engineer typing a key out of
/// a ticket should not have to match its case.
#[test]
fn a_substring_ignores_ascii_case() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(&conn, &spec(query(Some("ORDER-4"), None)));
    assert_eq!(found.progress.matched, 11);
}

/// THE CEL SEARCH. `orderId` 45 … 50 is 6 of the 50, and it is a question the
/// substring filter cannot answer at all — `>= 45` is not a byte pattern.
#[test]
fn a_cel_expression_filters_on_the_decoded_value() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(&conn, &spec(query(None, Some("value.orderId >= 45"))));

    let expected: BTreeSet<String> = (45..=50).map(|i| format!("order-{i}")).collect();
    assert_eq!(keys(&found.records), expected);
    assert_eq!(found.progress.matched, 6);
    assert_eq!(found.progress.scanned, SEEDED);
    for record in &found.records {
        let json = record.value.as_ref().unwrap().json.as_ref().unwrap();
        assert!(json["orderId"].as_i64().unwrap() >= 45);
    }
    assert_eq!(
        found.progress.unevaluated, 0,
        "every record on this topic is a shape the expression can read"
    );
    assert_eq!(found.progress.filter_error, None);
}

/// `value_text` is the binding the CEL cheatsheet teaches for "look anywhere in
/// the body", because it is the one that works on every shape. Here it is
/// pointed at JSON, which is exactly where `string(value)` — the expression it
/// replaced — fails.
#[test]
fn value_text_searches_the_body_of_a_json_record() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(
        &conn,
        &spec(query(
            None,
            Some(r#"value_text.contains("\"orderId\": 45")"#),
        )),
    );

    assert_eq!(found.progress.unevaluated, 0, "no record defeated it");
    assert_eq!(found.progress.matched, 1);
    assert_eq!(keys(&found.records), ["order-45".to_string()].into());
}

/// BOTH HALVES ARE AN AND. `order-4` matches 11 records, `orderId >= 45`
/// matches 6, and their intersection is `order-45` … `order-49` — 5. Neither
/// count alone can produce that, so this cannot pass by accident if the engine
/// ORs them or drops one.
#[test]
fn a_substring_and_an_expression_are_an_intersection() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(
        &conn,
        &spec(query(Some("order-4"), Some("value.orderId >= 45"))),
    );

    let expected: BTreeSet<String> = (45..=49).map(|i| format!("order-{i}")).collect();
    assert_eq!(keys(&found.records), expected);
    assert_eq!(found.progress.matched, 5);
    assert_eq!(found.progress.scanned, SEEDED);
}

/// BUFFER-CAP HONESTY — the Phase 2 acceptance gate ("search never silently
/// truncates") in one assertion.
///
/// The same 11-match search with room for 5 results must return 5 records and
/// still say `matched: 11`. A search that reported 5 matches would be quietly
/// answering a different question from the one asked, and there would be
/// nothing in the response to tell the user so.
#[test]
fn a_full_buffer_caps_the_results_and_never_the_count() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(
        &conn,
        &SearchSpec {
            max_buffered: 5,
            ..spec(query(Some("order-4"), None))
        },
    );

    assert_eq!(found.records.len(), 5, "the buffer holds five");
    assert_eq!(found.progress.buffered, 5);
    assert_eq!(found.progress.matched, 11, "and the count is the truth");
    assert_eq!(found.progress.scanned, SEEDED, "the scan ran to the end");
    // Every delivered record is a genuine hit — a cap must not fabricate.
    for key in keys(&found.records) {
        assert!(key.contains("order-4"), "{key} is not a match");
    }
}

/// A cap of zero is a legal (if odd) request: count everything, buffer nothing.
#[test]
fn a_zero_buffer_still_counts() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(
        &conn,
        &SearchSpec {
            max_buffered: 0,
            ..spec(query(Some("order-4"), None))
        },
    );
    assert!(found.records.is_empty());
    assert_eq!(found.progress.buffered, 0);
    assert_eq!(found.progress.matched, 11);
}

/// An empty query is a plain "read the whole topic".
#[test]
fn an_empty_query_matches_every_record() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(&conn, &spec(SearchQuery::default()));

    assert_eq!(found.progress.matched, SEEDED);
    assert_eq!(found.progress.scanned, SEEDED);
    assert_eq!(found.records.len(), SEEDED as usize);
}

/// PROGRESS COVERS EVERY PARTITION, including the ones with nothing in them.
/// A partition missing from this list is indistinguishable, in the UI, from one
/// that never started — and a finished search whose bar sits at 90% is a bug
/// report.
#[test]
fn progress_reports_every_partition_and_ends_at_its_watermark() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(&conn, &spec(SearchQuery::default()));

    assert_eq!(found.progress.per_partition.len(), PARTITIONS);
    let listed: Vec<i32> = found
        .progress
        .per_partition
        .iter()
        .map(|p| p.partition)
        .collect();
    let mut ascending = listed.clone();
    ascending.sort_unstable();
    assert_eq!(listed, ascending, "partitions are listed in order");

    let total: i64 = found
        .progress
        .per_partition
        .iter()
        .map(|p| p.end_offset)
        .sum();
    assert_eq!(total, SEEDED as i64, "the watermarks account for all 50");
    for partition in &found.progress.per_partition {
        assert_eq!(
            partition.current_offset, partition.end_offset,
            "partition {} finished short of its watermark",
            partition.partition
        );
    }
    assert!(found.progress.msgs_per_sec > 0.0, "a rate was measured");
    // Every partition here reached its watermark by reading records, so nothing
    // is "complete because it went quiet". A non-empty list on this fixture
    // means a partition stalled — which is exactly the thing the UI now
    // discloses, and exactly the thing this fixture must not be hiding.
    assert!(
        found.progress.assumed_complete.is_empty(),
        "a non-transactional topic reaches every watermark: {:?}",
        found.progress.assumed_complete
    );
}

/// A partition filter narrows the scan itself, not just the results: the
/// records outside it are never read, so `scanned` drops with `matched`.
#[test]
fn a_partition_filter_narrows_the_scan() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(
        &conn,
        &SearchSpec {
            partitions: Some(vec![3]),
            ..spec(SearchQuery::default())
        },
    );

    assert_eq!(found.progress.per_partition.len(), 1);
    assert_eq!(found.progress.per_partition[0].partition, 3);
    assert!(
        found.progress.scanned > 0,
        "partition 3 holds some of the 50"
    );
    assert!(found.progress.scanned < SEEDED);
    assert!(found.records.iter().all(|r| r.partition == 3));
}

/// The seek bounds the scan the same way it bounds a fetch.
#[test]
fn a_timestamp_seek_bounds_where_the_scan_starts() {
    if !integration() {
        return;
    }
    let conn = local_connection();

    let everything = run(
        &conn,
        &SearchSpec {
            seek: SeekSpec::Timestamp { timestamp_ms: 1 },
            ..spec(SearchQuery::default())
        },
    );
    assert_eq!(everything.progress.scanned, SEEDED);

    let future = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        + 3_600_000;
    let nothing = run(
        &conn,
        &SearchSpec {
            seek: SeekSpec::Timestamp {
                timestamp_ms: future,
            },
            ..spec(SearchQuery::default())
        },
    );
    // Nothing to read, but the partitions are still reported — as complete.
    assert_eq!(nothing.progress.scanned, 0);
    assert_eq!(nothing.progress.matched, 0);
    assert!(nothing.progress.done);
    assert_eq!(nothing.progress.per_partition.len(), PARTITIONS);
    for partition in &nothing.progress.per_partition {
        assert_eq!(partition.current_offset, partition.end_offset);
    }
}

/// `Latest { last_n }` names a window per partition, so this reads at most
/// 6 records — and finds the newest of them.
#[test]
fn a_latest_seek_searches_only_the_tail_of_each_partition() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(
        &conn,
        &SearchSpec {
            seek: SeekSpec::Latest { last_n: 1 },
            ..spec(SearchQuery::default())
        },
    );
    assert_eq!(found.progress.scanned, PARTITIONS as u64);
    assert_eq!(found.progress.matched, PARTITIONS as u64);
}

/// A CEL SYNTAX ERROR FAILS `start` — synchronously, before any broker call, so
/// the message can land under the expression the user is still editing rather
/// than as an empty result a second later.
#[test]
fn a_broken_expression_fails_the_search_before_it_starts() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let error = SearchSession::start(&conn, None, &spec(query(None, Some("value.orderId >="))))
        .expect_err("an incomplete expression")
        .to_string();

    assert!(error.contains("didn't parse"), "got {error}");
    assert!(
        error.contains("value.orderId >="),
        "the message has to name the expression: got {error}"
    );
}

/// ...and it fails even when the rest of the spec is nonsense too, because the
/// filter is compiled first. A user who typo'd an expression should be told
/// about the expression, not sent to the cluster.
#[test]
fn the_expression_is_checked_before_the_topic_exists() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let error = SearchSession::start(
        &conn,
        None,
        &SearchSpec {
            topic: "kavka-no-such-topic".into(),
            ..spec(query(None, Some("(")))
        },
    )
    .expect_err("both are wrong")
    .to_string();

    assert!(error.contains("didn't parse"), "got {error}");
    assert!(!error.contains("no topic called"), "got {error}");
}

#[test]
fn an_unknown_topic_says_so_rather_than_searching_nothing() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let error = SearchSession::start(
        &conn,
        None,
        &SearchSpec {
            topic: "kavka-no-such-topic".into(),
            ..spec(SearchQuery::default())
        },
    )
    .expect_err("unknown topic")
    .to_string();

    assert!(error.contains("no topic called"), "got {error}");
}

#[test]
fn an_unknown_partition_names_what_the_topic_actually_has() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let error = SearchSession::start(
        &conn,
        None,
        &SearchSpec {
            partitions: Some(vec![99]),
            ..spec(SearchQuery::default())
        },
    )
    .expect_err("unknown partition")
    .to_string();

    assert!(error.contains("6 partitions"), "got {error}");
    assert!(error.contains("99"), "got {error}");
}

/// A search that matches nothing is not a failure and not a hang: it scans
/// everything and ends, so the UI can say "checked all 50" rather than
/// "no results" while still running (docs/DESIGN.md §7).
#[test]
fn a_search_that_matches_nothing_still_finishes_and_says_what_it_checked() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let found = run(&conn, &spec(query(Some("no-such-order"), None)));

    assert!(found.records.is_empty());
    assert_eq!(found.progress.matched, 0);
    assert_eq!(found.progress.scanned, SEEDED);
    assert!(found.progress.done);
    assert_eq!(found.progress.error, None);
}

/// `stop()` on a search that has already finished is a no-op, and dropping a
/// session that was never read joins its workers rather than leaking them.
#[test]
fn stopping_is_idempotent_and_survives_a_finished_search() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let session =
        SearchSession::start(&conn, None, &spec(SearchQuery::default())).expect("start search");

    let deadline = Instant::now() + Duration::from_secs(30);
    while !session.progress().done {
        assert!(Instant::now() < deadline, "the search never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
    session.stop();
    session.stop();
    assert!(session.progress().done);

    let dropped_at = Instant::now();
    drop(session);
    assert!(dropped_at.elapsed() < Duration::from_secs(5));
}
