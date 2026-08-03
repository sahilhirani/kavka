//! Smoke test against the local dev cluster (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka --test local_cluster
#![cfg(feature = "kafka")]

use kavka_core::connection::ClusterConnection;
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};

fn local_profile(read_only: bool) -> ConnectionProfile {
    ConnectionProfile {
        id: "it-local".into(),
        name: "local docker".into(),
        environment: Environment::Dev,
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        read_only,
        schema_registry: None,
    }
}

#[test]
fn connect_overview_and_topics_against_local_cluster() {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return;
    }

    let conn = ClusterConnection::connect(local_profile(false)).expect("connect");

    let overview = conn.overview().expect("overview");
    assert_eq!(overview.brokers.len(), 1, "single-node dev cluster");
    assert!(overview.cluster_id.is_some(), "KRaft cluster id");

    let topics = conn.list_topics().expect("list topics");
    let names: Vec<&str> = topics.iter().map(|t| t.name.as_str()).collect();
    for expected in [
        "orders",
        "payments",
        "customers",
        "inventory",
        "dead-letter",
    ] {
        assert!(
            names.contains(&expected),
            "missing {expected}; got {names:?}"
        );
    }

    let orders = topics.iter().find(|t| t.name == "orders").unwrap();
    assert_eq!(orders.partitions, 6);
    assert_eq!(orders.replication_factor, 1);
    assert!(!orders.internal);

    let internal = topics.iter().find(|t| t.name == "__consumer_offsets");
    if let Some(t) = internal {
        assert!(t.internal, "__consumer_offsets flagged internal");
    }
}

#[test]
fn read_only_mode_blocks_mutations_in_core() {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return;
    }
    let conn = ClusterConnection::connect(local_profile(true)).expect("connect");
    let err = conn.ensure_writable("create_topic").unwrap_err();
    assert!(err.to_string().contains("read-only"));
}
