//! Metadata (API key 3), for the one thing this module needs from it: which
//! partitions a topic has.
//!
//! Versions 9-12. v9 is the first FLEXIBLE version, which is where the lower
//! bound comes from: a legacy encoder for v0-v8 would be code the integration
//! suite could never run (the dev cluster negotiates v12), and an untested
//! encoder is worse than a refusal that names the API. v12 is the ceiling
//! because v13 makes `TopicId` mandatory in the request, which is a different
//! shape of call than "tell me about this topic by name".
//!
//! The broker table and `ControllerId` used to be decoded too, for a
//! DescribeQuorum redirect that no longer exists (see [`super`] — on KRaft the
//! id a broker reports there is picked at random from the voters, so it is not
//! a routing answer). They are stepped over rather than stored: a field nothing
//! reads is a field someone will start reading, and this decoder's whole
//! defence is that it stops at what it uses.

use super::conn::{BrokerConnection, METADATA};
use super::errors;
use super::wire::{Decoder, Encoder};
use crate::Result;

#[derive(Debug, Clone)]
pub(crate) struct TopicMetadata {
    pub(crate) name: String,
    pub(crate) error_code: i16,
    /// Partition indexes, ascending.
    pub(crate) partitions: Vec<i32>,
}

/// Fetches metadata for exactly the named topics.
///
/// Deliberately no "all topics" mode: a null topic array makes the broker
/// serialize every partition of every topic, which on a large cluster is
/// megabytes of response for a field this module wants a list of indexes out
/// of.
pub(crate) fn fetch(conn: &mut BrokerConnection, topics: &[&str]) -> Result<Vec<TopicMetadata>> {
    let version = conn.negotiate(METADATA)?;
    let mut body = Encoder::new();
    body.compact_array_len(Some(topics.len()));
    for topic in topics {
        if version >= 10 {
            // TopicId: all-zero means "resolve by name", which is what a
            // request that carries a name is doing.
            body.zero_uuid();
        }
        body.compact_string(topic).tagged_fields();
    }
    body.bool(false); // allow_auto_topic_creation — never, from an admin tool
    if version <= 10 {
        body.bool(false); // include_cluster_authorized_operations (v8-v10)
    }
    body.bool(false); // include_topic_authorized_operations
    body.tagged_fields();

    let payload = conn.call(METADATA, version, body.finish())?;
    decode(&payload, version)
}

fn decode(payload: &[u8], version: i16) -> Result<Vec<TopicMetadata>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;

    // The broker table has to be WALKED to reach the topics — every field of it
    // is variable width — but nothing here keeps it.
    let broker_count = decoder.compact_array_len()?.unwrap_or(0);
    for _ in 0..broker_count {
        let _id = decoder.int32()?;
        let _host = decoder.compact_string()?;
        let _port = decoder.int32()?;
        let _rack = decoder.compact_nullable_string()?;
        decoder.tagged_fields()?;
    }

    let _cluster_id = decoder.compact_nullable_string()?;
    let _controller_id = decoder.int32()?;

    let topic_count = decoder.compact_array_len()?.unwrap_or(0);
    let mut topics = Vec::with_capacity(topic_count);
    for _ in 0..topic_count {
        let error_code = decoder.int16()?;
        let name = if version >= 12 {
            decoder.compact_nullable_string()?.unwrap_or_default()
        } else {
            decoder.compact_string()?
        };
        if version >= 10 {
            decoder.skip(16)?; // TopicId
        }
        let _is_internal = decoder.bool()?;

        let partition_count = decoder.compact_array_len()?.unwrap_or(0);
        let mut partitions = Vec::with_capacity(partition_count);
        for _ in 0..partition_count {
            let _partition_error = decoder.int16()?;
            let index = decoder.int32()?;
            let _leader_id = decoder.int32()?;
            let _leader_epoch = decoder.int32()?;
            let _replicas = decoder.compact_int32_array()?;
            let _isr = decoder.compact_int32_array()?;
            let _offline = decoder.compact_int32_array()?;
            decoder.tagged_fields()?;
            partitions.push(index);
        }
        let _topic_authorized_operations = decoder.int32()?;
        decoder.tagged_fields()?;

        partitions.sort_unstable();
        topics.push(TopicMetadata {
            name,
            error_code,
            partitions,
        });
    }
    // ClusterAuthorizedOperations (v8-v10) and the top-level tag buffer follow.
    // Nothing here reads them, and stopping short of fields we do not use is
    // what lets this decoder survive a broker that adds one.
    Ok(topics)
}

/// The partitions of one topic, with the broker's own error for that topic
/// turned into a message that names it.
pub(crate) fn partitions_of(conn: &mut BrokerConnection, topic: &str) -> Result<Vec<i32>> {
    let found = fetch(conn, &[topic])?
        .into_iter()
        .find(|t| t.name == topic)
        .ok_or_else(|| {
            crate::Error::Other(format!(
                "the cluster returned no metadata for topic {topic:?}"
            ))
        })?;
    errors::check(
        &format!("looking up the partitions of topic {topic:?}"),
        found.error_code,
        None,
    )?;
    if found.partitions.is_empty() {
        return Err(crate::Error::Other(format!(
            "topic {topic:?} has no partitions"
        )));
    }
    Ok(found.partitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built Metadata v12 response body: one broker, controller 1, one
    /// topic with two partitions.
    fn response_v12() -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int32(0); // throttle_time_ms
        enc.compact_array_len(Some(1));
        enc.int32(1) // node_id
            .compact_string("localhost")
            .int32(9092)
            .compact_nullable_string(None) // rack
            .tagged_fields();
        enc.compact_nullable_string(Some("5L6g3nShT-eMCtK--X86sw")); // cluster id
        enc.int32(1); // controller_id
        enc.compact_array_len(Some(1));
        enc.int16(0) // topic error code
            .compact_nullable_string(Some("orders"))
            .zero_uuid()
            .bool(false); // is_internal
        enc.compact_array_len(Some(2));
        for partition in [1i32, 0] {
            enc.int16(0) // partition error
                .int32(partition)
                .int32(1) // leader
                .int32(5) // leader epoch
                .compact_int32_array(Some(&[1])) // replicas
                .compact_int32_array(Some(&[1])) // isr
                .compact_int32_array(Some(&[])) // offline
                .tagged_fields();
        }
        enc.int32(i32::MIN) // topic authorized operations ("not requested")
            .tagged_fields(); // topic tags
        enc.int32(i32::MIN) // cluster authorized operations
            .tagged_fields();
        enc.finish()
    }

    /// The broker table and the controller id are variable-width fields the
    /// decoder must walk past to reach the topics — so a response that carries
    /// them still has to yield the right partitions, which is what this pins.
    #[test]
    fn a_v12_response_yields_each_topics_partitions() {
        let topics = decode(&response_v12(), 12).expect("decode");
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].name, "orders");
        assert_eq!(topics[0].error_code, 0);
        // Partition order on the wire is not guaranteed; the decoder sorts.
        assert_eq!(topics[0].partitions, vec![0, 1]);
    }

    /// Cut inside the broker array — past the trailing fields `decode`
    /// deliberately never reads, so this really does exercise the bounds check
    /// rather than the tolerance for a short trailer.
    #[test]
    fn a_truncated_response_is_an_error_rather_than_a_panic() {
        let err = decode(&response_v12()[..20], 12).expect_err("truncated");
        assert!(err.to_string().contains("could not decode"), "got {err}");
    }
}
