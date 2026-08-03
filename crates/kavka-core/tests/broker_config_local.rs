//! Broker configuration read/write against the local dev cluster
//! (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl \
//!           --test broker_config_local
//!
//! The whole file runs against a single dynamic key, `log.cleaner.threads`,
//! and it runs single-threaded by construction: the write tests share one
//! broker-wide value, so they live in one `#[test]` rather than racing each
//! other for it.
#![cfg(feature = "kafka")]

use kavka_core::admin::{broker_config_set, broker_configs, ConfigEntry};
use kavka_core::connection::ClusterConnection;
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};
use std::time::{Duration, Instant};

/// A per-broker dynamic config with no side effects worth worrying about on a
/// single-node dev cluster, and — unlike most of the interesting ones — not
/// set in dev/docker-compose.yml, so its baseline is Kafka's own default and
/// the `is_default` assertions below mean something.
const KEY: &str = "log.cleaner.threads";

/// A real broker config that is explicitly *not* dynamically updatable, for
/// the refusal case.
const STATIC_KEY: &str = "log.dirs";

/// IncrementalAlterConfigs answers once the controller commits; the broker
/// serves DescribeConfigs from its own replay of that record.
const SETTLE: Duration = Duration::from_secs(15);

fn integration() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

fn local_connection(read_only: bool) -> ClusterConnection {
    ClusterConnection::connect(ConnectionProfile {
        id: "it-broker-config".into(),
        name: "local docker".into(),
        environment: Environment::Dev,
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        read_only,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
    })
    .expect("connect")
}

fn only_broker(conn: &ClusterConnection) -> i32 {
    let overview = conn.overview().expect("overview");
    assert_eq!(overview.brokers.len(), 1, "single-node dev cluster");
    overview.brokers[0].id
}

fn entry(conn: &ClusterConnection, broker: i32, name: &str) -> ConfigEntry {
    broker_configs(conn, broker)
        .expect("broker configs")
        .into_iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("broker {broker} reports no \"{name}\""))
}

/// Polls the broker's own view of `KEY` until `settled` accepts it **twice
/// running**.
///
/// The repeat is not belt and braces. Kafka updates an entry's provenance and
/// its effective value from different places, so a single DescribeConfigs can
/// land between the two and report `DEFAULT_CONFIG` while still carrying the
/// override's value — which is exactly what made the first draft of this file
/// fail on its second run. Requiring two identical readings means the test
/// asserts on a settled broker rather than on whichever half of a change it
/// happened to catch.
fn until(
    conn: &ClusterConnection,
    broker: i32,
    what: &str,
    settled: impl Fn(&ConfigEntry) -> bool,
) -> ConfigEntry {
    let deadline = Instant::now() + SETTLE;
    let mut previous: Option<ConfigEntry> = None;
    loop {
        let seen = entry(conn, broker, KEY);
        if settled(&seen) && previous.as_ref() == Some(&seen) {
            return seen;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; last saw {seen:#?}"
        );
        previous = Some(seen);
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[test]
fn a_brokers_configuration_reads_back_sorted_and_provenanced() {
    if !integration() {
        return;
    }
    let conn = local_connection(true);
    let broker = only_broker(&conn);

    let configs = broker_configs(&conn, broker).expect("broker configs");
    assert!(
        configs.len() > 50,
        "a broker has many configs: {}",
        configs.len()
    );

    let names: Vec<&str> = configs.iter().map(|entry| entry.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "entries come back sorted by name");

    for expected in ["log.dirs", "num.io.threads", KEY] {
        assert!(names.contains(&expected), "missing {expected}");
    }

    // Provenance is Kafka's own word, not a Kavka invention.
    for entry in &configs {
        assert!(
            [
                "DEFAULT_CONFIG",
                "STATIC_BROKER_CONFIG",
                "DYNAMIC_BROKER_CONFIG",
                "DYNAMIC_DEFAULT_BROKER_CONFIG",
                "DYNAMIC_TOPIC_CONFIG",
                "UNKNOWN",
            ]
            .contains(&entry.source.as_str()),
            "unexpected source on {}: {}",
            entry.name,
            entry.source
        );
        // D5: Kavka never invents a value Kafka refused to send.
        assert!(
            !entry.is_sensitive || entry.value.is_none(),
            "{} is sensitive but carries a value",
            entry.name
        );
    }

    // dev/docker-compose.yml sets this one in the broker's environment, so it
    // is the worked example of "somebody set this" on a broker resource.
    let log_dirs = entry(&conn, broker, "log.dirs");
    assert_eq!(log_dirs.source, "STATIC_BROKER_CONFIG");
    assert!(
        !log_dirs.is_default,
        "server.properties is an override, not a default"
    );
}

#[test]
fn setting_a_broker_config_and_reverting_it_moves_the_default_flag() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let broker = only_broker(&conn);

    // Start from stock, whatever a previous run left behind.
    broker_config_set(&conn, broker, KEY, None).expect("revert to a known baseline");
    let stock = until(&conn, broker, "the baseline to settle", |entry| {
        entry.is_default
    });
    assert_eq!(stock.source, "DEFAULT_CONFIG");
    let stock_value = stock.value.clone().expect("a default has a value");

    // Set: the value changes, and so does its provenance. Both matter — a
    // value that took but still reads as DEFAULT_CONFIG is a config screen
    // that cannot show what has been changed.
    let raised = (stock_value.parse::<i32>().expect("a thread count") + 1).to_string();
    broker_config_set(&conn, broker, KEY, Some(&raised)).expect("set");
    let set = until(&conn, broker, "the new value to settle", |entry| {
        entry.value.as_deref() == Some(raised.as_str()) && !entry.is_default
    });
    assert_eq!(set.source, "DYNAMIC_BROKER_CONFIG");

    // Revert: `None` is a DELETE, not an empty string, so the entry goes back
    // to what the broker inherits rather than to "".
    broker_config_set(&conn, broker, KEY, None).expect("revert");
    let reverted = until(&conn, broker, "the revert to settle", |entry| {
        entry.is_default && entry.value.as_deref() == Some(stock_value.as_str())
    });
    assert_eq!(reverted.source, "DEFAULT_CONFIG");
    assert_eq!(reverted.value, Some(stock_value));
}

#[test]
fn a_config_kafka_will_not_change_dynamically_says_which_one() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let broker = only_broker(&conn);

    let err = broker_config_set(&conn, broker, STATIC_KEY, Some("/tmp/nope"))
        .unwrap_err()
        .to_string();
    assert!(err.contains(STATIC_KEY), "{err}");
}

#[test]
fn a_blank_config_name_is_refused_before_the_network() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let err = broker_config_set(&conn, only_broker(&conn), "   ", Some("1"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("name the configuration entry"), "{err}");
}

#[test]
fn read_only_refuses_a_broker_config_change() {
    if !integration() {
        return;
    }
    let conn = local_connection(true);
    let broker = only_broker(&conn);

    for value in [Some("2"), None] {
        let err = broker_config_set(&conn, broker, KEY, value).unwrap_err();
        assert!(err.to_string().contains("read-only"), "{err}");
    }

    // Reading a broker's configuration is not a mutation.
    broker_configs(&conn, broker).expect("read on a read-only connection");
}
