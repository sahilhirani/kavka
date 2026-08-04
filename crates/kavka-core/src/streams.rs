//! Kafka Streams topology INFERENCE.
//!
//! Kafka does not publish a Streams topology. `KafkaStreams#describe()` exists
//! inside the application's own JVM and goes nowhere near the broker, so a tool
//! standing outside the cluster has exactly two artefacts to reason from:
//!
//! 1. **What the group's members subscribe to**, from the same
//!    [`crate::admin::group_detail`] the consumer-group screen already reads.
//! 2. **The names of the topics on the cluster.** Streams creates its internal
//!    topics as `<application.id>-<name>-repartition` and
//!    `<application.id>-<name>-changelog`, and that convention is a documented
//!    part of Kafka Streams rather than an implementation detail.
//!
//! From those two, one real fact follows: **a repartition topic is exactly the
//! boundary between two sub-topologies.** That is what repartitioning *is* — the
//! upstream sub-topology writes the topic, a downstream one reads it, and the
//! two run as separate task sets. So the shape of the graph below is not a
//! guess; the labels on it are.
//!
//! # Everything this cannot know, and why it is said out loud
//!
//! [`StreamsTopology::caveats`] is not decoration. A picture of a data flow is
//! the most believable thing a monitoring tool can draw, and this one is
//! assembled from topic names — so every gap between it and the real topology
//! is carried WITH it, in the same payload, rather than left to a footnote in a
//! document nobody opens. The caveats are generated per topology, and the ones
//! that only apply sometimes (an ambiguous stage order, a changelog nothing
//! matched) name the specific topics they are about.
//!
//! # Pure
//!
//! [`infer`] takes its two artefacts and returns the topology — no cluster, no
//! feature gate, no clock. Everything below it is exhaustively unit-tested
//! against synthetic applications, because the alternative is a test suite that
//! needs a running Kafka Streams app per topology shape.

use crate::admin::GroupMember;
use crate::connection::ClusterConnection;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Kafka Streams' own suffixes for the topics it creates.
const REPARTITION_SUFFIX: &str = "-repartition";
const CHANGELOG_SUFFIX: &str = "-changelog";

/// The marker a Kafka Streams consumer puts in its client id:
/// `<application.id>-<processId>-StreamThread-<n>-consumer`. It is what
/// separates a Streams application from a plain consumer group that happens to
/// have no internal topics.
const STREAM_THREAD_MARKER: &str = "StreamThread";

/// What a node in the graph IS. The vocabulary is fixed by the IPC contract;
/// serde writes these as `source_topic`, `sub_topology`, `repartition`,
/// `changelog` and `sink_topic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A topic the application reads that it did not create.
    SourceTopic,
    /// One sub-topology: everything between two repartition boundaries.
    SubTopology,
    /// `<app-id>-<name>-repartition` — the boundary itself.
    Repartition,
    /// `<app-id>-<store>-changelog` — a state store's backing topic.
    Changelog,
    /// A topic named after the application that is neither of the above, so
    /// probably something it writes. See the caveat this always produces.
    SinkTopic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyNode {
    /// Stable within one topology, and referenced by [`TopologyEdge`].
    pub id: String,
    pub kind: NodeKind,
    /// What to show: a topic's own name, a store name with the application's
    /// prefix and Kafka's suffix stripped off, or `Sub-topology N`.
    pub label: String,
    /// The Kafka topics this node stands for — empty for a sub-topology, which
    /// is not a topic. The full names live here so the label can be short
    /// without hiding what it means (docs/DESIGN.md §7 rule 3).
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamsTopology {
    /// Kafka Streams sets `group.id` to `application.id`, so this is the group
    /// id — see the caveat that says so.
    pub app_id: String,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
    /// Always `true`, and a field rather than a constant on purpose: it travels
    /// with the payload, so a UI cannot render this graph without having been
    /// handed the fact that it was deduced.
    pub inferred: bool,
    /// What this inference could not know, in the words of this particular
    /// application. Never empty for a topology with anything in it.
    pub caveats: Vec<String>,
}

impl StreamsTopology {
    /// A group that is not a Streams application: nothing to draw, and a
    /// sentence saying how that was decided.
    fn nothing(app_id: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            inferred: true,
            caveats: vec![format!(
                "Nothing about \"{app_id}\" looks like a Kafka Streams application: no member's \
                 client id carries a StreamThread, and this cluster has no \
                 \"{app_id}-…-repartition\" or \"{app_id}-…-changelog\" topics. A plain consumer \
                 group has no topology to show."
            )],
        }
    }
}

/// One of the application's own internal topics.
struct Internal {
    topic: String,
    /// The middle segment: `<app-id>-` and the suffix stripped off. For
    /// `wordcount-Counts-repartition` this is `Counts` — the STORE name, which
    /// is what lets a changelog be matched to the repartition topic of the same
    /// store (Kafka Streams derives both names from it).
    name: String,
}

/// Infers a topology from a group's subscriptions and the cluster's topic list.
///
/// `topics` is every topic on the cluster; only the ones named after this
/// application are looked at, so a changelog belonging to a DIFFERENT
/// application is never claimed by this one.
pub fn infer(group_id: &str, members: &[GroupMember], topics: &[String]) -> StreamsTopology {
    let prefix = format!("{group_id}-");
    let mut repartitions = internal_topics(topics, &prefix, REPARTITION_SUFFIX);
    let changelogs = internal_topics(topics, &prefix, CHANGELOG_SUFFIX);

    let subscribed: BTreeSet<&str> = members
        .iter()
        .flat_map(|member| member.assignments.iter())
        .map(|tp| tp.topic.as_str())
        .collect();

    // Sink candidates: named after the application, not one of Kafka's internal
    // shapes, and NOT something the group is reading — an input topic called
    // `<app-id>-input` is extremely common, and drawing it as both a source and
    // a sink would be a picture of a loop the application does not have. The
    // suffix test here is deliberately not this application's: a topic ending
    // in `-repartition` is SOMEBODY's internal topic, and calling it an output
    // would be a worse guess than saying nothing.
    let sinks: Vec<String> = topics
        .iter()
        .filter(|topic| topic.starts_with(&prefix))
        .filter(|topic| !topic.ends_with(REPARTITION_SUFFIX) && !topic.ends_with(CHANGELOG_SUFFIX))
        .filter(|topic| !subscribed.contains(topic.as_str()))
        .cloned()
        .collect();
    // A repartition topic the group is reading but the topic list did not
    // mention (a list fetched a moment earlier, say) still belongs here: the
    // subscription is the stronger evidence of the two.
    for topic in &subscribed {
        if let Some(name) = strip(topic, &prefix, REPARTITION_SUFFIX) {
            if !repartitions.iter().any(|r| r.topic == *topic) {
                repartitions.push(Internal {
                    topic: (*topic).to_string(),
                    name,
                });
            }
        }
    }
    repartitions.sort_by(|a, b| a.topic.cmp(&b.topic));

    let looks_like_streams = !repartitions.is_empty()
        || !changelogs.is_empty()
        || members
            .iter()
            .any(|member| member.client_id.contains(STREAM_THREAD_MARKER));
    if !looks_like_streams {
        return StreamsTopology::nothing(group_id);
    }

    // Source topics: subscribed, and not one of this application's own.
    let sources: Vec<&str> = subscribed
        .iter()
        .copied()
        .filter(|topic| {
            strip(topic, &prefix, REPARTITION_SUFFIX).is_none()
                && strip(topic, &prefix, CHANGELOG_SUFFIX).is_none()
        })
        .collect();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut caveats = Vec::new();

    // --- the stages -------------------------------------------------------
    // Stage 0 reads the source topics; each repartition topic starts one more.
    // That count is exact — a repartition topic IS a sub-topology boundary —
    // even though which stage writes which repartition topic is not.
    let stage_count = repartitions.len() + 1;
    for stage in 0..stage_count {
        nodes.push(TopologyNode {
            id: stage_id(stage),
            kind: NodeKind::SubTopology,
            label: format!("Sub-topology {stage}"),
            topics: Vec::new(),
        });
    }

    for topic in &sources {
        nodes.push(TopologyNode {
            id: format!("source:{topic}"),
            kind: NodeKind::SourceTopic,
            label: (*topic).to_string(),
            topics: vec![(*topic).to_string()],
        });
        edges.push(TopologyEdge {
            from: format!("source:{topic}"),
            to: stage_id(0),
        });
    }

    // Which stage reads which repartition topic is true BY CONSTRUCTION: the
    // stage was created for it.
    let mut stage_of_store: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, repartition) in repartitions.iter().enumerate() {
        let stage = index + 1;
        stage_of_store.insert(repartition.name.as_str(), stage);
        nodes.push(TopologyNode {
            id: format!("repartition:{}", repartition.topic),
            kind: NodeKind::Repartition,
            label: repartition.name.clone(),
            topics: vec![repartition.topic.clone()],
        });
        edges.push(TopologyEdge {
            from: format!("repartition:{}", repartition.topic),
            to: stage_id(stage),
        });
    }
    // The WRITING side of a repartition topic is only unambiguous when there is
    // one of them: with two or more, any stage could feed any of them and the
    // alphabetical order they arrived in is not the application's data flow.
    if repartitions.len() == 1 {
        edges.push(TopologyEdge {
            from: stage_id(0),
            to: format!("repartition:{}", repartitions[0].topic),
        });
    }

    // --- changelogs -------------------------------------------------------
    // A changelog belongs to a state store; a store that also forced a
    // repartition gave both topics the SAME middle name, which places the
    // changelog in the stage that reads that repartition topic. Anything else
    // is placed only when there is one stage to place it in.
    let mut unplaced: Vec<String> = Vec::new();
    for changelog in &changelogs {
        let id = format!("changelog:{}", changelog.topic);
        nodes.push(TopologyNode {
            id: id.clone(),
            kind: NodeKind::Changelog,
            label: changelog.name.clone(),
            topics: vec![changelog.topic.clone()],
        });
        match stage_of_store.get(changelog.name.as_str()) {
            Some(stage) => edges.push(TopologyEdge {
                from: stage_id(*stage),
                to: id,
            }),
            None if stage_count == 1 => edges.push(TopologyEdge {
                from: stage_id(0),
                to: id,
            }),
            None => unplaced.push(changelog.topic.clone()),
        }
    }

    // --- sinks ------------------------------------------------------------
    for sink in &sinks {
        let id = format!("sink:{sink}");
        nodes.push(TopologyNode {
            id: id.clone(),
            kind: NodeKind::SinkTopic,
            label: sink.clone(),
            topics: vec![sink.clone()],
        });
        if stage_count == 1 {
            edges.push(TopologyEdge {
                from: stage_id(0),
                to: id,
            });
        } else {
            unplaced.push(sink.clone());
        }
    }

    // --- what this could not know ----------------------------------------
    caveats.push(format!(
        "This graph is deduced from what \"{group_id}\" subscribes to and from Kafka's \
         internal-topic naming. Kafka publishes no topology: describe() runs inside the \
         application, not on the broker."
    ));
    caveats.push(
        "Processor names are not in it. Kafka carries topics, not the map/filter/join/aggregate \
         steps between them, so each sub-topology is one box rather than the chain of operators \
         it really is."
            .into(),
    );
    caveats.push(
        "Joins are invisible. Two source topics feeding one sub-topology may be joined, merged, \
         or processed side by side — the group's subscription looks identical in all three cases."
            .into(),
    );
    caveats.push(format!(
        "Sinks are a guess from the name. A topic called \"{group_id}-…\" is shown as one; a \
         through() topic or a to() target named anything else is a topic this application writes \
         that nothing here can tell apart from any other topic on the cluster."
    ));
    caveats.push(format!(
        "The application id is assumed to be the group id (\"{group_id}\"), which is what Kafka \
         Streams sets by default. An application that set group.id itself would not match its own \
         internal topics here."
    ));
    if repartitions.len() > 1 {
        caveats.push(format!(
            "{} repartition topics means {stage_count} sub-topologies, but not their order: which \
             sub-topology WRITES each repartition topic is not recorded anywhere Kavka can read, \
             so the numbering below is this list's own and no arrows into them are drawn.",
            repartitions.len()
        ));
    }
    if members.is_empty() {
        caveats.push(format!(
            "\"{group_id}\" has no members right now, so its source topics are unknown — a \
             subscription lives on a member. Only the topics named after the application are \
             shown."
        ));
    }
    if !unplaced.is_empty() {
        caveats.push(format!(
            "Not placed in a sub-topology, because nothing says which one owns them: {}.",
            unplaced.join(", ")
        ));
    }

    StreamsTopology {
        app_id: group_id.to_string(),
        nodes,
        edges,
        inferred: true,
        caveats,
    }
}

fn stage_id(stage: usize) -> String {
    format!("sub:{stage}")
}

/// This application's internal topics of one kind, in a stable order.
fn internal_topics(topics: &[String], prefix: &str, suffix: &str) -> Vec<Internal> {
    let mut out: Vec<Internal> = topics
        .iter()
        .filter_map(|topic| {
            strip(topic, prefix, suffix).map(|name| Internal {
                topic: topic.clone(),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.topic.cmp(&b.topic));
    out
}

/// The middle of `<prefix><name><suffix>`, or `None` when the topic is not one
/// of this application's.
///
/// An EMPTY middle does not count: `wordcount--repartition` is not a topic
/// Kafka Streams creates, and treating it as one would produce a node with no
/// name.
fn strip(topic: &str, prefix: &str, suffix: &str) -> Option<String> {
    let name = topic.strip_prefix(prefix)?.strip_suffix(suffix)?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The inferred topology of the Streams application behind one consumer group.
///
/// Two reads: the group (for its members' subscriptions) and the cluster's
/// topic list. The group read is [`crate::admin::group_detail`] rather than a
/// narrower call so that a topology and the consumer-group screen can never
/// disagree about what a group is reading.
#[cfg(feature = "kafka")]
pub fn topology(conn: &ClusterConnection, group_id: &str) -> Result<StreamsTopology> {
    let detail = crate::admin::group_detail(conn, group_id)?;
    let topics: Vec<String> = conn
        .list_topics()?
        .into_iter()
        .map(|topic| topic.name)
        .collect();
    Ok(infer(group_id, &detail.members, &topics))
}

#[cfg(not(feature = "kafka"))]
pub fn topology(_conn: &ClusterConnection, _group_id: &str) -> Result<StreamsTopology> {
    Err(crate::Error::Other(
        "built without the `kafka` feature".into(),
    ))
}

/// The one thing the synthetic tests below cannot check: that [`topology`] is
/// wired to a real group's real subscriptions.
///
/// In the crate rather than `tests/` for the reason [`crate::admin`]'s own
/// integration module gives — creating a live consumer group needs a throwaway
/// rdkafka consumer, and rdkafka is a dependency of this crate rather than a
/// dev-dependency, so an external test target cannot see it.
///
/// ```text
/// docker compose -f dev/docker-compose.yml up -d --wait
/// KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl
/// ```
#[cfg(all(test, feature = "kafka"))]
mod it {
    use super::*;
    use crate::profiles::ConnectionProfile;
    use rdkafka::config::ClientConfig;
    use rdkafka::consumer::{BaseConsumer, Consumer};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    const BOOTSTRAP: &str = "localhost:9092";
    const SEEDED_TOPIC: &str = "orders";

    fn enabled() -> bool {
        if matches!(std::env::var("KAVKA_IT").as_deref(), Ok("1")) {
            return true;
        }
        eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml up to run this");
        false
    }

    /// Built through serde rather than a struct literal, for the reason
    /// `admin`'s copy gives: `ConnectionProfile` gains optional fields every
    /// phase, and a literal here would have to be edited each time.
    fn connect() -> ClusterConnection {
        let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({
            "id": "kavka-it-streams",
            "name": "dev cluster",
            "environment": "dev",
            "bootstrap_servers": [BOOTSTRAP],
            "auth": {"kind": "plaintext"},
            "read_only": true,
        }))
        .expect("the integration profile is a valid ConnectionProfile");
        ClusterConnection::connect(profile).expect(
            "the dev cluster must be up: docker compose -f dev/docker-compose.yml up -d --wait",
        )
    }

    /// A REAL consumer group: an ordinary application, with an ordinary client
    /// id, which is exactly what must not be mistaken for a Streams topology.
    #[test]
    fn a_live_consumer_group_is_read_from_the_cluster_and_is_not_a_streams_app() {
        if !enabled() {
            return;
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let group = format!("kavka-it-streams-{}-{nanos:x}", std::process::id());

        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", BOOTSTRAP)
            .set("group.id", &group)
            .set("client.id", "kavka-it-plain-consumer")
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .set("session.timeout.ms", "6000")
            .create()
            .expect("throwaway consumer");
        consumer.subscribe(&[SEEDED_TOPIC]).expect("subscribe");

        let conn = connect();
        let deadline = Instant::now() + Duration::from_secs(30);
        let topology = loop {
            // Polling is what drives the join; without it the group never forms.
            let _ = consumer.poll(Duration::from_millis(200));
            let topology = topology(&conn, &group);
            if let Ok(topology) = topology {
                if !topology.caveats.is_empty() {
                    break topology;
                }
            }
            assert!(Instant::now() < deadline, "{group} never joined");
        };

        assert_eq!(topology.app_id, group);
        assert!(topology.inferred);
        assert!(
            topology.nodes.is_empty(),
            "a plain consumer group is not a topology: {topology:?}"
        );
        assert!(
            topology.caveats[0].contains("StreamThread"),
            "got {:?}",
            topology.caveats
        );
    }

    /// The wrapper's other half: a group that does not exist fails the way the
    /// consumer-group screen already fails, because it is the same read.
    #[test]
    fn a_group_that_does_not_exist_fails_by_name() {
        if !enabled() {
            return;
        }
        let missing = format!("kavka-it-no-such-group-{}", std::process::id());
        let err = topology(&connect(), &missing)
            .expect_err("no such group")
            .to_string();
        assert!(err.contains(&missing), "got {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::TopicPartition;

    fn member(client_id: &str, assignments: &[(&str, i32)]) -> GroupMember {
        GroupMember {
            member_id: format!("{client_id}-1a2b"),
            client_id: client_id.to_string(),
            client_host: "/10.0.0.7".into(),
            assignments: assignments
                .iter()
                .map(|(topic, partition)| TopicPartition {
                    topic: (*topic).to_string(),
                    partition: *partition,
                })
                .collect(),
        }
    }

    /// A Streams consumer's client id, which is where the StreamThread marker
    /// lives: `<application.id>-<processId>-StreamThread-<n>-consumer`.
    fn stream_thread(app: &str, assignments: &[(&str, i32)]) -> GroupMember {
        member(
            &format!("{app}-4f2c9a3e-StreamThread-1-consumer"),
            assignments,
        )
    }

    fn topics(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    fn node<'a>(topology: &'a StreamsTopology, id: &str) -> &'a TopologyNode {
        topology
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("no node {id} in {:?}", ids(topology)))
    }

    fn ids(topology: &StreamsTopology) -> Vec<&str> {
        topology.nodes.iter().map(|node| node.id.as_str()).collect()
    }

    fn kinds(topology: &StreamsTopology, kind: NodeKind) -> Vec<&str> {
        topology
            .nodes
            .iter()
            .filter(|node| node.kind == kind)
            .map(|node| node.label.as_str())
            .collect()
    }

    fn has_edge(topology: &StreamsTopology, from: &str, to: &str) -> bool {
        topology
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to)
    }

    fn caveat_about(topology: &StreamsTopology, needle: &str) -> bool {
        topology
            .caveats
            .iter()
            .any(|caveat| caveat.contains(needle))
    }

    /// Every edge has to name nodes that exist, or the UI draws an arrow into
    /// nothing. Checked on every fixture below rather than as its own case.
    fn assert_edges_resolve(topology: &StreamsTopology) {
        let ids: BTreeSet<&str> = ids(topology).into_iter().collect();
        for edge in &topology.edges {
            assert!(
                ids.contains(edge.from.as_str()),
                "edge from unknown node {}: {:?}",
                edge.from,
                ids
            );
            assert!(
                ids.contains(edge.to.as_str()),
                "edge to unknown node {}: {:?}",
                edge.to,
                ids
            );
        }
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), topology.nodes.len(), "duplicate node ids");
        assert!(
            topology.inferred,
            "a topology is never anything but inferred"
        );
    }

    /// THE canonical shape: `stream → groupBy → count → to`. One repartition
    /// topic, one changelog for the same store, and therefore exactly two
    /// sub-topologies split at the repartition.
    #[test]
    fn a_word_count_app_becomes_two_sub_topologies_split_at_its_repartition() {
        let app = "wordcount";
        let topology = infer(
            app,
            &[stream_thread(
                app,
                &[
                    ("text-lines", 0),
                    ("text-lines", 1),
                    ("wordcount-Counts-repartition", 0),
                ],
            )],
            &topics(&[
                "text-lines",
                "wordcount-Counts-repartition",
                "wordcount-Counts-changelog",
                "unrelated-topic",
                "__consumer_offsets",
            ]),
        );
        assert_edges_resolve(&topology);

        assert_eq!(topology.app_id, app);
        assert_eq!(kinds(&topology, NodeKind::SubTopology).len(), 2);
        assert_eq!(kinds(&topology, NodeKind::SourceTopic), vec!["text-lines"]);
        // The label is the STORE name; the full topic is carried alongside it.
        assert_eq!(kinds(&topology, NodeKind::Repartition), vec!["Counts"]);
        assert_eq!(
            node(&topology, "repartition:wordcount-Counts-repartition").topics,
            vec!["wordcount-Counts-repartition"]
        );
        assert_eq!(kinds(&topology, NodeKind::Changelog), vec!["Counts"]);

        // source → sub 0 → repartition → sub 1 → changelog
        assert!(has_edge(&topology, "source:text-lines", "sub:0"));
        assert!(has_edge(
            &topology,
            "sub:0",
            "repartition:wordcount-Counts-repartition"
        ));
        assert!(has_edge(
            &topology,
            "repartition:wordcount-Counts-repartition",
            "sub:1"
        ));
        // The changelog lands in sub-topology 1 because its store name is the
        // one that forced the repartition — not because it was the last node.
        assert!(has_edge(
            &topology,
            "sub:1",
            "changelog:wordcount-Counts-changelog"
        ));

        // A topic belonging to nobody is not dragged in.
        assert!(!ids(&topology).iter().any(|id| id.contains("unrelated")));
        assert!(!ids(&topology).iter().any(|id| id.contains("__consumer")));
    }

    /// A stateless application: `filter → to`, no internal topics at all. It is
    /// still a Streams app, and the only thing that says so is the StreamThread
    /// in its client id.
    #[test]
    fn a_stateless_app_is_one_sub_topology_recognised_by_its_stream_threads() {
        let app = "router";
        let topology = infer(
            app,
            &[stream_thread(app, &[("raw-events", 0)])],
            &topics(&["raw-events", "router-clean", "router-rejected"]),
        );
        assert_edges_resolve(&topology);

        assert_eq!(
            kinds(&topology, NodeKind::SubTopology),
            vec!["Sub-topology 0"]
        );
        assert_eq!(kinds(&topology, NodeKind::SourceTopic), vec!["raw-events"]);
        // With ONE sub-topology there is nothing to be ambiguous about, so the
        // sinks named after the application are attached to it.
        assert_eq!(
            kinds(&topology, NodeKind::SinkTopic),
            vec!["router-clean", "router-rejected"]
        );
        assert!(has_edge(&topology, "sub:0", "sink:router-clean"));
        assert!(has_edge(&topology, "sub:0", "sink:router-rejected"));
        assert!(has_edge(&topology, "source:raw-events", "sub:0"));

        // And the guess is declared, every time, whether or not it landed.
        assert!(caveat_about(&topology, "Sinks are a guess from the name"));
    }

    /// Two repartition topics: three sub-topologies, and no honest way to say
    /// which one writes which. The read edges stay (they are true by
    /// construction); the write edges do not appear at all, and the caveat says
    /// why in the same breath.
    #[test]
    fn two_repartition_topics_leave_the_stage_order_unknown_and_say_so() {
        let app = "sessions";
        let topology = infer(
            app,
            &[stream_thread(
                app,
                &[
                    ("clicks", 0),
                    ("sessions-ByUser-repartition", 0),
                    ("sessions-ByRegion-repartition", 0),
                ],
            )],
            &topics(&[
                "clicks",
                "sessions-ByUser-repartition",
                "sessions-ByRegion-repartition",
                "sessions-ByUser-changelog",
            ]),
        );
        assert_edges_resolve(&topology);

        assert_eq!(kinds(&topology, NodeKind::SubTopology).len(), 3);
        // Read edges: each stage was created for the topic it reads.
        assert!(has_edge(
            &topology,
            "repartition:sessions-ByRegion-repartition",
            "sub:1"
        ));
        assert!(has_edge(
            &topology,
            "repartition:sessions-ByUser-repartition",
            "sub:2"
        ));
        // No stage claims to WRITE either of them.
        assert!(
            !topology
                .edges
                .iter()
                .any(|edge| edge.from.starts_with("sub:") && edge.to.starts_with("repartition:")),
            "{:?}",
            topology.edges
        );
        assert!(caveat_about(&topology, "which sub-topology WRITES"));
        assert!(caveat_about(&topology, "3 sub-topologies"));

        // The store-name match still places the changelog exactly.
        assert!(has_edge(
            &topology,
            "sub:2",
            "changelog:sessions-ByUser-changelog"
        ));
    }

    /// A changelog whose store matched no repartition topic, in an application
    /// with more than one stage: it is drawn, unattached, and named in a
    /// caveat. The alternative — hanging it off sub-topology 0 — would be a
    /// picture of a state store in the wrong half of the application.
    #[test]
    fn an_unmatched_changelog_is_left_unplaced_rather_than_guessed_at() {
        let app = "orders";
        let topology = infer(
            app,
            &[stream_thread(
                app,
                &[("orders-in", 0), ("orders-ByCustomer-repartition", 0)],
            )],
            &topics(&[
                "orders-in",
                "orders-ByCustomer-repartition",
                "orders-Totals-changelog",
                "orders-out",
            ]),
        );
        assert_edges_resolve(&topology);

        assert!(topology
            .nodes
            .iter()
            .any(|node| node.id == "changelog:orders-Totals-changelog"));
        assert!(
            !topology
                .edges
                .iter()
                .any(|edge| edge.to == "changelog:orders-Totals-changelog"),
            "{:?}",
            topology.edges
        );
        assert!(caveat_about(&topology, "Not placed in a sub-topology"));
        assert!(caveat_about(&topology, "orders-Totals-changelog"));
        // The sink is unplaceable for the same reason, and is named too.
        assert!(caveat_about(&topology, "orders-out"));
    }

    /// The prefix rule cuts both ways: another application's internal topics
    /// are not this one's, however similar the names look.
    #[test]
    fn internal_topics_belonging_to_another_application_are_not_claimed() {
        let app = "orders";
        let topology = infer(
            app,
            &[stream_thread(app, &[("orders-in", 0)])],
            &topics(&[
                "orders-in",
                // Another app's store, and another app entirely.
                "billing-Totals-changelog",
                "billing-ByCustomer-repartition",
                // A topic whose name merely CONTAINS this app's id.
                "legacy-orders-Totals-changelog",
                // And this one: prefix matches, but the middle is empty, so it
                // is not a name Kafka Streams generates.
                "orders--repartition",
            ]),
        );
        assert_edges_resolve(&topology);

        assert!(
            kinds(&topology, NodeKind::Changelog).is_empty(),
            "{topology:?}"
        );
        assert!(
            kinds(&topology, NodeKind::Repartition).is_empty(),
            "{topology:?}"
        );
        assert_eq!(kinds(&topology, NodeKind::SubTopology).len(), 1);
        // `orders--repartition` is this application's prefix with an EMPTY
        // store name, which Kafka Streams never generates. It is not adopted as
        // an internal topic, and it is not promoted to a sink either — a topic
        // ending in `-repartition` is somebody's internal topic, and guessing
        // that it is an output would be worse than saying nothing about it.
        assert!(
            kinds(&topology, NodeKind::SinkTopic).is_empty(),
            "{topology:?}"
        );
        assert!(
            !ids(&topology).iter().any(|id| id.contains("--repartition")),
            "{topology:?}"
        );
    }

    /// A topic named after the application that it READS is a source, not a
    /// sink. `<app-id>-input` is the commonest naming there is, and drawing it
    /// at both ends would be a picture of a loop the application does not have.
    #[test]
    fn a_topic_named_after_the_app_that_it_reads_is_a_source_and_not_also_a_sink() {
        let app = "orders";
        let topology = infer(
            app,
            &[stream_thread(app, &[("orders-input", 0)])],
            &topics(&["orders-input", "orders-output"]),
        );
        assert_edges_resolve(&topology); // one node per id, so not both

        assert_eq!(
            kinds(&topology, NodeKind::SourceTopic),
            vec!["orders-input"]
        );
        assert_eq!(kinds(&topology, NodeKind::SinkTopic), vec!["orders-output"]);
        assert!(has_edge(&topology, "source:orders-input", "sub:0"));
        assert!(has_edge(&topology, "sub:0", "sink:orders-output"));
    }

    /// A plain consumer group: no StreamThread anywhere, no internal topics.
    /// The answer is an empty topology that says how it decided, not a graph of
    /// one box.
    #[test]
    fn a_plain_consumer_group_has_no_topology_and_says_why() {
        let topology = infer(
            "billing-service",
            &[member("billing-worker-3", &[("orders", 0), ("orders", 1)])],
            &topics(&["orders", "payments", "billing-service-audit"]),
        );

        assert!(topology.nodes.is_empty(), "{topology:?}");
        assert!(topology.edges.is_empty(), "{topology:?}");
        assert!(topology.inferred);
        assert_eq!(topology.caveats.len(), 1);
        assert!(caveat_about(&topology, "StreamThread"));
        assert!(caveat_about(&topology, "no topology to show"));
        assert!(caveat_about(&topology, "billing-service-…-repartition"));
        // Note what did NOT happen: `billing-service-audit` starts with the
        // group id, and a topology was still not invented around it.
    }

    /// A stopped application. Its internal topics outlive it, so it is still
    /// recognisable — but subscriptions live on members, and there are none, so
    /// the source topics genuinely cannot be known. The gap is stated rather
    /// than filled.
    #[test]
    fn a_streams_app_with_no_members_keeps_its_internal_topics_and_loses_its_sources() {
        let topology = infer(
            "wordcount",
            &[],
            &topics(&[
                "text-lines",
                "wordcount-Counts-repartition",
                "wordcount-Counts-changelog",
            ]),
        );
        assert_edges_resolve(&topology);

        assert!(kinds(&topology, NodeKind::SourceTopic).is_empty());
        assert_eq!(kinds(&topology, NodeKind::Repartition), vec!["Counts"]);
        assert_eq!(kinds(&topology, NodeKind::SubTopology).len(), 2);
        assert!(caveat_about(&topology, "has no members right now"));
    }

    /// A repartition topic the group is READING that the topic list did not
    /// mention. The subscription is the stronger evidence and wins.
    #[test]
    fn a_subscribed_repartition_topic_counts_even_when_the_topic_list_missed_it() {
        let app = "wordcount";
        let topology = infer(
            app,
            &[stream_thread(
                app,
                &[("text-lines", 0), ("wordcount-Counts-repartition", 0)],
            )],
            &topics(&["text-lines"]),
        );
        assert_edges_resolve(&topology);

        assert_eq!(kinds(&topology, NodeKind::Repartition), vec!["Counts"]);
        assert_eq!(kinds(&topology, NodeKind::SubTopology).len(), 2);
        // And it is not ALSO counted as a source, which is what it would look
        // like to a naive "everything subscribed is a source" rule.
        assert_eq!(kinds(&topology, NodeKind::SourceTopic), vec!["text-lines"]);
    }

    /// Two members of the same application, each holding some partitions: the
    /// topic set is the union, and nothing appears twice.
    #[test]
    fn subscriptions_are_unioned_across_members_without_duplicating_nodes() {
        let app = "wordcount";
        let topology = infer(
            app,
            &[
                stream_thread(app, &[("text-lines", 0), ("more-text", 0)]),
                stream_thread(app, &[("text-lines", 1)]),
            ],
            &topics(&["text-lines", "more-text"]),
        );
        assert_edges_resolve(&topology); // includes the duplicate-id check

        assert_eq!(
            kinds(&topology, NodeKind::SourceTopic),
            vec!["more-text", "text-lines"],
            "sorted, and one node per topic"
        );
        assert_eq!(
            topology
                .edges
                .iter()
                .filter(|edge| edge.to == "sub:0")
                .count(),
            2
        );
    }

    /// The caveats are the load-bearing half of this module, so their presence
    /// is asserted as a set rather than one at a time.
    #[test]
    fn every_topology_carries_the_four_things_it_cannot_know() {
        let app = "wordcount";
        let topology = infer(
            app,
            &[stream_thread(app, &[("text-lines", 0)])],
            &topics(&["text-lines", "wordcount-Counts-changelog"]),
        );
        for expected in [
            "Kafka publishes no topology",
            "Processor names are not in it",
            "Joins are invisible",
            "Sinks are a guess from the name",
            "The application id is assumed to be the group id",
        ] {
            assert!(caveat_about(&topology, expected), "missing: {expected}");
        }
    }

    /// The IPC contract: field names, the `kind` vocabulary, and `inferred`.
    #[test]
    fn the_wire_shape_matches_the_contract() {
        let app = "wordcount";
        let topology = infer(
            app,
            &[stream_thread(
                app,
                &[("text-lines", 0), ("wordcount-Counts-repartition", 0)],
            )],
            &topics(&[
                "text-lines",
                "wordcount-Counts-repartition",
                "wordcount-Counts-changelog",
                "wordcount-out",
            ]),
        );
        let json = serde_json::to_value(&topology).expect("serialise");

        assert_eq!(json["app_id"], "wordcount");
        assert_eq!(json["inferred"], true);
        assert!(json["caveats"].as_array().expect("caveats").len() >= 5);
        let nodes = json["nodes"].as_array().expect("nodes");
        let kinds: BTreeSet<&str> = nodes
            .iter()
            .map(|node| node["kind"].as_str().expect("kind is a string"))
            .collect();
        assert_eq!(
            kinds,
            BTreeSet::from([
                "sub_topology",
                "source_topic",
                "repartition",
                "changelog",
                "sink_topic",
            ]),
            "the contract's whole vocabulary, spelled its way"
        );
        let first = &nodes[0];
        for field in ["id", "kind", "label", "topics"] {
            assert!(first.get(field).is_some(), "missing {field} in {first}");
        }
        let edge = &json["edges"][0];
        assert!(
            edge.get("from").is_some() && edge.get("to").is_some(),
            "{edge}"
        );
    }
}
