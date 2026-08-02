//! Serde pipeline (docs/ARCHITECTURE.md D4):
//! bytes -> SR-framing detection (magic 0x00 + schema id) -> decoder ->
//! canonical JSON + schema metadata. Custom serdes (Pro) arrive later as
//! sandboxed WASM modules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Json,
    Avro,
    Protobuf,
    JsonSchema,
    Xml,
    MessagePack,
    Cbor,
    Utf8,
    Hex,
}

/// A decoded message value plus how we decoded it — the "which schema decoded
/// this message" display is a top-voted ask across every competitor tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoded {
    pub format: Format,
    pub value: serde_json::Value,
    pub schema_subject: Option<String>,
    pub schema_version: Option<u32>,
    pub schema_id: Option<u32>,
}
