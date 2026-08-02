//! Streaming unbounded search (docs/ARCHITECTURE.md D3). Parallel partition
//! consumers, raw-byte prefilters, CEL push-down filters after
//! deserialization, progressive results over a bounded channel, cooperative
//! cancellation. No fetch-limit pre-commit — this is the feature Offset
//! Explorer's bounded search loses on.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub topic: String,
    /// None = all partitions.
    pub partitions: Option<Vec<u32>>,
    pub start: SeekPosition,
    pub end: SeekPosition,
    /// Cheap pre-deserialization filter applied to raw bytes.
    pub raw_prefilter: Option<String>,
    /// CEL expression evaluated against the decoded message.
    pub cel_filter: Option<String>,
    pub live_tail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeekPosition {
    Earliest,
    Latest,
    Offset { offset: i64 },
    Timestamp { epoch_ms: i64 },
}
