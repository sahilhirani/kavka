//! Admin operations: topics, configs, consumer groups, ACLs, quotas,
//! partition reassignment. Implemented over rdkafka's AdminClient where
//! possible; gaps (e.g. KIP-932 Share Groups) go through hand-rolled protocol
//! frames per docs/ARCHITECTURE.md D2.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicInfo {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u16,
    pub internal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub partition: u32,
    pub leader: i32,
    pub replicas: Vec<i32>,
    pub isr: Vec<i32>,
    pub earliest_offset: i64,
    pub latest_offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupLag {
    pub group: String,
    pub topic: String,
    pub partition: u32,
    pub committed: i64,
    pub end: i64,
    pub lag: i64,
}
