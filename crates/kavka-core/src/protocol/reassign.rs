//! AlterPartitionReassignments (45) and ListPartitionReassignments (46).
//!
//! Version 0 of each — which is also the only version Kafka defines, and both
//! are flexible from v0.
//!
//! The two calls are one feature: `alter` starts (or cancels) a move, `list`
//! reports what is still in flight. Cancelling is not a separate API — it is
//! `alter` with a NULL replica list, which is why [`super::wire`] distinguishes
//! a null `[]int32` from an empty one so carefully.

use super::conn::{
    BrokerConnection, ALTER_PARTITION_REASSIGNMENTS, LIST_PARTITION_REASSIGNMENTS,
    REQUEST_TIMEOUT_MS,
};
use super::elect::PartitionResult;
use super::errors;
use super::meta;
use super::wire::{Decoder, Encoder};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// "Put `topic`-`partition` on exactly these brokers, in this order." The first
/// entry becomes the preferred leader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassignmentSpec {
    pub topic: String,
    pub partition: i32,
    pub replicas: Vec<i32>,
}

/// One partition to act on by name, for cancellation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicPartition {
    pub topic: String,
    pub partition: i32,
}

/// A reassignment the cluster is still working through.
///
/// `adding`/`removing` are the brokers joining and leaving; `replicas` is the
/// full current set, which during a move contains BOTH. A partition whose move
/// has finished simply stops appearing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassignmentState {
    pub topic: String,
    pub partition: i32,
    pub replicas: Vec<i32>,
    pub adding: Vec<i32>,
    pub removing: Vec<i32>,
}

/// One partition's target replica set, or `None` to cancel its move — the
/// shape both `alter` and `cancel` funnel into, because on the wire they are
/// the same request.
type Assignment = (String, i32, Option<Vec<i32>>);

/// The same thing regrouped for encoding: one topic's partitions, each with
/// its target replicas (or `None` to cancel).
type TopicAssignments<'a> = Vec<(i32, Option<&'a [i32]>)>;

pub(crate) fn alter(
    conn: &mut BrokerConnection,
    specs: &[ReassignmentSpec],
) -> Result<Vec<PartitionResult>> {
    validate(specs)?;
    let assignments: Vec<Assignment> = specs
        .iter()
        .map(|s| (s.topic.clone(), s.partition, Some(s.replicas.clone())))
        .collect();
    send_alter(conn, &assignments)
}

/// The mistakes worth catching before the network, because Kafka answers them
/// with a bare INVALID_REPLICA_ASSIGNMENT that names neither the partition nor
/// what was wrong with it.
///
/// Deliberately NOT a broker-id existence check: which brokers exist is the
/// cluster's business and changes between this call and the next, so that one
/// stays the broker's answer and arrives per partition.
fn validate(specs: &[ReassignmentSpec]) -> Result<()> {
    if specs.is_empty() {
        return Err(Error::Other(
            "no partitions were selected to reassign".into(),
        ));
    }
    for spec in specs {
        if spec.replicas.is_empty() {
            return Err(Error::Other(format!(
                "{}-{} was given an empty replica list — a partition must live on at least one \
                 broker (to CANCEL a move, use the cancel action instead)",
                spec.topic, spec.partition
            )));
        }
        let mut seen = spec.replicas.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != spec.replicas.len() {
            return Err(Error::Other(format!(
                "{}-{} lists the same broker twice ({:?}) — a partition cannot have two copies on \
                 one broker",
                spec.topic, spec.partition, spec.replicas
            )));
        }
    }
    Ok(())
}

pub(crate) fn cancel(
    conn: &mut BrokerConnection,
    parts: &[TopicPartition],
) -> Result<Vec<PartitionResult>> {
    if parts.is_empty() {
        return Err(Error::Other("no partitions were selected to cancel".into()));
    }
    // A NULL replica list is Kafka's cancel: "revert to the replica set from
    // before the move".
    let assignments: Vec<Assignment> = parts
        .iter()
        .map(|p| (p.topic.clone(), p.partition, None))
        .collect();
    send_alter(conn, &assignments)
}

fn send_alter(
    conn: &mut BrokerConnection,
    assignments: &[Assignment],
) -> Result<Vec<PartitionResult>> {
    let version = conn.negotiate(ALTER_PARTITION_REASSIGNMENTS)?;
    let payload = conn.call(
        ALTER_PARTITION_REASSIGNMENTS,
        version,
        alter_request_body(assignments),
    )?;
    decode_alter(&payload)
}

/// Groups by topic, because the request is a list of topics each holding a list
/// of partitions — sending a topic twice is a malformed request.
fn alter_request_body(assignments: &[Assignment]) -> Vec<u8> {
    let mut by_topic: BTreeMap<&str, TopicAssignments<'_>> = BTreeMap::new();
    for (topic, partition, replicas) in assignments {
        by_topic
            .entry(topic.as_str())
            .or_default()
            .push((*partition, replicas.as_deref()));
    }

    let mut body = Encoder::new();
    body.int32(REQUEST_TIMEOUT_MS)
        .compact_array_len(Some(by_topic.len()));
    for (topic, partitions) in &by_topic {
        body.compact_string(topic)
            .compact_array_len(Some(partitions.len()));
        for (partition, replicas) in partitions {
            body.int32(*partition)
                .compact_int32_array(*replicas)
                .tagged_fields();
        }
        body.tagged_fields();
    }
    body.tagged_fields();
    body.finish()
}

fn decode_alter(payload: &[u8]) -> Result<Vec<PartitionResult>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;
    let code = decoder.int16()?;
    let message = decoder.compact_nullable_string()?;
    errors::check("altering partition reassignments", code, message.as_deref())?;

    let topics = decoder.compact_array_len()?.unwrap_or(0);
    let mut results = Vec::new();
    for _ in 0..topics {
        let topic = decoder.compact_string()?;
        let partitions = decoder.compact_array_len()?.unwrap_or(0);
        for _ in 0..partitions {
            let partition = decoder.int32()?;
            let code = decoder.int16()?;
            let message = decoder.compact_nullable_string()?;
            decoder.tagged_fields()?;
            results.push(PartitionResult {
                topic: topic.clone(),
                partition,
                error: errors::partition_error(code, message.as_deref()),
            });
        }
        decoder.tagged_fields()?;
    }
    results.sort_by(|a, b| a.topic.cmp(&b.topic).then(a.partition.cmp(&b.partition)));
    Ok(results)
}

/// Reassignments still in flight. `topic == None` asks about the whole cluster.
///
/// A NAMED topic has to carry its partition indexes, and that is the whole
/// reason this call reaches for metadata first. Kafka's
/// `ReplicationControlManager.listPartitionReassignments` iterates
/// `topic.partitionIndexes()` for each topic it is given, so an EMPTY array is
/// "none of them", not the "all of this topic" shorthand it reads as — the
/// request is accepted, the answer is always empty, and the caller concludes
/// the cluster is settled while a move is running. Exactly the shape of
/// [`super::elect`]'s selection, for exactly the same reason.
pub(crate) fn list(
    conn: &mut BrokerConnection,
    topic: Option<&str>,
) -> Result<Vec<ReassignmentState>> {
    let selection = match topic {
        None => None,
        Some(name) => Some((name.to_string(), meta::partitions_of(conn, name)?)),
    };
    let version = conn.negotiate(LIST_PARTITION_REASSIGNMENTS)?;
    let payload = conn.call(
        LIST_PARTITION_REASSIGNMENTS,
        version,
        list_request_body(selection.as_ref()),
    )?;
    decode_list(&payload)
}

fn list_request_body(selection: Option<&(String, Vec<i32>)>) -> Vec<u8> {
    let mut body = Encoder::new();
    body.int32(REQUEST_TIMEOUT_MS);
    match selection {
        // NULL = every topic. An empty array would ask about none.
        None => {
            body.compact_array_len(None);
        }
        Some((topic, partitions)) => {
            body.compact_array_len(Some(1))
                .compact_string(topic)
                .compact_int32_array(Some(partitions))
                .tagged_fields();
        }
    }
    body.tagged_fields();
    body.finish()
}

fn decode_list(payload: &[u8]) -> Result<Vec<ReassignmentState>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;
    let code = decoder.int16()?;
    let message = decoder.compact_nullable_string()?;
    errors::check("listing partition reassignments", code, message.as_deref())?;

    let topics = decoder.compact_array_len()?.unwrap_or(0);
    let mut states = Vec::new();
    for _ in 0..topics {
        let topic = decoder.compact_string()?;
        let partitions = decoder.compact_array_len()?.unwrap_or(0);
        for _ in 0..partitions {
            let partition = decoder.int32()?;
            let replicas = decoder.compact_int32_array()?;
            let adding = decoder.compact_int32_array()?;
            let removing = decoder.compact_int32_array()?;
            decoder.tagged_fields()?;
            states.push(ReassignmentState {
                topic: topic.clone(),
                partition,
                replicas,
                adding,
                removing,
            });
        }
        decoder.tagged_fields()?;
    }
    states.sort_by(|a, b| a.topic.cmp(&b.topic).then(a.partition.cmp(&b.partition)));
    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn spec(topic: &str, partition: i32, replicas: &[i32]) -> ReassignmentSpec {
        ReassignmentSpec {
            topic: topic.into(),
            partition,
            replicas: replicas.to_vec(),
        }
    }

    /// GOLDEN BYTES — an AlterPartitionReassignments v0 request moving
    /// `orders-0` onto brokers [2, 3].
    ///
    /// 00007530          timeout_ms 30000
    /// 02                Topics: compact array, 1 entry
    /// 07 6f7264657273   "orders"
    /// 02                Partitions: compact array, 1 entry
    /// 00000000          partition 0
    /// 03 00000002 00000003   Replicas: compact array of 2 int32
    /// 00                partition tag buffer
    /// 00                topic tag buffer
    /// 00                body tag buffer
    #[test]
    fn an_alter_request_body_is_byte_for_byte() {
        let assignments = vec![("orders".to_string(), 0, Some(vec![2, 3]))];
        assert_eq!(
            hex(&alter_request_body(&assignments)),
            "00007530\
             02\
             076f7264657273\
             02\
             00000000\
             030000000200000003\
             00\
             00\
             00"
        );
    }

    /// Cancelling is the SAME request with a null replica array (`00`), which
    /// is one byte away from an empty one (`01`) — and an empty one would ask
    /// for a partition with no replicas at all.
    #[test]
    fn a_cancel_sends_a_null_replica_array_not_an_empty_one() {
        // 00007530 timeout | 02 one topic | 07 "orders" | 02 one partition
        // | 00000000 partition 0 | 00 NULL replicas | 00 00 00 three tag buffers
        let cancel = vec![("orders".to_string(), 0, None)];
        assert_eq!(
            hex(&alter_request_body(&cancel)),
            "00007530\
             02\
             076f7264657273\
             02\
             00000000\
             00\
             000000"
        );
        let empty = vec![("orders".to_string(), 0, Some(vec![]))];
        assert_ne!(
            hex(&alter_request_body(&empty)),
            hex(&alter_request_body(&cancel))
        );
    }

    /// Several partitions of one topic must arrive as one topic entry — the
    /// request has no room for the same topic twice.
    #[test]
    fn partitions_of_one_topic_are_grouped_into_a_single_entry() {
        let assignments = vec![
            ("orders".to_string(), 2, Some(vec![1])),
            ("payments".to_string(), 0, Some(vec![1])),
            ("orders".to_string(), 0, Some(vec![1])),
        ];
        let body = alter_request_body(&assignments);
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.int32().unwrap(), REQUEST_TIMEOUT_MS);
        assert_eq!(decoder.compact_array_len().unwrap(), Some(2), "two topics");
        // BTreeMap ordering: "orders" before "payments", with both of its
        // partitions under the one entry.
        assert_eq!(decoder.compact_string().unwrap(), "orders");
        assert_eq!(decoder.compact_array_len().unwrap(), Some(2));
    }

    /// GOLDEN BYTES — a ListPartitionReassignments v0 request naming one topic
    /// and BOTH of its partitions.
    ///
    /// 00007530          timeout_ms 30000
    /// 02                Topics: compact array, 1 entry
    /// 07 6f7264657273   "orders"
    /// 03                PartitionIndexes: compact array, 2 entries
    /// 00000000 00000001 partitions 0 and 1
    /// 00                topic tag buffer
    /// 00                body tag buffer
    #[test]
    fn a_list_request_for_one_topic_names_its_partitions() {
        let selection = ("orders".to_string(), vec![0, 1]);
        assert_eq!(
            hex(&list_request_body(Some(&selection))),
            "00007530\
             02\
             076f7264657273\
             03\
             00000000\
             00000001\
             00\
             00"
        );
        // Cluster-wide is a NULL topic array — 00, not 01.
        assert_eq!(hex(&list_request_body(None)), "000075300000");
    }

    /// THE BUG THIS SHAPE EXISTS FOR. An empty `PartitionIndexes` under a named
    /// topic is not "all of them": the controller iterates the array, so an
    /// empty one asks about nothing and answers about nothing. The two requests
    /// must not be the same bytes, and the empty one must never be what `list`
    /// sends for a topic that has partitions.
    #[test]
    fn a_named_topic_with_no_partitions_asks_about_nothing_and_is_a_different_request() {
        let none = ("orders".to_string(), Vec::new());
        let all = ("orders".to_string(), vec![0, 1]);
        assert_ne!(
            hex(&list_request_body(Some(&none))),
            hex(&list_request_body(Some(&all)))
        );
        // 01 is the empty compact array; 03 is the two-element one.
        assert!(hex(&list_request_body(Some(&none))).contains("076f726465727301"));
        assert!(hex(&list_request_body(Some(&all))).contains("076f726465727303"));
    }

    #[test]
    fn an_alter_response_maps_each_partition() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(0)
            .compact_nullable_string(None)
            .compact_array_len(Some(1))
            .compact_string("orders")
            .compact_array_len(Some(2))
            .int32(0)
            .int16(0)
            .compact_nullable_string(None)
            .tagged_fields()
            .int32(1)
            .int16(39) // INVALID_REPLICA_ASSIGNMENT
            .compact_nullable_string(Some(
                "The manual partition assignment includes broker 7, \
                                           which is not part of the cluster",
            ))
            .tagged_fields()
            .tagged_fields()
            .tagged_fields();

        let results = decode_alter(&enc.finish()).expect("decode");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].error, None);
        let failed = results[1].error.as_deref().expect("partition 1 failed");
        assert!(
            failed.contains("INVALID_REPLICA_ASSIGNMENT (39)"),
            "{failed}"
        );
        assert!(failed.contains("broker 7"), "{failed}");
    }

    /// The top-level error the whole feature turns on: a second reassignment
    /// while one is running.
    #[test]
    fn a_reassignment_already_in_progress_fails_the_whole_call() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(60)
            .compact_nullable_string(Some("A reassignment is in progress"));
        let err = decode_alter(&enc.finish()).expect_err("in progress");
        assert!(
            err.to_string().contains("REASSIGNMENT_IN_PROGRESS (60)"),
            "{err}"
        );
        assert!(
            err.to_string().contains("A reassignment is in progress"),
            "{err}"
        );
    }

    #[test]
    fn a_list_response_reports_what_is_moving_where() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(0)
            .compact_nullable_string(None)
            .compact_array_len(Some(1))
            .compact_string("orders")
            .compact_array_len(Some(1))
            .int32(3)
            .compact_int32_array(Some(&[1, 2, 3])) // current set spans both
            .compact_int32_array(Some(&[3])) // adding
            .compact_int32_array(Some(&[1])) // removing
            .tagged_fields()
            .tagged_fields()
            .tagged_fields();

        assert_eq!(
            decode_list(&enc.finish()).expect("decode"),
            vec![ReassignmentState {
                topic: "orders".into(),
                partition: 3,
                replicas: vec![1, 2, 3],
                adding: vec![3],
                removing: vec![1],
            }]
        );
    }

    #[test]
    fn an_empty_list_response_is_an_empty_vec_not_an_error() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(0)
            .compact_nullable_string(None)
            .compact_array_len(Some(0))
            .tagged_fields();
        assert_eq!(decode_list(&enc.finish()).expect("decode"), vec![]);
    }

    /// Caught before the network, so the message can name the partition rather
    /// than leaving the broker to answer INVALID_REPLICA_ASSIGNMENT.
    #[test]
    fn obviously_impossible_specs_are_refused_locally() {
        // These never reach `send_alter`, so no connection is needed; the
        // validation is the part under test.
        assert!(validate(&[]).is_err());
        let err = validate(&[spec("orders", 0, &[])]).unwrap_err().to_string();
        assert!(err.contains("empty replica list"), "got {err}");
        assert!(err.contains("orders-0"), "got {err}");

        let err = validate(&[spec("orders", 1, &[2, 2])])
            .unwrap_err()
            .to_string();
        assert!(err.contains("same broker twice"), "got {err}");

        // A broker id that does not exist is NOT refused locally — that is the
        // cluster's answer, and it arrives per partition.
        assert!(validate(&[spec("orders", 1, &[3, 1, 2])]).is_ok());
        assert!(validate(&[spec("orders", 1, &[9_999])]).is_ok());
    }
}
