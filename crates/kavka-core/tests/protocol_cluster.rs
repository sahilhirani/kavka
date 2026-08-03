//! The two things a single broker structurally cannot test: a partition that
//! actually MOVES, and a leader election that actually ELECTS.
//!
//! Run:  docker compose -f dev/docker-compose.cluster.yml up -d --wait
//!       KAVKA_CLUSTER_IT=1 cargo test -p kavka-core --features kafka-ssl --test protocol_cluster
//!       docker compose -f dev/docker-compose.cluster.yml down -v
//!
//! Gated on `KAVKA_CLUSTER_IT` rather than `KAVKA_IT` so the ordinary suite —
//! and CI's default path — never waits on a three-node cluster. On
//! dev/docker-compose.yml every reassignment is a no-op and every election
//! answers ELECTION_NOT_NEEDED, which is worth asserting (see
//! `protocol_local.rs`) but is the opposite of what these two features are for.
//!
//! Each test owns its topic, named after the test and the process id, and
//! deletes it on the way out — so a run leaves the cluster as it found it and
//! two runs cannot collide.
#![cfg(feature = "kafka")]

use kavka_core::admin;
use kavka_core::connection::ClusterConnection;
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};
use kavka_core::protocol::{ProtocolClient, ReassignmentSpec, ReassignmentState};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long to let the controller finish a move before declaring it stuck.
/// Generous: these are empty partitions, so the work is metadata only, but a
/// cold CI runner can be slow to schedule three JVMs.
const SETTLE: Duration = Duration::from_secs(30);

/// One cluster, shared by every test in this binary — and cargo runs them on
/// parallel threads. That is fine for tests that only touch their own topic,
/// but `elect_leaders(None, ..)` is CLUSTER-WIDE by definition: it reaches into
/// whatever topic another test is halfway through moving, elects a leader
/// there, and makes both tests report each other's work as their own. So they
/// run one at a time. (Same reasoning as the keychain lock in
/// `src/connection.rs`: the resource under test is global, so the tests are
/// serialized rather than the assertions weakened.)
static CLUSTER: Mutex<()> = Mutex::new(());

/// Poison-tolerant: the guarded state is a Kafka cluster, which a failed
/// assertion on another thread cannot corrupt, and a poisoned lock would turn
/// one failure into five.
fn cluster_lock() -> MutexGuard<'static, ()> {
    CLUSTER.lock().unwrap_or_else(|e| e.into_inner())
}

fn cluster_profile() -> ConnectionProfile {
    ConnectionProfile {
        id: "it-protocol-cluster".into(),
        name: "local docker cluster".into(),
        environment: Environment::Dev,
        bootstrap_servers: std::env::var("KAVKA_TEST_CLUSTER_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:19092,localhost:19093,localhost:19094".into())
            .split(',')
            .map(str::to_string)
            .collect(),
        auth: AuthConfig::Plaintext,
        read_only: false,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
        wasm_serdes: Vec::new(),
    }
}

fn enabled() -> bool {
    if std::env::var("KAVKA_CLUSTER_IT").is_err() {
        eprintln!("skipped: set KAVKA_CLUSTER_IT=1 with dev/docker-compose.cluster.yml running");
        return false;
    }
    true
}

/// A topic this test alone owns, deleted by [`Topic`]'s `Drop`.
struct Topic {
    name: String,
    conn: ClusterConnection,
}

impl Topic {
    fn create(label: &str, partitions: u32, replication_factor: u16) -> Self {
        let name = format!("kavka-it-{label}-{}", std::process::id());
        let conn = ClusterConnection::connect(cluster_profile()).expect("connect");
        // A leftover from an interrupted run would have the wrong shape.
        let _ = admin::delete_topic(&conn, &name);
        admin::create_topic(&conn, &name, partitions, replication_factor, &[])
            .unwrap_or_else(|e| panic!("creating {name}: {e}"));
        Self { name, conn }
    }

    /// This partition, as the cluster currently sees it.
    ///
    /// Retried, deliberately: `topic_detail` reads each partition's watermarks
    /// from its leader, so a describe issued while leadership is moving answers
    /// NOT_LEADER_FOR_PARTITION — which is a transient of the very operation
    /// these tests perform, not a failure of it.
    fn partition(&self, partition: i32) -> admin::PartitionDetail {
        let deadline = Instant::now() + SETTLE;
        loop {
            let outcome = admin::topic_detail(&self.conn, &self.name).map(|detail| {
                detail
                    .partitions
                    .into_iter()
                    .find(|p| p.partition == partition)
            });
            match outcome {
                Ok(Some(found)) => return found,
                Ok(None) => assert!(
                    Instant::now() < deadline,
                    "{}-{partition} never appeared",
                    self.name
                ),
                Err(e) => assert!(Instant::now() < deadline, "describing {}: {e}", self.name),
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Waits until `check` holds of the partition, so a test asserts an
    /// outcome rather than a sleep.
    fn wait_for(
        &self,
        partition: i32,
        what: &str,
        check: impl Fn(&admin::PartitionDetail) -> bool,
    ) -> admin::PartitionDetail {
        let deadline = Instant::now() + SETTLE;
        loop {
            let detail = self.partition(partition);
            if check(&detail) {
                return detail;
            }
            assert!(
                Instant::now() < deadline,
                "{}-{partition} never {what} (last seen: {detail:?})",
                self.name
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Topic {
    fn drop(&mut self) {
        let _ = admin::delete_topic(&self.conn, &self.name);
    }
}

fn client() -> ProtocolClient {
    ProtocolClient::connect(&cluster_profile()).expect("protocol connect")
}

/// Waits for the cluster to report nothing in flight, and returns how many
/// times it saw the move ITSELF — so the test can say whether it observed the
/// in-progress state or only its aftermath.
fn drain(client: &mut ProtocolClient, topic: &str) -> usize {
    let deadline = Instant::now() + SETTLE;
    let mut seen_in_progress = 0;
    loop {
        let in_flight = client.reassign_list(Some(topic)).expect("list");
        if in_flight.is_empty() {
            return seen_in_progress;
        }
        seen_in_progress += 1;
        assert!(
            Instant::now() < deadline,
            "the reassignment never drained: {in_flight:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ---------------------------------------------------------------------------

/// Three voters, one leader — the shape a single node cannot show.
#[test]
fn quorum_describe_sees_all_three_voters() {
    if !enabled() {
        return;
    }
    let _cluster = cluster_lock();
    let quorum = client().quorum_describe().expect("describe quorum");

    let mut voters: Vec<i32> = quorum.voters.iter().map(|v| v.replica_id).collect();
    voters.sort_unstable();
    assert_eq!(voters, vec![1, 2, 3], "got {:?}", quorum.voters);
    assert!(
        (1..=3).contains(&quorum.leader_id),
        "leader {} is not one of the voters",
        quorum.leader_id
    );
    assert!(quorum.high_watermark > 0);

    // The followers must have fetch ages; the leader has none for itself.
    let with_age = quorum
        .voters
        .iter()
        .filter(|v| v.last_fetch_age_ms.is_some())
        .count();
    assert!(
        with_age >= 2,
        "the two followers should have fetch timestamps: {:?}",
        quorum.voters
    );
}

/// A REAL move: one partition, one replica, relocated to a different broker.
#[test]
fn a_partition_is_reassigned_onto_a_different_broker() {
    if !enabled() {
        return;
    }
    let _cluster = cluster_lock();
    let topic = Topic::create("reassign", 1, 1);
    let mut client = client();

    let before = topic.partition(0);
    assert_eq!(before.replicas.len(), 1, "{before:?}");
    let from = before.replicas[0];
    let to = (1..=3).find(|id| *id != from).expect("another broker");

    let results = client
        .reassign_alter(&[ReassignmentSpec {
            topic: topic.name.clone(),
            partition: 0,
            replicas: vec![to],
        }])
        .expect("alter reassignments");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].error, None, "{results:?}");

    drain(&mut client, &topic.name);
    let after = topic.wait_for(0, "moved", |p| p.replicas == vec![to]);
    assert_eq!(after.replicas, vec![to], "the partition did not move");
    assert_eq!(after.leader, to, "the new replica must lead it");
    assert_ne!(from, to);
}

/// THE NAMED-TOPIC LIST, against a move that is really in flight.
///
/// This is the half `protocol_local.rs` structurally cannot reach: on one
/// broker every reassignment is a no-op, so an empty answer is the correct
/// answer and a `list` that always answered `[]` would look right. It always
/// did answer `[]` for a named topic — the request carried an EMPTY
/// `PartitionIndexes` array, and Kafka's `ReplicationControlManager` iterates
/// that array, so it asked about none of the topic's partitions. Here there is
/// something to see, and the assertion is that the named form sees it.
///
/// Six partitions rather than one: the controller finishes each separately, so
/// six of them make "all of it completed inside a single round trip" the
/// implausible case rather than the coin flip it would be with one.
#[test]
fn a_move_in_flight_is_visible_through_the_named_topic_list() {
    if !enabled() {
        return;
    }
    let _cluster = cluster_lock();
    let topic = Topic::create("list-named", 6, 1);
    let mut client = client();

    let mut targets = Vec::new();
    let mut specs = Vec::new();
    for partition in 0..6 {
        let from = topic.partition(partition).replicas[0];
        let to = (1..=3).find(|id| *id != from).expect("another broker");
        targets.push(to);
        specs.push(ReassignmentSpec {
            topic: topic.name.clone(),
            partition,
            replicas: vec![to],
        });
    }
    let results = client.reassign_alter(&specs).expect("alter reassignments");
    assert!(
        results.iter().all(|r| r.error.is_none()),
        "the cluster refused the plan: {results:?}"
    );

    // Polled tight, starting immediately: the first poll goes out one round
    // trip after the acceptance, which is inside the window where the new
    // replicas are still catching up.
    let deadline = Instant::now() + SETTLE;
    let mut seen: Vec<ReassignmentState> = Vec::new();
    loop {
        let in_flight = client
            .reassign_list(Some(&topic.name))
            .expect("list the named topic");
        for state in &in_flight {
            assert_eq!(
                state.topic, topic.name,
                "the named-topic list answered about another topic: {state:?}"
            );
        }
        if in_flight.is_empty() {
            break;
        }
        if seen.is_empty() {
            seen = in_flight;
        }
        assert!(
            Instant::now() < deadline,
            "the reassignment never drained: {seen:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        !seen.is_empty(),
        "no poll of the NAMED topic ever saw the move — six partitions changing broker cannot \
         all complete inside one round trip, so this is the named form asking about no \
         partitions at all"
    );
    // And what it saw has to be the move that was asked for, not some other
    // shape of answer that happens to be non-empty.
    for state in &seen {
        let target = targets[state.partition as usize];
        assert!(
            state.adding.contains(&target) || state.replicas.contains(&target),
            "{state:?} is not the move to broker {target} that was requested"
        );
    }

    // The cluster-wide form agrees about the end state.
    assert!(client.reassign_list(None).expect("list all").is_empty());
    for partition in 0..6 {
        let target = targets[partition as usize];
        topic.wait_for(partition, "moved", |p| p.replicas == vec![target]);
    }
}

/// Cancelling a move. Two replicas are added at once so the move has enough
/// work to still be in flight when the cancel arrives — but the assertion is
/// the END STATE, not the timing: whether the cancel caught it mid-flight or
/// the move had already finished, the cluster must end up settled with no
/// reassignment outstanding.
#[test]
fn a_reassignment_can_be_cancelled_and_the_cluster_settles() {
    if !enabled() {
        return;
    }
    let _cluster = cluster_lock();
    let topic = Topic::create("cancel", 1, 1);
    let mut client = client();
    let from = topic.partition(0).replicas[0];
    let targets: Vec<i32> = (1..=3).filter(|id| *id != from).collect();

    client
        .reassign_alter(&[ReassignmentSpec {
            topic: topic.name.clone(),
            partition: 0,
            replicas: targets.clone(),
        }])
        .expect("alter");

    let results = client
        .reassign_cancel(&[kavka_core::protocol::TopicPartition {
            topic: topic.name.clone(),
            partition: 0,
        }])
        .expect("cancel");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(
        results[0].error, None,
        "a cancel is benign whether or not there was anything to cancel: {results:?}"
    );

    drain(&mut client, &topic.name);
    // Either outcome is legitimate — cancelled back to `from`, or the move
    // completed first. What must NOT happen is being stuck between the two.
    let settled = topic.partition(0);
    assert!(
        settled.replicas == vec![from] || settled.replicas == targets,
        "left mid-move: {settled:?}"
    );
    assert!(client
        .reassign_list(Some(&topic.name))
        .expect("list")
        .is_empty());
}

/// A REAL election. Reordering a partition's replicas makes the SECOND broker
/// the preferred leader while the FIRST keeps the leadership — Kafka does not
/// re-elect on a reorder, and the compose file turns off the background
/// rebalancer that otherwise would. `elect_leaders` then has to move it, and
/// the assertion is that the leader changed to the broker it was not.
#[test]
fn elect_leaders_moves_leadership_to_the_preferred_replica() {
    if !enabled() {
        return;
    }
    let _cluster = cluster_lock();
    let topic = Topic::create("elect", 1, 2);
    let mut client = client();

    let before = topic.partition(0);
    assert_eq!(before.replicas.len(), 2, "{before:?}");
    assert_eq!(
        before.leader, before.replicas[0],
        "a fresh topic leads with its preferred replica: {before:?}"
    );
    let reversed: Vec<i32> = before.replicas.iter().rev().copied().collect();

    client
        .reassign_alter(&[ReassignmentSpec {
            topic: topic.name.clone(),
            partition: 0,
            replicas: reversed.clone(),
        }])
        .expect("reorder the replicas");
    drain(&mut client, &topic.name);
    let reordered = topic.wait_for(0, "reordered", |p| p.replicas == reversed);
    // Both replicas are in the ISR, so both are electable.
    assert_eq!(reordered.isr.len(), 2, "{reordered:?}");

    // If the reorder alone already moved leadership there is nothing left for
    // the election to prove, so say so rather than passing quietly.
    let expected = reversed[0];
    assert_eq!(
        reordered.leader, before.leader,
        "the reorder itself moved the leader, so this test no longer exercises an election"
    );
    assert_ne!(
        reordered.leader, expected,
        "the preferred replica is already the leader"
    );

    let results = client
        .elect_leaders(Some(&topic.name), None)
        .expect("elect leaders");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(
        results[0].error, None,
        "an election that had work to do must not report an error: {results:?}"
    );

    let elected = topic.wait_for(0, "changed leader", |p| p.leader == expected);
    assert_eq!(
        elected.leader, expected,
        "ElectLeaders did not move leadership to the preferred replica"
    );
}

/// The cluster-wide form, on a cluster where at least one partition genuinely
/// needs electing: the result must name that partition and report no error.
#[test]
fn electing_across_the_cluster_reports_the_partitions_it_moved() {
    if !enabled() {
        return;
    }
    let _cluster = cluster_lock();
    let topic = Topic::create("elect-all", 1, 2);
    let mut client = client();

    let before = topic.partition(0);
    let reversed: Vec<i32> = before.replicas.iter().rev().copied().collect();
    client
        .reassign_alter(&[ReassignmentSpec {
            topic: topic.name.clone(),
            partition: 0,
            replicas: reversed.clone(),
        }])
        .expect("reorder");
    drain(&mut client, &topic.name);
    topic.wait_for(0, "reordered", |p| p.replicas == reversed);

    let results = client.elect_leaders(None, None).expect("elect everywhere");
    let ours = results
        .iter()
        .find(|r| r.topic == topic.name)
        .unwrap_or_else(|| panic!("{} is missing from {results:?}", topic.name));
    assert_eq!(ours.partition, 0);
    assert_eq!(ours.error, None, "{ours:?}");
    // Deliberately no assertion about the OTHER rows: this call elects across
    // the whole cluster, so what else appears here is whatever else happens to
    // exist on it — including internal topics — and a test that asserted on
    // those would be asserting about its neighbours.

    topic.wait_for(0, "changed leader", |p| p.leader == reversed[0]);
}
