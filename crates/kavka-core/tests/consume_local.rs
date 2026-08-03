//! Consume engine against the local dev cluster (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl --test consume_local
//!
//! These read only. The produce-and-tail roundtrip lives in `src/consume.rs`'s
//! own test module, where rdkafka's producer is reachable without making it a
//! dev-dependency of the whole crate (see the note there).
#![cfg(feature = "kafka")]

use kavka_core::connection::ClusterConnection;
use kavka_core::consume::{fetch_messages, FetchSpec, SeekSpec};
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};
use kavka_core::serdes::{Encoding, MessageRecord};
use std::collections::{BTreeSet, HashMap};

/// Seeded by dev/docker-compose.yml: 50 keyed JSON messages over 6 partitions.
const SEEDED: usize = 50;
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
        id: "it-consume".into(),
        name: "local docker".into(),
        environment: Environment::Dev,
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        // Reading must not need a writable connection.
        read_only: true,
        schema_registry: None,
    })
    .expect("connect")
}

/// Every read here is uncancelled and registry-free. The cancel path has its
/// own test in `src/consume.rs`, where rdkafka's producer is reachable.
fn fetch(conn: &ClusterConnection, spec: &FetchSpec) -> kavka_core::Result<Vec<MessageRecord>> {
    fetch_messages(conn, None, spec, None)
}

fn spec(seek: SeekSpec) -> FetchSpec {
    FetchSpec {
        topic: "orders".into(),
        seek,
        partitions: None,
        max_messages: 500,
        max_value_bytes: None,
    }
}

/// The highest offset held per partition, learned from a full read.
fn last_offsets(records: &[MessageRecord]) -> HashMap<i32, i64> {
    let mut highest: HashMap<i32, i64> = HashMap::new();
    for record in records {
        let slot = highest.entry(record.partition).or_insert(record.offset);
        *slot = (*slot).max(record.offset);
    }
    highest
}

#[test]
fn earliest_reads_every_seeded_record_in_chronological_order() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let records = fetch(&conn, &spec(SeekSpec::Earliest)).expect("fetch");

    assert_eq!(
        records.len(),
        SEEDED,
        "the seeded topic is read exactly once"
    );

    // Every key, once, across every partition.
    let keys: BTreeSet<String> = records
        .iter()
        .map(|r| {
            r.key
                .as_ref()
                .expect("seeded records are keyed")
                .text
                .clone()
        })
        .collect();
    let expected: BTreeSet<String> = (1..=SEEDED).map(|i| format!("order-{i}")).collect();
    assert_eq!(keys, expected);

    let partitions: BTreeSet<i32> = records.iter().map(|r| r.partition).collect();
    assert_eq!(partitions.len(), PARTITIONS, "spread over every partition");

    for record in &records {
        let key = record.key.as_ref().unwrap();
        assert_eq!(
            key.encoding,
            Encoding::Utf8,
            "a plain key is text, not JSON"
        );
        let value = record.value.as_ref().expect("not a tombstone");
        assert_eq!(value.encoding, Encoding::Json);
        assert!(!value.truncated);
        let json = value.json.as_ref().unwrap();
        // `raw_len` is the length on the wire, not of the re-serialization —
        // the seeder writes `7.50` and serde_json renders it back as `7.5`.
        assert!(
            value.raw_len >= json.to_string().len(),
            "raw_len {} is shorter than the value it decoded",
            value.raw_len
        );
        assert_eq!(json["status"], "created");
        assert!(json["amount"].is_number());
        let order_id = json["orderId"].as_i64().expect("orderId is a number");
        assert_eq!(format!("order-{order_id}"), key.text);
        assert!(record.timestamp_ms.is_some(), "brokers stamp every record");
        assert!(record.headers.is_empty(), "the seeder sends no headers");
    }

    // Cross-partition chronological sort: partitions arrive interleaved off the
    // wire, so this is the engine's ordering, not the broker's.
    let ordered: Vec<(i64, i32, i64)> = records
        .iter()
        .map(|r| (r.timestamp_ms.unwrap_or(i64::MIN), r.partition, r.offset))
        .collect();
    let mut sorted = ordered.clone();
    sorted.sort_unstable();
    assert_eq!(ordered, sorted, "records must arrive sorted");
}

#[test]
fn latest_takes_the_last_n_of_each_partition() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let all = fetch(&conn, &spec(SeekSpec::Earliest)).expect("fetch all");
    let highest = last_offsets(&all);

    let last_n = 5;
    let records = fetch(&conn, &spec(SeekSpec::Latest { last_n })).expect("fetch latest");

    assert!(
        records.len() <= last_n as usize * PARTITIONS,
        "at most {last_n} per partition, got {}",
        records.len()
    );
    assert!(!records.is_empty(), "the topic is not empty");

    let mut per_partition: HashMap<i32, usize> = HashMap::new();
    for record in &records {
        *per_partition.entry(record.partition).or_default() += 1;
        let end = highest[&record.partition];
        assert!(
            record.offset > end - i64::from(last_n),
            "partition {} offset {} is not within the last {last_n} (ends at {end})",
            record.partition,
            record.offset,
        );
    }
    for (partition, count) in per_partition {
        assert!(
            count <= last_n as usize,
            "partition {partition} got {count}"
        );
    }
}

#[test]
fn a_partition_filter_reads_only_that_partition() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let records = fetch(
        &conn,
        &FetchSpec {
            partitions: Some(vec![3]),
            ..spec(SeekSpec::Earliest)
        },
    )
    .expect("fetch");

    assert!(!records.is_empty(), "partition 3 holds some of the 50");
    assert!(records.iter().all(|r| r.partition == 3));
    assert!(records.len() < SEEDED);
}

#[test]
fn seeking_to_an_offset_starts_there_and_reads_to_the_end() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let all = fetch(
        &conn,
        &FetchSpec {
            partitions: Some(vec![0]),
            ..spec(SeekSpec::Earliest)
        },
    )
    .expect("fetch partition 0");
    assert!(all.len() >= 2, "partition 0 needs a couple of records");
    let from = all[1].offset;

    let records = fetch(
        &conn,
        &spec(SeekSpec::Offset {
            partition: 0,
            offset: from,
        }),
    )
    .expect("fetch from offset");

    assert_eq!(records.len(), all.len() - 1);
    assert!(records.iter().all(|r| r.partition == 0 && r.offset >= from));
}

#[test]
fn seeking_past_the_end_reads_nothing_rather_than_waiting() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let started = std::time::Instant::now();
    let records = fetch(
        &conn,
        &spec(SeekSpec::Offset {
            partition: 0,
            offset: i64::MAX,
        }),
    )
    .expect("fetch");

    assert!(records.is_empty());
    // The plan is empty, so this must not spend a single poll interval.
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn a_timestamp_before_the_topic_existed_reads_everything() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let records = fetch(&conn, &spec(SeekSpec::Timestamp { timestamp_ms: 1 })).expect("fetch");

    assert_eq!(records.len(), SEEDED);
}

#[test]
fn a_timestamp_in_the_future_reads_nothing() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let future = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        + 3_600_000;

    let records = fetch(
        &conn,
        &spec(SeekSpec::Timestamp {
            timestamp_ms: future,
        }),
    )
    .expect("fetch");
    assert!(records.is_empty());
}

#[test]
fn an_unknown_topic_says_so_rather_than_returning_nothing() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let err = fetch(
        &conn,
        &FetchSpec {
            topic: "kavka-no-such-topic".into(),
            ..spec(SeekSpec::Earliest)
        },
    )
    .expect_err("unknown topic")
    .to_string();

    assert!(err.contains("no topic called"), "got {err}");
}

#[test]
fn an_unknown_partition_names_what_the_topic_actually_has() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let err = fetch(
        &conn,
        &FetchSpec {
            partitions: Some(vec![99]),
            ..spec(SeekSpec::Earliest)
        },
    )
    .expect_err("unknown partition")
    .to_string();

    assert!(err.contains("6 partitions"), "got {err}");
    assert!(err.contains("99"), "got {err}");
}

#[test]
fn seeking_to_a_partition_the_topic_lacks_says_what_it_has() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let err = fetch(
        &conn,
        &spec(SeekSpec::Offset {
            partition: 99,
            offset: 0,
        }),
    )
    .expect_err("unknown partition")
    .to_string();

    assert!(err.contains("no partition 99 to seek in"), "got {err}");
    assert!(err.contains("numbered 0 to 5"), "got {err}");
}

/// The same seek against a partition that DOES exist but is filtered out is a
/// different mistake with a different fix, and it used to be reported as this
/// topic having no such partition — sending the user after a cluster problem
/// that was never there.
#[test]
fn seeking_outside_the_partition_filter_blames_the_filter() {
    if !integration() {
        return;
    }
    let conn = local_connection();
    let err = fetch(
        &conn,
        &FetchSpec {
            partitions: Some(vec![0, 1]),
            ..spec(SeekSpec::Offset {
                partition: 3,
                offset: 0,
            })
        },
    )
    .expect_err("partition 3 is filtered out")
    .to_string();

    assert!(
        err.contains("partition 3 isn't in the partition filter"),
        "got {err}"
    );
    assert!(
        err.contains("clear the filter"),
        "the message has to name the fix: got {err}"
    );
    assert!(
        !err.contains("no partition 3"),
        "partition 3 exists — claiming otherwise is the bug: got {err}"
    );
}
