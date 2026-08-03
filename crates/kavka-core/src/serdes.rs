//! Serde pipeline (docs/ARCHITECTURE.md D4):
//! bytes -> SR-framing detection (magic 0x00 + schema id) -> decoder ->
//! canonical JSON + schema metadata. Custom serdes arrive later (Phase 5) as
//! sandboxed WASM modules.
//!
//! # Detection order, and why it is that order
//!
//! Every step below is a *guess* about bytes that carry no type tag, so the
//! ladder is ordered by how much evidence each format leaves behind:
//!
//! 1. **Confluent framing** — a literal magic byte plus a registry-assigned
//!    schema id. This is the only step with an external source of truth, so it
//!    goes first and, once matched, does not fall through to the guesses: a
//!    payload that says "I am schema 217" and cannot be decoded is reported as
//!    undecodable-with-a-schema-id, never silently re-read as something else.
//!    JSON cannot start with `0x00`, so nothing legitimate is stolen here.
//! 2. **JSON** — a self-delimiting grammar that fails loudly on anything else.
//!    Restricted to a top-level object or array (see [`looks_like_json`]).
//! 3. **UTF-8** — before the binary tree formats, because MessagePack and CBOR
//!    both use byte ranges (0x80–0xBF as leading bytes) that are *invalid* as
//!    UTF-8 starts, so real MessagePack/CBOR documents essentially never
//!    survive a UTF-8 check, while real text frequently parses as some
//!    nonsense MessagePack value.
//! 4. **MessagePack**, then **CBOR** — MessagePack first only because its
//!    fixmap/fixarray/fixstr prefixes make accidental matches rarer than
//!    CBOR's. Both require the whole buffer to be consumed by exactly one
//!    top-level map or array, which is what keeps arbitrary binary out.
//! 5. **Hex** — the honest fallback. Never fails, never lies.
//!
//! The user always gets the last word: the payload inspector's `Read as…`
//! (docs/DESIGN.md §5.10) re-runs a chosen decoder over the same bytes.

use crate::sr::{RegisteredSchema, SchemaKind, SchemaRegistry};
use crate::{Error, Result};
use apache_avro::Schema as AvroSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Display cap when a caller doesn't specify one. Matches the payload
/// inspector's own perf guard (docs/DESIGN.md §5.10: "payloads over 256 KB
/// render as raw text").
pub const DEFAULT_MAX_VALUE_BYTES: usize = 262_144;

/// Confluent wire format: `[0x00][4-byte big-endian schema id][payload]`.
const CONFLUENT_MAGIC: u8 = 0x00;
const CONFLUENT_HEADER_LEN: usize = 5;

/// How the bytes were read. Serializes to exactly the strings in the Phase 1
/// IPC contract (`"json" | "utf8" | "avro" | "msgpack" | "cbor" | "hex"`) — an
/// enum rather than a `String` so a typo is a compile error rather than a
/// silently unstyled row in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
    Utf8,
    Avro,
    #[serde(rename = "msgpack")]
    MsgPack,
    Cbor,
    Hex,
}

/// Which schema decoded (or failed to decode) a payload. The id survives even
/// when the registry is unreachable — "Avro, schema 217, registry unreachable"
/// is a far more actionable thing to show than a wall of hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaMeta {
    pub schema_id: u32,
    pub subject: Option<String>,
    pub version: Option<i32>,
}

/// A decoded key or value: the display form, the structured form when there is
/// one, and how we got there. `raw_len` is always the true byte length on the
/// wire, even when `text` was cut short.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedPayload {
    pub encoding: Encoding,
    /// Pretty display form: pretty-printed JSON for the structured encodings,
    /// the text itself for UTF-8, space-separated byte pairs for hex.
    pub text: String,
    /// The canonical JSON tree, when the encoding produced one.
    pub json: Option<serde_json::Value>,
    pub raw_len: usize,
    /// `text` shows less than `raw_len` bytes' worth of the payload.
    pub truncated: bool,
    pub schema: Option<SchemaMeta>,
}

/// One Kafka record header. `value` is `None` only for a genuinely null header
/// value (Kafka allows them); `is_text` says whether the rendered `value` is
/// the header's own text or a hex rendering of bytes that were not text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub key: String,
    pub value: Option<String>,
    pub is_text: bool,
}

/// One record as the UI consumes it. A `value` of `None` is a tombstone — the
/// distinction between "no value" and "an empty value" is load-bearing on a
/// compacted topic, so it is carried in the type rather than as an empty
/// payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub partition: i32,
    pub offset: i64,
    pub timestamp_ms: Option<i64>,
    pub key: Option<DecodedPayload>,
    pub value: Option<DecodedPayload>,
    pub headers: Vec<HeaderEntry>,
}

/// Decodes one key or value.
///
/// `registry` is the session's Schema Registry client, or `None` when the
/// profile configures no registry. `max_display_bytes` caps what `text` shows;
/// it never changes `raw_len`.
pub fn decode(
    bytes: &[u8],
    registry: Option<&SchemaRegistry>,
    max_display_bytes: usize,
) -> DecodedPayload {
    let raw_len = bytes.len();
    let framing = confluent_schema_id(bytes);

    // Perf guard (docs/DESIGN.md §5.10). Over the cap we never attempt a
    // structured parse: the point of the cap is that the user is not made to
    // wait on a multi-megabyte payload they cannot read anyway, and parsing it
    // only to throw away all but the first 256 KB of the rendering is the one
    // way to spend the whole cost and keep none of the benefit.
    if raw_len > max_display_bytes {
        let head = &bytes[..max_display_bytes];
        let (encoding, text) = match utf8_prefix(head).filter(|s| is_displayable(s)) {
            Some(text) => (Encoding::Utf8, text.to_string()),
            None => (Encoding::Hex, hex_text(head)),
        };
        return DecodedPayload {
            encoding,
            text,
            json: None,
            raw_len,
            truncated: true,
            // The framing bytes are in the first five, so the id survives even
            // here: the inspector can still name the subject it would decode as.
            schema: framing.map(schema_meta_id_only),
        };
    }

    if let Some(schema_id) = framing {
        return decode_confluent(bytes, schema_id, registry);
    }

    if let Some(json) = decode_json(bytes) {
        return structured(Encoding::Json, json, raw_len, None);
    }
    if let Some(text) = utf8_prefix(bytes).filter(|s| is_displayable(s)) {
        return DecodedPayload {
            encoding: Encoding::Utf8,
            text: text.to_string(),
            json: None,
            raw_len,
            truncated: false,
            schema: None,
        };
    }
    if let Some(json) = decode_msgpack(bytes) {
        return structured(Encoding::MsgPack, json, raw_len, None);
    }
    if let Some(json) = decode_cbor(bytes) {
        return structured(Encoding::Cbor, json, raw_len, None);
    }
    hex_payload(bytes, None)
}

/// Decodes a header value. Header bytes are short by construction (Kafka
/// rejects oversized record batches long before a header gets interesting), so
/// this is text-or-hex with no ladder and no cap.
pub fn decode_header(key: &str, value: Option<&[u8]>) -> HeaderEntry {
    match value {
        None => HeaderEntry {
            key: key.to_string(),
            value: None,
            is_text: false,
        },
        Some(bytes) => match std::str::from_utf8(bytes)
            .ok()
            .filter(|s| is_displayable(s))
        {
            Some(text) => HeaderEntry {
                key: key.to_string(),
                value: Some(text.to_string()),
                is_text: true,
            },
            None => HeaderEntry {
                key: key.to_string(),
                value: Some(hex_text(bytes)),
                is_text: false,
            },
        },
    }
}

/// The schema id in a Confluent-framed payload, if these bytes carry the
/// framing at all.
fn confluent_schema_id(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < CONFLUENT_HEADER_LEN || bytes[0] != CONFLUENT_MAGIC {
        return None;
    }
    Some(u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]))
}

fn schema_meta_id_only(schema_id: u32) -> SchemaMeta {
    SchemaMeta {
        schema_id,
        subject: None,
        version: None,
    }
}

fn schema_meta(schema_id: u32, registered: &RegisteredSchema) -> SchemaMeta {
    SchemaMeta {
        schema_id,
        subject: registered.subject.clone(),
        version: registered.version,
    }
}

/// The framed path. Any failure — no registry configured, registry
/// unreachable, a schema type we cannot decode, or bytes that do not match the
/// schema — degrades to hex **with the schema metadata still attached**, and
/// records the reason once on the registry so the session reports it once
/// rather than once per message.
fn decode_confluent(
    bytes: &[u8],
    schema_id: u32,
    registry: Option<&SchemaRegistry>,
) -> DecodedPayload {
    let body = &bytes[CONFLUENT_HEADER_LEN..];
    let Some(registry) = registry else {
        // No registry configured is not an error worth a banner: the user may
        // simply not have one. The schema id in the metadata is the hint.
        return hex_payload(bytes, Some(schema_meta_id_only(schema_id)));
    };
    let Some(registered) = registry.lookup(schema_id) else {
        return hex_payload(bytes, Some(schema_meta_id_only(schema_id)));
    };
    let meta = schema_meta(schema_id, &registered);

    match &registered.kind {
        SchemaKind::Avro(schema) => {
            // A single Avro datum, not an object-container file: the framing
            // replaces the container's header, which is the whole point of the
            // Confluent wire format.
            let mut reader = body;
            match apache_avro::from_avro_datum(schema, &mut reader, None)
                .map_err(|e| e.to_string())
                .and_then(|value| serde_json::Value::try_from(value).map_err(|e| e.to_string()))
            {
                Ok(json) => structured(Encoding::Avro, json, bytes.len(), Some(meta)),
                Err(cause) => {
                    registry.note_error(format!(
                        "schema {schema_id}{} decoded no Avro from a message: {cause}",
                        subject_suffix(&registered)
                    ));
                    hex_payload(bytes, Some(meta))
                }
            }
        }
        // A JSON-Schema subject frames plain JSON after the header, so the
        // registry lookup buys the subject/version display and the payload
        // decodes with no schema involvement at all.
        SchemaKind::Json => match decode_json(body) {
            Some(json) => structured(Encoding::Json, json, bytes.len(), Some(meta)),
            None => hex_payload(bytes, Some(meta)),
        },
        SchemaKind::Unsupported(kind) => {
            registry.note_error(format!(
                "schema {schema_id}{} is {kind}, which this build cannot decode yet",
                subject_suffix(&registered)
            ));
            hex_payload(bytes, Some(meta))
        }
    }
}

fn subject_suffix(registered: &RegisteredSchema) -> String {
    match &registered.subject {
        Some(subject) => format!(" ({subject})"),
        None => String::new(),
    }
}

fn structured(
    encoding: Encoding,
    json: serde_json::Value,
    raw_len: usize,
    schema: Option<SchemaMeta>,
) -> DecodedPayload {
    DecodedPayload {
        encoding,
        text: serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string()),
        json: Some(json),
        raw_len,
        truncated: false,
        schema,
    }
}

fn hex_payload(bytes: &[u8], schema: Option<SchemaMeta>) -> DecodedPayload {
    DecodedPayload {
        encoding: Encoding::Hex,
        text: hex_text(bytes),
        json: None,
        raw_len: bytes.len(),
        truncated: false,
        schema,
    }
}

/// Space-separated byte pairs — the form the inspector's Hex tab shows and the
/// form a human can read an offset out of.
fn hex_text(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(3));
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// JSON detection, deliberately narrower than "serde_json accepts it": a bare
/// `42`, `true` or `"done"` is valid JSON *and* is exactly what a key like
/// `42` looks like, so accepting scalars would relabel half the plain-text
/// keys in a cluster as JSON. A document is an object or an array.
fn looks_like_json(bytes: &[u8]) -> bool {
    matches!(
        bytes.iter().find(|b| !b.is_ascii_whitespace()),
        Some(b'{') | Some(b'[')
    )
}

fn decode_json(bytes: &[u8]) -> Option<serde_json::Value> {
    if !looks_like_json(bytes) {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}

/// The longest valid-UTF-8 prefix, accepting a multi-byte character cut in
/// half by the display cap (`error_len() == None` is precisely "ran out of
/// input mid-character") but rejecting bytes that are invalid on their own
/// terms.
fn utf8_prefix(bytes: &[u8]) -> Option<&str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&bytes[..e.valid_up_to()]).ok(),
        Err(_) => None,
    }
}

/// Whether a string is text a human was meant to read. Binary that happens to
/// be valid UTF-8 is common (any byte under 0x80 is), and rendering it as text
/// produces a row of invisible control characters that looks like an empty
/// message — worse than hex, which at least tells the truth.
fn is_displayable(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
}

/// MessagePack, requiring the whole buffer to be one map or array. Both halves
/// of that matter: trailing bytes mean we guessed wrong, and a top-level
/// scalar is the shape random binary most often fakes.
fn decode_msgpack(bytes: &[u8]) -> Option<serde_json::Value> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor).ok()?;
    if cursor.position() != bytes.len() as u64 {
        return None;
    }
    if !matches!(value, rmpv::Value::Map(_) | rmpv::Value::Array(_)) {
        return None;
    }
    Some(msgpack_to_json(value))
}

fn msgpack_to_json(value: rmpv::Value) -> serde_json::Value {
    use rmpv::Value as V;
    match value {
        V::Nil => serde_json::Value::Null,
        V::Boolean(b) => serde_json::Value::Bool(b),
        V::Integer(i) => integer_to_json(i.as_i64(), i.as_u64(), i.as_f64()),
        V::F32(f) => float_to_json(f.into()),
        V::F64(f) => float_to_json(f),
        V::String(s) => match s.into_str() {
            Some(text) => serde_json::Value::String(text),
            // A MessagePack "str" whose bytes are not UTF-8 is malformed; show
            // the bytes rather than replacement characters.
            None => serde_json::Value::String("<invalid utf-8>".into()),
        },
        V::Binary(bytes) => bytes_to_json(&bytes),
        V::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(msgpack_to_json).collect())
        }
        V::Map(entries) => serde_json::Value::Object(
            entries
                .into_iter()
                .map(|(k, v)| (map_key(msgpack_to_json(k)), msgpack_to_json(v)))
                .collect(),
        ),
        V::Ext(tag, bytes) => serde_json::json!({
            "ext_type": tag,
            "data": bytes_to_json(&bytes),
        }),
    }
}

/// CBOR, under the same whole-buffer-one-document rule as MessagePack. CBOR is
/// the most permissive grammar in the ladder — a great many byte strings are
/// technically valid CBOR — which is why it sits last before hex.
fn decode_cbor(bytes: &[u8]) -> Option<serde_json::Value> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value: ciborium::value::Value = ciborium::de::from_reader(&mut cursor).ok()?;
    if cursor.position() != bytes.len() as u64 {
        return None;
    }
    if !matches!(
        value,
        ciborium::value::Value::Map(_) | ciborium::value::Value::Array(_)
    ) {
        return None;
    }
    Some(cbor_to_json(value))
}

fn cbor_to_json(value: ciborium::value::Value) -> serde_json::Value {
    use ciborium::value::Value as V;
    match value {
        V::Null => serde_json::Value::Null,
        V::Bool(b) => serde_json::Value::Bool(b),
        V::Integer(i) => {
            let wide = i128::from(i);
            integer_to_json(
                i64::try_from(wide).ok(),
                u64::try_from(wide).ok(),
                Some(wide as f64),
            )
        }
        V::Float(f) => float_to_json(f),
        V::Text(text) => serde_json::Value::String(text),
        V::Bytes(bytes) => bytes_to_json(&bytes),
        V::Array(items) => serde_json::Value::Array(items.into_iter().map(cbor_to_json).collect()),
        V::Map(entries) => serde_json::Value::Object(
            entries
                .into_iter()
                .map(|(k, v)| (map_key(cbor_to_json(k)), cbor_to_json(v)))
                .collect(),
        ),
        // A tag is a hint about how to read the value it wraps (RFC 8949 §3.4);
        // keeping the value and naming the tag beats dropping either.
        V::Tag(tag, inner) => serde_json::json!({
            "tag": tag,
            "value": cbor_to_json(*inner),
        }),
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

/// Byte strings become arrays of byte values, matching what `apache-avro` does
/// for its own `bytes`/`fixed` fields — one rule for binary across every
/// encoding, and the inspector's Hex tab is where raw bytes are meant to be
/// read anyway.
fn bytes_to_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::Value::Array(bytes.iter().map(|b| serde_json::json!(b)).collect())
}

fn integer_to_json(
    as_i64: Option<i64>,
    as_u64: Option<u64>,
    as_f64: Option<f64>,
) -> serde_json::Value {
    if let Some(i) = as_i64 {
        return serde_json::json!(i);
    }
    if let Some(u) = as_u64 {
        return serde_json::json!(u);
    }
    // Outside i64/u64 there is no JSON number that holds it exactly; a string
    // keeps every digit, which a lossy float would not.
    match as_f64 {
        Some(f) => serde_json::Value::String(format!("{f}")),
        None => serde_json::Value::Null,
    }
}

fn float_to_json(f: f64) -> serde_json::Value {
    // JSON has no NaN or infinity (RFC 8259 §6).
    serde_json::Number::from_f64(f).map_or_else(
        || serde_json::Value::String(f.to_string()),
        serde_json::Value::Number,
    )
}

// ---------------------------------------------------------------------------
// The encode side (Phase 2 produce). The same framing constants as `decode`,
// read in the other direction, so the two cannot drift apart.
// ---------------------------------------------------------------------------

/// Encodes `json` as a single Avro datum against `schema` and wraps it in the
/// Confluent framing [`decode`] reads back: `[0x00][4-byte big-endian schema
/// id][datum]`. Not an object-container file — the framing replaces the
/// container's header, which is the whole point of the Confluent wire format.
///
/// # Why the mismatch check is hand-written rather than left to apache-avro
///
/// `Value::resolve` answers *whether* a value fits a schema, not *where* it
/// stopped fitting: for a forty-field record its error names neither the field
/// nor the value. Producing is a form the user is typing into, and "this JSON
/// does not fit the schema" is the least actionable sentence Kavka could put
/// under it (docs/DESIGN.md §7: what happened, then the next click).
/// [`avro_mismatch`] walks the schema and the JSON together and names the path.
///
/// It also catches the one mistake resolution is *silent* about: a field the
/// schema does not have. Avro drops unknown fields, so a typo'd `amont` would
/// otherwise encode cleanly with `amount` left at its default — a message that
/// is wrong in exactly the way nobody checks for.
pub fn encode_avro(
    schema: &AvroSchema,
    schema_id: u32,
    json: &serde_json::Value,
) -> Result<Vec<u8>> {
    if let Some(problem) = avro_mismatch(schema, json, "") {
        return Err(Error::Other(problem));
    }
    let datum = apache_avro::types::Value::from(json.clone())
        .resolve(schema)
        .and_then(|value| apache_avro::to_avro_datum(schema, value))
        // The fallback, for the shapes the walker leaves to apache-avro
        // (fixed, decimals, wide unions, named references).
        .map_err(|e| Error::Other(format!("this value does not fit the schema: {e}")))?;

    let mut framed = Vec::with_capacity(CONFLUENT_HEADER_LEN + datum.len());
    framed.push(CONFLUENT_MAGIC);
    framed.extend_from_slice(&schema_id.to_be_bytes());
    framed.extend_from_slice(&datum);
    Ok(framed)
}

/// The first place `json` and `schema` disagree, named — or `None` when this
/// walker can find nothing wrong, in which case apache-avro's own resolution
/// gets the last word.
///
/// `path` is the dotted route to the value being checked, empty at the root.
fn avro_mismatch(schema: &AvroSchema, json: &serde_json::Value, path: &str) -> Option<String> {
    use serde_json::Value as J;

    let expected = |what: &str| {
        Some(format!(
            "{} expects {what}, but the JSON has {}",
            at_path(path),
            json_shape(json)
        ))
    };

    match schema {
        AvroSchema::Null => match json {
            J::Null => None,
            _ => expected("null"),
        },
        AvroSchema::Boolean => match json {
            J::Bool(_) => None,
            _ => expected("a boolean"),
        },
        AvroSchema::Int | AvroSchema::Date | AvroSchema::TimeMillis => match json.as_i64() {
            Some(n) if i32::try_from(n).is_ok() => None,
            Some(n) => Some(format!(
                "{} expects int, and {n} is outside the 32-bit range — that field needs to be a \
                 long",
                at_path(path)
            )),
            None => expected("a whole number (int)"),
        },
        AvroSchema::Long
        | AvroSchema::TimeMicros
        | AvroSchema::TimestampMillis
        | AvroSchema::TimestampMicros
        | AvroSchema::TimestampNanos
        | AvroSchema::LocalTimestampMillis
        | AvroSchema::LocalTimestampMicros
        | AvroSchema::LocalTimestampNanos => match json.as_i64() {
            Some(_) => None,
            None => expected("a whole number (long)"),
        },
        AvroSchema::Float | AvroSchema::Double => match json.as_f64() {
            Some(_) => None,
            None => expected("a number"),
        },
        AvroSchema::String | AvroSchema::Uuid => match json {
            J::String(_) => None,
            _ => expected("a string"),
        },
        AvroSchema::Enum(enumeration) => match json {
            J::String(symbol) if enumeration.symbols.contains(symbol) => None,
            J::String(symbol) => Some(format!(
                "{} is an enum and {symbol:?} is not one of its symbols: {}",
                at_path(path),
                enumeration.symbols.join(", ")
            )),
            _ => expected("a string"),
        },
        AvroSchema::Record(record) => {
            let J::Object(map) = json else {
                return expected(&format!("the record {}", record.name.name));
            };
            if let Some(unknown) = map
                .keys()
                .find(|key| !record.fields.iter().any(|field| field.name == **key))
            {
                let known: Vec<&str> = record.fields.iter().map(|f| f.name.as_str()).collect();
                return Some(format!(
                    "{} has no field {unknown:?} — {} takes: {}",
                    at_path(path),
                    record.name.name,
                    known.join(", ")
                ));
            }
            record.fields.iter().find_map(|field| {
                let child = join_path(path, &field.name);
                match map.get(&field.name) {
                    Some(value) => avro_mismatch(&field.schema, value, &child),
                    // A field with a default is optional; one without is not.
                    None if field.default.is_none() => Some(format!(
                        "{} is required by {} and the JSON has no such key",
                        at_path(&child),
                        record.name.name
                    )),
                    None => None,
                }
            })
        }
        AvroSchema::Array(array) => {
            let J::Array(items) = json else {
                return expected("an array");
            };
            items
                .iter()
                .enumerate()
                .find_map(|(i, item)| avro_mismatch(&array.items, item, &format!("{path}[{i}]")))
        }
        AvroSchema::Map(map_schema) => {
            let J::Object(entries) = json else {
                return expected("an object");
            };
            entries.iter().find_map(|(key, value)| {
                avro_mismatch(&map_schema.types, value, &join_path(path, key))
            })
        }
        AvroSchema::Union(union) => {
            let variants = union.variants();
            if json.is_null() {
                return if variants.iter().any(|v| matches!(v, AvroSchema::Null)) {
                    None
                } else {
                    expected("a value, not null")
                };
            }
            // The nullable idiom — `["null", T]` — is the only union whose
            // intended branch is unambiguous, so it is the only one worth
            // checking by hand. Anything wider is left to resolution, which
            // tries every variant and is right to.
            let mut real = variants.iter().filter(|v| !matches!(v, AvroSchema::Null));
            match (real.next(), real.next()) {
                (Some(only), None) => avro_mismatch(only, json, path),
                _ => None,
            }
        }
        // Bytes, fixed, decimals and named references have more than one legal
        // JSON spelling; apache-avro's resolution knows them all and this
        // walker would only guess.
        _ => None,
    }
}

fn at_path(path: &str) -> String {
    if path.is_empty() {
        "the value".to_string()
    } else {
        format!("field {path:?}")
    }
}

fn join_path(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}.{name}")
    }
}

/// What the JSON actually is, in the words the error message needs.
fn json_shape(json: &serde_json::Value) -> &'static str {
    match json {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(n) if n.is_f64() => "a fractional number",
        serde_json::Value::Number(_) => "a whole number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// JSON object keys are strings; MessagePack and CBOR maps are keyed by
/// arbitrary values. Strings pass through unquoted, everything else renders as
/// its JSON literal (`1`, `true`, `[1,2]`).
///
/// This is lossy in one pathological case — a map holding both `1` and `"1"`
/// collapses to one key — and that is accepted rather than worked around: JSON
/// has no representation for a non-string key, so the alternative is emitting
/// maps as arrays of pairs and changing the shape of every payload for a case
/// no real producer emits. The inspector's Hex tab is the escape hatch.
fn map_key(key: serde_json::Value) -> String {
    match key {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: usize = DEFAULT_MAX_VALUE_BYTES;

    fn framed(schema_id: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = vec![CONFLUENT_MAGIC];
        bytes.extend_from_slice(&schema_id.to_be_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn json_objects_decode_to_pretty_text_and_a_tree() {
        let decoded = decode(br#"{"orderId":7,"status":"created"}"#, None, CAP);
        assert_eq!(decoded.encoding, Encoding::Json);
        assert_eq!(decoded.json.as_ref().unwrap()["orderId"], 7);
        assert!(
            decoded.text.contains("\n  \"orderId\": 7"),
            "{}",
            decoded.text
        );
        assert_eq!(decoded.raw_len, 32);
        assert!(!decoded.truncated);
        assert!(decoded.schema.is_none());
    }

    #[test]
    fn json_detection_ignores_leading_whitespace_but_not_scalars() {
        assert_eq!(decode(b"  [1,2,3]", None, CAP).encoding, Encoding::Json);
        // A bare scalar is what a plain-text key looks like; it stays text.
        for scalar in [&b"42"[..], b"true", b"\"done\"", b"null"] {
            assert_eq!(
                decode(scalar, None, CAP).encoding,
                Encoding::Utf8,
                "{scalar:?} should not read as JSON"
            );
        }
    }

    #[test]
    fn malformed_json_falls_through_to_text() {
        let decoded = decode(b"{not json", None, CAP);
        assert_eq!(decoded.encoding, Encoding::Utf8);
        assert_eq!(decoded.text, "{not json");
        assert!(decoded.json.is_none());
    }

    #[test]
    fn plain_keys_decode_as_utf8() {
        let decoded = decode(b"order-42", None, CAP);
        assert_eq!(decoded.encoding, Encoding::Utf8);
        assert_eq!(decoded.text, "order-42");
        assert_eq!(decoded.raw_len, 8);
        assert!(decoded.json.is_none());
    }

    #[test]
    fn control_bytes_are_hex_not_mojibake() {
        // Valid UTF-8 (every byte < 0x80) but not text anyone can read.
        let decoded = decode(&[0x01, 0x02, 0x03, 0x04], None, CAP);
        assert_eq!(decoded.encoding, Encoding::Hex);
        assert_eq!(decoded.text, "01 02 03 04");
    }

    #[test]
    fn an_empty_payload_is_empty_text_not_a_tombstone() {
        // Zero bytes are valid UTF-8, so this is an empty *value* — a different
        // fact from a tombstone, which is a `None` value on the record and
        // renders as `∅ tombstone` (docs/DESIGN.md §5.2).
        let decoded = decode(b"", None, CAP);
        assert_eq!(decoded.encoding, Encoding::Utf8);
        assert_eq!(decoded.text, "");
        assert_eq!(decoded.raw_len, 0);
        assert!(!decoded.truncated);
    }

    #[test]
    fn messagepack_maps_decode_to_json() {
        // {"a": 1, "b": [true, null]} as MessagePack.
        let bytes = [0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0x92, 0xc3, 0xc0];
        let decoded = decode(&bytes, None, CAP);
        assert_eq!(decoded.encoding, Encoding::MsgPack);
        let json = decoded.json.unwrap();
        assert_eq!(json["a"], 1);
        assert_eq!(json["b"], serde_json::json!([true, null]));
    }

    #[test]
    fn messagepack_with_trailing_bytes_is_not_messagepack() {
        let mut bytes = vec![0x91, 0x01]; // [1]
        bytes.push(0xff); // …plus junk
        assert_eq!(decode(&bytes, None, CAP).encoding, Encoding::Hex);
    }

    #[test]
    fn cbor_maps_decode_to_json() {
        // {"a": 1, "b": [true, null]} as CBOR.
        let bytes = [0xa2, 0x61, b'a', 0x01, 0x61, b'b', 0x82, 0xf5, 0xf6];
        let decoded = decode(&bytes, None, CAP);
        assert_eq!(decoded.encoding, Encoding::Cbor);
        let json = decoded.json.unwrap();
        assert_eq!(json["a"], 1);
        assert_eq!(json["b"], serde_json::json!([true, null]));
    }

    #[test]
    fn hex_is_the_fallback_for_bytes_nothing_claims() {
        // 0xfe/0xff are invalid UTF-8 starts and never-used MessagePack/CBOR
        // leading bytes.
        let decoded = decode(&[0xfe, 0xff, 0xfe, 0xff], None, CAP);
        assert_eq!(decoded.encoding, Encoding::Hex);
        assert_eq!(decoded.text, "fe ff fe ff");
        assert!(decoded.json.is_none());
        assert!(!decoded.truncated);
    }

    #[test]
    fn confluent_framing_without_a_registry_keeps_the_schema_id() {
        let bytes = framed(217, &[0x02, 0x06, b'a', b'b', b'c']);
        let decoded = decode(&bytes, None, CAP);

        assert_eq!(decoded.encoding, Encoding::Hex);
        let schema = decoded.schema.expect("framing carries an id");
        assert_eq!(schema.schema_id, 217);
        assert_eq!(schema.subject, None);
        assert_eq!(schema.version, None);
        // The framing bytes are part of the payload the user sees.
        assert!(
            decoded.text.starts_with("00 00 00 00 d9"),
            "{}",
            decoded.text
        );
    }

    #[test]
    fn framing_needs_the_magic_byte_and_a_full_header() {
        // Four bytes cannot carry magic + id, so this is just text.
        assert_eq!(decode(&[0x00, 0x00, 0x00, 0x01], None, CAP).schema, None);
        // A non-zero first byte is not Confluent framing.
        assert_eq!(
            decode(&[0x01, 0x00, 0x00, 0x00, 0x2a], None, CAP).schema,
            None
        );
    }

    #[test]
    fn oversized_payloads_are_previewed_without_a_parse() {
        let big = format!("{{\"pad\":\"{}\"}}", "x".repeat(200));
        let decoded = decode(big.as_bytes(), None, 64);

        assert_eq!(decoded.encoding, Encoding::Utf8);
        assert!(decoded.truncated);
        assert_eq!(decoded.raw_len, big.len());
        assert_eq!(decoded.text.len(), 64);
        // No structured parse was attempted, by design.
        assert!(decoded.json.is_none());
    }

    #[test]
    fn truncation_never_splits_a_character() {
        // 'é' is two bytes; cutting at 3 lands inside the second one.
        let text = "aéé";
        let decoded = decode(text.as_bytes(), None, 3);
        assert!(decoded.truncated);
        assert_eq!(decoded.text, "aé");
        assert_eq!(decoded.raw_len, 5);
    }

    #[test]
    fn oversized_binary_previews_as_hex_and_keeps_its_schema_id() {
        let bytes = framed(9, &[0xff; 100]);
        let decoded = decode(&bytes, None, 16);

        assert_eq!(decoded.encoding, Encoding::Hex);
        assert!(decoded.truncated);
        assert_eq!(decoded.raw_len, 105);
        assert_eq!(decoded.schema.unwrap().schema_id, 9);
    }

    #[test]
    fn headers_render_as_text_or_hex_and_distinguish_null() {
        let text = decode_header("trace-id", Some(b"abc-123"));
        assert_eq!(text.value.as_deref(), Some("abc-123"));
        assert!(text.is_text);

        // 0xff is not a legal UTF-8 lead byte in any position. (0xde 0xad
        // would be: it decodes to U+07AD, which is real text.)
        let binary = decode_header("sig", Some(&[0xff, 0xfe]));
        assert_eq!(binary.value.as_deref(), Some("ff fe"));
        assert!(!binary.is_text);

        let absent = decode_header("flag", None);
        assert_eq!(absent.value, None);
        assert!(!absent.is_text);
    }

    /// `{"type":"record","name":"Order","fields":[
    ///    {"name":"orderId","type":"int"},{"name":"status","type":"string"}]}`
    const ORDER_SCHEMA: &str = r#"{"schema":"{\"type\":\"record\",\"name\":\"Order\",\"fields\":[{\"name\":\"orderId\",\"type\":\"int\"},{\"name\":\"status\",\"type\":\"string\"}]}"}"#;
    const ORDER_VERSIONS: &str = r#"[{"subject":"orders-value","version":4}]"#;

    /// `{orderId: 7, status: "created"}` as an Avro datum, written out by hand
    /// so the test pins the wire format rather than agreeing with whatever
    /// `apache-avro` would have produced:
    ///   `0x0e`  int 7, zigzag-encoded  (7 << 1 = 14)
    ///   `0x0e`  string length 7, zigzag-encoded
    ///   ...     the seven bytes of "created"
    const ORDER_DATUM: [u8; 9] = [0x0e, 0x0e, b'c', b'r', b'e', b'a', b't', b'e', b'd'];

    fn registry_config(url: &str) -> crate::profiles::SchemaRegistryConfig {
        crate::profiles::SchemaRegistryConfig {
            url: url.to_string(),
            username: None,
            password: None,
        }
    }

    #[test]
    fn sr_framed_avro_decodes_to_json_with_its_subject_and_version() {
        let server = crate::sr::canned::CannedRegistry::start(vec![
            ("/schemas/ids/217", 200, ORDER_SCHEMA),
            ("/schemas/ids/217/versions", 200, ORDER_VERSIONS),
        ]);
        let registry = SchemaRegistry::new(&registry_config(server.url()));
        let bytes = framed(217, &ORDER_DATUM);

        let decoded = decode(&bytes, Some(&registry), CAP);

        assert_eq!(decoded.encoding, Encoding::Avro);
        assert_eq!(
            decoded.json.unwrap(),
            serde_json::json!({"orderId": 7, "status": "created"})
        );
        assert_eq!(decoded.raw_len, 14, "the framing bytes count as payload");
        assert_eq!(
            decoded.schema,
            Some(SchemaMeta {
                schema_id: 217,
                subject: Some("orders-value".into()),
                version: Some(4),
            })
        );
        assert_eq!(registry.error(), None);
    }

    #[test]
    fn a_registry_that_cannot_resolve_the_id_still_names_it() {
        // No routes: every lookup 404s.
        let server = crate::sr::canned::CannedRegistry::start(vec![]);
        let registry = SchemaRegistry::new(&registry_config(server.url()));

        let decoded = decode(&framed(217, &ORDER_DATUM), Some(&registry), CAP);

        assert_eq!(decoded.encoding, Encoding::Hex);
        assert_eq!(decoded.schema.unwrap().schema_id, 217);
        assert!(registry.error().is_some(), "the reason is kept for the UI");
    }

    #[test]
    fn bytes_that_do_not_match_their_schema_stay_hex_and_keep_the_subject() {
        let server = crate::sr::canned::CannedRegistry::start(vec![
            ("/schemas/ids/217", 200, ORDER_SCHEMA),
            ("/schemas/ids/217/versions", 200, ORDER_VERSIONS),
        ]);
        let registry = SchemaRegistry::new(&registry_config(server.url()));
        // A string length that runs off the end of the payload.
        let decoded = decode(&framed(217, &[0x0e, 0xfe, 0xff]), Some(&registry), CAP);

        assert_eq!(decoded.encoding, Encoding::Hex);
        let schema = decoded.schema.unwrap();
        assert_eq!(schema.schema_id, 217);
        assert_eq!(schema.subject.as_deref(), Some("orders-value"));
        let error = registry.error().expect("recorded once");
        assert!(error.contains("orders-value"), "got {error}");
    }

    #[test]
    fn a_framed_payload_is_never_re_read_as_something_else() {
        // The body is perfectly good JSON, but the framing says it is schema
        // 5 — guessing past that would silently drop the schema id the user
        // needs to see to understand why the decode failed.
        let server = crate::sr::canned::CannedRegistry::start(vec![]);
        let registry = SchemaRegistry::new(&registry_config(server.url()));
        let decoded = decode(&framed(5, br#"{"a":1}"#), Some(&registry), CAP);

        assert_eq!(decoded.encoding, Encoding::Hex);
        assert_eq!(decoded.schema.unwrap().schema_id, 5);
    }

    #[test]
    fn map_keys_that_are_not_strings_keep_their_literal_form() {
        // MessagePack {1: "a", true: "b"} — integer and boolean keys.
        let bytes = [0x82, 0x01, 0xa1, b'a', 0xc3, 0xa1, b'b'];
        let json = decode(&bytes, None, CAP).json.unwrap();
        assert_eq!(json["1"], "a");
        assert_eq!(json["true"], "b");
        assert_eq!(json.as_object().unwrap().len(), 2);
    }

    /// The Phase 1 IPC contract fixes these strings and the TypeScript side
    /// mirrors them literally. Renaming a variant would compile, ship, and
    /// silently stop matching in the UI — so the wire form is asserted here
    /// rather than left to `rename_all`.
    #[test]
    fn the_ipc_wire_form_is_fixed() {
        for (encoding, expected) in [
            (Encoding::Json, "json"),
            (Encoding::Utf8, "utf8"),
            (Encoding::Avro, "avro"),
            (Encoding::MsgPack, "msgpack"),
            (Encoding::Cbor, "cbor"),
            (Encoding::Hex, "hex"),
        ] {
            assert_eq!(serde_json::to_value(encoding).unwrap(), expected);
        }

        let record = MessageRecord {
            partition: 3,
            offset: 8412,
            timestamp_ms: Some(1_700_000_000_000),
            key: Some(decode(b"order-1", None, CAP)),
            value: None,
            headers: vec![decode_header("trace-id", Some(b"abc"))],
        };
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            serde_json::json!({
                "partition": 3,
                "offset": 8412,
                "timestamp_ms": 1_700_000_000_000i64,
                "key": {
                    "encoding": "utf8",
                    "text": "order-1",
                    "json": null,
                    "raw_len": 7,
                    "truncated": false,
                    "schema": null,
                },
                // A tombstone is a null value, not an empty payload.
                "value": null,
                "headers": [{"key": "trace-id", "value": "abc", "is_text": true}],
            })
        );

        assert_eq!(
            serde_json::to_value(SchemaMeta {
                schema_id: 217,
                subject: Some("orders-value".into()),
                version: Some(4),
            })
            .unwrap(),
            serde_json::json!({"schema_id": 217, "subject": "orders-value", "version": 4})
        );
    }

    #[test]
    fn non_finite_floats_do_not_produce_invalid_json() {
        assert_eq!(float_to_json(f64::NAN), serde_json::json!("NaN"));
        assert_eq!(float_to_json(1.5), serde_json::json!(1.5));
    }

    // -----------------------------------------------------------------------
    // The encode side.
    // -----------------------------------------------------------------------

    /// The same Order the decode tests use, as a schema rather than as a
    /// registry response.
    const ORDER_AVRO: &str = r#"{"type":"record","name":"Order","fields":[{"name":"orderId","type":"int"},{"name":"status","type":"string"}]}"#;

    /// One of everything the mismatch walker has an opinion about.
    const RICH_AVRO: &str = r#"{
        "type": "record",
        "name": "Order",
        "fields": [
            {"name": "orderId", "type": "int"},
            {"name": "status", "type": {"type": "enum", "name": "Status", "symbols": ["created", "paid"]}},
            {"name": "customer", "type": {"type": "record", "name": "Customer", "fields": [
                {"name": "name", "type": "string"},
                {"name": "vip", "type": "boolean"}
            ]}},
            {"name": "tags", "type": {"type": "array", "items": "string"}},
            {"name": "note", "type": ["null", "string"], "default": null},
            {"name": "channel", "type": "string", "default": "web"}
        ]
    }"#;

    fn order_schema() -> AvroSchema {
        AvroSchema::parse_str(ORDER_AVRO).expect("the fixture schema parses")
    }

    fn rich_schema() -> AvroSchema {
        AvroSchema::parse_str(RICH_AVRO).expect("the fixture schema parses")
    }

    fn rich_value() -> serde_json::Value {
        serde_json::json!({
            "orderId": 7,
            "status": "created",
            "customer": {"name": "Ada", "vip": true},
            "tags": ["rush", "gift"],
            "note": null,
            "channel": "web",
        })
    }

    fn encode_error(schema: &AvroSchema, json: &serde_json::Value) -> String {
        encode_avro(schema, 1, json)
            .expect_err("this value should not have encoded")
            .to_string()
    }

    /// The framing is the contract between the two halves of this file, so it
    /// is pinned byte for byte rather than merely round-tripped.
    #[test]
    fn encoding_produces_the_exact_confluent_framing() {
        let bytes = encode_avro(
            &order_schema(),
            217,
            &serde_json::json!({"orderId": 7, "status": "created"}),
        )
        .expect("encodes");

        let mut expected = vec![0x00, 0x00, 0x00, 0x00, 0xd9];
        expected.extend_from_slice(&ORDER_DATUM);
        assert_eq!(bytes, expected);
        assert_eq!(confluent_schema_id(&bytes), Some(217));
        assert_eq!(&bytes[CONFLUENT_HEADER_LEN..], &ORDER_DATUM);
    }

    /// The decoder is the only judge that matters: bytes this function wrote,
    /// read back by the ladder at the top of this file.
    #[test]
    fn what_encode_writes_decode_reads() {
        let server = crate::sr::canned::CannedRegistry::start(vec![
            ("/schemas/ids/217", 200, ORDER_SCHEMA),
            ("/schemas/ids/217/versions", 200, ORDER_VERSIONS),
        ]);
        let registry = SchemaRegistry::new(&registry_config(server.url()));
        let original = serde_json::json!({"orderId": 42, "status": "shipped"});

        let bytes = encode_avro(&order_schema(), 217, &original).expect("encodes");
        let decoded = decode(&bytes, Some(&registry), CAP);

        assert_eq!(decoded.encoding, Encoding::Avro);
        assert_eq!(decoded.json.unwrap(), original);
        assert_eq!(decoded.schema.unwrap().schema_id, 217);
    }

    #[test]
    fn every_shape_in_a_real_schema_encodes() {
        let bytes = encode_avro(&rich_schema(), 5, &rich_value()).expect("encodes");
        assert_eq!(confluent_schema_id(&bytes), Some(5));

        let mut body = &bytes[CONFLUENT_HEADER_LEN..];
        let value = apache_avro::from_avro_datum(&rich_schema(), &mut body, None).expect("decodes");
        assert_eq!(
            serde_json::Value::try_from(value).expect("json"),
            rich_value()
        );
    }

    /// A field with a default is optional; the encoder fills it in.
    #[test]
    fn fields_with_defaults_may_be_left_out() {
        let mut sparse = rich_value();
        let object = sparse.as_object_mut().unwrap();
        object.remove("note");
        object.remove("channel");

        let bytes = encode_avro(&rich_schema(), 5, &sparse).expect("encodes");
        let mut body = &bytes[CONFLUENT_HEADER_LEN..];
        let value = apache_avro::from_avro_datum(&rich_schema(), &mut body, None).expect("decodes");
        assert_eq!(
            serde_json::Value::try_from(value).expect("json"),
            rich_value(),
            "the defaults are what came back"
        );
    }

    #[test]
    fn a_type_mismatch_names_the_field_and_both_types() {
        let mut wrong = rich_value();
        wrong["orderId"] = serde_json::json!("seven");
        let err = encode_error(&rich_schema(), &wrong);
        assert!(err.contains(r#"field "orderId""#), "got {err}");
        assert!(err.contains("int"), "got {err}");
        assert!(err.contains("a string"), "got {err}");
    }

    #[test]
    fn a_mismatch_inside_a_nested_record_names_the_whole_path() {
        let mut wrong = rich_value();
        wrong["customer"]["vip"] = serde_json::json!("yes");
        let err = encode_error(&rich_schema(), &wrong);
        assert!(err.contains(r#"field "customer.vip""#), "got {err}");
        assert!(err.contains("a boolean"), "got {err}");
    }

    #[test]
    fn a_mismatch_inside_an_array_names_the_index() {
        let mut wrong = rich_value();
        wrong["tags"] = serde_json::json!(["rush", 7]);
        let err = encode_error(&rich_schema(), &wrong);
        assert!(err.contains(r#"field "tags[1]""#), "got {err}");
    }

    /// The mistake apache-avro is silent about: a typo'd field name would
    /// otherwise encode cleanly, with the real field left at its default.
    #[test]
    fn an_unknown_field_is_named_rather_than_silently_dropped() {
        let mut typo = rich_value();
        let object = typo.as_object_mut().unwrap();
        object.remove("channel");
        object.insert("chanel".into(), serde_json::json!("web"));

        let err = encode_error(&rich_schema(), &typo);
        assert!(err.contains(r#"no field "chanel""#), "got {err}");
        // ...and it says what the schema does take, so the fix is one glance.
        assert!(err.contains("channel"), "got {err}");
        assert!(err.contains("orderId"), "got {err}");
    }

    #[test]
    fn a_missing_required_field_is_named() {
        let mut missing = rich_value();
        missing.as_object_mut().unwrap().remove("orderId");
        let err = encode_error(&rich_schema(), &missing);
        assert!(err.contains(r#"field "orderId""#), "got {err}");
        assert!(err.contains("required"), "got {err}");
    }

    #[test]
    fn an_enum_lists_the_symbols_it_would_have_accepted() {
        let mut wrong = rich_value();
        wrong["status"] = serde_json::json!("cancelled");
        let err = encode_error(&rich_schema(), &wrong);
        assert!(err.contains("cancelled"), "got {err}");
        assert!(err.contains("created, paid"), "got {err}");
    }

    #[test]
    fn a_nullable_field_takes_null_or_its_own_type_and_nothing_else() {
        let mut with_note = rich_value();
        with_note["note"] = serde_json::json!("gift wrap");
        assert!(encode_avro(&rich_schema(), 5, &with_note).is_ok());

        let mut wrong = rich_value();
        wrong["note"] = serde_json::json!(7);
        let err = encode_error(&rich_schema(), &wrong);
        assert!(err.contains(r#"field "note""#), "got {err}");
        assert!(err.contains("a string"), "got {err}");
    }

    #[test]
    fn an_int_field_says_so_when_the_number_is_too_wide_for_one() {
        let mut wide = rich_value();
        wide["orderId"] = serde_json::json!(i64::from(i32::MAX) + 1);
        let err = encode_error(&rich_schema(), &wide);
        assert!(err.contains(r#"field "orderId""#), "got {err}");
        assert!(err.contains("32-bit"), "got {err}");
        assert!(err.contains("long"), "got {err}");
    }

    #[test]
    fn a_value_of_the_wrong_shape_entirely_says_what_the_schema_wanted() {
        let err = encode_error(&rich_schema(), &serde_json::json!("just a string"));
        assert!(err.contains("the value"), "got {err}");
        assert!(err.contains("Order"), "got {err}");
    }
}
