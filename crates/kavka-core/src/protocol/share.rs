//! Share groups (KIP-932): ListGroups (16) v5, ShareGroupDescribe (77) v1 and
//! DescribeShareGroupOffsets (90) v0.
//!
//! A share group is Kafka 4.1's queue-shaped consumer: many consumers read the
//! *same* partition, records are acknowledged individually, and there is no
//! per-partition committed offset — only a share-partition START offset, the
//! point before which everything has been dealt with. That is why
//! [`ShareGroupOffset`] carries `start_offset` and no lag: a group whose
//! members are all sharing one partition has no "position" to be behind.
//!
//! # Why these are here and not in [`crate::admin`]
//!
//! librdkafka has no share-group client at all — it cannot join one, list one
//! or describe one — so there is nothing for the admin layer to wrap. These are
//! the frames, encoded by hand, over the connection [`super`] already has.
//!
//! # Two things that route
//!
//! **ListGroups is answered per broker, about the groups THAT broker
//! coordinates.** Asking one broker returns one broker's share of them, which
//! looks exactly like a complete answer and is not. So the list is a fan-out
//! across the whole broker table ([`super::ProtocolClient::share_groups_list`]);
//! this module supplies the single-broker half.
//!
//! **Describe and offsets go to the group's coordinator**, which
//! [`find_coordinator`] resolves. A broker that is not the coordinator answers
//! NOT_COORDINATOR rather than forwarding.
//!
//! # The feature flag
//!
//! Share groups are gated on the KRaft feature `share.version`, which is 0 on a
//! fresh Kafka 4.1 cluster. A broker with the feature off still ADVERTISES
//! APIs 77 and 90 in its ApiVersions table — verified against apache/kafka:4.1.0
//! — and then answers UNSUPPORTED_VERSION to every call, while ListGroups
//! cheerfully returns an empty list. Version negotiation therefore cannot see
//! this, and an empty list is not an honest answer to "does this cluster have
//! share groups". Both halves are covered:
//!
//! - BEFORE the call, from the finalized feature levels the ApiVersions reply
//!   already carries ([`ensure_enabled`]) — which is what lets an empty list
//!   mean "no share groups" and nothing else;
//! - AFTER it, by mapping the broker's own UNSUPPORTED_VERSION onto the same
//!   sentence ([`check_group`]), for any broker that reports its features
//!   differently or not at all.

use super::conn::{
    Api, BrokerConnection, DESCRIBE_SHARE_GROUP_OFFSETS, FIND_COORDINATOR, LIST_GROUPS,
    SHARE_GROUP_DESCRIBE, SHARE_VERSION_FEATURE,
};
use super::errors;
use super::reassign::TopicPartition;
use super::wire::{Decoder, Encoder};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// The `TypesFilter` value for a share group. Kafka parses this
/// case-insensitively; it is sent lowercase because that is how the broker
/// spells it back.
const SHARE_GROUP_TYPE: &str = "share";

/// The finalized level of `share.version` that means "on". Kafka defines 0 (off)
/// and 1 (KIP-932) as of 4.1.
const SHARE_VERSION_ENABLED: i16 = 1;

/// FindCoordinator's `KeyType` for a group coordinator. Share groups are
/// described BY THE GROUP COORDINATOR (the share coordinator owns durable
/// acknowledgement state, and the group coordinator asks it), so this is 0 —
/// the same key type a classic consumer group resolves with.
const COORDINATOR_TYPE_GROUP: i8 = 0;

/// One share group as the list view shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareGroupInfo {
    pub group_id: String,
    pub state: String,
    pub member_count: u32,
}

/// One member of a share group, and what it is currently reading.
///
/// Unlike a consumer group's members, several of these can hold the SAME
/// topic-partition: that is the point of a share group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareGroupMember {
    pub member_id: String,
    pub client_id: String,
    pub assignments: Vec<TopicPartition>,
}

/// Where a share group has got to on one partition.
///
/// `start_offset` is `None` when the group has no share-partition state there
/// yet — the broker sends -1, and reporting that as an offset would invent a
/// position the group has never held. There is deliberately no lag here: a
/// share group's records are acknowledged individually, so "how far behind is
/// it" has no single-number answer the way it does for a consumer group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareGroupOffset {
    pub topic: String,
    pub partition: i32,
    pub start_offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareGroupDetail {
    pub group_id: String,
    pub state: String,
    pub members: Vec<ShareGroupMember>,
    pub offsets: Vec<ShareGroupOffset>,
}

/// Where a group's coordinator lives, as the cluster advertises it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coordinator {
    pub(crate) host: String,
    pub(crate) port: u16,
}

// ---------------------------------------------------------------------------
// The one sentence about the feature being off
// ---------------------------------------------------------------------------

/// Why share groups are not available, in the words that name the fix.
enum Missing<'a> {
    /// The broker's ApiVersions table has no ShareGroupDescribe at all.
    NoApi,
    /// The cluster's finalized `share.version`, which is below 1.
    FeatureOff(i16),
    /// The broker answered UNSUPPORTED_VERSION to a share-group call.
    Refused(&'a str),
}

/// The sentence the contract requires, plus which of the three ways the cluster
/// said it — and, for the two that a `kafka-features.sh` run fixes, that run.
fn unsupported(address: &str, missing: Missing<'_>) -> Error {
    let head = "this broker doesn't have share groups enabled (Kafka 4.1+ with share.version=1)";
    let turn_it_on = format!(
        "Turn them on with: kafka-features.sh --bootstrap-server {address} upgrade --feature \
         share.version=1"
    );
    Error::Other(match missing {
        Missing::NoApi => format!(
            "{head} — {address} does not answer {} (API key {}) at all, so it is running \
             something older than Kafka 4.1.",
            SHARE_GROUP_DESCRIBE.name, SHARE_GROUP_DESCRIBE.key
        ),
        Missing::FeatureOff(level) => {
            format!("{head} — {address} reports share.version={level}. {turn_it_on}")
        }
        Missing::Refused(api) => format!(
            "{head} — {address} answered {} to {api}, which is what a Kafka 4.1 broker says while \
             share.version is still 0. {turn_it_on}",
            errors::describe(errors::UNSUPPORTED_VERSION, None)
        ),
    })
}

/// Refuses, before any round trip, on a cluster that cannot have share groups.
///
/// The feature level is only trusted when the broker actually reported one:
/// `None` means it said nothing (see
/// [`super::conn::BrokerConnection::feature_level`]), and refusing on silence
/// would break a cluster that supports the feature perfectly well. In that case
/// the call goes out and the broker's own answer decides.
pub(crate) fn ensure_enabled(conn: &BrokerConnection) -> Result<()> {
    if !conn.supports(SHARE_GROUP_DESCRIBE) {
        return Err(unsupported(conn.address(), Missing::NoApi));
    }
    match conn.feature_level(SHARE_VERSION_FEATURE) {
        Some(level) if level < SHARE_VERSION_ENABLED => {
            Err(unsupported(conn.address(), Missing::FeatureOff(level)))
        }
        _ => Ok(()),
    }
}

/// One group-scoped error code, in the vocabulary of the screen it reaches.
///
/// UNSUPPORTED_VERSION is the disabled feature arriving the other way round —
/// see the module docs. The three coordinator codes are transient by nature and
/// say so, because "try again" is the correct next click for all three and
/// nothing else is.
fn check_group(
    what: &str,
    address: &str,
    api: Api,
    code: i16,
    message: Option<&str>,
) -> Result<()> {
    match code {
        errors::NONE => Ok(()),
        errors::UNSUPPORTED_VERSION => Err(unsupported(address, Missing::Refused(api.name))),
        errors::NOT_COORDINATOR | errors::COORDINATOR_NOT_AVAILABLE => Err(Error::Other(format!(
            "{what} failed: {} — the broker that coordinates this group has changed. Try again.",
            errors::describe(code, message)
        ))),
        errors::COORDINATOR_LOAD_IN_PROGRESS => Err(Error::Other(format!(
            "{what} failed: {} — the group coordinator is still loading its state. Try again in a \
             moment.",
            errors::describe(code, message)
        ))),
        _ => Err(Error::Other(format!(
            "{what} failed: {}",
            errors::describe(code, message)
        ))),
    }
}

/// The "no such group" message, shaped like [`crate::admin`]'s so a share group
/// and a consumer group go missing in the same words.
fn no_such_group(group_id: &str) -> Error {
    Error::Other(format!(
        "this cluster has no share group named \"{group_id}\" — a share group appears once an \
         application joins it, and disappears again when its last member leaves and its state \
         expires"
    ))
}

// ---------------------------------------------------------------------------
// ListGroups (16) v5
// ---------------------------------------------------------------------------

/// The share groups THIS broker coordinates, with their member counts.
///
/// Two round trips, on purpose: ListGroups carries no member count (it never
/// has), so the ids it returns are handed straight back to ShareGroupDescribe.
/// That second call needs no routing — the broker that listed a group is by
/// definition its coordinator.
pub(crate) fn list_on(conn: &mut BrokerConnection) -> Result<Vec<ShareGroupInfo>> {
    let listed = list_share_group_ids(conn)?;
    if listed.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<&str> = listed.iter().map(|(id, _)| id.as_str()).collect();
    let described = describe_on(conn, &ids)?;

    let mut out = Vec::with_capacity(described.len());
    for group in described {
        match group.error {
            // Listed a moment ago, gone now: the group's last member left
            // between the two calls. Dropping it is what the user would see on
            // the next refresh anyway; reporting it as a failure would make an
            // ordinary shutdown look like a broken cluster.
            Some(code) if code == errors::GROUP_ID_NOT_FOUND => continue,
            Some(code) => {
                check_group(
                    &format!("describing share group \"{}\"", group.group_id),
                    conn.address(),
                    SHARE_GROUP_DESCRIBE,
                    code,
                    group.error_message.as_deref(),
                )?;
            }
            None => {}
        }
        out.push(ShareGroupInfo {
            member_count: u32::try_from(group.members.len()).unwrap_or(u32::MAX),
            group_id: group.group_id,
            state: group.state,
        });
    }
    Ok(out)
}

/// ListGroups v5 filtered to share groups: `(group id, state)` per group.
fn list_share_group_ids(conn: &mut BrokerConnection) -> Result<Vec<(String, String)>> {
    let version = conn.negotiate(LIST_GROUPS)?;
    let payload = conn.call(LIST_GROUPS, version, list_request_body())?;
    decode_list(&payload, conn.address())
}

/// No states filter (every state), one type filter (`share`).
///
/// The type filter is the whole reason this module lists groups at all: without
/// it the broker answers with every group of every type, and the reply's
/// `GroupType` field would then have to be trusted to separate them — a filter
/// the BROKER applies is one the answer cannot be wrong about.
fn list_request_body() -> Vec<u8> {
    let mut body = Encoder::new();
    body.compact_array_len(Some(0)) // StatesFilter: none, i.e. every state
        .compact_array_len(Some(1))
        .compact_string(SHARE_GROUP_TYPE)
        .tagged_fields();
    body.finish()
}

fn decode_list(payload: &[u8], address: &str) -> Result<Vec<(String, String)>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;
    let code = decoder.int16()?;
    check_group("listing share groups", address, LIST_GROUPS, code, None)?;

    let count = decoder.compact_array_len()?.unwrap_or(0);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let group_id = decoder.compact_string()?;
        let _protocol_type = decoder.compact_string()?;
        let state = decoder.compact_string()?;
        let _group_type = decoder.compact_string()?;
        decoder.tagged_fields()?;
        out.push((group_id, state));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ShareGroupDescribe (77) v1
// ---------------------------------------------------------------------------

/// One group as ShareGroupDescribe reports it, error and all — the per-group
/// error is kept rather than raised here, because a batch describing several
/// groups must be able to fail one of them and answer about the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DescribedShareGroup {
    pub(crate) group_id: String,
    pub(crate) state: String,
    pub(crate) members: Vec<ShareGroupMember>,
    pub(crate) error: Option<i16>,
    pub(crate) error_message: Option<String>,
}

/// Describes one or more share groups on the broker that coordinates them.
pub(crate) fn describe_on(
    conn: &mut BrokerConnection,
    group_ids: &[&str],
) -> Result<Vec<DescribedShareGroup>> {
    let version = conn.negotiate(SHARE_GROUP_DESCRIBE)?;
    let payload = conn.call(
        SHARE_GROUP_DESCRIBE,
        version,
        describe_request_body(group_ids),
    )?;
    decode_describe(&payload)
}

/// One group's members and state, with the per-group error raised.
pub(crate) fn detail_on(
    conn: &mut BrokerConnection,
    group_id: &str,
) -> Result<(String, Vec<ShareGroupMember>)> {
    let address = conn.address().to_string();
    let group = describe_on(conn, &[group_id])?
        .into_iter()
        .find(|group| group.group_id == group_id)
        .ok_or_else(|| {
            Error::Other(format!(
                "{address} answered a describe of \"{group_id}\" without mentioning it"
            ))
        })?;
    if let Some(code) = group.error {
        if code == errors::GROUP_ID_NOT_FOUND {
            return Err(no_such_group(group_id));
        }
        // `error` is only ever `Some` for a non-NONE code, so this always
        // returns — the `?` is what makes that a fact rather than a comment.
        check_group(
            &format!("describing share group \"{group_id}\""),
            &address,
            SHARE_GROUP_DESCRIBE,
            code,
            group.error_message.as_deref(),
        )?;
    }
    Ok((group.state, group.members))
}

/// `IncludeAuthorizedOperations` is false: the contract has no field for an
/// operations bitfield, and asking for one costs the broker an authorizer pass
/// per group.
fn describe_request_body(group_ids: &[&str]) -> Vec<u8> {
    let mut body = Encoder::new();
    body.compact_array_len(Some(group_ids.len()));
    for group_id in group_ids {
        body.compact_string(group_id);
    }
    body.bool(false) // IncludeAuthorizedOperations
        .tagged_fields();
    body.finish()
}

fn decode_describe(payload: &[u8]) -> Result<Vec<DescribedShareGroup>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;

    let count = decoder.compact_array_len()?.unwrap_or(0);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let error_code = decoder.int16()?;
        let error_message = decoder.compact_nullable_string()?;
        let group_id = decoder.compact_string()?;
        let state = decoder.compact_string()?;
        let _group_epoch = decoder.int32()?;
        let _assignment_epoch = decoder.int32()?;
        let _assignor_name = decoder.compact_string()?;

        let member_count = decoder.compact_array_len()?.unwrap_or(0);
        let mut members = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            let member_id = decoder.compact_string()?;
            let _rack_id = decoder.compact_nullable_string()?;
            let _member_epoch = decoder.int32()?;
            let client_id = decoder.compact_string()?;
            let _client_host = decoder.compact_string()?;
            let subscribed = decoder.compact_array_len()?.unwrap_or(0);
            for _ in 0..subscribed {
                let _topic = decoder.compact_string()?;
            }
            let assignments = decode_assignment(&mut decoder)?;
            decoder.tagged_fields()?;
            members.push(ShareGroupMember {
                member_id,
                client_id,
                assignments,
            });
        }
        let _authorized_operations = decoder.int32()?;
        decoder.tagged_fields()?;

        members.sort_by(|a, b| a.member_id.cmp(&b.member_id));
        out.push(DescribedShareGroup {
            group_id,
            state,
            members,
            error: (error_code != errors::NONE).then_some(error_code),
            error_message,
        });
    }
    Ok(out)
}

/// The `Assignment` struct: topic-partitions, flattened into the pairs the
/// contract carries.
fn decode_assignment(decoder: &mut Decoder<'_>) -> Result<Vec<TopicPartition>> {
    let topics = decoder.compact_array_len()?.unwrap_or(0);
    let mut out = Vec::new();
    for _ in 0..topics {
        decoder.skip(16)?; // TopicId — the name is right behind it
        let topic = decoder.compact_string()?;
        for partition in decoder.compact_int32_array()? {
            out.push(TopicPartition {
                topic: topic.clone(),
                partition,
            });
        }
        decoder.tagged_fields()?;
    }
    decoder.tagged_fields()?; // the Assignment struct's own buffer
    out.sort_by(|a, b| a.topic.cmp(&b.topic).then(a.partition.cmp(&b.partition)));
    Ok(out)
}

// ---------------------------------------------------------------------------
// DescribeShareGroupOffsets (90) v0
// ---------------------------------------------------------------------------

/// Every share-partition start offset the group has state for.
///
/// A NULL topic list, which is this API's "all of them" — the alternative is
/// naming every topic the members subscribe to, which would miss the partitions
/// of a topic the group has state for but nobody is currently reading.
pub(crate) fn offsets_on(
    conn: &mut BrokerConnection,
    group_id: &str,
) -> Result<Vec<ShareGroupOffset>> {
    let version = conn.negotiate(DESCRIBE_SHARE_GROUP_OFFSETS)?;
    let payload = conn.call(
        DESCRIBE_SHARE_GROUP_OFFSETS,
        version,
        offsets_request_body(group_id),
    )?;
    decode_offsets(&payload, group_id, conn.address())
}

fn offsets_request_body(group_id: &str) -> Vec<u8> {
    let mut body = Encoder::new();
    body.compact_array_len(Some(1)) // one group
        .compact_string(group_id)
        .compact_array_len(None) // Topics: NULL = every topic-partition
        .tagged_fields() // group
        .tagged_fields();
    body.finish()
}

fn decode_offsets(payload: &[u8], group_id: &str, address: &str) -> Result<Vec<ShareGroupOffset>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;

    let groups = decoder.compact_array_len()?.unwrap_or(0);
    let mut out = Vec::new();
    for _ in 0..groups {
        let listed_group = decoder.compact_string()?;
        let topics = decoder.compact_array_len()?.unwrap_or(0);
        let mut rows = Vec::new();
        for _ in 0..topics {
            let topic = decoder.compact_string()?;
            decoder.skip(16)?; // TopicId
            let partitions = decoder.compact_array_len()?.unwrap_or(0);
            for _ in 0..partitions {
                let partition = decoder.int32()?;
                let start_offset = decoder.int64()?;
                let _leader_epoch = decoder.int32()?;
                let code = decoder.int16()?;
                let message = decoder.compact_nullable_string()?;
                decoder.tagged_fields()?;
                // A partition-level failure fails the CALL. The contract has no
                // per-row error field, so the alternatives are dropping the row
                // (a partition that silently is not there) or reporting the
                // error as a missing start offset (a partition that claims to
                // have no state when the truth is that we were not allowed to
                // look). Both are lies; this is not.
                check_group(
                    &format!("reading share group \"{listed_group}\" on {topic}-{partition}"),
                    address,
                    DESCRIBE_SHARE_GROUP_OFFSETS,
                    code,
                    message.as_deref(),
                )?;
                rows.push(ShareGroupOffset {
                    topic: topic.clone(),
                    partition,
                    // -1 is "this group has no share-partition state here yet".
                    start_offset: (start_offset >= 0).then_some(start_offset),
                });
            }
            decoder.tagged_fields()?;
        }
        let code = decoder.int16()?;
        let message = decoder.compact_nullable_string()?;
        decoder.tagged_fields()?;
        if code == errors::GROUP_ID_NOT_FOUND {
            return Err(no_such_group(group_id));
        }
        check_group(
            &format!("reading the offsets of share group \"{listed_group}\""),
            address,
            DESCRIBE_SHARE_GROUP_OFFSETS,
            code,
            message.as_deref(),
        )?;
        out.extend(rows);
    }
    out.sort_by(|a, b| a.topic.cmp(&b.topic).then(a.partition.cmp(&b.partition)));
    Ok(out)
}

// ---------------------------------------------------------------------------
// FindCoordinator (10) v4-6
// ---------------------------------------------------------------------------

/// Which broker coordinates this group.
///
/// Batched form (v4+) with a single key: the reply's `Coordinators` array
/// carries a per-key error, which is what makes "this group's coordinator is
/// not available" distinguishable from "the call failed".
pub(crate) fn find_coordinator(conn: &mut BrokerConnection, group_id: &str) -> Result<Coordinator> {
    let version = conn.negotiate(FIND_COORDINATOR)?;
    let payload = conn.call(
        FIND_COORDINATOR,
        version,
        coordinator_request_body(group_id),
    )?;
    decode_coordinator(&payload, group_id, conn.address())
}

fn coordinator_request_body(group_id: &str) -> Vec<u8> {
    let mut body = Encoder::new();
    // `Key` is v0-3 only and absent here; `KeyType` is v1+ and stays.
    body.int8(COORDINATOR_TYPE_GROUP)
        .compact_array_len(Some(1))
        .compact_string(group_id)
        .tagged_fields();
    body.finish()
}

fn decode_coordinator(payload: &[u8], group_id: &str, address: &str) -> Result<Coordinator> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;

    let count = decoder.compact_array_len()?.unwrap_or(0);
    for _ in 0..count {
        let key = decoder.compact_string()?;
        let _node_id = decoder.int32()?;
        let host = decoder.compact_string()?;
        let port = decoder.int32()?;
        let code = decoder.int16()?;
        let message = decoder.compact_nullable_string()?;
        decoder.tagged_fields()?;
        if key != group_id {
            continue;
        }
        check_group(
            &format!("finding the coordinator for group \"{group_id}\""),
            address,
            FIND_COORDINATOR,
            code,
            message.as_deref(),
        )?;
        let port = u16::try_from(port).map_err(|_| {
            Error::Other(format!(
                "{address} says group \"{group_id}\" is coordinated by {host}:{port}, which is not \
                 a usable port"
            ))
        })?;
        return Ok(Coordinator { host, port });
    }
    Err(Error::Other(format!(
        "{address} answered a coordinator lookup for \"{group_id}\" without mentioning it"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // -----------------------------------------------------------------------
    // Requests — golden bytes
    // -----------------------------------------------------------------------

    /// GOLDEN BYTES — ListGroups v5, filtered to share groups.
    ///
    /// 01              StatesFilter: compact array, EMPTY (0 + 1) — every state.
    ///                 Note this is not `00`, which is a NULL array; Kafka
    ///                 treats both as "no filter", and only one of them is what
    ///                 the schema calls an empty list.
    /// 02              TypesFilter: compact array, 1 entry
    /// 06 7368617265   "share" (5 + 1)
    /// 00              body tag buffer
    #[test]
    fn a_list_groups_v5_request_body_filters_to_share_groups() {
        assert_eq!(hex(&list_request_body()), "010206736861726500");

        let body = list_request_body();
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.compact_array_len().unwrap(), Some(0), "states");
        assert_eq!(decoder.compact_array_len().unwrap(), Some(1), "types");
        assert_eq!(decoder.compact_string().unwrap(), "share");
    }

    /// GOLDEN BYTES — ShareGroupDescribe v1 for one group.
    ///
    /// 02                    GroupIds: compact array, 1 entry
    /// 07 6f7264657273       "orders" (6 + 1)
    /// 00                    IncludeAuthorizedOperations = false
    /// 00                    body tag buffer
    #[test]
    fn a_share_group_describe_v1_request_body_is_byte_for_byte() {
        assert_eq!(
            hex(&describe_request_body(&["orders"])),
            "02076f72646572730000"
        );
        // Batched: the list form describes every group one broker listed in a
        // single round trip.
        let body = describe_request_body(&["a", "b"]);
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.compact_array_len().unwrap(), Some(2));
        assert_eq!(decoder.compact_string().unwrap(), "a");
        assert_eq!(decoder.compact_string().unwrap(), "b");
        assert!(!decoder.bool().unwrap(), "authorized operations");
    }

    /// GOLDEN BYTES — DescribeShareGroupOffsets v0.
    ///
    /// 02              Groups: compact array, 1 entry
    /// 07 6f7264657273 "orders" as the group id
    /// 00              Topics: NULL — every topic-partition the group has state
    ///                 for. `01` (an EMPTY array) would ask about nothing.
    /// 00              group tag buffer
    /// 00              body tag buffer
    #[test]
    fn a_describe_share_group_offsets_v0_request_asks_about_every_topic() {
        assert_eq!(
            hex(&offsets_request_body("orders")),
            "02076f7264657273000000"
        );
        let body = offsets_request_body("orders");
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.compact_array_len().unwrap(), Some(1));
        assert_eq!(decoder.compact_string().unwrap(), "orders");
        assert_eq!(
            decoder.compact_array_len().unwrap(),
            None,
            "a null topic list, not an empty one"
        );
    }

    /// GOLDEN BYTES — FindCoordinator v6 for a group key.
    ///
    /// 00              KeyType = 0 (GROUP)
    /// 02              CoordinatorKeys: compact array, 1 entry
    /// 05 626f7373     "boss" (4 + 1)
    /// 00              body tag buffer
    #[test]
    fn a_find_coordinator_v6_request_body_is_byte_for_byte() {
        assert_eq!(hex(&coordinator_request_body("boss")), "000205626f737300");
        let body = coordinator_request_body("boss");
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.int8().unwrap(), 0, "GROUP");
        assert_eq!(decoder.compact_array_len().unwrap(), Some(1));
        assert_eq!(decoder.compact_string().unwrap(), "boss");
    }

    // -----------------------------------------------------------------------
    // Responses
    // -----------------------------------------------------------------------

    fn list_response(groups: &[(&str, &str)]) -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int32(0) // throttle
            .int16(0) // error code
            .compact_array_len(Some(groups.len()));
        for (group_id, state) in groups {
            enc.compact_string(group_id)
                .compact_string("share") // ProtocolType
                .compact_string(state)
                .compact_string("share") // GroupType
                .tagged_fields();
        }
        enc.tagged_fields();
        enc.finish()
    }

    #[test]
    fn a_list_response_yields_each_group_and_its_state() {
        let groups = decode_list(
            &list_response(&[("work-queue", "Stable"), ("dlq-drain", "Empty")]),
            "localhost:9092",
        )
        .expect("decode");
        assert_eq!(
            groups,
            vec![
                ("work-queue".to_string(), "Stable".to_string()),
                ("dlq-drain".to_string(), "Empty".to_string()),
            ]
        );
        assert_eq!(decode_list(&list_response(&[]), "b:9092").unwrap(), vec![]);
    }

    /// One canned member: `(member_id, client_id, [(topic, [partitions])])`.
    type CannedMember<'a> = (&'a str, &'a str, Vec<(&'a str, Vec<i32>)>);
    /// One canned described group: `(error code, error message, group id,
    /// state, members)`.
    type CannedGroup<'a> = (
        i16,
        Option<&'a str>,
        &'a str,
        &'a str,
        Vec<CannedMember<'a>>,
    );

    fn describe_response(groups: &[CannedGroup<'_>]) -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int32(0).compact_array_len(Some(groups.len()));
        for (code, message, group_id, state, members) in groups {
            enc.int16(*code)
                .compact_nullable_string(*message)
                .compact_string(group_id)
                .compact_string(state)
                .int32(7) // GroupEpoch
                .int32(7) // AssignmentEpoch
                .compact_string("simple") // AssignorName
                .compact_array_len(Some(members.len()));
            for (member_id, client_id, assignment) in members {
                enc.compact_string(member_id)
                    .compact_nullable_string(None) // RackId
                    .int32(3) // MemberEpoch
                    .compact_string(client_id)
                    .compact_string("/10.0.0.7") // ClientHost
                    .compact_array_len(Some(assignment.len()));
                for (topic, _) in assignment {
                    enc.compact_string(topic); // SubscribedTopicNames
                }
                enc.compact_array_len(Some(assignment.len()));
                for (topic, partitions) in assignment {
                    enc.zero_uuid()
                        .compact_string(topic)
                        .compact_int32_array(Some(partitions))
                        .tagged_fields();
                }
                enc.tagged_fields() // Assignment struct
                    .tagged_fields(); // member
            }
            enc.int32(i32::MIN) // AuthorizedOperations
                .tagged_fields();
        }
        enc.tagged_fields();
        enc.finish()
    }

    #[test]
    fn a_describe_response_yields_members_and_their_assignments() {
        let described = decode_describe(&describe_response(&[(
            0,
            None,
            "work-queue",
            "Stable",
            vec![
                ("m-2", "worker-b", vec![("jobs", vec![0, 1])]),
                ("m-1", "worker-a", vec![("jobs", vec![0]), ("dlq", vec![2])]),
            ],
        )]))
        .expect("decode");

        assert_eq!(described.len(), 1);
        let group = &described[0];
        assert_eq!(group.group_id, "work-queue");
        assert_eq!(group.state, "Stable");
        assert_eq!(group.error, None);
        // Members come back in a stable order, whatever the broker's was.
        assert_eq!(
            group
                .members
                .iter()
                .map(|m| &m.member_id)
                .collect::<Vec<_>>(),
            vec!["m-1", "m-2"]
        );
        assert_eq!(
            group.members[0].assignments,
            vec![
                TopicPartition {
                    topic: "dlq".into(),
                    partition: 2
                },
                TopicPartition {
                    topic: "jobs".into(),
                    partition: 0
                },
            ]
        );
        // THE share-group shape: two members holding the same partition.
        assert!(group.members[1].assignments.contains(&TopicPartition {
            topic: "jobs".into(),
            partition: 0
        }));
    }

    /// The per-group error is carried, not raised — a batch that describes four
    /// groups must be able to fail one and answer about three.
    #[test]
    fn a_per_group_describe_error_is_carried_on_the_group() {
        let described = decode_describe(&describe_response(&[
            (
                errors::GROUP_ID_NOT_FOUND,
                Some("Group gone not found."),
                "gone",
                "",
                vec![],
            ),
            (0, None, "here", "Empty", vec![]),
        ]))
        .expect("decode");
        assert_eq!(described[0].error, Some(errors::GROUP_ID_NOT_FOUND));
        assert_eq!(
            described[0].error_message.as_deref(),
            Some("Group gone not found.")
        );
        assert_eq!(described[1].error, None);
    }

    /// A group that vanished between the list and the describe is dropped, not
    /// reported: its last member left, which is not a failure of anything.
    #[test]
    fn a_group_that_disappears_between_the_two_calls_is_dropped_from_the_list() {
        let described = decode_describe(&describe_response(&[
            (errors::GROUP_ID_NOT_FOUND, None, "gone", "", vec![]),
            (0, None, "here", "Stable", vec![("m", "c", vec![])]),
        ]))
        .expect("decode");
        let kept: Vec<ShareGroupInfo> = described
            .into_iter()
            .filter(|g| g.error != Some(errors::GROUP_ID_NOT_FOUND))
            .map(|g| ShareGroupInfo {
                member_count: g.members.len() as u32,
                group_id: g.group_id,
                state: g.state,
            })
            .collect();
        assert_eq!(
            kept,
            vec![ShareGroupInfo {
                group_id: "here".into(),
                state: "Stable".into(),
                member_count: 1,
            }]
        );
    }

    /// One canned topic's partitions: `(partition, start offset, error code)`.
    type CannedTopic<'a> = (&'a str, Vec<(i32, i64, i16)>);

    fn offsets_response(group_id: &str, group_error: i16, topics: &[CannedTopic<'_>]) -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int32(0)
            .compact_array_len(Some(1))
            .compact_string(group_id)
            .compact_array_len(Some(topics.len()));
        for (topic, partitions) in topics {
            enc.compact_string(topic)
                .zero_uuid()
                .compact_array_len(Some(partitions.len()));
            for (partition, start_offset, code) in partitions {
                enc.int32(*partition)
                    .int64(*start_offset)
                    .int32(4) // LeaderEpoch
                    .int16(*code)
                    .compact_nullable_string(None)
                    .tagged_fields();
            }
            enc.tagged_fields();
        }
        enc.int16(group_error)
            .compact_nullable_string(None)
            .tagged_fields()
            .tagged_fields();
        enc.finish()
    }

    #[test]
    fn an_offsets_response_sorts_and_keeps_minus_one_as_no_state() {
        let offsets = decode_offsets(
            &offsets_response(
                "work-queue",
                0,
                &[
                    ("jobs", vec![(1, 500, 0), (0, 42, 0)]),
                    ("dlq", vec![(0, -1, 0)]),
                ],
            ),
            "work-queue",
            "localhost:9092",
        )
        .expect("decode");

        assert_eq!(
            offsets,
            vec![
                ShareGroupOffset {
                    topic: "dlq".into(),
                    partition: 0,
                    // -1 is "no share-partition state yet", not offset -1.
                    start_offset: None,
                },
                ShareGroupOffset {
                    topic: "jobs".into(),
                    partition: 0,
                    start_offset: Some(42),
                },
                ShareGroupOffset {
                    topic: "jobs".into(),
                    partition: 1,
                    start_offset: Some(500),
                },
            ]
        );
    }

    #[test]
    fn a_partition_level_offsets_error_fails_the_call_naming_the_partition() {
        let err = decode_offsets(
            &offsets_response("work-queue", 0, &[("jobs", vec![(3, -1, 29)])]),
            "work-queue",
            "localhost:9092",
        )
        .expect_err("TOPIC_AUTHORIZATION_FAILED");
        let message = err.to_string();
        assert!(message.contains("jobs-3"), "got {message}");
        assert!(
            message.contains("TOPIC_AUTHORIZATION_FAILED (29)"),
            "got {message}"
        );
    }

    #[test]
    fn a_group_level_offsets_error_of_not_found_names_the_group() {
        let err = decode_offsets(
            &offsets_response("ghost", errors::GROUP_ID_NOT_FOUND, &[]),
            "ghost",
            "localhost:9092",
        )
        .expect_err("no such group");
        let message = err.to_string();
        assert!(
            message.contains("no share group named \"ghost\""),
            "{message}"
        );
    }

    fn coordinator_response(entries: &[(&str, &str, i32, i16)]) -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int32(0).compact_array_len(Some(entries.len()));
        for (key, host, port, code) in entries {
            enc.compact_string(key)
                .int32(1) // NodeId
                .compact_string(host)
                .int32(*port)
                .int16(*code)
                .compact_nullable_string(None)
                .tagged_fields();
        }
        enc.tagged_fields();
        enc.finish()
    }

    #[test]
    fn a_coordinator_response_yields_the_endpoint_for_the_key_that_was_asked_about() {
        let found = decode_coordinator(
            &coordinator_response(&[
                ("other-group", "broker-9", 9092, 0),
                ("work-queue", "broker-2", 9094, 0),
            ]),
            "work-queue",
            "localhost:9092",
        )
        .expect("decode");
        assert_eq!(
            found,
            Coordinator {
                host: "broker-2".into(),
                port: 9094,
            }
        );
    }

    #[test]
    fn a_coordinator_that_is_not_available_says_to_try_again() {
        let err = decode_coordinator(
            &coordinator_response(&[("work-queue", "", -1, errors::COORDINATOR_NOT_AVAILABLE)]),
            "work-queue",
            "localhost:9092",
        )
        .expect_err("no coordinator");
        let message = err.to_string();
        assert!(
            message.contains("COORDINATOR_NOT_AVAILABLE (15)"),
            "{message}"
        );
        assert!(message.contains("Try again"), "{message}");
    }

    // -----------------------------------------------------------------------
    // The feature being off
    // -----------------------------------------------------------------------

    /// THE mapping the contract names, against the code the dev broker actually
    /// answers.
    ///
    /// apache/kafka:4.1.0 with `share.version=0` advertises ShareGroupDescribe
    /// (API key 77) in its ApiVersions table and then answers UNSUPPORTED_VERSION
    /// (35) to every call — checked by hand against dev/docker-compose.yml
    /// before that file enabled the feature, and by
    /// `kafka-share-groups.sh --describe`, which reports the same 35 as
    /// "unexpected error UNSUPPORTED_VERSION". So this code, on this API, is not
    /// a version-negotiation failure: it is the feature switch, and it has to
    /// read as one.
    #[test]
    fn the_brokers_unsupported_version_becomes_the_feature_sentence() {
        let err = check_group(
            "describing share group \"work-queue\"",
            "localhost:9092",
            SHARE_GROUP_DESCRIBE,
            errors::UNSUPPORTED_VERSION,
            None,
        )
        .expect_err("35 on a share call means the feature is off");
        let message = err.to_string();
        assert!(
            message.contains(
                "this broker doesn't have share groups enabled (Kafka 4.1+ with share.version=1)"
            ),
            "got {message}"
        );
        // What happened, then the next click (docs/DESIGN.md §7).
        assert!(
            message.contains("UNSUPPORTED_VERSION (35)"),
            "got {message}"
        );
        assert!(
            message.contains("upgrade --feature share.version=1"),
            "got {message}"
        );
        assert!(message.contains("localhost:9092"), "got {message}");
    }

    /// The same sentence for the cluster that says so up front, and for the
    /// Kafka that has never heard of the API — one sentence, three routes to
    /// it, because they are one situation to the person reading it.
    #[test]
    fn every_route_to_the_feature_being_off_says_the_same_sentence() {
        let head =
            "this broker doesn't have share groups enabled (Kafka 4.1+ with share.version=1)";
        let no_api = unsupported("kafka-1:9092", Missing::NoApi).to_string();
        assert!(no_api.contains(head), "got {no_api}");
        assert!(no_api.contains("API key 77"), "got {no_api}");
        assert!(no_api.contains("older than Kafka 4.1"), "got {no_api}");
        // Nothing to turn on: this cluster needs upgrading, not configuring.
        assert!(!no_api.contains("kafka-features.sh"), "got {no_api}");

        let off = unsupported("kafka-1:9092", Missing::FeatureOff(0)).to_string();
        assert!(off.contains(head), "got {off}");
        assert!(off.contains("share.version=0"), "got {off}");
        assert!(
            off.contains(
                "kafka-features.sh --bootstrap-server kafka-1:9092 upgrade --feature \
                 share.version=1"
            ),
            "got {off}"
        );
    }

    /// A group-level code that is none of the special ones still reaches the
    /// user with Kafka's own name for it.
    #[test]
    fn an_ordinary_group_error_keeps_kafkas_token() {
        let err = check_group(
            "listing share groups",
            "localhost:9092",
            LIST_GROUPS,
            30, // GROUP_AUTHORIZATION_FAILED
            Some("Not authorized to access group"),
        )
        .expect_err("30");
        let message = err.to_string();
        assert!(message.contains("listing share groups failed"), "{message}");
        assert!(
            message.contains("GROUP_AUTHORIZATION_FAILED (30)"),
            "{message}"
        );
        assert!(message.contains("Not authorized"), "{message}");
    }

    // -----------------------------------------------------------------------
    // The IPC contract
    // -----------------------------------------------------------------------

    /// The field names ARE the contract (the TypeScript in `apps/desktop/src`
    /// mirrors them), and the live suite cannot pin them: nothing in this
    /// repository can join a share group, so a populated detail never comes off
    /// a real broker. It comes off the decoder instead.
    #[test]
    fn the_wire_types_serialise_with_the_contract_field_names() {
        let described = decode_describe(&describe_response(&[(
            0,
            None,
            "work-queue",
            "Stable",
            vec![("m-1", "worker-a", vec![("jobs", vec![0, 1])])],
        )]))
        .expect("decode");
        let group = described.into_iter().next().expect("one group");
        let detail = ShareGroupDetail {
            group_id: group.group_id,
            state: group.state,
            members: group.members,
            offsets: decode_offsets(
                &offsets_response("work-queue", 0, &[("jobs", vec![(0, 42, 0), (1, -1, 0)])]),
                "work-queue",
                "localhost:9092",
            )
            .expect("decode offsets"),
        };

        let json = serde_json::to_value(&detail).expect("serialise");
        assert_eq!(json["group_id"], "work-queue");
        assert_eq!(json["state"], "Stable");
        assert_eq!(json["members"][0]["member_id"], "m-1");
        assert_eq!(json["members"][0]["client_id"], "worker-a");
        assert_eq!(json["members"][0]["assignments"][0]["topic"], "jobs");
        assert_eq!(json["members"][0]["assignments"][0]["partition"], 0);
        assert_eq!(json["offsets"][0]["topic"], "jobs");
        assert_eq!(json["offsets"][0]["partition"], 0);
        assert_eq!(json["offsets"][0]["start_offset"], 42);
        // `start_offset: null` is the contract's "no state here yet" — not a
        // missing key, and not a zero.
        assert!(json["offsets"][1]["start_offset"].is_null());

        let listed = serde_json::to_value(ShareGroupInfo {
            group_id: "work-queue".into(),
            state: "Stable".into(),
            member_count: 3,
        })
        .expect("serialise");
        assert_eq!(listed["group_id"], "work-queue");
        assert_eq!(listed["state"], "Stable");
        assert_eq!(listed["member_count"], 3);
    }

    #[test]
    fn a_truncated_reply_is_an_error_rather_than_a_panic() {
        let full = describe_response(&[(0, None, "work-queue", "Stable", vec![])]);
        let err = decode_describe(&full[..full.len() / 2]).expect_err("truncated");
        assert!(err.to_string().contains("could not decode"), "got {err}");
    }
}
