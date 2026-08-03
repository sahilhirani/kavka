//! The hand-rolled wire protocol client against the local dev cluster
//! (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl --test protocol_local
//!
//! Everything here is a live round trip: framing, ApiVersions negotiation,
//! and the six typed calls of the Phase 3b contract. The unit tests in
//! `src/protocol` pin the bytes; these pin the fact that a real Kafka accepts
//! them.
//!
//! SINGLE NODE. The dev cluster is one broker that is also the sole controller
//! and sole voter, which bounds what can be checked here — see
//! `dev/docker-compose.cluster.yml` and the `KAVKA_CLUSTER_IT` tests in
//! `protocol_cluster.rs` for the multi-broker half (a real reassignment, and a
//! preferred-leader election that actually moves a leader).
#![cfg(feature = "kafka")]

use kavka_core::connection::ClusterConnection;
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};
use kavka_core::protocol::{
    self, ProtocolClient, QuotaEntityPart, QuotaOp, ReassignmentSpec, TopicPartition,
};

/// The topic dev/docker-compose.yml seeds: 6 partitions, replication factor 1.
const TOPIC: &str = "orders";

/// The dev cluster's only broker (and only controller, and only voter).
const BROKER: i32 = 1;

fn local_profile(read_only: bool) -> ConnectionProfile {
    ConnectionProfile {
        id: "it-protocol".into(),
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
    }
}

/// `false` (and a printed note) when the suite is not enabled, matching every
/// other `*_local.rs` here.
fn enabled() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

fn client() -> ProtocolClient {
    ProtocolClient::connect(&local_profile(false)).expect("protocol connect")
}

/// A quota entity nothing else touches. `label` keeps concurrently running
/// tests off each other's entity — cargo runs them on parallel threads against
/// one cluster — and the process id does the same for two concurrent runs (or a
/// stale entity left by an interrupted one).
fn test_user(label: &str) -> Vec<QuotaEntityPart> {
    vec![QuotaEntityPart {
        entity_type: "user".into(),
        name: Some(format!("kavka-it-{label}-{}", std::process::id())),
    }]
}

fn default_user() -> Vec<QuotaEntityPart> {
    vec![QuotaEntityPart {
        entity_type: "user".into(),
        name: None,
    }]
}

fn remove(key: &str) -> Vec<QuotaOp> {
    vec![QuotaOp {
        key: key.to_string(),
        value: None,
    }]
}

// ---------------------------------------------------------------------------
// Connection + negotiation
// ---------------------------------------------------------------------------

/// The whole framing layer in one assertion: if ApiVersions negotiation had the
/// header, the length prefix, the correlation id or the flexible encoding
/// wrong, `connect` could not return.
#[test]
fn connecting_negotiates_protocol_versions_against_a_real_broker() {
    if !enabled() {
        return;
    }
    let mut client = client();
    // A negotiated call that reads nothing but the version table.
    assert!(
        client.reassign_list(None).is_ok(),
        "a negotiated call must round trip"
    );
}

// ---------------------------------------------------------------------------
// DescribeQuorum (55)
// ---------------------------------------------------------------------------

#[test]
fn quorum_describe_reports_the_single_node_as_leader_and_sole_voter() {
    if !enabled() {
        return;
    }
    let quorum = client().quorum_describe().expect("describe quorum");

    assert_eq!(
        quorum.leader_id, BROKER,
        "the only node leads its own quorum"
    );
    assert!(
        quorum.leader_epoch >= 1,
        "got epoch {}",
        quorum.leader_epoch
    );
    assert!(
        quorum.high_watermark > 0,
        "a running KRaft cluster has written metadata records: {}",
        quorum.high_watermark
    );
    assert_eq!(
        quorum.voters.len(),
        1,
        "single-node quorum: {:?}",
        quorum.voters
    );
    assert_eq!(quorum.voters[0].replica_id, BROKER);
    assert!(
        quorum.voters[0].log_end_offset >= quorum.high_watermark,
        "the leader's log cannot be behind its own high watermark"
    );
    // Combined broker+controller, so the broker is not a separate observer.
    assert!(
        quorum.observers.len() <= 1,
        "unexpected observers: {:?}",
        quorum.observers
    );
    // Ages are computed against the local clock, so they must be sane rather
    // than the raw broker timestamps.
    for replica in quorum.voters.iter().chain(quorum.observers.iter()) {
        if let Some(age) = replica.last_fetch_age_ms {
            assert!(
                (0..3_600_000).contains(&age),
                "implausible fetch age {age}ms — a broker timestamp leaked through"
            );
        }
    }
}

/// The IPC contract is a serde shape; the field names are the contract.
#[test]
fn quorum_info_serialises_with_the_contract_field_names() {
    if !enabled() {
        return;
    }
    let quorum = client().quorum_describe().expect("describe quorum");
    let json = serde_json::to_value(&quorum).expect("serialise");

    for field in [
        "leader_id",
        "leader_epoch",
        "high_watermark",
        "voters",
        "observers",
    ] {
        assert!(json.get(field).is_some(), "missing {field} in {json}");
    }
    let voter = &json["voters"][0];
    for field in [
        "replica_id",
        "log_end_offset",
        "last_fetch_age_ms",
        "last_caught_up_age_ms",
    ] {
        assert!(voter.get(field).is_some(), "missing {field} in {voter}");
    }
}

// ---------------------------------------------------------------------------
// ElectLeaders (43)
// ---------------------------------------------------------------------------

/// On one broker every partition already has its preferred leader, so Kafka
/// answers ELECTION_NOT_NEEDED for all six. That is the benign no-op the error
/// table exists for: it must reach the caller as a success, not as six errors.
#[test]
fn elect_leaders_on_a_settled_topic_is_a_benign_no_op_per_partition() {
    if !enabled() {
        return;
    }
    let results = client()
        .elect_leaders(Some(TOPIC), None)
        .expect("elect leaders");

    assert_eq!(results.len(), 6, "one row per partition: {results:?}");
    for result in &results {
        assert_eq!(result.topic, TOPIC);
        assert_eq!(
            result.error, None,
            "ELECTION_NOT_NEEDED must not surface as a failure: {result:?}"
        );
    }
    assert_eq!(
        results.iter().map(|r| r.partition).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
}

#[test]
fn elect_leaders_accepts_an_explicit_partition_subset() {
    if !enabled() {
        return;
    }
    let results = client()
        .elect_leaders(Some(TOPIC), Some(&[0, 2]))
        .expect("elect leaders");
    assert_eq!(
        results.iter().map(|r| r.partition).collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert!(results.iter().all(|r| r.error.is_none()), "{results:?}");
}

/// The cluster-wide form. Kafka OMITS partitions that already have their
/// preferred leader from this answer, so on a settled cluster the correct
/// result is an empty list — not an error, and not six rows.
#[test]
fn elect_leaders_across_the_cluster_reports_only_what_needed_doing() {
    if !enabled() {
        return;
    }
    let results = client().elect_leaders(None, None).expect("elect leaders");
    assert!(
        results.iter().all(|r| r.error.is_none()),
        "nothing should fail on a settled cluster: {results:?}"
    );
}

#[test]
fn elect_leaders_on_a_topic_that_does_not_exist_names_it() {
    if !enabled() {
        return;
    }
    let err = client()
        .elect_leaders(Some("kavka-no-such-topic"), None)
        .expect_err("no such topic")
        .to_string();
    assert!(err.contains("kavka-no-such-topic"), "got {err}");
}

// ---------------------------------------------------------------------------
// List/AlterPartitionReassignments (46, 45)
// ---------------------------------------------------------------------------

#[test]
fn reassign_list_is_empty_on_a_settled_cluster() {
    if !enabled() {
        return;
    }
    let mut client = client();
    assert_eq!(
        client.reassign_list(None).expect("list all"),
        vec![],
        "nothing should be moving"
    );
    assert_eq!(
        client.reassign_list(Some(TOPIC)).expect("list one topic"),
        vec![]
    );
}

/// Moving a partition onto the broker it is already on. Kafka accepts the
/// request and has nothing to do, so the list either never shows it or drains
/// immediately — both are "accepted, and then settled", which is what the
/// caller has to be able to observe.
///
/// The second half is the regression guard for the named-topic form of
/// `reassign_list`. On one broker there is no move to watch, so a poll loop
/// alone cannot tell a working `list` from one that answers `[]` to every
/// question — which is exactly what a named topic used to get: the request
/// carried an EMPTY `PartitionIndexes` array, and Kafka's controller iterates
/// that array, so it asked about no partitions at all and reported nothing
/// moving whatever was moving. What proves the topic is really resolved and
/// really sent is that a topic which does NOT exist now fails by name instead
/// of coming back as an empty, cheerful "nothing in flight".
#[test]
fn reassign_alter_to_the_current_broker_is_accepted_and_drains() {
    if !enabled() {
        return;
    }
    let mut client = client();
    let results = client
        .reassign_alter(&[ReassignmentSpec {
            topic: TOPIC.into(),
            partition: 0,
            replicas: vec![BROKER],
        }])
        .expect("alter reassignments");

    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].topic, TOPIC);
    assert_eq!(results[0].partition, 0);
    assert_eq!(results[0].error, None, "{results:?}");

    // The NAMED-topic list has to drain. A no-op reassignment on one broker
    // settles at once, so a couple of seconds is generous; the loop exists so
    // the test reports "still moving after N tries" rather than flaking on
    // timing. Every state it does see must be about the topic that was asked
    // about — a list that answered with somebody else's partitions would be a
    // request with the wrong topic in it.
    let mut remaining = Vec::new();
    for _ in 0..20 {
        remaining = client.reassign_list(Some(TOPIC)).expect("list one topic");
        assert!(
            remaining.iter().all(|state| state.topic == TOPIC),
            "the named-topic list answered about another topic: {remaining:?}"
        );
        if remaining.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(remaining, vec![], "the reassignment never drained");

    // The named form resolves the topic's partitions before it asks, so a
    // topic with none to resolve is an error that names it — not `Ok([])`.
    // This is the assertion the old encoder could not pass.
    let err = client
        .reassign_list(Some("kavka-no-such-topic"))
        .expect_err("a named topic that does not exist cannot be listed")
        .to_string();
    assert!(err.contains("kavka-no-such-topic"), "got {err}");

    // And the cluster-wide form is still the null-topic request, which asks
    // about everything and needs no resolution at all.
    assert_eq!(client.reassign_list(None).expect("list all"), vec![]);
}

/// The kept-alive client the desktop shell caches (see `protocol_call` in
/// `apps/desktop/src-tauri/src/lib.rs`): several calls over ONE socket, and the
/// connection still reporting itself healthy afterwards.
///
/// The reassignment monitor polls every two seconds, so "the same client can
/// answer again" is not a nicety — it is what makes that poll one login rather
/// than one login per poll.
#[test]
fn one_client_serves_repeated_calls_over_a_single_connection() {
    if !enabled() {
        return;
    }
    let mut client = client();
    for _ in 0..5 {
        client.reassign_list(Some(TOPIC)).expect("list");
    }
    client.quotas_list().expect("list quotas");
    client.quorum_describe().expect("describe quorum");
    assert!(
        client.is_healthy(),
        "a connection that answered every call must not be reported as broken"
    );

    // A KAFKA-level refusal must not condemn the socket: it arrives in a
    // complete, correctly framed reply, so the next call on the same client
    // has to work.
    let _ = client
        .reassign_list(Some("kavka-no-such-topic"))
        .expect_err("no such topic");
    assert!(
        client.is_healthy(),
        "an UNKNOWN_TOPIC_OR_PARTITION is not a transport failure"
    );
    client
        .reassign_list(Some(TOPIC))
        .expect("the same client still works after a broker-level refusal");
}

/// Cancelling something that is not moving. NO_REASSIGNMENT_IN_PROGRESS means
/// "already in the state you asked for", so it is benign — the same rule as
/// ELECTION_NOT_NEEDED.
#[test]
fn reassign_cancel_on_a_settled_partition_is_a_benign_no_op() {
    if !enabled() {
        return;
    }
    let results = client()
        .reassign_cancel(&[TopicPartition {
            topic: TOPIC.into(),
            partition: 1,
        }])
        .expect("cancel");

    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].error, None, "{results:?}");
}

/// The broker's own per-partition error, surfaced with Kafka's name for it —
/// the whole call must NOT fail, because a batch where one partition is wrong
/// still tells the user about the others.
#[test]
fn reassigning_onto_a_broker_that_does_not_exist_fails_that_partition_only() {
    if !enabled() {
        return;
    }
    let results = client()
        .reassign_alter(&[
            ReassignmentSpec {
                topic: TOPIC.into(),
                partition: 2,
                replicas: vec![9_999],
            },
            ReassignmentSpec {
                topic: TOPIC.into(),
                partition: 3,
                replicas: vec![BROKER],
            },
        ])
        .expect("the call itself succeeds; the partition does not");

    assert_eq!(results.len(), 2, "{results:?}");
    let bad = results
        .iter()
        .find(|r| r.partition == 2)
        .expect("partition 2");
    let error = bad.error.as_deref().expect("broker 9999 does not exist");
    assert!(
        error.contains("INVALID_REPLICA_ASSIGNMENT") || error.contains("BROKER_ID_NOT_REGISTERED"),
        "expected the broker's own name for the fault, got {error}"
    );
    assert_eq!(
        results
            .iter()
            .find(|r| r.partition == 3)
            .and_then(|r| r.error.clone()),
        None,
        "a valid partition in the same batch must still succeed"
    );
}

/// Caught before the network so the message can name the partition.
#[test]
fn an_empty_replica_list_is_refused_locally() {
    if !enabled() {
        return;
    }
    let err = client()
        .reassign_alter(&[ReassignmentSpec {
            topic: TOPIC.into(),
            partition: 0,
            replicas: vec![],
        }])
        .expect_err("a partition cannot live nowhere")
        .to_string();
    assert!(err.contains("orders-0"), "got {err}");
    assert!(err.contains("empty replica list"), "got {err}");
}

// ---------------------------------------------------------------------------
// Describe/AlterClientQuotas (48, 49)
// ---------------------------------------------------------------------------

/// Polls `quotas_list` until this entity's `key` reads as `expected`
/// (`None` = absent), and returns the entity as the cluster last reported it.
///
/// A poll rather than a straight assertion, deliberately: AlterClientQuotas is
/// answered by the CONTROLLER once the record is committed, while
/// DescribeClientQuotas is answered from the BROKER's metadata image, which
/// applies that record a moment later. Reading immediately after writing is
/// therefore a race Kafka never promised to win — invisible on an idle cluster,
/// and reproducible the moment the rest of the suite is running alongside it.
fn wait_for_quota(
    client: &mut ProtocolClient,
    entity: &[QuotaEntityPart],
    key: &str,
    expected: Option<f64>,
) -> Option<kavka_core::protocol::QuotaEntity> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let listed = client.quotas_list().expect("list quotas");
        let found = listed.iter().find(|q| q.entity == entity).cloned();
        let value = found
            .as_ref()
            .and_then(|q| q.values.iter().find(|v| v.key == key))
            .map(|v| v.value);
        if value == expected {
            return found;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{key} on {entity:?} is {value:?}, expected {expected:?} (whole list: {listed:?})"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// The full lifecycle: set, find, remove, confirm gone.
#[test]
fn a_user_quota_can_be_set_found_and_removed() {
    if !enabled() {
        return;
    }
    let mut client = client();
    let entity = test_user("lifecycle");

    // Clean up anything a previous interrupted run left behind.
    let _ = client.quotas_alter(&entity, &remove("producer_byte_rate"));

    client
        .quotas_alter(
            &entity,
            &[QuotaOp {
                key: "producer_byte_rate".into(),
                value: Some(1_048_576.0),
            }],
        )
        .expect("set the quota");

    let found = wait_for_quota(
        &mut client,
        &entity,
        "producer_byte_rate",
        Some(1_048_576.0),
    )
    .expect("the entity is listed once its quota is");
    assert_eq!(found.entity, entity);

    client
        .quotas_alter(&entity, &remove("producer_byte_rate"))
        .expect("remove the quota");

    // `None` here is the whole assertion: the quota is gone, not zeroed.
    wait_for_quota(&mut client, &entity, "producer_byte_rate", None);
}

/// The `<default>` entity — a NULL name on the wire — is a different entity
/// from any named one, and a describe that quietly omitted it would be the
/// classic bug this asserts against.
///
/// The value is chosen to be inert: 1 GiB/s is effectively unlimited, so even
/// if this test were interrupted between set and remove, the dev cluster would
/// not start throttling the rest of the suite.
#[test]
fn the_default_user_entity_round_trips_as_its_own_entity() {
    if !enabled() {
        return;
    }
    let mut client = client();
    let entity = default_user();
    let _ = client.quotas_alter(&entity, &remove("producer_byte_rate"));

    client
        .quotas_alter(
            &entity,
            &[QuotaOp {
                key: "producer_byte_rate".into(),
                value: Some(1_073_741_824.0),
            }],
        )
        .expect("set the <default> quota");

    let found = wait_for_quota(
        &mut client,
        &entity,
        "producer_byte_rate",
        Some(1_073_741_824.0),
    )
    .expect("the <default> entity is listed once its quota is");
    assert_eq!(found.entity[0].name, None, "<default> is a null name");
    assert_eq!(found.entity[0].entity_type, "user");

    client
        .quotas_alter(&entity, &remove("producer_byte_rate"))
        .expect("remove the <default> quota");

    wait_for_quota(&mut client, &entity, "producer_byte_rate", None);
}

/// The serde shape the IPC contract pins.
#[test]
fn quota_entities_serialise_with_the_contract_field_names() {
    if !enabled() {
        return;
    }
    let mut client = client();
    let entity = test_user("serde");
    client
        .quotas_alter(
            &entity,
            &[QuotaOp {
                key: "consumer_byte_rate".into(),
                value: Some(2_097_152.0),
            }],
        )
        .expect("set");

    let found = wait_for_quota(
        &mut client,
        &entity,
        "consumer_byte_rate",
        Some(2_097_152.0),
    )
    .expect("the entity is listed once its quota is");
    let json = serde_json::to_value(&found).expect("serialise");
    assert_eq!(json["entity"][0]["entity_type"], "user");
    assert_eq!(
        json["entity"][0]["name"],
        found.entity[0].name.clone().unwrap()
    );
    assert_eq!(json["values"][0]["key"], "consumer_byte_rate");
    assert_eq!(json["values"][0]["value"], 2_097_152.0);

    client
        .quotas_alter(&entity, &remove("consumer_byte_rate"))
        .expect("clean up");
}

#[test]
fn an_unknown_quota_entity_type_is_refused_before_the_round_trip() {
    if !enabled() {
        return;
    }
    let err = client()
        .quotas_alter(
            &[QuotaEntityPart {
                entity_type: "users".into(),
                name: Some("alice".into()),
            }],
            &remove("producer_byte_rate"),
        )
        .expect_err("`users` is not an entity type")
        .to_string();
    assert!(err.contains("client-id"), "got {err}");
}

// ---------------------------------------------------------------------------
// Read-only enforcement (docs/ARCHITECTURE.md D5)
// ---------------------------------------------------------------------------

/// Through the one-shot entry points — the IPC surface — because that is where
/// the check has to happen BEFORE the network, and a `ClusterConnection` is
/// what carries the flag.
#[test]
fn read_only_refuses_every_mutating_call_in_core() {
    if !enabled() {
        return;
    }
    let conn = ClusterConnection::connect(local_profile(true)).expect("connect");

    let elect = protocol::elect_leaders(&conn, Some(TOPIC), None)
        .expect_err("read-only must refuse an election")
        .to_string();
    assert!(elect.contains("read-only"), "got {elect}");
    assert!(elect.contains("elect_leaders"), "got {elect}");

    let alter = protocol::reassign_alter(
        &conn,
        &[ReassignmentSpec {
            topic: TOPIC.into(),
            partition: 0,
            replicas: vec![BROKER],
        }],
    )
    .expect_err("read-only must refuse a reassignment")
    .to_string();
    assert!(alter.contains("read-only"), "got {alter}");
    assert!(alter.contains("reassign_alter"), "got {alter}");

    let cancel = protocol::reassign_cancel(
        &conn,
        &[TopicPartition {
            topic: TOPIC.into(),
            partition: 0,
        }],
    )
    .expect_err("read-only must refuse a cancel")
    .to_string();
    assert!(cancel.contains("read-only"), "got {cancel}");
    assert!(cancel.contains("reassign_cancel"), "got {cancel}");

    let quota = protocol::quotas_alter(
        &conn,
        &test_user("read-only"),
        &remove("producer_byte_rate"),
    )
    .expect_err("read-only must refuse a quota change")
    .to_string();
    assert!(quota.contains("read-only"), "got {quota}");
    assert!(quota.contains("quotas_alter"), "got {quota}");
}

/// The other half of D5: read-only must not mean "cannot look".
#[test]
fn read_only_still_allows_every_read() {
    if !enabled() {
        return;
    }
    let conn = ClusterConnection::connect(local_profile(true)).expect("connect");

    assert_eq!(
        protocol::quorum_describe(&conn)
            .expect("describe")
            .leader_id,
        BROKER
    );
    assert_eq!(protocol::reassign_list(&conn, None).expect("list"), vec![]);
    protocol::quotas_list(&conn).expect("list quotas");
}
