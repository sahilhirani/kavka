//! ACL read/write against the local dev cluster (dev/docker-compose.yml).
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl --test acl_local
//!
//! These need an authorizer, which is why the compose file loads
//! `StandardAuthorizer` with `User:ANONYMOUS` as a superuser: without one the
//! broker answers SECURITY_DISABLED to every ACL call, and with one that could
//! deny anything the rest of the suite would start failing for reasons that
//! have nothing to do with what it tests.
//!
//! Every test owns a principal of its own (`User:kavka-it-*`) and a resource
//! name of its own, so the file is safe to run with cargo's default
//! parallelism, and each one purges its principal before it starts — a run
//! killed halfway through must not poison the next one.
#![cfg(feature = "kafka")]

use kavka_core::acl::{acls_create, acls_delete, acls_list, AclBinding, AclFilter};
use kavka_core::connection::ClusterConnection;
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};
use std::time::{Duration, Instant};

/// How long to let a write reach the broker's authorizer. CreateAcls answers
/// once the controller has committed the record; the broker serves DescribeAcls
/// from its own replay of that log, so the two are a metadata propagation apart
/// and a bare read-after-write is a race rather than a test.
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
        id: "it-acl".into(),
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

fn binding(
    principal: &str,
    resource_name: &str,
    pattern_type: &str,
    operation: &str,
    permission: &str,
    host: &str,
) -> AclBinding {
    AclBinding {
        resource_type: "topic".into(),
        resource_name: resource_name.into(),
        pattern_type: pattern_type.into(),
        principal: principal.into(),
        host: host.into(),
        operation: operation.into(),
        permission: permission.into(),
    }
}

/// Everything for one principal, whatever it is — the filter a cleanup uses,
/// and the one that proves a delete left nothing behind.
fn everything_for(principal: &str) -> AclBinding {
    AclBinding {
        resource_type: "any".into(),
        resource_name: String::new(),
        pattern_type: "any".into(),
        principal: principal.into(),
        host: String::new(),
        operation: "any".into(),
        permission: "any".into(),
    }
}

fn by_principal(principal: &str) -> AclFilter {
    AclFilter {
        principal: Some(principal.into()),
        ..AclFilter::default()
    }
}

/// Leaves the principal with no ACLs, whatever a previous run did.
fn purge(conn: &ClusterConnection, principal: &str) {
    acls_delete(conn, &everything_for(principal)).expect("purge");
}

/// Polls `probe` until `settled` accepts what it returns, then returns it.
/// Panics with the last reading rather than a bare timeout, because "what did
/// it actually see" is the whole diagnostic.
fn until(
    what: &str,
    mut probe: impl FnMut() -> Vec<AclBinding>,
    settled: impl Fn(&[AclBinding]) -> bool,
) -> Vec<AclBinding> {
    let deadline = Instant::now() + SETTLE;
    loop {
        let seen = probe();
        if settled(&seen) {
            return seen;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; last saw {seen:#?}"
        );
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[test]
fn a_literal_topic_acl_is_created_listed_and_deleted() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let principal = "User:kavka-it-literal";
    let topic = "it-acl-literal";
    purge(&conn, principal);

    let acl = binding(principal, topic, "literal", "read", "allow", "*");
    acls_create(&conn, std::slice::from_ref(&acl)).expect("create");

    // Filtered: the exact row comes back, in the vocabulary it was written in.
    let filtered = until(
        "the new ACL to appear under its principal",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        |seen| seen.len() == 1,
    );
    assert_eq!(filtered, vec![acl.clone()]);

    // Unfiltered: the same row is in the cluster-wide listing. Asserted by
    // containment, not equality — other tests in this file own ACLs too.
    let all = acls_list(&conn, &AclFilter::default()).expect("list all");
    assert!(all.contains(&acl), "cluster listing missed it: {all:#?}");

    // Narrowing by resource as well as principal finds the same one row.
    let by_resource = acls_list(
        &conn,
        &AclFilter {
            resource_type: Some("topic".into()),
            resource_name: Some(topic.into()),
            principal: Some(principal.into()),
        },
    )
    .expect("list by resource");
    assert_eq!(by_resource, vec![acl.clone()]);

    // Delete reports what it removed, not what it was asked to remove.
    let deleted = acls_delete(&conn, &acl).expect("delete");
    assert_eq!(deleted, vec![acl.clone()]);

    let gone = until(
        "the deleted ACL to disappear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        <[AclBinding]>::is_empty,
    );
    assert!(gone.is_empty());
}

#[test]
fn a_prefixed_pattern_round_trips_as_prefixed() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let principal = "User:kavka-it-prefixed";
    let prefix = "it-acl-prefix-";
    purge(&conn, principal);

    let acl = binding(principal, prefix, "prefixed", "write", "allow", "*");
    acls_create(&conn, std::slice::from_ref(&acl)).expect("create");

    let seen = until(
        "the prefixed ACL to appear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        |seen| seen.len() == 1,
    );
    // The pattern type is the whole point: a prefixed rule read back as
    // literal would be a rule over one topic named "it-acl-prefix-" rather
    // than over every topic starting with it.
    assert_eq!(seen[0].pattern_type, "prefixed");
    assert_eq!(seen, vec![acl.clone()]);

    assert_eq!(acls_delete(&conn, &acl).expect("delete"), vec![acl.clone()]);
    until(
        "the prefixed ACL to disappear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        <[AclBinding]>::is_empty,
    );
}

#[test]
fn a_deny_entry_round_trips_as_deny() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let principal = "User:kavka-it-deny";
    let topic = "it-acl-deny";
    purge(&conn, principal);

    // A host as well as a permission: both are fields Kafka will happily
    // default if they go over the wire wrong, so both are asserted.
    let acl = binding(principal, topic, "literal", "describe", "deny", "10.0.4.19");
    acls_create(&conn, std::slice::from_ref(&acl)).expect("create");

    let seen = until(
        "the deny entry to appear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        |seen| seen.len() == 1,
    );
    assert_eq!(seen[0].permission, "deny");
    assert_eq!(seen[0].host, "10.0.4.19");
    assert_eq!(seen[0].operation, "describe");
    assert_eq!(seen, vec![acl.clone()]);

    assert_eq!(acls_delete(&conn, &acl).expect("delete"), vec![acl.clone()]);
    until(
        "the deny entry to disappear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        <[AclBinding]>::is_empty,
    );
}

/// One filter, several matches: the return value has to be the set Kafka
/// removed, which is the only way an operator learns that a broad filter did
/// more than they meant.
#[test]
fn a_wide_delete_filter_returns_every_binding_it_removed() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let principal = "User:kavka-it-wide";
    purge(&conn, principal);

    let mut written = vec![
        binding(principal, "it-acl-wide-a", "literal", "read", "allow", "*"),
        binding(principal, "it-acl-wide-b", "literal", "write", "allow", "*"),
        binding(
            principal,
            "it-acl-wide-",
            "prefixed",
            "describe",
            "deny",
            "*",
        ),
    ];
    acls_create(&conn, &written).expect("create");
    until(
        "all three ACLs to appear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        |seen| seen.len() == written.len(),
    );

    let mut deleted = acls_delete(&conn, &everything_for(principal)).expect("delete");
    written.sort();
    deleted.sort();
    assert_eq!(deleted, written);

    until(
        "all three ACLs to disappear",
        || acls_list(&conn, &by_principal(principal)).expect("list"),
        <[AclBinding]>::is_empty,
    );
}

#[test]
fn read_only_refuses_every_mutating_acl_path() {
    if !integration() {
        return;
    }
    let conn = local_connection(true);

    let acl = binding(
        "User:kavka-it-readonly",
        "it-acl-readonly",
        "literal",
        "read",
        "allow",
        "*",
    );
    for err in [
        acls_create(&conn, std::slice::from_ref(&acl)).unwrap_err(),
        acls_delete(&conn, &acl).unwrap_err(),
    ] {
        assert!(err.to_string().contains("read-only"), "{err}");
    }

    // Reading is not a mutation, and a read-only connection must still work.
    acls_list(&conn, &AclFilter::default()).expect("list on a read-only connection");
}

/// A bad enum name is caught in Kavka's own vocabulary, before anything
/// reaches the cluster — the error names the value and lists the ones that
/// exist, rather than surfacing librdkafka's "Invalid operation".
#[test]
fn an_operation_kafka_does_not_have_is_refused_by_name() {
    if !integration() {
        return;
    }
    let conn = local_connection(false);
    let mut acl = binding(
        "User:kavka-it-bad",
        "it-acl-bad",
        "literal",
        "read",
        "allow",
        "*",
    );
    acl.operation = "publish".into();

    let err = acls_create(&conn, std::slice::from_ref(&acl))
        .unwrap_err()
        .to_string();
    assert!(err.contains("publish"), "{err}");
    assert!(err.contains("idempotent_write"), "{err}");
}
