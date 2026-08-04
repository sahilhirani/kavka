//! Share groups (KIP-932) against the local dev cluster
//! (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl --test share_local
//!
//! # What these can and cannot reach
//!
//! Everything here is a live round trip through ListGroups v5,
//! ShareGroupDescribe v1, DescribeShareGroupOffsets v0 and the FindCoordinator
//! v6 lookup that routes the last two — so the frames, the negotiated versions
//! and the honest errors are all checked against a real Kafka rather than
//! against this build's own idea of them.
//!
//! **What is NOT here is a share group with anything in it, and that is a hard
//! limit rather than an omission.** A share group only exists once a client
//! JOINS one, which needs the ShareGroupHeartbeat/ShareFetch/ShareAcknowledge
//! protocol — and librdkafka does not implement it, so rdkafka cannot join one,
//! Kavka cannot join one, and neither can any Rust client at the time of
//! writing. `kafka-share-consumer.sh` in the broker image could create one, but
//! a test that shells into a container to manufacture its subject is testing
//! docker exec.
//!
//! So the split is deliberate:
//!
//! - **Populated groups** — members, assignments, start offsets, the state
//!   machine — are covered by golden-frame unit tests in
//!   `src/protocol/share.rs`, both directions: canned responses built with the
//!   same encoder the requests use, and request bodies asserted byte for byte
//!   against the Kafka 4.1 message schemas.
//! - **The live half** — that a real broker accepts those exact bytes, answers
//!   them, and that its "no such group" and "feature is off" replies become the
//!   sentences the contract requires — is here.
//!
//! # The feature flag
//!
//! dev/docker-compose.yml upgrades `share.version` to 1 (see the comment
//! there); without that, every call in this file would answer with the
//! feature-is-off error instead. That error's mapping is unit-tested from the
//! code the broker actually sends — UNSUPPORTED_VERSION (35), observed from
//! this same image before the compose file enabled the feature.
#![cfg(feature = "kafka")]

use kavka_core::connection::ClusterConnection;
use kavka_core::profiles::{AuthConfig, ConnectionProfile};
use kavka_core::protocol::{self, ProtocolClient};

fn local_profile(read_only: bool) -> ConnectionProfile {
    ConnectionProfile {
        id: "it-share".into(),
        name: "local docker".into(),
        environment: "dev".into(),
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        read_only,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
        wasm_serdes: Vec::new(),
    }
}

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

/// A group id nothing else can be using: named for the test, the process, and
/// the fact that it is not supposed to exist.
fn absent_group(label: &str) -> String {
    format!(
        "kavka-it-no-such-share-group-{label}-{}",
        std::process::id()
    )
}

// ---------------------------------------------------------------------------
// ListGroups (16) v5 — the type filter, and the fan-out
// ---------------------------------------------------------------------------

/// The whole read path in one assertion.
///
/// An empty list is the *correct* answer on this cluster — nothing here can
/// join a share group — and it is only worth asserting because of everything
/// that has to work to produce it: ListGroups negotiated at v5 (a broker that
/// rejected the version, or the `TypesFilter` field, would error rather than
/// answer), the fan-out reaching every broker in the metadata table, and the
/// feature gate NOT firing, which is what proves the compose file's
/// `share.version=1` upgrade took and that this build read it out of the
/// ApiVersions reply.
#[test]
fn share_groups_list_is_empty_and_that_is_an_answer_rather_than_a_refusal() {
    if !enabled() {
        return;
    }
    let groups = client()
        .share_groups_list()
        .expect("a cluster with share.version=1 lists share groups");
    assert_eq!(
        groups,
        vec![],
        "nothing in this suite can join a share group, so there must be none"
    );
}

/// If the compose file's feature upgrade had not run, THIS is the error every
/// call here would carry — so the test that it does not is worth stating as
/// itself. A regression in the feature-level decoding shows up here first.
#[test]
fn the_feature_is_on_so_nothing_reports_it_as_off() {
    if !enabled() {
        return;
    }
    let mut client = client();
    let listed = client.share_groups_list();
    let described = client.share_group_detail(&absent_group("feature"));

    for outcome in [listed.map(|_| ()), described.map(|_| ())] {
        if let Err(err) = outcome {
            assert!(
                !err.to_string().contains("share groups enabled"),
                "the dev cluster should have share.version=1 — run `docker compose -f \
                 dev/docker-compose.yml up -d` to apply it: {err}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// ShareGroupDescribe (77) v1 + FindCoordinator (10) v6
// ---------------------------------------------------------------------------

/// A group that does not exist has to fail by NAME, not with a shrug.
///
/// This is the live half of the error contract: FindCoordinator resolves (Kafka
/// hashes the id onto a coordinator whether or not the group exists), the
/// describe goes to that broker, and the broker answers GROUP_ID_NOT_FOUND —
/// which this build turns into a sentence that says what a share group is and
/// when it appears.
#[test]
fn describing_a_share_group_that_does_not_exist_names_it() {
    if !enabled() {
        return;
    }
    let group = absent_group("describe");
    let err = client()
        .share_group_detail(&group)
        .expect_err("this group does not exist")
        .to_string();

    assert!(err.contains(&group), "got {err}");
    assert!(err.contains("no share group named"), "got {err}");
    // The failure must not be the version negotiation or the feature gate
    // wearing a different hat: those are different situations with different
    // fixes, and this one is neither.
    assert!(!err.contains("share groups enabled"), "got {err}");
    assert!(!err.contains("no common version"), "got {err}");
}

/// An empty group id is refused by Kafka itself (INVALID_GROUP_ID), and that
/// answer has to arrive with Kafka's own name on it rather than as a decode
/// failure — this is the path where a wrong request encoding would show up as
/// something incoherent instead.
#[test]
fn an_invalid_group_id_comes_back_with_kafkas_own_name_for_it() {
    if !enabled() {
        return;
    }
    let err = client()
        .share_group_detail("")
        .expect_err("the empty string is not a group id")
        .to_string();
    assert!(
        err.contains("INVALID_GROUP_ID") || err.contains("no share group named"),
        "got {err}"
    );
    assert!(!err.contains("could not decode"), "got {err}");
}

// ---------------------------------------------------------------------------
// Connection reuse and health
// ---------------------------------------------------------------------------

/// The share calls run over the SAME socket the client already holds, on a
/// single-broker cluster: the fan-out and the coordinator lookup both resolve
/// to this connection's own endpoint, and reusing it is what keeps a share-group
/// screen from paying a TCP connect and a full SASL exchange per refresh.
///
/// The proof is indirect but firm — a Kafka-level refusal leaves the connection
/// healthy, so if every call had dialled a fresh socket the assertions below
/// would still pass; what they pin is that repeated calls on one client keep
/// working and that a broker-level "no such group" never condemns it.
#[test]
fn one_client_serves_repeated_share_calls_over_a_single_connection() {
    if !enabled() {
        return;
    }
    let mut client = client();
    for _ in 0..3 {
        client.share_groups_list().expect("list share groups");
    }
    let _ = client
        .share_group_detail(&absent_group("health"))
        .expect_err("no such group");
    assert!(
        client.is_healthy(),
        "a GROUP_ID_NOT_FOUND arrives in a complete frame — it is not a transport failure"
    );
    client
        .share_groups_list()
        .expect("the same client still works after a broker-level refusal");
    // And it interleaves with the rest of the module's calls on that one socket.
    client.quorum_describe().expect("describe quorum");
    client.share_groups_list().expect("list share groups again");
    assert!(client.is_healthy());
}

// ---------------------------------------------------------------------------
// Read-only (docs/ARCHITECTURE.md D5)
// ---------------------------------------------------------------------------

/// Both share-group calls READ. Read-only mode must not mean "cannot look".
#[test]
fn read_only_still_allows_both_share_group_reads() {
    if !enabled() {
        return;
    }
    let conn = ClusterConnection::connect(local_profile(true)).expect("connect");

    assert_eq!(
        protocol::share_groups_list(&conn).expect("list share groups"),
        vec![]
    );
    // The label carries no hyphenated "read-only" on purpose: it lands in the
    // group id, and the assertion below is about the word appearing in the
    // MESSAGE.
    let err = protocol::share_group_detail(&conn, &absent_group("ro"))
        .expect_err("the group still does not exist")
        .to_string();
    assert!(
        !err.contains("read-only"),
        "describing a share group is a read: {err}"
    );
}
