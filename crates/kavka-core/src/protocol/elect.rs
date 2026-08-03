//! ElectLeaders (API key 43) — preferred-leader election.
//!
//! Version 2 only, and that is the version negotiation will insist on: v2 is
//! the first FLEXIBLE version (Kafka 2.4), and it is the one the integration
//! suite exercises. See the module docs on [`super`] for why an unexercised
//! legacy encoder is not worth having.
//!
//! Only PREFERRED election is offered. UNCLEAN election (`ElectionType` 1)
//! elects a replica that is *not* in the ISR, which means accepting data loss;
//! that is not a button an admin tool should grow by accident, and the moment
//! it exists the IPC contract has to carry the choice and the UI has to explain
//! it. Kavka's contract has no such field, so this sends `0` and says so.

use super::conn::{BrokerConnection, ELECT_LEADERS, REQUEST_TIMEOUT_MS};
use super::errors;
use super::meta;
use super::wire::{Decoder, Encoder};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// `ElectionType` 0 — elect the preferred (first) replica, and only if it is in
/// the ISR.
const PREFERRED: i8 = 0;

/// One partition's outcome, as the IPC contract names it. `error` is `None` for
/// success AND for the benign no-ops — see [`super::errors::BENIGN`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionResult {
    pub topic: String,
    pub partition: i32,
    pub error: Option<String>,
}

/// Elects preferred leaders.
///
/// `topic == None` means every eligible partition in the cluster. Note that
/// Kafka answers that form by OMITTING partitions that already have their
/// preferred leader (`ReplicationControlManager.electLeaders`), so an empty
/// result there means "nothing needed doing", not "nothing happened". Naming a
/// topic gets a row per partition either way, which is why the UI should name
/// one when it wants to show a table.
pub(crate) fn elect(
    conn: &mut BrokerConnection,
    topic: Option<&str>,
    partitions: Option<&[i32]>,
) -> Result<Vec<PartitionResult>> {
    let selection =
        match (topic, partitions) {
            (None, None) => None,
            (None, Some(_)) => return Err(Error::Other(
                "partition numbers were given without a topic — a partition number means nothing \
                 on its own"
                    .into(),
            )),
            // No partitions named: ask the cluster which ones the topic has, rather
            // than sending an empty list (which Kafka reads as "no partitions" and
            // answers with an empty result).
            (Some(name), None) => Some((name.to_string(), meta::partitions_of(conn, name)?)),
            (Some(name), Some(chosen)) => {
                if chosen.is_empty() {
                    return Err(Error::Other(format!(
                        "no partitions were selected for topic {name:?}"
                    )));
                }
                let mut chosen = chosen.to_vec();
                chosen.sort_unstable();
                chosen.dedup();
                Some((name.to_string(), chosen))
            }
        };

    let version = conn.negotiate(ELECT_LEADERS)?;
    let payload = conn.call(ELECT_LEADERS, version, request_body(selection.as_ref()))?;
    decode(&payload)
}

fn request_body(selection: Option<&(String, Vec<i32>)>) -> Vec<u8> {
    let mut body = Encoder::new();
    body.int8(PREFERRED);
    match selection {
        // A NULL topic array is "every eligible partition"; an EMPTY one would
        // be "none", so the distinction is the whole request.
        None => {
            body.compact_array_len(None);
        }
        Some((topic, partitions)) => {
            body.compact_array_len(Some(1))
                .compact_string(topic)
                .compact_array_len(Some(partitions.len()));
            for partition in partitions {
                body.int32(*partition);
            }
            body.tagged_fields();
        }
    }
    body.int32(REQUEST_TIMEOUT_MS).tagged_fields();
    body.finish()
}

fn decode(payload: &[u8]) -> Result<Vec<PartitionResult>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;
    let code = decoder.int16()?;
    errors::check("electing preferred leaders", code, None)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// GOLDEN BYTES — the flexible-version REQUEST half of the pair, verified
    /// field by field. (The flexible request HEADER that precedes it is pinned
    /// separately, byte for byte, by `conn::tests::api_versions_v3_request_is_byte_for_byte`.)
    ///
    /// 00                election type 0 = PREFERRED
    /// 02                TopicPartitions: compact array, 1 entry (1 + 1)
    /// 07 6f7264657273   topic, COMPACT string, len 6 + 1 = 7: "orders"
    /// 03                Partitions: compact array, 2 entries (2 + 1)
    /// 00000000          partition 0
    /// 00000001          partition 1
    /// 00                TopicPartitions element tag buffer
    /// 00007530          timeout_ms = 30000
    /// 00                body tag buffer
    #[test]
    fn an_elect_leaders_v2_request_body_is_byte_for_byte() {
        let body = request_body(Some(&("orders".to_string(), vec![0, 1])));
        assert_eq!(
            hex(&body),
            "00\
             02\
             076f7264657273\
             03\
             00000000\
             00000001\
             00\
             00007530\
             00"
        );
        assert_eq!(REQUEST_TIMEOUT_MS, 30_000, "the golden bytes encode 30000");

        // The first byte decides whether this is a safe election or a lossy
        // one, so it is asserted as a value rather than as an offset into the
        // hex above: UNCLEAN (1) would elect a replica outside the ISR, which
        // is to say it would accept losing the records only the ISR has.
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.int8().expect("election type"), PREFERRED);
        assert_eq!(PREFERRED, 0, "Kafka's ElectionType.PREFERRED");
    }

    /// "Every eligible partition" is a NULL array (`00`), not an empty one
    /// (`01`) — the difference between "all" and "none".
    #[test]
    fn electing_everywhere_sends_a_null_topic_array_not_an_empty_one() {
        // 00 election type | 00 NULL topic array | 00007530 timeout | 00 tags
        assert_eq!(hex(&request_body(None)), "00000000753000");
        // An empty array would be 0x01 here, and would elect nothing at all.
        assert_ne!(request_body(None)[1], 0x01);
    }

    /// GOLDEN BYTES — the flexible-version RESPONSE half. A single-broker
    /// cluster's answer for one partition of `orders`.
    ///
    /// 00000000    throttle_time_ms
    /// 0000        top-level error code NONE
    /// 02          ReplicaElectionResults: compact array, 1 entry
    /// 07 6f7264657273   topic "orders"
    /// 02          PartitionResult: compact array, 1 entry
    /// 00000000    partition 0
    /// 0054        error code 84 = ELECTION_NOT_NEEDED
    /// 00          error message: COMPACT nullable string, null
    /// 00          PartitionResult tag buffer
    /// 00          ReplicaElectionResults tag buffer
    /// 00          body tag buffer
    #[test]
    fn an_elect_leaders_v2_response_body_decodes_from_golden_bytes() {
        let body: Vec<u8> = [
            0x00, 0x00, 0x00, 0x00, // throttle
            0x00, 0x00, // error code
            0x02, // 1 topic
            0x07, b'o', b'r', b'd', b'e', b'r', b's', 0x02, // 1 partition
            0x00, 0x00, 0x00, 0x00, // partition 0
            0x00, 0x54, // ELECTION_NOT_NEEDED
            0x00, // null message
            0x00, 0x00, 0x00, // three tag buffers
        ]
        .to_vec();

        let results = decode(&body).expect("decode");
        assert_eq!(
            results,
            vec![PartitionResult {
                topic: "orders".into(),
                partition: 0,
                // The point of the benign set: an election that was not needed
                // is a success, so nothing reaches the UI as an error.
                error: None,
            }]
        );
    }

    #[test]
    fn a_real_partition_error_keeps_the_brokers_message() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(0)
            .compact_array_len(Some(1))
            .compact_string("orders")
            .compact_array_len(Some(2))
            .int32(0)
            .int16(80) // PREFERRED_LEADER_NOT_AVAILABLE
            .compact_nullable_string(Some("Preferred leader 2 is not in the ISR"))
            .tagged_fields()
            .int32(1)
            .int16(0)
            .compact_nullable_string(None)
            .tagged_fields()
            .tagged_fields()
            .tagged_fields();

        let results = decode(&enc.finish()).expect("decode");
        assert_eq!(results.len(), 2);
        let failed = results[0].error.as_deref().expect("partition 0 failed");
        assert!(
            failed.contains("PREFERRED_LEADER_NOT_AVAILABLE (80)"),
            "{failed}"
        );
        assert!(failed.contains("not in the ISR"), "{failed}");
        assert_eq!(results[1].error, None);
    }

    #[test]
    fn a_top_level_error_fails_the_whole_call() {
        let mut enc = Encoder::new();
        enc.int32(0).int16(31); // CLUSTER_AUTHORIZATION_FAILED
        let err = decode(&enc.finish()).expect_err("authorization");
        assert!(
            err.to_string().contains("electing preferred leaders"),
            "{err}"
        );
        assert!(
            err.to_string().contains("CLUSTER_AUTHORIZATION_FAILED"),
            "{err}"
        );
    }

    #[test]
    fn results_come_back_sorted_so_a_table_does_not_reorder_itself() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(0)
            .compact_array_len(Some(1))
            .compact_string("orders")
            .compact_array_len(Some(3));
        for partition in [2i32, 0, 1] {
            enc.int32(partition)
                .int16(0)
                .compact_nullable_string(None)
                .tagged_fields();
        }
        enc.tagged_fields().tagged_fields();

        let results = decode(&enc.finish()).expect("decode");
        assert_eq!(
            results.iter().map(|r| r.partition).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }
}
