//! DescribeQuorum (API key 55) — the KRaft metadata quorum's leader, its
//! voters, and the observers following it.
//!
//! Versions 0-1. v1 is what the IPC contract needs: it added
//! `LastFetchTimestamp` and `LastCaughtUpTimestamp`, which are the whole of the
//! "is this voter keeping up?" question. v2 adds a `Nodes` array of listener
//! endpoints and per-level `ErrorMessage`s; nothing in the contract reads
//! either, so it is not implemented and the negotiation caps at v1.
//!
//! ROUTING. This is a controller API. In KRaft a broker is a raft *observer*
//! and forwards the request to the active controller, which is the only way it
//! can work for a desktop client at all: the controller listener is usually not
//! advertised to clients (the dev compose file is a fair example — the
//! controller is on an internal port that is never published). So the request
//! goes to whichever broker is already connected, and a broker that answers
//! "not me" is REPORTED rather than chased — see [`QuorumReply`] and
//! [`super::ProtocolClient::quorum_describe`] for why Metadata's `ControllerId`
//! is not the redirect it looks like.

use super::conn::{BrokerConnection, DESCRIBE_QUORUM};
use super::errors;
use super::wire::{Decoder, Encoder};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// The topic KRaft keeps its metadata log in. Always one partition.
const METADATA_TOPIC: &str = "__cluster_metadata";
const METADATA_PARTITION: i32 = 0;

/// One voter or observer, as the IPC contract names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaState {
    pub replica_id: i32,
    pub log_end_offset: i64,
    /// Milliseconds since this replica last fetched, or `None` when the leader
    /// has never heard from it (or is describing itself).
    ///
    /// An AGE rather than the wire's absolute timestamp: the broker's clock and
    /// the desktop's are different clocks, and a UI that renders "last fetch:
    /// 14:03:11" from a broker timestamp is quietly showing the wrong time
    /// zone, the wrong minute, or both.
    pub last_fetch_age_ms: Option<i64>,
    pub last_caught_up_age_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuorumInfo {
    pub leader_id: i32,
    pub leader_epoch: i32,
    pub high_watermark: i64,
    pub voters: Vec<ReplicaState>,
    pub observers: Vec<ReplicaState>,
}

/// What one DescribeQuorum round trip produced.
#[derive(Debug)]
pub(crate) enum QuorumReply {
    Described(QuorumInfo),
    /// The broker answered "not me" — NOT_CONTROLLER or NOT_LEADER_OR_FOLLOWER,
    /// at the top level or on the partition. Reported to the user with the
    /// code, because there is nowhere honest to forward it to.
    NotHere(i16),
}

pub(crate) fn describe(conn: &mut BrokerConnection) -> Result<QuorumReply> {
    let version = conn.negotiate(DESCRIBE_QUORUM)?;
    let payload = conn.call(DESCRIBE_QUORUM, version, request_body())?;
    decode(&payload, version, now_ms())
}

fn request_body() -> Vec<u8> {
    let mut body = Encoder::new();
    body.compact_array_len(Some(1))
        .compact_string(METADATA_TOPIC)
        .compact_array_len(Some(1))
        .int32(METADATA_PARTITION)
        .tagged_fields() // partition
        .tagged_fields() // topic
        .tagged_fields(); // body
    body.finish()
}

fn decode(payload: &[u8], version: i16, now_ms: i64) -> Result<QuorumReply> {
    let mut decoder = Decoder::new(payload);
    let code = decoder.int16()?;
    if is_elsewhere(code) {
        return Ok(QuorumReply::NotHere(code));
    }
    errors::check("describing the metadata quorum", code, None)?;

    let topics = decoder.compact_array_len()?.unwrap_or(0);
    for _ in 0..topics {
        let topic = decoder.compact_string()?;
        let partitions = decoder.compact_array_len()?.unwrap_or(0);
        if partitions == 0 {
            decoder.tagged_fields()?;
            continue;
        }
        let partition = decoder.int32()?;
        let code = decoder.int16()?;
        if is_elsewhere(code) {
            return Ok(QuorumReply::NotHere(code));
        }
        errors::check(
            &format!("describing the metadata quorum ({topic}-{partition})"),
            code,
            None,
        )?;
        let leader_id = decoder.int32()?;
        let leader_epoch = decoder.int32()?;
        let high_watermark = decoder.int64()?;
        let voters = replica_states(&mut decoder, version, now_ms)?;
        let observers = replica_states(&mut decoder, version, now_ms)?;

        // `__cluster_metadata` has exactly one partition, and that is the one
        // this request asked for — so the first entry is the answer, and the
        // rest of the frame (the partition and topic tag buffers, v2's `Nodes`,
        // the top-level tag buffer) is deliberately not read.
        return Ok(QuorumReply::Described(QuorumInfo {
            leader_id,
            leader_epoch,
            high_watermark,
            voters,
            observers,
        }));
    }
    Err(Error::Other(format!(
        "the cluster returned no quorum information for {METADATA_TOPIC}-{METADATA_PARTITION} — \
         this is a KRaft-only view, so a ZooKeeper-backed cluster has nothing to show here"
    )))
}

fn replica_states(
    decoder: &mut Decoder<'_>,
    version: i16,
    now_ms: i64,
) -> Result<Vec<ReplicaState>> {
    let count = decoder.compact_array_len()?.unwrap_or(0);
    let mut states = Vec::with_capacity(count);
    for _ in 0..count {
        let replica_id = decoder.int32()?;
        let log_end_offset = decoder.int64()?;
        let (last_fetch, last_caught_up) = if version >= 1 {
            (decoder.int64()?, decoder.int64()?)
        } else {
            (-1, -1)
        };
        decoder.tagged_fields()?;
        states.push(ReplicaState {
            replica_id,
            log_end_offset,
            last_fetch_age_ms: age_ms(now_ms, last_fetch),
            last_caught_up_age_ms: age_ms(now_ms, last_caught_up),
        });
    }
    Ok(states)
}

/// Kafka writes `-1` (the schema default) when there is no timestamp, and the
/// leader has no fetch timestamp for itself.
///
/// Clamped at zero: the two clocks are not the same clock, and "-3 ms ago"
/// reads as a bug rather than as skew.
fn age_ms(now_ms: i64, timestamp_ms: i64) -> Option<i64> {
    if timestamp_ms <= 0 {
        return None;
    }
    Some(now_ms.saturating_sub(timestamp_ms).max(0))
}

/// The two codes that mean "ask a different node".
fn is_elsewhere(code: i16) -> bool {
    code == errors::NOT_CONTROLLER || code == errors::NOT_LEADER_OR_FOLLOWER
}

fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// GOLDEN BYTES — the DescribeQuorum request body, hand-verified.
    ///
    /// 02                        compact array, 1 topic (1 + 1)
    /// 13 5f5f636c75737465725f6d65746164617461
    ///                           compact string, len 18 + 1 = 0x13:
    ///                           "__cluster_metadata"
    /// 02                        compact array, 1 partition
    /// 00000000                  partition index 0
    /// 00                        partition tag buffer
    /// 00                        topic tag buffer
    /// 00                        body tag buffer
    #[test]
    fn the_request_body_is_byte_for_byte() {
        assert_eq!(
            hex(&request_body()),
            "0213\
             5f5f636c75737465725f6d65746164617461\
             02\
             00000000\
             000000"
        );
        assert_eq!(request_body().len(), 1 + 1 + 18 + 1 + 4 + 3);
    }

    /// A v1 response for a single-node cluster: leader 1, one voter, no
    /// observers.
    fn response_v1(voter_fetch_ms: i64) -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int16(0) // top-level error
            .compact_array_len(Some(1))
            .compact_string(METADATA_TOPIC)
            .compact_array_len(Some(1))
            .int32(0) // partition index
            .int16(0) // partition error
            .int32(1) // leader id
            .int32(3) // leader epoch
            .int64(4_242); // high watermark
        enc.compact_array_len(Some(1)) // voters
            .int32(1)
            .int64(4_242)
            .int64(voter_fetch_ms)
            .int64(voter_fetch_ms)
            .tagged_fields();
        enc.compact_array_len(Some(0)) // observers
            .tagged_fields() // partition
            .tagged_fields() // topic
            .tagged_fields(); // body
        enc.finish()
    }

    #[test]
    fn a_v1_response_decodes_into_the_ipc_shape() {
        let now = 1_700_000_000_000;
        let QuorumReply::Described(quorum) = decode(&response_v1(now - 250), 1, now).unwrap()
        else {
            panic!("expected a described quorum");
        };
        assert_eq!(quorum.leader_id, 1);
        assert_eq!(quorum.leader_epoch, 3);
        assert_eq!(quorum.high_watermark, 4_242);
        assert_eq!(quorum.observers, vec![]);
        assert_eq!(
            quorum.voters,
            vec![ReplicaState {
                replica_id: 1,
                log_end_offset: 4_242,
                last_fetch_age_ms: Some(250),
                last_caught_up_age_ms: Some(250),
            }]
        );
    }

    /// Kafka's "no timestamp" sentinel, and the leader describing itself.
    #[test]
    fn an_absent_timestamp_becomes_no_age_rather_than_a_huge_one() {
        let now = 1_700_000_000_000;
        let QuorumReply::Described(quorum) = decode(&response_v1(-1), 1, now).unwrap() else {
            panic!("expected a described quorum");
        };
        assert_eq!(quorum.voters[0].last_fetch_age_ms, None);
        assert_eq!(quorum.voters[0].last_caught_up_age_ms, None);

        // Broker clock ahead of ours: skew must not render as a negative age.
        assert_eq!(age_ms(now, now + 5_000), Some(0));
        assert_eq!(age_ms(now, 0), None);
    }

    /// v0 has no timestamps at all — the fields are simply absent from the
    /// wire, not present-and-zero.
    #[test]
    fn a_v0_response_reports_no_ages_without_reading_past_the_frame() {
        let mut enc = Encoder::new();
        enc.int16(0)
            .compact_array_len(Some(1))
            .compact_string(METADATA_TOPIC)
            .compact_array_len(Some(1))
            .int32(0)
            .int16(0)
            .int32(2)
            .int32(9)
            .int64(17)
            .compact_array_len(Some(1))
            .int32(2)
            .int64(17)
            .tagged_fields()
            .compact_array_len(Some(0))
            .tagged_fields()
            .tagged_fields()
            .tagged_fields();

        let QuorumReply::Described(quorum) = decode(&enc.finish(), 0, 1_700_000_000_000).unwrap()
        else {
            panic!("expected a described quorum");
        };
        assert_eq!(quorum.leader_id, 2);
        assert_eq!(quorum.voters[0].last_fetch_age_ms, None);
    }

    /// The redirect the caller retries on, at both levels it can arrive at.
    #[test]
    fn a_not_controller_reply_is_a_redirect_not_a_failure() {
        let mut top = Encoder::new();
        top.int16(errors::NOT_CONTROLLER);
        assert!(matches!(
            decode(&top.finish(), 1, 0).unwrap(),
            QuorumReply::NotHere(41)
        ));

        let mut per_partition = Encoder::new();
        per_partition
            .int16(0)
            .compact_array_len(Some(1))
            .compact_string(METADATA_TOPIC)
            .compact_array_len(Some(1))
            .int32(0)
            .int16(errors::NOT_LEADER_OR_FOLLOWER);
        assert!(matches!(
            decode(&per_partition.finish(), 1, 0).unwrap(),
            QuorumReply::NotHere(6)
        ));
    }

    #[test]
    fn a_real_error_is_reported_with_kafkas_own_name_for_it() {
        let mut enc = Encoder::new();
        enc.int16(31); // CLUSTER_AUTHORIZATION_FAILED
        let err = decode(&enc.finish(), 1, 0).expect_err("not a redirect");
        let message = err.to_string();
        assert!(
            message.contains("describing the metadata quorum"),
            "{message}"
        );
        assert!(
            message.contains("CLUSTER_AUTHORIZATION_FAILED"),
            "{message}"
        );
    }

    #[test]
    fn an_empty_topic_list_says_this_is_a_kraft_only_view() {
        let mut enc = Encoder::new();
        enc.int16(0).compact_array_len(Some(0));
        let err = decode(&enc.finish(), 1, 0).expect_err("nothing to describe");
        assert!(err.to_string().contains("KRaft-only"), "got {err}");
    }
}
