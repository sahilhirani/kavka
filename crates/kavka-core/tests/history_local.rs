//! The lag sampler against the local dev cluster (dev/docker-compose.yml).
//!
//! The unit tests in `src/history.rs` drive the store with synthetic samples;
//! this is the other half — that what a real broker answers actually becomes
//! stored history, and that the lag written to disk is the lag the groups screen
//! shows.
//!
//! Run:  docker compose -f dev/docker-compose.yml up -d --wait
//!       KAVKA_IT=1 cargo test -p kavka-core --features kafka --test history_local
#![cfg(feature = "kafka")]

use kavka_core::admin;
use kavka_core::connection::ClusterConnection;
use kavka_core::history::{self, HistoryStore};
use kavka_core::profiles::{AuthConfig, ConnectionProfile, Environment};
use std::path::PathBuf;

fn enabled() -> bool {
    if std::env::var("KAVKA_IT").is_err() {
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
        return false;
    }
    true
}

fn local_profile() -> ConnectionProfile {
    ConnectionProfile {
        id: "it-history".into(),
        name: "local docker".into(),
        environment: Environment::Dev,
        bootstrap_servers: vec![
            std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
        ],
        auth: AuthConfig::Plaintext,
        read_only: false,
        schema_registry: None,
        connect_clusters: Vec::new(),
        metrics_endpoint: None,
        sampler_interval_ms: None,
    }
}

/// Scratch history dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("kavka-history-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }

    fn store(&self) -> HistoryStore {
        HistoryStore::open(history::store_path(&self.0, "it-history")).expect("open store")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The whole path, end to end: what the coordinator says -> samples -> redb ->
/// the query a chart draws from.
#[test]
fn a_real_groups_offsets_become_stored_history() {
    if !enabled() {
        return;
    }
    let conn = ClusterConnection::connect(local_profile()).expect("connect");
    let Some(group) = admin::groups_list(&conn).expect("groups_list").pop() else {
        eprintln!(
            "skipped: this cluster has no consumer groups — run the admin or consume \
             integration tests first"
        );
        return;
    };

    let detail = admin::group_detail(&conn, &group.group_id).expect("group_detail");
    let ts_ms = history::now_ms();
    let batch = history::samples_from_detail(&detail, ts_ms);
    if batch.is_empty() {
        eprintln!("skipped: {} has no offsets to sample", group.group_id);
        return;
    }

    let dir = TempDir::new("roundtrip");
    let store = dir.store();
    assert_eq!(store.append_samples(&batch, ts_ms).unwrap(), batch.len());

    // Read back through the query the UI uses, wide enough that nothing is
    // downsampled away.
    let back = store
        .query(&group.group_id, None, ts_ms - 1, ts_ms + 1, 1_000)
        .expect("query");
    assert_eq!(back.len(), batch.len());

    // The lag on disk is the lag the groups screen shows, for every partition.
    for stored in &back {
        let live = detail
            .offsets
            .iter()
            .find(|offset| offset.topic == stored.topic && offset.partition == stored.partition)
            .unwrap_or_else(|| {
                panic!("{}/{} is not in the detail", stored.topic, stored.partition)
            });
        assert_eq!(stored.committed, live.committed);
        assert_eq!(stored.end_offset, live.end_offset);
        assert_eq!(stored.lag, live.lag);
    }

    let windows = store.groups().expect("groups");
    let window = windows
        .iter()
        .find(|window| window.group_id == group.group_id)
        .expect("the sampled group is in the index");
    assert_eq!(window.first_ts_ms, ts_ms);
    assert_eq!(window.last_ts_ms, ts_ms);
}

/// One whole tick of the sampler, as the shell's loop calls it.
///
/// Skipped on a cluster carrying an unusual number of leftover groups: a tick
/// costs an OffsetFetch and a watermark lookup per partition per group, which
/// is the right cost in production and a slow test on a dev cluster that has
/// accumulated one group per previous integration run. (Sixty-six of them takes
/// about thirteen seconds; the cap is where that stops being a test and starts
/// being a wait.)
#[test]
fn one_tick_of_the_sampler_writes_what_the_cluster_says() {
    if !enabled() {
        return;
    }
    let conn = ClusterConnection::connect(local_profile()).expect("connect");
    let groups = admin::groups_list(&conn).expect("groups_list");
    if groups.len() > 100 {
        eprintln!(
            "skipped: {} consumer groups on this cluster — recreate it with \
             `docker compose -f dev/docker-compose.yml down -v` to run this one",
            groups.len()
        );
        return;
    }

    let dir = TempDir::new("tick");
    let store = dir.store();
    let tick = history::sample_groups(&conn, &store);
    let run = &tick.run;

    assert_eq!(run.errors, Vec::<String>::new(), "{run:?}");
    assert_eq!(run.error_summary(), None);
    assert_eq!(run.groups, groups.len());
    assert!(run.ts_ms > 0);
    // The tick hands its samples back for the alert evaluator, and they are the
    // same ones it wrote.
    assert_eq!(tick.samples.len(), run.written);
    assert!(tick.samples.iter().all(|sample| sample.ts_ms == run.ts_ms));

    // Everything the tick wrote is queryable, and every group in the index is
    // one the cluster actually named.
    let indexed = store.groups().expect("groups");
    for window in &indexed {
        assert!(
            groups.iter().any(|group| group.group_id == window.group_id),
            "{} is in the history but not on the cluster",
            window.group_id
        );
        assert_eq!(window.first_ts_ms, run.ts_ms);
    }
    let written: usize = indexed
        .iter()
        .map(|window| {
            store
                .query(&window.group_id, None, 0, i64::MAX, 10_000)
                .expect("query")
                .len()
        })
        .sum();
    assert_eq!(written, run.written);
}

/// A second tick extends the series rather than replacing it, and the store
/// survives being closed and reopened — the Phase 4 acceptance criterion, on a
/// real cluster's numbers.
#[test]
fn a_second_tick_extends_the_series_and_survives_a_reopen() {
    if !enabled() {
        return;
    }
    let conn = ClusterConnection::connect(local_profile()).expect("connect");
    let Some(group) = admin::groups_list(&conn).expect("groups_list").pop() else {
        eprintln!("skipped: this cluster has no consumer groups");
        return;
    };

    let dir = TempDir::new("reopen");
    let mut ticks = 0;
    for step in 0..2 {
        let store = dir.store();
        let detail = admin::group_detail(&conn, &group.group_id).expect("group_detail");
        // Two ticks a second apart, so they land in different keys without the
        // test waiting on a real interval.
        let batch = history::samples_from_detail(&detail, history::now_ms() + step * 1_000);
        if batch.is_empty() {
            eprintln!("skipped: {} has no offsets to sample", group.group_id);
            return;
        }
        store
            .append_samples(&batch, history::now_ms())
            .expect("append");
        ticks += 1;
        // The store is dropped here — the next iteration reopens the file.
    }
    assert_eq!(ticks, 2);

    let store = dir.store();
    let back = store
        .query(&group.group_id, None, 0, i64::MAX, 10_000)
        .expect("query");
    let one_partition: Vec<_> = back
        .iter()
        .filter(|sample| sample.topic == back[0].topic && sample.partition == back[0].partition)
        .collect();
    assert_eq!(
        one_partition.len(),
        2,
        "each tick is its own point on the series"
    );
    assert!(one_partition[0].ts_ms < one_partition[1].ts_ms);
}
