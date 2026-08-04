//! SQL over topics (docs/ROADMAP.md Phase 5a) — a bounded scan, one in-memory
//! table, and DataFusion on top of it.
//!
//! This is the feature teams pay four figures a year for elsewhere: an engineer
//! who does not know Kafka's vocabulary can still answer "how many orders failed
//! yesterday, by partition". It is deliberately the *small* version of that
//! idea, and the smallness is the honest part — see "What a query is not",
//! below.
//!
//! # The shape of one query
//!
//! ```text
//!   start()  ─┬─ plan the SQL against the FIXED `messages` schema
//!             │     (a typo, an unknown column or a write statement fails HERE,
//!             │      before a single broker call — and the output columns are
//!             │      known, so the UI can draw its header before any row lands)
//!             ├─ resolve partitions + watermarks
//!             └─ one worker thread:
//!                   phase 1  poll ─► decode ─► Row ─► RecordBatch   (scanned++)
//!                            …stops at scan_cap, the byte ceiling, the
//!                              watermarks, a quiet broker, or a cancel
//!                   phase 2  DataFusion streams the result
//!                              ─► rows ─► bounded queue   (produced_rows++)
//!
//!   caller  ◄─ next_rows(timeout) ◄─ queue ─► progress()
//! ```
//!
//! Everything here BLOCKS, like the rest of the core; the Tauri shell wraps it
//! in its `blocking()` helper. [`SqlSession`] follows [`crate::search`]'s
//! pattern exactly — planning and consumer creation on the calling thread so a
//! bad query is an error from `start`, `stop` idempotent and safe from any
//! thread, `Drop` joining the worker so no session can leak a consumer that
//! keeps fetching from a cluster nobody is looking at.
//!
//! # What a query is not
//!
//! **It is not a stream.** A query answers about a *bounded window* of the topic
//! — the records between the seek position and the end watermarks captured when
//! it started, up to [`SqlSpec::scan_cap`]. That is a real limitation and it is
//! the one that makes an aggregate trustworthy: `count(*)` over a window that
//! kept growing under the query would be a number with no meaning. Live-tailing
//! is [`crate::consume::TailSession`]; an unbounded scan with a filter is
//! [`crate::search`], which streams to the end watermarks and never truncates.
//!
//! **It never answers quietly-partially.** Four things can leave a scan short of
//! its window — the scan cap, the row cap, the scan's memory ceiling, and a
//! partition that went quiet before reaching the end watermark captured at the
//! start — and every one of them sets [`SqlProgress::capped`]. A capped answer
//! is still a *correct* answer to a smaller question ("the first 50,000 records
//! of this window"), never a wrong answer to the question asked.
//!
//! The fourth is the subtle one. A quiet partition is usually a transactional
//! topic's markers taking offsets a consumer never receives — in which case
//! everything readable really has been read — but it is indistinguishable, from
//! inside the poll loop, from a broker that stopped answering half way. So it
//! is treated as the ceiling it might be rather than the completion it might
//! also be: an aggregate over a scan that silently lost half a partition is a
//! confidently wrong number, which is the one outcome this module exists to
//! prevent.
//!
//! **A cancelled query answers nothing at all.** Stopping mid-scan and running
//! the SQL anyway would hand back `count(*) = 41,912` for a topic holding two
//! million records, with no field in the contract able to say so. So a cancel
//! during the scan ends the session with no rows; a cancel during execution
//! stops the row stream where it is. Cancellation is the user's own doing, and
//! a half-scanned answer is worth less than none.
//!
//! # What a query can read
//!
//! One table, [`MESSAGES_TABLE`], whose columns are fixed and listed in
//! [`sql_surface`] — the same string the UI can put in a help panel, so the
//! documentation and the build cannot drift.
//!
//! **A payload no decoder could read is NULL, not a hex dump.** `value_text`
//! and `key_text` are null for a record whose bytes rendered as hex, which is
//! the same rule the CEL filter follows (`crate::search`, "a filter can only
//! match what it can read"): matching `value_text LIKE '%fe%'` against a hex
//! *rendering* nobody wrote a query against would be a lie that looks like a
//! result. The row is still there and still counted — only its text is absent.
//! To find those records by their raw bytes, use the search bar's substring
//! filter, which reads the wire form before anything decodes it.
//!
//! # Read-only, and enforced here
//!
//! DataFusion's SQL surface includes `CREATE EXTERNAL TABLE … LOCATION`, `COPY …
//! TO`, `INSERT` and `SET`. The first two read and write **local files**, which
//! is not a thing a Kafka client should do on behalf of a query typed into a
//! text box. Every one of them is refused by [`sql_options`] before the plan is
//! executed, in core rather than in the UI (docs/ARCHITECTURE.md D5) — and the
//! session is never given the dynamic file catalogue that would let
//! `FROM 'somefile.parquet'` resolve at all.

use crate::consume::SeekSpec;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[cfg(feature = "kafka")]
use crate::cancel::CancelToken;
#[cfg(feature = "kafka")]
use crate::connection::auth::KavkaClientContext;
#[cfg(feature = "kafka")]
use crate::connection::ClusterConnection;
#[cfg(feature = "kafka")]
use crate::profiles::SchemaRegistryConfig;
// The clock this scan stops on, shared with the other engines: what counts as
// the source going quiet has one definition, and the decode below deliberately
// does not count. See [`crate::quiet`].
#[cfg(feature = "kafka")]
use crate::quiet::{SourceSilence, METADATA_TIMEOUT};
#[cfg(feature = "kafka")]
use crate::serdes::{self, Encoding, MessageRecord, SharedDecoder, DEFAULT_MAX_VALUE_BYTES};
#[cfg(feature = "kafka")]
use crate::sr::SchemaRegistry;
#[cfg(feature = "kafka")]
use datafusion::arrow::array::{
    Array, ArrayRef, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array,
    Int8Array, LargeStringArray, StringArray, StringViewArray, UInt16Array, UInt32Array,
    UInt64Array, UInt8Array,
};
#[cfg(feature = "kafka")]
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
#[cfg(feature = "kafka")]
use datafusion::arrow::record_batch::RecordBatch;
#[cfg(feature = "kafka")]
use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};
#[cfg(feature = "kafka")]
use datafusion::datasource::MemTable;
#[cfg(feature = "kafka")]
use datafusion::error::DataFusionError;
#[cfg(feature = "kafka")]
use datafusion::prelude::{SQLOptions, SessionConfig, SessionContext};
#[cfg(feature = "kafka")]
use futures::StreamExt;
#[cfg(feature = "kafka")]
use rdkafka::consumer::{BaseConsumer, Consumer};
#[cfg(feature = "kafka")]
use rdkafka::message::{Headers, Message};
#[cfg(feature = "kafka")]
use rdkafka::{Offset, TopicPartitionList};
#[cfg(feature = "kafka")]
use std::collections::{HashMap, VecDeque};
#[cfg(feature = "kafka")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "kafka")]
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(feature = "kafka")]
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// The wire contract. Outside the `kafka` gate on purpose: these are the shapes
// the IPC layer and the TypeScript side are written against, and they have to
// stay readable (and testable) on the bare tier that tooling and clippy use.
// ---------------------------------------------------------------------------

/// Hard cap on how many RECORDS one query may read off a topic, applied in core
/// rather than trusted from the caller.
///
/// The whole scan is held in memory as Arrow arrays so DataFusion can sort and
/// group it, so this is the number that decides whether a query costs a
/// second or the app. A UI bug asking for ten million must cost a **capped
/// answer**, not a swap storm.
pub const MAX_SCANNED: u32 = 100_000;

/// Hard cap on how many ROWS one query may return. 10,000 rows of JSON is
/// already a large IPC payload, and past it a person is not reading a table,
/// they are asking for an export.
pub const MAX_ROWS: u32 = 10_000;

/// Hard ceiling on the decoded text one scan may hold, across every column.
///
/// [`MAX_SCANNED`] alone does not bound memory: a payload may be up to
/// [`crate::serdes::DEFAULT_MAX_VALUE_BYTES`] (256 KB) of display text, so
/// 100,000 records is 25 GB in the worst case. This is the second ceiling, and
/// like the other two it sets [`SqlProgress::capped`] rather than failing —
/// "the first 3,100 records of this window" is an answer; an out-of-memory kill
/// is not.
pub const MAX_SCAN_BYTES: usize = 256 * 1024 * 1024;

/// The one table a query may read.
pub const MESSAGES_TABLE: &str = "messages";

/// One SQL query over one topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlSpec {
    pub topic: String,
    /// The SQL, as typed. Parsed and planned by [`SqlSession::start`] before
    /// any broker call.
    pub query: String,
    pub seek: SeekSpec,
    /// `None` = every partition.
    pub partitions: Option<Vec<i32>>,
    /// How many records the scan may read, capped at [`MAX_SCANNED`].
    pub scan_cap: u32,
    /// How many rows the answer may hold, capped at [`MAX_ROWS`].
    pub max_rows: u32,
}

/// One column of an answer: the name the query gave it, and its type in SQL
/// spelling (see [`sql_type_name`] for the mapping from Arrow's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlColumn {
    pub name: String,
    pub data_type: String,
}

/// The truth about a running query. Every count is cumulative and monotonic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlProgress {
    /// Records read off the topic and fed to the query.
    pub scanned: u64,
    /// Rows the query has produced so far.
    pub produced_rows: u64,
    /// The query is over — finished, cancelled, or failed.
    pub done: bool,
    /// Set when the query **failed**: a broker error, a SQL error, a plan
    /// DataFusion could not execute. A query that legitimately matched nothing
    /// is not an error.
    pub error: Option<String>,
    /// **A ceiling bit, and the answer is a partial one.** True when the scan
    /// stopped at [`SqlSpec::scan_cap`] (or [`MAX_SCANNED`], or
    /// [`MAX_SCAN_BYTES`]) with records still unread, when the result was
    /// truncated at [`SqlSpec::max_rows`] (or [`MAX_ROWS`]), or when a partition
    /// went quiet before reaching the end watermark captured at the start.
    ///
    /// This flag is the entire honesty of the feature. An aggregate over a
    /// silently truncated scan is not a slightly-wrong number, it is a
    /// confidently-wrong one, and nothing in a `count(*)` can disclose that by
    /// itself.
    pub capped: bool,
}

/// The `messages` table, as `(name, SQL type)` in column order.
///
/// **This list is the contract.** [`messages_schema`] builds the Arrow schema
/// from the same names in the same order, and a test asserts the two agree,
/// so a column added to one and not the other fails to build rather than
/// producing a table whose header and rows disagree.
const MESSAGES_COLUMNS: &[(&str, &str)] = &[
    ("partition", "BIGINT"),
    ("offset", "BIGINT"),
    ("timestamp_ms", "BIGINT"),
    ("key_text", "VARCHAR"),
    ("value_text", "VARCHAR"),
    ("value_json", "VARCHAR"),
    ("headers_json", "VARCHAR"),
];

/// The columns of the `messages` virtual table, for a UI that wants to offer
/// them before a query has been written.
///
/// Deliberately not gated on `kafka`: it is a list of strings, and the SQL
/// panel's column chips should not need a librdkafka build to render.
pub fn messages_columns() -> Vec<SqlColumn> {
    MESSAGES_COLUMNS
        .iter()
        .map(|(name, data_type)| SqlColumn {
            name: (*name).to_string(),
            data_type: (*data_type).to_string(),
        })
        .collect()
}

/// Every function named in [`SQL_SURFACE`], for the tripwire that keeps the
/// documentation and the build honest about each other.
///
/// Two directions are checked by `the_documented_sql_surface_is_the_one_that_is
/// _built`: each of these is registered in a real session (so the doc cannot
/// promise a function this build does not have), and each appears in the
/// surface text (so the list cannot quietly outgrow what the user is told).
#[cfg(all(test, feature = "kafka"))]
const DOCUMENTED_FUNCTIONS: &[&str] = &[
    // aggregates
    "count",
    "sum",
    "avg",
    "mean",
    "min",
    "max",
    "median",
    "stddev",
    "var",
    "corr",
    "covar",
    "approx_distinct",
    "approx_median",
    "approx_percentile_cont",
    "array_agg",
    "string_agg",
    "bit_and",
    "bit_or",
    "bit_xor",
    "bool_and",
    "bool_or",
    // window
    "row_number",
    "rank",
    "dense_rank",
    "lag",
    "lead",
    "first_value",
    "last_value",
    "nth_value",
    "ntile",
    "percent_rank",
    "cume_dist",
    // strings
    "lower",
    "upper",
    "btrim",
    "ltrim",
    "rtrim",
    "length",
    "char_length",
    "octet_length",
    "substr",
    "substring",
    "left",
    "right",
    "lpad",
    "rpad",
    "replace",
    "translate",
    "split_part",
    "strpos",
    "position",
    "instr",
    "starts_with",
    "ends_with",
    "contains",
    "concat",
    "concat_ws",
    "repeat",
    "reverse",
    "initcap",
    "ascii",
    "chr",
    "to_hex",
    "encode",
    "decode",
    "levenshtein",
    "find_in_set",
    "substr_index",
    "overlay",
    // regex
    "regexp_like",
    "regexp_match",
    "regexp_replace",
    "regexp_count",
    "regexp_instr",
    // time
    "to_timestamp",
    "to_timestamp_millis",
    "to_timestamp_micros",
    "to_timestamp_nanos",
    "to_timestamp_seconds",
    "from_unixtime",
    "to_unixtime",
    "date_trunc",
    "date_part",
    "date_bin",
    "to_char",
    "date_format",
    "now",
    "current_date",
    "current_time",
    "make_date",
    "make_time",
    // numbers
    "abs",
    "ceil",
    "floor",
    "round",
    "trunc",
    "power",
    "sqrt",
    "cbrt",
    "exp",
    "ln",
    "log",
    "log10",
    "log2",
    "signum",
    "gcd",
    "lcm",
    "greatest",
    "least",
    "random",
    // everything else
    "coalesce",
    "nullif",
    "nvl",
    "nvl2",
    "ifnull",
    "arrow_cast",
    "arrow_typeof",
    "get_field",
    "named_struct",
    "uuid",
    "version",
];

/// What a query may say, in one string the UI can render in a help panel.
///
/// **Verified against the build, not written from memory.** Every function
/// named here is asserted to be registered in a real session, and the
/// deliberately-absent ones (JSON, hashing, array constructors) are asserted to
/// be absent — see [`DOCUMENTED_FUNCTIONS`]. Promising `json_get` in a help
/// panel and then failing on it is the kind of lie a test can prevent.
pub const SQL_SURFACE: &str = r#"Kavka SQL v1 — what a query can say

ONE TABLE
  Every query reads `messages`: the scan window of the topic selected above the
  query box. The topic is not named in FROM, there are no other tables, no
  catalogue to browse, and no reading files from disk.

COLUMNS
  partition     BIGINT   never null
  offset        BIGINT   never null. `offset` is a SQL keyword, so quote it —
                         "offset" — anywhere the parser could read it as the
                         OFFSET clause, which includes ORDER BY.
  timestamp_ms  BIGINT   null on a record written before KIP-32
  key_text      VARCHAR  null for a keyless record, and null for a key no
                         decoder could read
  value_text    VARCHAR  the display text. Null for a tombstone, and null for a
                         payload no decoder could read — search by substring
                         instead, which reads the raw bytes.
  value_json    VARCHAR  the value as COMPACT JSON text when it decoded to JSON,
                         otherwise null
  headers_json  VARCHAR  every header as a JSON object, never null ({} when
                         there are none)

READS ONLY
  CREATE, INSERT, UPDATE, DELETE, COPY and SET are refused before anything runs.
  A query cannot write to the cluster, to a file, or to the session.

SUPPORTED
  SELECT [DISTINCT] … FROM messages [WHERE] [GROUP BY] [HAVING] [ORDER BY]
  [LIMIT n [OFFSET n]]
  CASE, CAST / TRY_CAST, IN, BETWEEN, IS [NOT] NULL, LIKE / ILIKE, || , and the
  usual arithmetic and comparison operators
  Sub-queries, CTEs (WITH), UNION [ALL], INTERSECT, EXCEPT, and self-joins of
  `messages`
  Window functions — OVER (PARTITION BY … ORDER BY …) — row_number, rank,
  dense_rank, lag, lead, first_value, last_value, nth_value, ntile,
  percent_rank, cume_dist
  Aggregates — count, sum, avg / mean, min, max, median, stddev, var, corr,
  covar, approx_distinct, approx_median, approx_percentile_cont, array_agg,
  string_agg, bit_and, bit_or, bit_xor, bool_and, bool_or
  Strings — lower, upper, btrim, ltrim, rtrim, length, char_length,
  octet_length, substr, substring, left, right, lpad, rpad, replace, translate,
  split_part, strpos, position, instr, starts_with, ends_with, contains, concat,
  concat_ws, repeat, reverse, initcap, ascii, chr, to_hex, encode, decode,
  levenshtein, find_in_set, substr_index, overlay
  Regex — regexp_like, regexp_match, regexp_replace, regexp_count, regexp_instr
  Time — to_timestamp, to_timestamp_millis, to_timestamp_micros,
  to_timestamp_nanos, to_timestamp_seconds, from_unixtime, to_unixtime,
  date_trunc, date_part, date_bin, to_char, date_format, now, current_date,
  current_time, make_date, make_time
  Numbers — abs, ceil, floor, round, trunc, power, sqrt, cbrt, exp, ln, log,
  log10, log2, signum, gcd, lcm, greatest, least, random
  Other — coalesce, nullif, nvl, nvl2, ifnull, arrow_cast, arrow_typeof,
  get_field, named_struct, uuid, version

NOT IN THIS BUILD
  JSON functions. DataFusion ships none at all, so `value_json` is text and you
  match it as text:
      WHERE value_json LIKE '%"status":"failed"%'
      WHERE regexp_like(value_json, '"amount":[0-9]{4,}')
  `value_json` is COMPACT — no space after `:` or `,` — which is what makes
  those patterns stable.
  Array and list functions (make_array, array_element, unnest), hashing (md5,
  sha256, sha224, sha384, sha512), and the Parquet / CSV / JSON file readers are
  compiled out on purpose: none of them has anything to read here, and each one
  is weight in an installer whose size is a feature.

WHAT BOUNDS AN ANSWER
  The scan reads at most `scan_cap` records — Kavka's own ceiling is 100,000 —
  and the answer holds at most `max_rows` rows, ceiling 10,000. Whenever either
  bites, or the scan reaches its memory ceiling, `capped` is true on the
  progress event and the answer is a partial one. It is never quietly partial.
  A query answers about the records that existed when it started: anything
  produced while it runs is a question for the next one."#;

/// See [`SQL_SURFACE`]. A function so the UI has something to call over IPC
/// without the constant becoming part of the IPC contract's shape.
pub fn sql_surface() -> &'static str {
    SQL_SURFACE
}

// ---------------------------------------------------------------------------
// The table: Arrow schema, and one record's worth of it.
// ---------------------------------------------------------------------------

/// The Arrow schema of [`MESSAGES_TABLE`], built from [`MESSAGES_COLUMNS`]'s
/// names in [`MESSAGES_COLUMNS`]'s order.
///
/// `partition` is BIGINT rather than INT although a partition id is an `i32`:
/// a mixed-width integer table is a table where `partition = 3` sometimes needs
/// a cast and sometimes does not, and the widening is free.
#[cfg(feature = "kafka")]
fn messages_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("partition", DataType::Int64, false),
        Field::new("offset", DataType::Int64, false),
        Field::new("timestamp_ms", DataType::Int64, true),
        Field::new("key_text", DataType::Utf8, true),
        Field::new("value_text", DataType::Utf8, true),
        Field::new("value_json", DataType::Utf8, true),
        Field::new("headers_json", DataType::Utf8, false),
    ]))
}

/// One record, already reduced to exactly what the table holds.
///
/// The reduction happens the moment a record is polled, before it joins the
/// pack waiting to become a [`RecordBatch`], for two reasons: a
/// [`MessageRecord`] carries the parsed JSON tree *and* its pretty-printed
/// rendering, neither of which the table wants, and [`Row::footprint`] can only
/// be exact about memory that is actually being kept.
#[cfg(feature = "kafka")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    partition: i64,
    offset: i64,
    timestamp_ms: Option<i64>,
    key_text: Option<String>,
    value_text: Option<String>,
    value_json: Option<String>,
    headers_json: String,
}

#[cfg(feature = "kafka")]
impl Row {
    /// The mapping the whole feature rests on.
    ///
    /// - **A tombstone** has no value at all: `value_text` and `value_json` are
    ///   both null, which is what lets `WHERE value_text IS NULL` mean "find the
    ///   deletions" on a compacted topic.
    /// - **A keyless record** has a null `key_text`, distinct from a record
    ///   whose key is the empty string.
    /// - **A payload no decoder could read** renders as hex, and its text is
    ///   null here — see the module docs. The row survives; only its text is
    ///   gone.
    /// - **A value that is not JSON** — plain text, a truncated payload, hex —
    ///   has a null `value_json`. Not an error, not an empty string: a record
    ///   that isn't JSON is the normal case on most topics, and `IS NULL` is how
    ///   SQL says so.
    fn from_record(record: MessageRecord) -> Self {
        let headers_json = headers_object(&record);
        // `filter` before `map`: an undecodable payload has text (a hex dump)
        // and must still come out as NULL.
        let readable = |payload: &crate::serdes::DecodedPayload| payload.encoding != Encoding::Hex;
        let value_json = record
            .value
            .as_ref()
            .and_then(|value| value.json.as_ref())
            .map(serde_json::Value::to_string);
        Self {
            partition: i64::from(record.partition),
            offset: record.offset,
            timestamp_ms: record.timestamp_ms,
            key_text: record.key.filter(readable).map(|key| key.text),
            value_text: record.value.filter(readable).map(|value| value.text),
            value_json,
            headers_json,
        }
    }

    /// What this row costs to keep, near enough to bound a scan by.
    ///
    /// The integers are ignored: seven of them per row is noise beside a payload
    /// measured in kilobytes, and a ceiling that has to be *approximately* right
    /// is better served by a cheap sum than an exact one.
    fn footprint(&self) -> usize {
        self.key_text.as_ref().map_or(0, String::len)
            + self.value_text.as_ref().map_or(0, String::len)
            + self.value_json.as_ref().map_or(0, String::len)
            + self.headers_json.len()
    }
}

/// Every header of one record as a JSON object.
///
/// **A duplicate name keeps the last value** — Kafka permits repeats, JSON does
/// not, and an object is the shape a `LIKE` pattern can actually be written
/// against. **A header Kafka stored with no value at all is JSON `null`**,
/// distinct from one whose value is the empty string. A header whose bytes are
/// not text is its hex rendering, exactly the string the message table shows:
/// unlike a payload, a header is short enough that its hex form is something
/// people genuinely correlate on.
#[cfg(feature = "kafka")]
fn headers_object(record: &MessageRecord) -> String {
    let mut object = serde_json::Map::with_capacity(record.headers.len());
    for header in &record.headers {
        object.insert(
            header.key.clone(),
            match header.value.as_deref() {
                Some(value) => serde_json::Value::String(value.to_string()),
                None => serde_json::Value::Null,
            },
        );
    }
    serde_json::Value::Object(object).to_string()
}

/// Turns a pack of rows into one [`RecordBatch`] of [`messages_schema`].
#[cfg(feature = "kafka")]
fn record_batch(rows: &[Row]) -> std::result::Result<RecordBatch, DataFusionError> {
    let columns: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.partition),
        )),
        Arc::new(Int64Array::from_iter_values(rows.iter().map(|r| r.offset))),
        Arc::new(Int64Array::from_iter(rows.iter().map(|r| r.timestamp_ms))),
        Arc::new(StringArray::from_iter(
            rows.iter().map(|r| r.key_text.as_deref()),
        )),
        Arc::new(StringArray::from_iter(
            rows.iter().map(|r| r.value_text.as_deref()),
        )),
        Arc::new(StringArray::from_iter(
            rows.iter().map(|r| r.value_json.as_deref()),
        )),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.headers_json.as_str()),
        )),
    ];
    RecordBatch::try_new(messages_schema(), columns).map_err(DataFusionError::from)
}

// ---------------------------------------------------------------------------
// Types, and rows on their way out.
// ---------------------------------------------------------------------------

/// An Arrow type in SQL spelling, for a column header a person has to read.
///
/// `Int64` is a true statement about the array and a poor label for a column;
/// `BIGINT` is the word the user typed in the `CAST` that produced it. Anything
/// outside the set a query over this table can actually produce falls back to
/// Arrow's own name rather than being guessed at — a wrong label is worse than
/// an unfamiliar one.
#[cfg(feature = "kafka")]
fn sql_type_name(data_type: &DataType) -> String {
    use DataType::*;
    match data_type {
        Null => "NULL".into(),
        Boolean => "BOOLEAN".into(),
        Int8 | Int16 | Int32 => "INT".into(),
        Int64 => "BIGINT".into(),
        UInt8 | UInt16 | UInt32 => "INT UNSIGNED".into(),
        UInt64 => "BIGINT UNSIGNED".into(),
        Float16 | Float32 => "FLOAT".into(),
        Float64 => "DOUBLE".into(),
        Utf8 | LargeUtf8 | Utf8View => "VARCHAR".into(),
        Binary | LargeBinary | BinaryView | FixedSizeBinary(_) => "VARBINARY".into(),
        Date32 | Date64 => "DATE".into(),
        Time32(_) | Time64(_) => "TIME".into(),
        Timestamp(_, _) => "TIMESTAMP".into(),
        Interval(_) => "INTERVAL".into(),
        Duration(_) => "DURATION".into(),
        Decimal128(precision, scale) | Decimal256(precision, scale) => {
            format!("DECIMAL({precision}, {scale})")
        }
        List(_) | LargeList(_) | FixedSizeList(_, _) | ListView(_) | LargeListView(_) => {
            "ARRAY".into()
        }
        Struct(_) => "STRUCT".into(),
        Map(_, _) => "MAP".into(),
        other => format!("{other:?}"),
    }
}

/// One output column of a batch as JSON.
///
/// Numbers stay numbers and text stays text — a UI that has to right-align a
/// `count(*)` cannot do it against `"41912"`. Everything the query can produce
/// but this list does not name (a timestamp, an interval, a struct, a decimal)
/// becomes its **SQL display text**, which is what a table cell was going to
/// show anyway; falling back is what keeps a `CAST(… AS TIMESTAMP)` from being
/// an error instead of a column.
///
/// A float that is NaN or infinite has no JSON form, so it comes out as null —
/// documented rather than silently rendered as a string that would sort wrong.
#[cfg(feature = "kafka")]
fn column_values(array: &ArrayRef) -> std::result::Result<Vec<serde_json::Value>, DataFusionError> {
    let len = array.len();
    let mut out = Vec::with_capacity(len);

    /// Downcasts, then maps each non-null slot through `$convert`.
    macro_rules! collect {
        ($kind:ty, $convert:expr) => {{
            let typed = array.as_any().downcast_ref::<$kind>().ok_or_else(|| {
                DataFusionError::Internal(format!("{:?} array", array.data_type()))
            })?;
            #[allow(clippy::redundant_closure_call)]
            for i in 0..len {
                out.push(if typed.is_null(i) {
                    serde_json::Value::Null
                } else {
                    ($convert)(typed.value(i))
                });
            }
            return Ok(out);
        }};
    }

    match array.data_type() {
        DataType::Null => out.resize(len, serde_json::Value::Null),
        DataType::Boolean => collect!(BooleanArray, serde_json::Value::Bool),
        DataType::Int8 => collect!(Int8Array, |v: i8| serde_json::Value::from(i64::from(v))),
        DataType::Int16 => collect!(Int16Array, |v: i16| serde_json::Value::from(i64::from(v))),
        DataType::Int32 => collect!(Int32Array, |v: i32| serde_json::Value::from(i64::from(v))),
        DataType::Int64 => collect!(Int64Array, serde_json::Value::from),
        DataType::UInt8 => collect!(UInt8Array, |v: u8| serde_json::Value::from(u64::from(v))),
        DataType::UInt16 => collect!(UInt16Array, |v: u16| serde_json::Value::from(u64::from(v))),
        DataType::UInt32 => collect!(UInt32Array, |v: u32| serde_json::Value::from(u64::from(v))),
        DataType::UInt64 => collect!(UInt64Array, serde_json::Value::from),
        DataType::Float32 => collect!(Float32Array, |v: f32| serde_json::Value::from(f64::from(v))),
        DataType::Float64 => collect!(Float64Array, serde_json::Value::from),
        DataType::Utf8 => collect!(StringArray, |v: &str| serde_json::Value::String(v.into())),
        DataType::LargeUtf8 => {
            collect!(LargeStringArray, |v: &str| serde_json::Value::String(
                v.into()
            ))
        }
        DataType::Utf8View => {
            collect!(StringViewArray, |v: &str| serde_json::Value::String(
                v.into()
            ))
        }
        _ => {
            let formatter = ArrayFormatter::try_new(array.as_ref(), &FormatOptions::default())?;
            for i in 0..len {
                out.push(if array.is_null(i) {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(formatter.value(i).to_string())
                });
            }
        }
    }
    Ok(out)
}

/// A whole batch as row-major JSON — the shape the IPC contract carries.
#[cfg(feature = "kafka")]
fn json_rows(
    batch: &RecordBatch,
) -> std::result::Result<Vec<Vec<serde_json::Value>>, DataFusionError> {
    let columns: Vec<Vec<serde_json::Value>> = batch
        .columns()
        .iter()
        .map(column_values)
        .collect::<std::result::Result<_, _>>()?;
    Ok((0..batch.num_rows())
        .map(|row| columns.iter().map(|column| column[row].clone()).collect())
        .collect())
}

// ---------------------------------------------------------------------------
// The engine.
// ---------------------------------------------------------------------------

/// Rows per [`RecordBatch`], and DataFusion's own batch size. 8192 is Arrow's
/// convention and DataFusion's default; matching it means the scan's batches
/// are the ones the operators want and never get re-chunked.
#[cfg(feature = "kafka")]
const BATCH_ROWS: usize = 8_192;

/// Most a single [`SqlSession::next_rows`] hands back. Bounds one IPC event so
/// a 10,000-row answer arrives as twenty events the webview can render between,
/// rather than one it has to parse in a frame.
#[cfg(feature = "kafka")]
const ROW_BATCH: usize = 500;

/// How long the scan waits on one poll. Also the worst-case latency of a
/// [`SqlSession::stop`] during phase 1.
#[cfg(feature = "kafka")]
const POLL_INTERVAL: Duration = Duration::from_millis(250);

// The quiet deadline (and the metadata budget its cold-start tier is derived
// from) live in [`crate::quiet`], shared with the other scan engines so that
// "the source has gone quiet" means the same thing in all of them.

/// The options every query is planned under: **no DDL, no DML, no statements**.
///
/// This is the guardrail, not a preference. Without it a query box is a way to
/// run `COPY (SELECT …) TO '/tmp/x'` and `CREATE EXTERNAL TABLE … LOCATION
/// '…'` — file writes and file reads, from a text box in a Kafka client. The
/// check runs on the *logical plan*, before anything executes, so a refused
/// statement never touches a disk.
#[cfg(feature = "kafka")]
pub fn sql_options() -> SQLOptions {
    SQLOptions::new()
        .with_allow_ddl(false)
        .with_allow_dml(false)
        .with_allow_statements(false)
}

/// A session over the scanned batches, with the `messages` table registered.
///
/// `target_partitions(1)`: the scan is capped at [`MAX_SCANNED`] rows held in
/// RAM, so the repartition exchanges DataFusion would insert for parallelism
/// cost more than they save at this size — and single-partition execution makes
/// an `ORDER BY` with ties come out the same way twice, which a person
/// comparing two runs of the same query is entitled to.
///
/// `information_schema` stays off (its default): a query gets one table and no
/// catalogue to walk. The dynamic file catalogue that would make
/// `FROM 'somefile.parquet'` resolve is likewise never enabled — see the module
/// docs.
#[cfg(feature = "kafka")]
fn session(batches: Vec<RecordBatch>) -> std::result::Result<SessionContext, DataFusionError> {
    let config = SessionConfig::new()
        .with_target_partitions(1)
        .with_batch_size(BATCH_ROWS)
        .with_information_schema(false);
    let context = SessionContext::new_with_config(config);
    let table = MemTable::try_new(messages_schema(), vec![batches])?;
    context.register_table(MESSAGES_TABLE, Arc::new(table))?;
    Ok(context)
}

/// Runs `work` on a thread of our own and hands back what it returned.
///
/// **DataFusion is async and this crate is not.** A `Runtime::block_on` panics
/// when the calling thread already has a runtime entered — which the Tauri
/// shell's blocking pool does — so the runtime is built and driven on a thread
/// that provably has none. One thread per plan and one per execution, both
/// short-lived, against a query that reads a topic: not a cost worth avoiding.
#[cfg(feature = "kafka")]
fn on_our_own_thread<T: Send>(work: impl FnOnce() -> T + Send) -> Result<T> {
    std::thread::scope(|scope| {
        scope.spawn(work).join().map_err(|_| {
            Error::Other(
                "the SQL engine stopped unexpectedly, so this query has no answer — the details \
                 are in the log"
                    .into(),
            )
        })
    })
}

#[cfg(feature = "kafka")]
fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Other(format!("starting the SQL engine: {e}")))
}

/// Plans a query against the `messages` schema with **no data and no cluster**,
/// and answers with the columns it would produce.
///
/// This is what makes a typo cost nothing: the schema is fixed, so every
/// question SQL can be wrong about — a parse error, a column that does not
/// exist, a function this build does not have, a statement that would write —
/// is answerable before a broker is contacted, and the answer includes the
/// header the UI needs to draw its table before the first row lands.
pub fn plan_columns(query: &str) -> Result<Vec<SqlColumn>> {
    #[cfg(not(feature = "kafka"))]
    {
        let _ = query;
        Err(Error::Other("built without the `kafka` feature".into()))
    }
    #[cfg(feature = "kafka")]
    {
        on_our_own_thread(|| {
            let runtime = runtime()?;
            runtime.block_on(async {
                let context = session(Vec::new()).map_err(explain)?;
                let frame = context
                    .sql_with_options(query, sql_options())
                    .await
                    .map_err(explain)?;
                Ok(frame
                    .schema()
                    .fields()
                    .iter()
                    .map(|field| SqlColumn {
                        name: field.name().clone(),
                        data_type: sql_type_name(field.data_type()),
                    })
                    .collect())
            })
        })?
    }
}

/// One running query.
///
/// Dropping the session stops it and joins its worker, so the consumer — and
/// its broker connection — is gone before `drop` returns.
#[cfg(feature = "kafka")]
pub struct SqlSession {
    shared: Arc<SqlShared>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "kafka")]
struct SqlShared {
    rows: Mutex<RowQueue>,
    ready: Condvar,
    cancel: CancelToken,
    /// The output columns, known at `start` from the plan. Fixed for the life
    /// of the session.
    columns: Vec<SqlColumn>,
    scanned: AtomicU64,
    produced_rows: AtomicU64,
    capped: AtomicBool,
    error: Mutex<Option<String>>,
}

#[cfg(feature = "kafka")]
struct RowQueue {
    rows: VecDeque<Vec<serde_json::Value>>,
    /// The worker has left; once `rows` drains, this session is over.
    ended: bool,
}

/// One partition's read window, `[start, end)`.
#[cfg(feature = "kafka")]
#[derive(Clone, Copy, Debug)]
struct PartitionSlot {
    partition: i32,
    start: i64,
    end: i64,
}

/// Everything the worker needs, bundled so the thread body takes one argument.
#[cfg(feature = "kafka")]
struct Worker {
    consumer: BaseConsumer<KavkaClientContext>,
    slots: Vec<PartitionSlot>,
    shared: Arc<SqlShared>,
    registry: Option<Arc<SchemaRegistry>>,
    /// The profile's plugin for this topic, if one claims it — resolved on the
    /// calling thread because the worker outlives `start`.
    decoder: Option<SharedDecoder>,
    topic: String,
    query: String,
    scan_cap: u64,
    max_rows: usize,
}

#[cfg(feature = "kafka")]
impl SqlSession {
    /// Plans the query, plans the scan, and starts the worker.
    ///
    /// Everything that can fail deterministically fails here, on the calling
    /// thread, and **in that order**: the SQL first — before a single broker
    /// call, so a typo never costs a round trip — then the topic, the
    /// partitions, the watermarks and the consumer. A returned session is one
    /// that is genuinely running.
    pub fn start(
        conn: &ClusterConnection,
        profile_sr: Option<&SchemaRegistryConfig>,
        spec: &SqlSpec,
    ) -> Result<Self> {
        let columns = plan_columns(&spec.query)?;

        let consumer = conn.new_session_consumer()?;
        let slots = seek_plan(&consumer, spec)?;
        let mut assignment = TopicPartitionList::new();
        for slot in &slots {
            assignment
                .add_partition_offset(&spec.topic, slot.partition, Offset::Offset(slot.start))
                .map_err(|e| {
                    Error::Other(format!(
                        "seeking {}[{}] to offset {}: {e}",
                        spec.topic, slot.partition, slot.start
                    ))
                })?;
        }
        // An assignment is only made when there is something to read: assigning
        // an empty list is an error on some librdkafka builds, and a window with
        // nothing in it is a perfectly good query over zero rows.
        if !slots.is_empty() {
            consumer.assign(&assignment).map_err(|e| {
                Error::Other(format!("assigning partitions of {}: {e}", spec.topic))
            })?;
        }

        let shared = Arc::new(SqlShared {
            rows: Mutex::new(RowQueue {
                rows: VecDeque::new(),
                ended: false,
            }),
            ready: Condvar::new(),
            cancel: CancelToken::new(),
            columns,
            scanned: AtomicU64::new(0),
            produced_rows: AtomicU64::new(0),
            capped: AtomicBool::new(false),
            error: Mutex::new(None),
        });

        let worker = Worker {
            consumer,
            slots,
            shared: Arc::clone(&shared),
            registry: profile_sr.map(|config| Arc::new(SchemaRegistry::new(config))),
            decoder: conn.decoder_for(&spec.topic),
            topic: spec.topic.clone(),
            query: spec.query.clone(),
            scan_cap: u64::from(spec.scan_cap.min(MAX_SCANNED)),
            max_rows: spec.max_rows.min(MAX_ROWS) as usize,
        };

        tracing::debug!(
            topic = %spec.topic,
            partitions = worker.slots.len(),
            scan_cap = worker.scan_cap,
            "sql query started"
        );

        let handle = std::thread::Builder::new()
            .name("kavka-sql".into())
            .spawn(move || {
                let shared = Arc::clone(&worker.shared);
                run(worker);
                shared.finished();
            });

        match handle {
            Ok(handle) => Ok(Self {
                shared,
                worker: Some(handle),
            }),
            Err(e) => {
                // Nothing will ever set `ended`, and a session whose readers
                // wait forever is worse than one that failed to start.
                shared.finished();
                Err(Error::Other(format!("starting the SQL worker: {e}")))
            }
        }
    }

    /// The answer's columns, known from the plan before the first row exists.
    pub fn columns(&self) -> Vec<SqlColumn> {
        self.shared.columns.clone()
    }

    /// Waits up to `timeout` for result rows.
    ///
    /// `None` means the query is over and will produce nothing further.
    /// `Some(empty)` means it is still running and has produced nothing yet — a
    /// state the UI must be able to say out loud (docs/DESIGN.md §7: never "no
    /// results" while something is still running), which is why it is not folded
    /// into `None`.
    pub fn next_rows(&self, timeout: Duration) -> Option<Vec<Vec<serde_json::Value>>> {
        let deadline = Instant::now() + timeout;
        let mut queue = self.shared.lock();
        loop {
            if !queue.rows.is_empty() {
                let take = queue.rows.len().min(ROW_BATCH);
                return Some(queue.rows.drain(..take).collect());
            }
            // Ordering matters: rows queued before the worker left are still
            // delivered, and only a drained-and-ended queue is `None`.
            if queue.ended {
                return None;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Some(Vec::new());
            }
            let (guard, wait) = self
                .shared
                .ready
                .wait_timeout(queue, remaining)
                .unwrap_or_else(|e| e.into_inner());
            queue = guard;
            if wait.timed_out() && queue.rows.is_empty() && !queue.ended {
                return Some(Vec::new());
            }
        }
    }

    pub fn progress(&self) -> SqlProgress {
        self.shared.snapshot()
    }

    /// The token the worker checks. Exposed so a caller that already keys
    /// cancellation by profile can cancel a query the same way it cancels a
    /// fetch.
    pub fn cancel_token(&self) -> CancelToken {
        self.shared.cancel.clone()
    }

    /// Asks the worker to finish. Idempotent, and safe from any thread (the
    /// shell's `sql_stop` runs on a different one from the reader). Returns
    /// immediately; the worker notices within one poll interval.
    pub fn stop(&self) {
        self.shared.cancel.cancel();
        self.shared.ready.notify_all();
    }
}

/// Deliberately not derived: the derived form would dump every queued row into
/// any log line or panic message that touches a session.
#[cfg(feature = "kafka")]
impl std::fmt::Debug for SqlSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let progress = self.shared.snapshot();
        f.debug_struct("SqlSession")
            .field("scanned", &progress.scanned)
            .field("produced_rows", &progress.produced_rows)
            .field("capped", &progress.capped)
            .field("done", &progress.done)
            .finish()
    }
}

#[cfg(feature = "kafka")]
impl Drop for SqlSession {
    /// Joins the worker so its consumer — and the broker connection under it —
    /// is gone before `drop` returns.
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(feature = "kafka")]
impl SqlShared {
    /// Poison-tolerant, like everywhere else in the crate: a panicking reader
    /// must not turn a running query into a permanently locked one.
    fn lock(&self) -> MutexGuard<'_, RowQueue> {
        self.rows.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, rows: Vec<Vec<serde_json::Value>>) {
        let count = rows.len() as u64;
        {
            let mut queue = self.lock();
            queue.rows.extend(rows);
        }
        self.produced_rows.fetch_add(count, Ordering::Relaxed);
        self.ready.notify_all();
    }

    /// A ceiling bit. Set once, never cleared — an answer that was truncated
    /// stays truncated.
    fn note_capped(&self) {
        self.capped.store(true, Ordering::Relaxed);
    }

    /// First failure wins: the one that stopped the query is the one worth
    /// showing, and the others are usually its echoes.
    fn fail(&self, message: String) {
        let mut slot = self.error.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    fn finished(&self) {
        self.lock().ended = true;
        self.ready.notify_all();
    }

    fn snapshot(&self) -> SqlProgress {
        SqlProgress {
            scanned: self.scanned.load(Ordering::Relaxed),
            produced_rows: self.produced_rows.load(Ordering::Relaxed),
            done: self.lock().ended,
            error: self.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            capped: self.capped.load(Ordering::Relaxed),
        }
    }
}

/// The worker: scan, then query.
#[cfg(feature = "kafka")]
fn run(worker: Worker) {
    let Worker {
        consumer,
        slots,
        shared,
        registry,
        decoder,
        topic,
        query,
        scan_cap,
        max_rows,
    } = worker;

    let batches = match scan(
        &consumer,
        &slots,
        &shared,
        registry.as_deref(),
        decoder.as_ref(),
        &topic,
        scan_cap,
    ) {
        Ok(batches) => batches,
        Err(e) => {
            shared.fail(e.to_string());
            return;
        }
    };
    // The broker connection goes before the query starts. A DataFusion sort of
    // 100,000 rows is not long, but it is long enough that holding a fetch
    // session open through it is holding it for nothing.
    drop(consumer);

    if let Some(error) = registry.as_deref().and_then(SchemaRegistry::take_error) {
        tracing::warn!(
            %topic,
            %error,
            "schema registry unavailable; affected payloads are null in value_text"
        );
    }

    if shared.cancel.is_cancelled() {
        // See the module docs: a half-read scan would answer `count(*)` with a
        // number that looks exactly like the right one.
        tracing::debug!(%topic, "sql query cancelled during the scan; no answer");
        return;
    }

    if let Err(e) = execute(&query, batches, max_rows, &shared) {
        shared.fail(e.to_string());
    }
}

/// Phase 1: read the window, decode it, and pack it into Arrow batches.
///
/// Stops at whichever comes first — every partition reaching its captured end
/// watermark, `scan_cap`, [`MAX_SCAN_BYTES`], a quiet broker, or a cancel.
///
/// The first is the only one that is not a ceiling. `scan_cap` and
/// [`MAX_SCAN_BYTES`] say so through [`SqlProgress::capped`], and so does a
/// quiet broker **when any partition is short of its end** — see the comment at
/// that branch for why silence is treated as a ceiling rather than as
/// completion.
#[cfg(feature = "kafka")]
fn scan(
    consumer: &BaseConsumer<KavkaClientContext>,
    slots: &[PartitionSlot],
    shared: &SqlShared,
    registry: Option<&SchemaRegistry>,
    decoder: Option<&SharedDecoder>,
    topic: &str,
    scan_cap: u64,
) -> Result<Vec<RecordBatch>> {
    // partition -> the end watermark it must reach to be finished.
    let mut pending: HashMap<i32, i64> = slots.iter().map(|s| (s.partition, s.end)).collect();
    // partition -> the next offset the scan expects there. Kept beside
    // `pending` so the quiet deadline can tell "everything readable has been
    // read" from "this partition stopped answering half way", which are the
    // same event from the poll loop's point of view and different answers.
    let mut positions: HashMap<i32, i64> = slots.iter().map(|s| (s.partition, s.start)).collect();
    let mut batches: Vec<RecordBatch> = Vec::new();
    let mut pack: Vec<Row> = Vec::with_capacity(BATCH_ROWS.min(scan_cap.max(1) as usize));
    let mut scanned: u64 = 0;
    let mut bytes: usize = 0;
    // The cold-start tier is spent on the first record only, and the decode
    // below is explicitly not the source going quiet — see [`crate::quiet`] for
    // both, and for the flake that put the type there.
    let mut silence = SourceSilence::waiting_for_first(Instant::now());

    while !pending.is_empty() {
        if shared.cancel.is_cancelled() {
            tracing::debug!(%topic, "sql scan cancelled");
            break;
        }
        // Checked before the poll, so `scanned` never overshoots: a capped scan
        // reads exactly `scan_cap` records, which is what makes the partial
        // answer a reproducible one.
        if scanned >= scan_cap {
            shared.note_capped();
            tracing::debug!(%topic, scanned, "sql scan stopped at its record cap");
            break;
        }
        if bytes >= MAX_SCAN_BYTES {
            shared.note_capped();
            tracing::debug!(%topic, scanned, bytes, "sql scan stopped at its memory cap");
            break;
        }
        if silence.expired(Instant::now()) {
            // Not an error: a transactional topic's offsets can be occupied by
            // markers a consumer never receives, so the watermark alone cannot
            // always be reached, and everything readable HAS been read.
            //
            // It is not automatically "not a cap" either, and that is this
            // module's own invariant (see [`SqlProgress::capped`]): the OTHER
            // reason a partition goes quiet is a broker that stopped answering,
            // and an aggregate over a scan that lost half a partition is a
            // confidently wrong number rather than a slightly wrong one.
            // Nothing here can tell the two apart — so a partition that stopped
            // SHORT of the end watermark captured at the start sets `capped`,
            // exactly like the record and byte ceilings do, and only a scan
            // whose every partition actually arrived may answer uncapped.
            let mut short: Vec<i32> = pending
                .iter()
                .filter(|(partition, end)| {
                    positions.get(partition).copied().unwrap_or(i64::MIN) < **end
                })
                .map(|(partition, _)| *partition)
                .collect();
            short.sort_unstable();
            if short.is_empty() {
                tracing::debug!(
                    %topic,
                    partitions = pending.len(),
                    "no more records arrived; every partition was already at its end offset"
                );
            } else {
                // Named in the log rather than in the progress payload:
                // `SqlProgress` has one string field and it means "the query
                // FAILED" — the view raises an error banner and a danger signal
                // off it — so a partial-but-valid answer cannot ride there
                // without a contract change. `capped` is how this feature
                // states partiality, and it states it the same way for all four
                // causes.
                tracing::warn!(
                    %topic,
                    scanned,
                    partitions = ?short,
                    "these partitions went quiet short of their end offset; the answer is capped"
                );
                shared.note_capped();
            }
            break;
        }
        match consumer.poll(POLL_INTERVAL) {
            None => continue,
            Some(Err(e)) => return Err(Error::Other(format!("reading {topic}: {e}"))),
            Some(Ok(message)) => {
                silence.heard_from_source(Instant::now());
                let partition = message.partition();
                let offset = message.offset();
                let Some(&end) = pending.get(&partition) else {
                    continue;
                };
                positions.insert(partition, offset.saturating_add(1));
                // At or past the watermark captured at the start: a producer is
                // still writing. A query answers about the topic as it was when
                // it was asked, so this partition is done.
                if offset >= end {
                    pending.remove(&partition);
                    continue;
                }
                // `value_text` decodes through the schema registry, and one
                // lookup there is bounded by an HTTP timeout twice the whole
                // after-data budget — for a single record. Charging it to the
                // source would let a slow registry, not a quiet broker, decide
                // that this scan is short and cap the answer.
                let row = Row::from_record(record(&message, registry, decoder));
                silence.waited_on_decode(Instant::now());
                bytes += row.footprint();
                pack.push(row);
                scanned += 1;
                shared.scanned.store(scanned, Ordering::Relaxed);
                if pack.len() >= BATCH_ROWS {
                    batches.push(record_batch(&pack).map_err(explain)?);
                    pack.clear();
                }
                if offset + 1 >= end {
                    pending.remove(&partition);
                }
            }
        }
    }

    if !pack.is_empty() {
        batches.push(record_batch(&pack).map_err(explain)?);
    }
    Ok(batches)
}

/// Phase 2: hand the batches to DataFusion and stream the answer out.
///
/// The plan is built a second time here — the first was
/// [`plan_columns`]'s, against the same schema with no rows — because a
/// `MemTable` is immutable and the data does not exist yet when a query has to
/// be validated. Planning is microseconds; validating before a broker call is
/// the whole point.
#[cfg(feature = "kafka")]
fn execute(
    query: &str,
    batches: Vec<RecordBatch>,
    max_rows: usize,
    shared: &Arc<SqlShared>,
) -> Result<()> {
    on_our_own_thread(move || {
        let runtime = runtime()?;
        runtime.block_on(async move {
            let context = session(batches).map_err(explain)?;
            let frame = context
                .sql_with_options(query, sql_options())
                .await
                .map_err(explain)?;
            let mut stream = frame.execute_stream().await.map_err(explain)?;
            let mut produced = 0usize;
            loop {
                if shared.cancel.is_cancelled() {
                    tracing::debug!("sql query cancelled while streaming its answer");
                    break;
                }
                let Some(next) = stream.next().await else {
                    break;
                };
                let batch = next.map_err(explain)?;
                if batch.num_rows() == 0 {
                    continue;
                }
                let room = max_rows.saturating_sub(produced);
                if batch.num_rows() > room {
                    // Exact, and it costs at most one batch too many: a batch
                    // that overflows the budget proves there WAS more, which is
                    // the only way to tell a truncated answer from one that
                    // happened to be exactly `max_rows` long.
                    shared.note_capped();
                    if room > 0 {
                        shared.push(json_rows(&batch.slice(0, room)).map_err(explain)?);
                    }
                    break;
                }
                produced += batch.num_rows();
                shared.push(json_rows(&batch).map_err(explain)?);
            }
            Ok(())
        })
    })?
}

/// Turns a DataFusion failure into something a person can act on: a
/// plain-language first line, then DataFusion's own message verbatim
/// underneath (docs/DESIGN.md §7 — "plain title → cause and fix → the raw
/// string, verbatim and selectable").
///
/// The raw text is never dropped and never edited. It is the line an expert
/// reads, and the one a bug report needs.
#[cfg(feature = "kafka")]
fn explain(error: DataFusionError) -> Error {
    let raw = error.to_string();
    Error::Other(format!("{}\n{raw}", plain_line(&raw)))
}

/// The first line of a SQL error, in Kavka's voice.
///
/// Split out from [`explain`] so it can be tested against captured DataFusion
/// strings without a session — it is a pure, total function over the raw
/// message, exactly like the UI's own `classifyError` (docs/DESIGN.md §7).
#[cfg(feature = "kafka")]
fn plain_line(raw: &str) -> String {
    let columns: Vec<&str> = MESSAGES_COLUMNS.iter().map(|(name, _)| *name).collect();

    // The one the doctrine names explicitly: when we know the answer, put the
    // answer in the message. DataFusion's own text offers a spelling suggestion
    // OR the field list, never both, so the full list is built here instead —
    // seven names is not too many to just say.
    if let Some(name) = between(raw, "No field named ", |c| c == '.' || c == ',') {
        return format!(
            "{MESSAGES_TABLE} has no column called {name} — the columns are {}.",
            columns.join(", ")
        );
    }
    if let Some(name) = between(raw, "Invalid function '", |c| c == '\'') {
        return format!(
            "There's no SQL function called {name} in this build — the ones there are, are listed \
             beside the query box."
        );
    }
    if raw.contains("not found") && raw.contains("table") {
        return format!(
            "A query reads one table, {MESSAGES_TABLE} — the topic is chosen above the query box, \
             not named in FROM."
        );
    }
    if raw.contains("DDL not supported")
        || raw.contains("DML not supported")
        || raw.contains("Statement not supported")
    {
        return "SQL over topics only reads — CREATE, INSERT, UPDATE, DELETE, COPY and SET aren't \
                run."
            .into();
    }
    if raw.starts_with("SQL error") {
        return "This query didn't parse.".into();
    }
    "This query couldn't run.".into()
}

/// The text between `prefix` and the first character `end` answers for, or
/// `None` when `prefix` is absent. Trailing punctuation belongs to the sentence,
/// not to the name.
#[cfg(feature = "kafka")]
fn between(haystack: &str, prefix: &str, end: impl Fn(char) -> bool) -> Option<String> {
    let rest = haystack.split_once(prefix)?.1;
    let name = rest.find(end).map_or(rest, |at| &rest[..at]);
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(feature = "kafka")]
fn record(
    message: &rdkafka::message::BorrowedMessage<'_>,
    registry: Option<&SchemaRegistry>,
    decoder: Option<&SharedDecoder>,
) -> MessageRecord {
    let custom = decoder.map(|plugin| plugin.as_ref());
    let headers = message.headers().map_or_else(Vec::new, |headers| {
        (0..headers.count())
            .map(|i| {
                let header = headers.get(i);
                serdes::decode_header(header.key, header.value)
            })
            .collect()
    });
    MessageRecord {
        partition: message.partition(),
        offset: message.offset(),
        timestamp_ms: message.timestamp().to_millis(),
        key: message
            .key()
            .map(|bytes| serdes::decode_with(bytes, registry, DEFAULT_MAX_VALUE_BYTES, custom)),
        // `None` here is a tombstone, not an empty value.
        value: message
            .payload()
            .map(|bytes| serdes::decode_with(bytes, registry, DEFAULT_MAX_VALUE_BYTES, custom)),
        // NOT inspected here, unlike `consume` and `search`. The `messages`
        // table has no dead-letter column, and this record is reduced to a
        // [`Row`] by its caller and dropped — so a pass over the headers of
        // every one of up to 100,000 records would compute a field nothing can
        // read. The headers survive in `headers_json`, which is where a query
        // asks the same question:
        //   WHERE headers_json LIKE '%__connect.errors.topic%'
        // A dead-letter column here would change that, and this line with it.
        dlq: None,
        masked: false,
        headers,
    }
}

/// Turns a [`SeekSpec`] into the per-partition read windows the scan covers.
///
/// Unlike [`crate::search`]'s planner, an empty window is **dropped**: this
/// module reports no per-partition progress, so a partition with nothing to read
/// is a partition with nothing to say.
#[cfg(feature = "kafka")]
fn seek_plan(
    consumer: &BaseConsumer<KavkaClientContext>,
    spec: &SqlSpec,
) -> Result<Vec<PartitionSlot>> {
    let metadata = consumer
        .fetch_metadata(Some(&spec.topic), METADATA_TIMEOUT)
        .map_err(|e| Error::Other(format!("asking the cluster about {}: {e}", spec.topic)))?;
    let mut available: Vec<i32> = metadata
        .topics()
        .iter()
        .filter(|found| found.name() == spec.topic)
        .flat_map(|found| found.partitions().iter().map(|p| p.id()))
        .collect();
    available.sort_unstable();
    if available.is_empty() {
        return Err(Error::Other(format!(
            "this cluster has no topic called {:?}",
            spec.topic
        )));
    }

    let selected: Vec<i32> = match spec.partitions.as_deref() {
        None => available.clone(),
        Some(requested) => {
            let unknown: Vec<String> = requested
                .iter()
                .filter(|p| !available.contains(p))
                .map(i32::to_string)
                .collect();
            if !unknown.is_empty() {
                return Err(Error::Other(format!(
                    "{} has {} partitions, so there is no partition {}",
                    spec.topic,
                    available.len(),
                    unknown.join(", ")
                )));
            }
            let mut selected = requested.to_vec();
            selected.sort_unstable();
            selected.dedup();
            selected
        }
    };

    let mut plan = Vec::with_capacity(selected.len());
    match spec.seek {
        SeekSpec::Offset { partition, offset } => {
            // Existence is checked against what the TOPIC has, never against the
            // filter: told "orders has no partition 3" while partition 3 is
            // sitting there in the topic list, a user goes looking for a cluster
            // problem that does not exist.
            if !available.contains(&partition) {
                return Err(Error::Other(format!(
                    "{} has no partition {partition} to seek in — it has {}, numbered 0 to {}",
                    spec.topic,
                    available.len(),
                    available.len().saturating_sub(1)
                )));
            }
            if !selected.contains(&partition) {
                return Err(Error::Other(format!(
                    "partition {partition} isn't in the partition filter — clear the filter, or \
                     seek within it"
                )));
            }
            let (low, high) = watermarks(consumer, &spec.topic, partition)?;
            plan.push(PartitionSlot {
                partition,
                start: offset.clamp(low, high),
                end: high,
            });
        }
        SeekSpec::Timestamp { timestamp_ms } => {
            let mut request = TopicPartitionList::new();
            for partition in &selected {
                request
                    .add_partition_offset(&spec.topic, *partition, Offset::Offset(timestamp_ms))
                    .map_err(|e| Error::Other(format!("building the timestamp lookup: {e}")))?;
            }
            let resolved = consumer
                .offsets_for_times(request, METADATA_TIMEOUT)
                .map_err(|e| {
                    Error::Other(format!(
                        "asking {} for the offsets at that time: {e}",
                        spec.topic
                    ))
                })?;
            for partition in &selected {
                let (low, high) = watermarks(consumer, &spec.topic, *partition)?;
                let start = resolved
                    .find_partition(&spec.topic, *partition)
                    .and_then(|found| match found.offset() {
                        Offset::Offset(offset) => Some(offset),
                        _ => None,
                    })
                    .unwrap_or(high)
                    .clamp(low, high);
                plan.push(PartitionSlot {
                    partition: *partition,
                    start,
                    end: high,
                });
            }
        }
        SeekSpec::Earliest | SeekSpec::Latest { .. } => {
            let last_n = match spec.seek {
                SeekSpec::Latest { last_n } => Some(i64::from(last_n)),
                _ => None,
            };
            for partition in &selected {
                let (low, high) = watermarks(consumer, &spec.topic, *partition)?;
                let start = match last_n {
                    // `low` guards a compacted or retention-trimmed partition
                    // whose earliest offset is far above zero.
                    Some(n) => high.saturating_sub(n).max(low),
                    None => low,
                };
                plan.push(PartitionSlot {
                    partition: *partition,
                    start,
                    end: high,
                });
            }
        }
    }

    plan.retain(|slot| slot.end > slot.start);
    plan.sort_by_key(|slot| slot.partition);
    Ok(plan)
}

#[cfg(feature = "kafka")]
fn watermarks(
    consumer: &BaseConsumer<KavkaClientContext>,
    topic: &str,
    partition: i32,
) -> Result<(i64, i64)> {
    consumer
        .fetch_watermarks(topic, partition, METADATA_TIMEOUT)
        .map_err(|e| Error::Other(format!("reading the offsets of {topic}[{partition}]: {e}")))
}

// ---------------------------------------------------------------------------
// The wire shape, which no build tier may change.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod wire {
    use super::*;

    /// The Phase 5a IPC contract fixes these shapes and the TypeScript side is
    /// written against them literally, so a rename would compile and ship.
    #[test]
    fn the_ipc_wire_form_is_fixed() {
        let raw = serde_json::json!({
            "topic": "orders",
            "query": "select count(*) from messages",
            "seek": {"kind": "earliest"},
            "partitions": [0, 1],
            "scan_cap": 50_000,
            "max_rows": 1_000,
        });
        let spec: SqlSpec = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(spec.topic, "orders");
        assert_eq!(spec.scan_cap, 50_000);
        assert_eq!(serde_json::to_value(&spec).unwrap(), raw);

        let progress = SqlProgress {
            scanned: 100_000,
            produced_rows: 10_000,
            done: true,
            error: None,
            capped: true,
        };
        assert_eq!(
            serde_json::to_value(&progress).unwrap(),
            serde_json::json!({
                "scanned": 100_000,
                "produced_rows": 10_000,
                "done": true,
                "error": null,
                "capped": true,
            })
        );

        assert_eq!(
            serde_json::to_value(SqlColumn {
                name: "count(*)".into(),
                data_type: "BIGINT".into(),
            })
            .unwrap(),
            serde_json::json!({"name": "count(*)", "data_type": "BIGINT"})
        );
    }

    /// The column list is a contract with the UI's chips and with the surface
    /// doc; both are strings, and neither can be checked by the compiler.
    #[test]
    fn the_virtual_table_is_the_documented_one() {
        let columns = messages_columns();
        assert_eq!(columns.len(), 7);
        for column in &columns {
            assert!(
                SQL_SURFACE.contains(&column.name),
                "{} is a column and is not in the surface doc",
                column.name
            );
        }
        assert_eq!(columns[0].name, "partition");
        assert_eq!(columns[0].data_type, "BIGINT");
        assert_eq!(columns[6].name, "headers_json");
        assert_eq!(sql_surface(), SQL_SURFACE);
    }
}

// ---------------------------------------------------------------------------
// The mapping and the engine, with no cluster involved. Gated on `kafka`
// because `datafusion` is (see Cargo.toml) — but nothing here talks to a broker,
// which is what makes the SQL surface itself testable in milliseconds.
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "kafka"))]
mod engine {
    use super::*;
    use crate::serdes::{decode, decode_header, HeaderEntry};

    fn record_of(key: Option<&[u8]>, value: Option<&[u8]>, headers: Vec<HeaderEntry>) -> Row {
        Row::from_record(MessageRecord {
            partition: 3,
            offset: 8412,
            timestamp_ms: Some(1_700_000_000_000),
            key: key.map(|bytes| decode(bytes, None, DEFAULT_MAX_VALUE_BYTES)),
            value: value.map(|bytes| decode(bytes, None, DEFAULT_MAX_VALUE_BYTES)),
            headers,
            dlq: None,
            masked: false,
        })
    }

    /// Rows straight from JSON, for the query tests: `(partition, offset, key,
    /// value)` with a fixed timestamp.
    fn rows(fixture: &[(i64, i64, Option<&str>, Option<&str>)]) -> Vec<Row> {
        fixture
            .iter()
            .map(|(partition, offset, key, value)| {
                Row::from_record(MessageRecord {
                    partition: i32::try_from(*partition).expect("a fixture partition fits i32"),
                    offset: *offset,
                    timestamp_ms: Some(1_700_000_000_000 + offset),
                    key: key.map(|k| decode(k.as_bytes(), None, DEFAULT_MAX_VALUE_BYTES)),
                    value: value.map(|v| decode(v.as_bytes(), None, DEFAULT_MAX_VALUE_BYTES)),
                    headers: Vec::new(),
                    dlq: None,
                    masked: false,
                })
            })
            .collect()
    }

    /// Runs a query over the given rows and answers with `(columns, rows)`.
    /// The same execution path a session uses, minus the topic.
    fn query(rows: &[Row], sql: &str) -> Result<(Vec<SqlColumn>, Vec<Vec<serde_json::Value>>)> {
        let columns = plan_columns(sql)?;
        let batch = record_batch(rows).map_err(explain)?;
        let shared = Arc::new(SqlShared {
            rows: Mutex::new(RowQueue {
                rows: VecDeque::new(),
                ended: false,
            }),
            ready: Condvar::new(),
            cancel: CancelToken::new(),
            columns: columns.clone(),
            scanned: AtomicU64::new(0),
            produced_rows: AtomicU64::new(0),
            capped: AtomicBool::new(false),
            error: Mutex::new(None),
        });
        execute(sql, vec![batch], MAX_ROWS as usize, &shared)?;
        let queued = shared.lock().rows.iter().cloned().collect();
        Ok((columns, queued))
    }

    fn cell(value: &serde_json::Value) -> String {
        value.to_string()
    }

    // --- the mapping ------------------------------------------------------

    /// THE SCHEMA IS THE CONTRACT. Two lists — the one the UI is given and the
    /// one Arrow is built from — and a test rather than a comment keeping them
    /// the same list.
    #[test]
    fn the_arrow_schema_and_the_published_columns_are_the_same_table() {
        let schema = messages_schema();
        let published = messages_columns();
        assert_eq!(schema.fields().len(), published.len());
        for (field, column) in schema.fields().iter().zip(&published) {
            assert_eq!(field.name(), &column.name);
            assert_eq!(sql_type_name(field.data_type()), column.data_type);
        }
        // Nullability is documented rather than typed, so it is pinned here.
        assert!(!schema.field(0).is_nullable(), "partition");
        assert!(!schema.field(1).is_nullable(), "offset");
        assert!(schema.field(2).is_nullable(), "timestamp_ms");
        assert!(schema.field(3).is_nullable(), "key_text");
        assert!(schema.field(4).is_nullable(), "value_text");
        assert!(schema.field(5).is_nullable(), "value_json");
        assert!(!schema.field(6).is_nullable(), "headers_json");
    }

    #[test]
    fn a_json_record_maps_to_text_and_to_compact_json() {
        let row = record_of(
            Some(b"order-45"),
            Some(br#"{"orderId": 45, "status": "created"}"#),
            vec![decode_header("trace-id", Some(b"abc-123"))],
        );
        assert_eq!(row.partition, 3);
        assert_eq!(row.offset, 8412);
        assert_eq!(row.timestamp_ms, Some(1_700_000_000_000));
        assert_eq!(row.key_text.as_deref(), Some("order-45"));
        // COMPACT, because the surface doc teaches LIKE patterns against it and
        // a pretty-printed rendering would make every one of them wrong.
        assert_eq!(
            row.value_json.as_deref(),
            Some(r#"{"orderId":45,"status":"created"}"#)
        );
        assert_eq!(row.headers_json, r#"{"trace-id":"abc-123"}"#);
        assert!(
            row.value_text.as_deref().is_some_and(|t| t.contains("45")),
            "the display text is the inspector's, pretty-printed: {:?}",
            row.value_text
        );
    }

    /// A TOMBSTONE IS NOT AN EMPTY VALUE. On a compacted topic the difference is
    /// the difference between "this key was deleted" and "this key holds
    /// nothing", and `WHERE value_text IS NULL` is how a query asks the first.
    #[test]
    fn a_tombstone_is_null_in_both_value_columns() {
        let row = record_of(Some(b"order-9"), None, Vec::new());
        assert_eq!(row.key_text.as_deref(), Some("order-9"));
        assert_eq!(row.value_text, None);
        assert_eq!(row.value_json, None);
        assert_eq!(row.headers_json, "{}", "no headers is an empty object");

        // ...and an EMPTY value is not a tombstone: it has text, and that text
        // is the empty string.
        let empty = record_of(Some(b"order-9"), Some(b""), Vec::new());
        assert_eq!(empty.value_text.as_deref(), Some(""));
        assert_eq!(empty.value_json, None);
    }

    #[test]
    fn a_keyless_record_is_null_in_key_text() {
        let row = record_of(None, Some(br#"{"n":1}"#), Vec::new());
        assert_eq!(row.key_text, None);
        // ...and an empty key is not an absent one.
        let empty = record_of(Some(b""), Some(br#"{"n":1}"#), Vec::new());
        assert_eq!(empty.key_text.as_deref(), Some(""));
    }

    /// A VALUE THAT IS NOT JSON IS NOT AN ERROR. Most topics carry at least some
    /// records that are plain text, and a query over them must return rows with
    /// a null `value_json`, not fail.
    #[test]
    fn a_value_that_is_not_json_has_text_and_a_null_value_json() {
        for payload in [
            b"connect timeout after 30s".as_slice(),
            b"not json at all".as_slice(),
            b"{oops".as_slice(),
            b"42abc".as_slice(),
        ] {
            let row = record_of(Some(b"k"), Some(payload), Vec::new());
            assert_eq!(
                row.value_json,
                None,
                "{:?} is not JSON",
                String::from_utf8_lossy(payload)
            );
            assert!(row.value_text.is_some(), "but it still has text");
        }
    }

    /// THE READ RULE, in a column. A payload no decoder could read renders as
    /// hex, and hex is not something anyone writes a `LIKE` against — so the
    /// text is null and the row survives.
    #[test]
    fn an_undecodable_payload_is_null_rather_than_a_hex_dump() {
        let binary: &[u8] = &[0xfe, 0xff, 0xfe, 0xff, 0x01, 0x02];
        let row = record_of(Some(binary), Some(binary), Vec::new());
        assert_eq!(row.key_text, None);
        assert_eq!(row.value_text, None);
        assert_eq!(row.value_json, None);
        // The row is still a row: its coordinates are intact and it is counted.
        assert_eq!(row.partition, 3);
        assert_eq!(row.offset, 8412);
    }

    #[test]
    fn headers_become_an_object_with_nulls_kept_and_repeats_collapsed() {
        let row = record_of(
            Some(b"k"),
            Some(b"{}"),
            vec![
                decode_header("retry", None),
                decode_header("stage", Some(b"first")),
                decode_header("stage", Some(b"second")),
            ],
        );
        let object: serde_json::Value = serde_json::from_str(&row.headers_json).expect("json");
        assert_eq!(object["retry"], serde_json::Value::Null, "no value at all");
        assert_eq!(object["stage"], "second", "a repeat keeps the last");
    }

    #[test]
    fn a_record_from_before_kip_32_has_a_null_timestamp() {
        let mut record = MessageRecord {
            partition: 0,
            offset: 1,
            timestamp_ms: None,
            key: None,
            value: None,
            headers: Vec::new(),
            dlq: None,
            masked: false,
        };
        assert_eq!(Row::from_record(record.clone()).timestamp_ms, None);
        record.timestamp_ms = Some(7);
        assert_eq!(Row::from_record(record).timestamp_ms, Some(7));
    }

    /// The memory ceiling is the one cap whose *trigger* no test can reach
    /// cheaply — 256 MiB of seeded payload is not a unit test — so the
    /// arithmetic it is made of gets pinned instead: a row's footprint is the
    /// text it actually keeps, and a row that keeps nothing costs nothing.
    #[test]
    fn a_rows_footprint_is_the_text_it_keeps() {
        let json = record_of(Some(b"key"), Some(br#"{"n":1}"#), Vec::new());
        let expected = json.key_text.as_ref().unwrap().len()
            + json.value_text.as_ref().unwrap().len()
            + json.value_json.as_ref().unwrap().len()
            + json.headers_json.len();
        assert_eq!(json.footprint(), expected);
        assert!(json.footprint() > 0);

        // A tombstone with no key and no headers is four nulls and an empty
        // object: its whole cost is the two braces.
        let empty = record_of(None, None, Vec::new());
        assert_eq!(empty.footprint(), "{}".len());

        // An undecodable payload is null in both text columns, so it costs
        // nothing here even though its bytes were large — which is the right
        // answer: nothing of it is being kept.
        let hex = record_of(None, Some(&[0xfeu8; 4096]), Vec::new());
        assert_eq!(hex.footprint(), "{}".len());
    }

    #[test]
    fn a_batch_of_mixed_rows_matches_the_schema() {
        let mixed = vec![
            record_of(Some(b"a"), Some(br#"{"n":1}"#), Vec::new()),
            record_of(None, None, Vec::new()),
            record_of(Some(b"c"), Some(b"plain"), Vec::new()),
        ];
        let batch = record_batch(&mixed).expect("a batch");
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.schema(), messages_schema());
        // An empty scan is still a table — this is what makes `count(*)` over an
        // empty window answer 0 instead of failing.
        assert_eq!(record_batch(&[]).expect("an empty batch").num_rows(), 0);
    }

    // --- the SQL surface --------------------------------------------------

    /// THE DOCUMENTATION IS CHECKED AGAINST THE BUILD, IN BOTH DIRECTIONS.
    ///
    /// A help panel that promises `json_get` and then fails on it is worse than
    /// no help panel. Every function the surface doc names is registered in a
    /// real session here, and the ones deliberately compiled out are asserted
    /// absent — so trimming a feature in Cargo.toml breaks this test rather
    /// than a user's query.
    #[test]
    fn the_documented_sql_surface_is_the_one_that_is_built() {
        let context = session(Vec::new()).expect("a session");
        let state = context.state();
        let mut names: std::collections::HashSet<String> =
            state.scalar_functions().keys().cloned().collect();
        names.extend(state.aggregate_functions().keys().cloned());
        names.extend(state.window_functions().keys().cloned());

        for function in DOCUMENTED_FUNCTIONS {
            assert!(
                names.contains(*function),
                "the surface doc names {function}, which this build does not register"
            );
            assert!(
                SQL_SURFACE.contains(function),
                "{function} is in DOCUMENTED_FUNCTIONS and not in the surface text"
            );
        }

        // The ones the Cargo.toml features deliberately leave out. DataFusion
        // ships NO json function of any kind — that is the single most
        // important true sentence in the surface doc, and it is pinned here.
        for absent in [
            "json_get",
            "json_extract",
            "json_as_text",
            "from_json",
            "md5",
            "sha256",
            "sha512",
            "make_array",
            "array_element",
        ] {
            assert!(
                !names.contains(absent),
                "{absent} is registered, so the surface doc is wrong about it"
            );
        }
    }

    // --- planning, and what it refuses ------------------------------------

    /// The columns of an answer are known before a single record is read, which
    /// is what lets the UI draw a header instead of a spinner.
    #[test]
    fn a_query_is_planned_and_typed_before_anything_is_scanned() {
        let columns = plan_columns(
            "SELECT partition, count(*) AS n, avg(length(value_text)) AS mean \
             FROM messages GROUP BY partition",
        )
        .expect("plans");
        assert_eq!(
            columns
                .iter()
                .map(|c| (c.name.as_str(), c.data_type.as_str()))
                .collect::<Vec<_>>(),
            vec![("partition", "BIGINT"), ("n", "BIGINT"), ("mean", "DOUBLE"),]
        );
    }

    /// AN UNKNOWN COLUMN NAMES THE COLUMNS THAT EXIST (docs/DESIGN.md §7 — "when
    /// the broker tells us the answer, put the answer in the message"). The raw
    /// DataFusion text is kept underneath for `Show details`.
    #[test]
    fn an_unknown_column_is_told_which_columns_there_are() {
        let error = plan_columns("SELECT order_id FROM messages")
            .expect_err("no such column")
            .to_string();
        let first = error.lines().next().expect("a first line");

        assert!(first.contains("order_id"), "got {first}");
        for column in [
            "partition",
            "offset",
            "timestamp_ms",
            "key_text",
            "value_text",
            "value_json",
            "headers_json",
        ] {
            assert!(
                first.contains(column),
                "the first line has to list {column}: got {first}"
            );
        }
        assert!(
            error.contains("No field named"),
            "DataFusion's own message is kept verbatim: got {error}"
        );
        assert!(
            error.lines().count() > 1,
            "the plain line and the raw one are separate lines"
        );
    }

    #[test]
    fn a_query_that_does_not_parse_says_so_and_keeps_the_parser_error() {
        // Deliberately an INCOMPLETE expression rather than word soup:
        // `SELECT FROM WHERE messages` parses fine — sqlparser reads `where` as
        // a table name — and lands on the "no such table" line instead, which is
        // the right answer to what it actually says.
        let error = plan_columns("SELECT * FROM messages WHERE partition >")
            .expect_err("an incomplete expression")
            .to_string();
        assert!(error.starts_with("This query didn't parse."), "got {error}");
        assert!(error.contains("SQL error"), "got {error}");
    }

    #[test]
    fn a_query_against_another_table_is_told_where_the_topic_comes_from() {
        let error = plan_columns("SELECT * FROM orders")
            .expect_err("no such table")
            .to_string();
        let first = error.lines().next().expect("a first line");
        assert!(first.contains("messages"), "got {first}");
        assert!(first.contains("above the query box"), "got {first}");
    }

    #[test]
    fn a_function_this_build_does_not_have_is_named_rather_than_guessed_at() {
        let error = plan_columns("SELECT json_get(value_json, 'status') FROM messages")
            .expect_err("no such function")
            .to_string();
        let first = error.lines().next().expect("a first line");
        assert!(first.contains("json_get"), "got {first}");
        assert!(first.contains("no SQL function"), "got {first}");
    }

    /// THE GUARDRAIL. A query box that can write a file is not a query box.
    /// Every one of these is refused on the LOGICAL PLAN, before execution, so
    /// nothing is created, written or read from disk on the way to the refusal.
    #[test]
    fn nothing_that_writes_is_planned_at_all() {
        for statement in [
            "CREATE TABLE evil AS SELECT * FROM messages",
            "CREATE EXTERNAL TABLE evil STORED AS CSV LOCATION '/etc/passwd'",
            "INSERT INTO messages VALUES (1, 1, 1, 'a', 'b', 'c', '{}')",
            "COPY (SELECT * FROM messages) TO 'stolen.csv'",
            "SET datafusion.execution.batch_size = 1",
            "DROP TABLE messages",
        ] {
            let error = plan_columns(statement)
                .expect_err(&format!("{statement} must be refused"))
                .to_string();
            assert!(
                error.starts_with("SQL over topics only reads")
                    || error.starts_with("This query didn't parse."),
                "{statement} was refused with: {error}"
            );
        }
        // ...and the file catalogue that would make this resolve is never
        // enabled, so a path in FROM is just a table that does not exist.
        assert!(plan_columns("SELECT * FROM 'C:/Windows/win.ini'").is_err());
    }

    /// `offset` is a SQL keyword. The surface doc tells the user to quote it,
    /// and this is the test that keeps that sentence true in both directions.
    #[test]
    fn the_offset_column_is_a_keyword_and_the_doc_says_so() {
        assert!(
            plan_columns(r#"SELECT "offset" FROM messages ORDER BY "offset" LIMIT 5"#).is_ok(),
            "quoted, it works everywhere"
        );
        assert!(
            SQL_SURFACE.contains(r#"quote it —"#),
            "the doc has to teach the quoting"
        );
    }

    // --- queries, over rows that never came from a cluster -----------------

    fn orders() -> Vec<Row> {
        rows(&[
            (
                0,
                0,
                Some("a-1"),
                Some(r#"{"status":"created","amount":10}"#),
            ),
            (
                0,
                1,
                Some("a-2"),
                Some(r#"{"status":"failed","amount":20}"#),
            ),
            (
                1,
                0,
                Some("b-1"),
                Some(r#"{"status":"failed","amount":30}"#),
            ),
            (
                1,
                1,
                Some("b-2"),
                Some(r#"{"status":"shipped","amount":40}"#),
            ),
            (1, 2, Some("b-3"), None),
        ])
    }

    #[test]
    fn count_star_counts_every_row_including_the_tombstones() {
        let (columns, rows) = query(&orders(), "SELECT count(*) FROM messages").expect("runs");
        assert_eq!(columns.len(), 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(cell(&rows[0][0]), "5");
    }

    #[test]
    fn a_where_clause_over_value_json_finds_exactly_the_matching_rows() {
        let (_, rows) = query(
            &orders(),
            r#"SELECT key_text FROM messages WHERE value_json LIKE '%"status":"failed"%' ORDER BY key_text"#,
        )
        .expect("runs");
        let found: Vec<String> = rows.iter().map(|row| cell(&row[0])).collect();
        assert_eq!(found, vec!["\"a-2\"", "\"b-1\""]);
    }

    #[test]
    fn group_by_partition_splits_the_rows_and_they_add_up() {
        let (_, rows) = query(
            &orders(),
            "SELECT partition, count(*) AS n FROM messages GROUP BY partition ORDER BY partition",
        )
        .expect("runs");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (cell(&rows[0][0]), cell(&rows[0][1])),
            ("0".into(), "2".into())
        );
        assert_eq!(
            (cell(&rows[1][0]), cell(&rows[1][1])),
            ("1".into(), "3".into())
        );
    }

    #[test]
    fn order_by_and_limit_take_the_top_of_a_sorted_answer() {
        let (_, rows) = query(
            &orders(),
            r#"SELECT key_text FROM messages ORDER BY partition DESC, "offset" DESC LIMIT 2"#,
        )
        .expect("runs");
        assert_eq!(
            rows.iter().map(|row| cell(&row[0])).collect::<Vec<_>>(),
            vec!["\"b-3\"", "\"b-2\""]
        );
    }

    #[test]
    fn a_tombstone_is_findable_and_countable() {
        let (_, rows) = query(
            &orders(),
            "SELECT count(*) FROM messages WHERE value_text IS NULL",
        )
        .expect("runs");
        assert_eq!(cell(&rows[0][0]), "1");
    }

    /// Numbers come back as JSON numbers and text as JSON strings — a UI cannot
    /// right-align `"5"`.
    #[test]
    fn the_row_encoding_keeps_numbers_numeric() {
        let (_, rows) = query(
            &orders(),
            "SELECT count(*) AS n, max(key_text) AS k, count(*) > 1 AS many FROM messages",
        )
        .expect("runs");
        assert!(rows[0][0].is_number(), "count is a number");
        assert!(rows[0][1].is_string(), "max(text) is a string");
        assert!(rows[0][2].is_boolean(), "a comparison is a boolean");
    }

    /// A type this module does not name explicitly still becomes a cell rather
    /// than an error — the fallback is the whole reason `CAST(… AS TIMESTAMP)`
    /// works at all.
    #[test]
    fn a_type_outside_the_json_set_falls_back_to_its_sql_text() {
        let (columns, rows) = query(
            &orders(),
            "SELECT to_timestamp_millis(timestamp_ms) AS at FROM messages ORDER BY at LIMIT 1",
        )
        .expect("runs");
        assert_eq!(columns[0].data_type, "TIMESTAMP");
        assert!(
            rows[0][0].is_string(),
            "rendered, not dropped: {:?}",
            rows[0][0]
        );
        assert!(cell(&rows[0][0]).contains("2023"), "{:?}", rows[0][0]);
    }

    /// Aggregates, sub-queries and window functions in one query — the three
    /// things the surface doc claims and the three a "SELECT only" toy would
    /// not have.
    #[test]
    fn the_bigger_half_of_the_surface_actually_runs() {
        let (_, rows) = query(
            &orders(),
            "WITH failed AS (SELECT partition, key_text FROM messages \
             WHERE value_json LIKE '%failed%') \
             SELECT partition, key_text, row_number() OVER (PARTITION BY partition ORDER BY key_text) AS n \
             FROM failed ORDER BY partition",
        )
        .expect("runs");
        assert_eq!(rows.len(), 2);
        assert_eq!(cell(&rows[0][2]), "1");
    }

    /// The surface doc claims set operations and self-joins by name, so each
    /// one is planned here rather than assumed. Planning is enough: a construct
    /// DataFusion cannot plan is one it cannot run, and these are about the
    /// grammar rather than the arithmetic.
    #[test]
    fn every_construct_the_surface_doc_names_actually_plans() {
        for sql in [
            "SELECT partition FROM messages UNION SELECT partition FROM messages",
            "SELECT partition FROM messages UNION ALL SELECT partition FROM messages",
            "SELECT partition FROM messages INTERSECT SELECT partition FROM messages",
            "SELECT partition FROM messages EXCEPT SELECT partition FROM messages",
            "SELECT a.key_text FROM messages a JOIN messages b ON a.partition = b.partition",
            "SELECT DISTINCT partition FROM messages",
            "SELECT partition FROM messages WHERE partition IN \
             (SELECT partition FROM messages WHERE value_text IS NULL)",
            "SELECT partition, count(*) FROM messages GROUP BY partition HAVING count(*) > 1",
            "SELECT CASE WHEN value_text IS NULL THEN 'tombstone' ELSE 'record' END FROM messages",
            "SELECT CAST(timestamp_ms AS VARCHAR), TRY_CAST(key_text AS BIGINT) FROM messages",
            "SELECT key_text || '-' || value_text FROM messages WHERE partition BETWEEN 0 AND 3",
            "SELECT key_text FROM messages WHERE value_text ILIKE '%FAILED%' \
             ORDER BY key_text LIMIT 5 OFFSET 2",
            "SELECT regexp_like(value_json, '\"amount\":[0-9]+') FROM messages",
            "SELECT date_trunc('hour', to_timestamp_millis(timestamp_ms)) FROM messages",
        ] {
            assert!(plan_columns(sql).is_ok(), "the doc claims this: {sql}");
        }
    }

    #[test]
    fn a_query_over_an_empty_window_answers_rather_than_failing() {
        let (_, rows) = query(&[], "SELECT count(*) FROM messages").expect("runs");
        assert_eq!(cell(&rows[0][0]), "0");
        let (_, none) = query(&[], "SELECT key_text FROM messages").expect("runs");
        assert!(none.is_empty());
    }

    // --- the plain-language layer, over captured strings -------------------

    #[test]
    fn the_first_line_is_a_total_function_over_anything_datafusion_says() {
        assert!(
            plain_line("Schema error: No field named foo. Did you mean 'bar'?")
                .contains("no column called foo")
        );
        assert!(
            plain_line("Error during planning: Invalid function 'nope'.\nDid you mean 'now'?")
                .contains("nope")
        );
        assert!(
            plain_line("Error during planning: DDL not supported: CreateExternalTable")
                .starts_with("SQL over topics only reads")
        );
        assert!(
            plain_line("SQL error: ParserError(\"Expected an expression\")")
                .starts_with("This query didn't parse.")
        );
        // Anything unrecognised still gets a sentence, and the raw text is what
        // `explain` puts underneath it.
        assert_eq!(
            plain_line("Resources exhausted"),
            "This query couldn't run."
        );
        assert_eq!(plain_line(""), "This query couldn't run.");
    }
}

/// The half that needs a cluster. It lives here rather than in `tests/` for the
/// same reason `src/search.rs`'s does: rdkafka is a feature-gated dependency of
/// this crate, and a `dev-dependency` on it would make plain
/// `cargo test -p kavka-core` require CMake, breaking the bare-toolchain tier
/// the feature split exists to protect (see Cargo.toml).
///
/// Run:  docker compose -f dev/docker-compose.yml up -d --wait
///       KAVKA_IT=1 cargo test -p kavka-core --features kafka-ssl
#[cfg(all(test, feature = "kafka"))]
mod cluster {
    use super::*;
    use crate::profiles::{AuthConfig, ConnectionProfile};
    use rdkafka::config::ClientConfig;
    use rdkafka::producer::{BaseProducer, BaseRecord, Producer};

    fn bootstrap() -> String {
        std::env::var("KAVKA_TEST_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into())
    }

    fn integration() -> bool {
        if std::env::var("KAVKA_IT").is_err() {
            eprintln!("skipped: set KAVKA_IT=1 with dev/docker-compose.yml running");
            return false;
        }
        true
    }

    fn connection() -> ClusterConnection {
        ClusterConnection::connect(ConnectionProfile {
            id: "it-sql".into(),
            name: "local docker".into(),
            environment: "dev".into(),
            bootstrap_servers: vec![bootstrap()],
            auth: AuthConfig::Plaintext,
            read_only: false,
            schema_registry: None,
            connect_clusters: Vec::new(),
            metrics_endpoint: None,
            sampler_interval_ms: None,
            wasm_serdes: Vec::new(),
        })
        .expect("connect")
    }

    fn unique_topic(what: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("kavka-it-{what}-{}-{nanos:x}", std::process::id())
    }

    fn await_topic(conn: &ClusterConnection, topic: &str, partitions: usize) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(detail) = crate::admin::topic_detail(conn, topic) {
                if detail.partitions.len() == partitions {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "{topic} never reported {partitions} partitions"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// THE GOLDEN CORPUS. Fifty orders, round-robin over the partitions, with a
    /// status that is a deterministic function of the index — so every expected
    /// count in these tests is arithmetic rather than a number someone once
    /// observed.
    ///
    /// `n % 5 == 0` is `failed`: 10 of the 50. `n % 5 == 1` is `shipped`: 10.
    /// The remaining 30 are `created`.
    fn seed_orders(topic: &str, count: usize, partitions: i32) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .set("queue.buffering.max.messages", "1000000")
            .set("batch.num.messages", "10000")
            .set("linger.ms", "20")
            .create()
            .expect("producer");
        for n in 0..count {
            let key = format!("order-{n}");
            let status = match n % 5 {
                0 => "failed",
                1 => "shipped",
                _ => "created",
            };
            let payload = format!(
                r#"{{"orderId":{n},"status":"{status}","amount":{}}}"#,
                n * 10
            );
            let partition = i32::try_from(n).expect("a fixture fits i32") % partitions;
            if n % 1_000 == 0 {
                producer.poll(Duration::ZERO);
            }
            loop {
                match producer.send(
                    BaseRecord::to(topic)
                        .key(&key)
                        .payload(&payload)
                        .partition(partition),
                ) {
                    Ok(()) => break,
                    // The local queue is full; let it drain rather than dropping
                    // the record and seeding a topic that is short.
                    Err((_, _)) => producer.poll(Duration::from_millis(50)),
                }
            }
        }
        producer.flush(Duration::from_secs(120)).expect("flush");
    }

    /// Seeds one partition inside a COMMITTED TRANSACTION, which leaves a
    /// control record on the last offset — the only fixture that produces a
    /// topic whose end watermark no consumer can reach, and therefore the only
    /// way to exercise the quiet deadline's honesty without unplugging a broker.
    fn seed_transactional(topic: &str, count: usize) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap())
            .set("acks", "all")
            .set("transactional.id", unique_topic("tx"))
            .create()
            .expect("producer");
        producer
            .init_transactions(Duration::from_secs(30))
            .expect("init transactions");
        producer.begin_transaction().expect("begin");
        for n in 0..count {
            let key = format!("order-{n}");
            let payload = format!(
                r#"{{"orderId":{n},"status":"created","amount":{}}}"#,
                n * 10
            );
            producer
                .send(
                    BaseRecord::to(topic)
                        .key(&key)
                        .payload(&payload)
                        .partition(0),
                )
                .map_err(|(e, _)| e)
                .expect("seeding send");
        }
        producer
            .commit_transaction(Duration::from_secs(30))
            .expect("commit the transaction");
    }

    /// Reads `topic` once through the browse path, so the *next* client this
    /// process opens starts warm.
    ///
    /// Nothing about the scan under test needs this; a cold runner does. The
    /// first consumer a process opens against a freshly started cluster pays for
    /// librdkafka's init, a TCP connect, a metadata round trip and the group
    /// coordinator lookup that a `group.id` client makes even when it assigns by
    /// hand — and on a brand-new broker that lookup is what creates
    /// `__consumer_offsets`. On a loaded CI runner that bill has landed on the
    /// far side of the scan's cold-start budget, and a scan that read nothing
    /// answers `count(*) = 0`, which looks exactly like a correct answer.
    ///
    /// So the bill is paid here, before the clock the test is actually about.
    /// It is an assertion too, and deliberately: if the fixture is not readable
    /// this fails here, naming the fixture, instead of downstream as a wrong
    /// count.
    fn warm_the_consumer_path(conn: &ClusterConnection, topic: &str, at_least: usize) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let read = crate::consume::fetch_messages(
                conn,
                None,
                &crate::consume::FetchSpec {
                    topic: topic.into(),
                    seek: SeekSpec::Earliest,
                    partitions: None,
                    max_messages: at_least as u32,
                    max_value_bytes: None,
                },
                None,
            )
            .expect("browse the fixture");
            if read.len() >= at_least {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{topic} never served its {at_least} seeded records to a plain browse, \
                 so there is nothing for the scan to be right or wrong about"
            );
        }
    }

    fn spec(topic: &str, query: &str) -> SqlSpec {
        SqlSpec {
            topic: topic.into(),
            query: query.into(),
            seek: SeekSpec::Earliest,
            partitions: None,
            scan_cap: MAX_SCANNED,
            max_rows: MAX_ROWS,
        }
    }

    /// Runs one query to completion and answers with its rows and final
    /// progress.
    fn run_query(
        conn: &ClusterConnection,
        spec: &SqlSpec,
    ) -> (Vec<Vec<serde_json::Value>>, SqlProgress, Vec<SqlColumn>) {
        let session = SqlSession::start(conn, None, spec).expect("start");
        let columns = session.columns();
        let mut rows = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(120);
        while let Some(batch) = session.next_rows(Duration::from_millis(250)) {
            rows.extend(batch);
            assert!(Instant::now() < deadline, "the query never finished");
        }
        let progress = session.progress();
        assert!(progress.done, "a drained session is a finished one");
        assert_eq!(progress.error, None, "the query failed: {progress:?}");
        (rows, progress, columns)
    }

    fn number(value: &serde_json::Value) -> i64 {
        value
            .as_i64()
            .unwrap_or_else(|| panic!("{value:?} is not a number"))
    }

    /// One topic, four queries: the golden-corpus gate. They share a topic
    /// because seeding is the slow part and every one of them is a read.
    #[test]
    fn the_golden_corpus_answers_exactly() {
        if !integration() {
            return;
        }
        const COUNT: usize = 50;
        let conn = connection();
        let topic = unique_topic("sql-golden");
        crate::admin::create_topic(&conn, &topic, 3, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 3);
        seed_orders(&topic, COUNT, 3);

        // 1. count(*) over the whole window.
        let (rows, progress, columns) =
            run_query(&conn, &spec(&topic, "SELECT count(*) FROM messages"));
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].data_type, "BIGINT");
        assert_eq!(rows.len(), 1);
        assert_eq!(number(&rows[0][0]), COUNT as i64);
        assert_eq!(progress.scanned, COUNT as u64);
        assert_eq!(progress.produced_rows, 1);
        assert!(!progress.capped, "50 records is under every ceiling");

        // 2. WHERE over the JSON text. Ten of the fifty are `failed`, and the
        //    fixture's arithmetic — not an observation — says which.
        let (rows, _, _) = run_query(
            &conn,
            &spec(
                &topic,
                r#"SELECT key_text FROM messages WHERE value_json LIKE '%"status":"failed"%' ORDER BY key_text"#,
            ),
        );
        let failed: Vec<String> = rows
            .iter()
            .map(|row| row[0].as_str().expect("text").to_string())
            .collect();
        let expected: Vec<String> = {
            let mut names: Vec<String> = (0..COUNT)
                .filter(|n| n % 5 == 0)
                .map(|n| format!("order-{n}"))
                .collect();
            names.sort();
            names
        };
        assert_eq!(failed, expected, "exactly the failed orders, in order");
        assert_eq!(failed.len(), 10);

        // 3. GROUP BY partition. The counts add up to the whole corpus, which
        //    is the property that catches a scan that lost a partition.
        let (rows, _, _) = run_query(
            &conn,
            &spec(
                &topic,
                "SELECT partition, count(*) AS n FROM messages GROUP BY partition ORDER BY partition",
            ),
        );
        assert_eq!(rows.len(), 3, "one row per partition");
        let total: i64 = rows.iter().map(|row| number(&row[1])).sum();
        assert_eq!(total, COUNT as i64);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(number(&row[0]), index as i64);
            // Round-robin over three partitions: 17, 17, 16.
            let expected = (0..COUNT).filter(|n| n % 3 == index).count() as i64;
            assert_eq!(number(&row[1]), expected, "partition {index}");
        }

        // 4. ORDER BY offset with a LIMIT — the keyword column, quoted, and the
        //    top of a sorted answer.
        let (rows, progress, _) = run_query(
            &conn,
            &spec(
                &topic,
                r#"SELECT partition, "offset", key_text FROM messages
                   ORDER BY partition, "offset" LIMIT 4"#,
            ),
        );
        assert_eq!(rows.len(), 4);
        assert_eq!(progress.produced_rows, 4);
        assert!(
            !progress.capped,
            "a LIMIT the user typed is not a ceiling Kavka imposed"
        );
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(number(&row[0]), 0, "partition 0 sorts first");
            assert_eq!(number(&row[1]), index as i64);
            // Partition 0 holds orders 0, 3, 6, 9 — the round-robin.
            assert_eq!(
                row[2].as_str(),
                Some(format!("order-{}", index * 3).as_str())
            );
        }

        let _ = crate::admin::delete_topic(&conn, &topic);
    }

    /// A CAPPED SCAN SAYS SO, AND ITS ANSWER IS STILL CORRECT — for the smaller
    /// question it actually asked.
    ///
    /// This is the failure mode the whole `capped` flag exists for: without it,
    /// `count(*)` over a topic of 200 records with a scan cap of 50 answers
    /// "50", which is indistinguishable from the truth about a topic of 50.
    #[test]
    fn a_capped_scan_is_exact_and_says_it_was_capped() {
        if !integration() {
            return;
        }
        const COUNT: usize = 200;
        const CAP: u32 = 50;
        let conn = connection();
        let topic = unique_topic("sql-capped");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);
        seed_orders(&topic, COUNT, 1);

        let (rows, progress, _) = run_query(
            &conn,
            &SqlSpec {
                scan_cap: CAP,
                ..spec(&topic, "SELECT count(*) FROM messages")
            },
        );
        assert!(progress.capped, "the cap bit, and it has to say so");
        assert_eq!(
            progress.scanned,
            u64::from(CAP),
            "a capped scan reads exactly its cap, never one more"
        );
        assert_eq!(
            number(&rows[0][0]),
            i64::from(CAP),
            "and the answer is exactly right about what it read"
        );

        // One partition read from the earliest offset, so the capped window is
        // the FIRST 50 records — a partial answer that is reproducible, not an
        // arbitrary sample.
        let (rows, progress, _) = run_query(
            &conn,
            &SqlSpec {
                scan_cap: CAP,
                ..spec(&topic, r#"SELECT max("offset") AS last FROM messages"#)
            },
        );
        assert!(progress.capped);
        assert_eq!(number(&rows[0][0]), i64::from(CAP) - 1);

        // The ROW cap is the other half of the same flag, and it is independent
        // of the scan: 200 records fit under the scan cap, 10 rows do not fit
        // under a max_rows of 10 when there are 200 of them.
        let (rows, progress, _) = run_query(
            &conn,
            &SqlSpec {
                max_rows: 10,
                ..spec(&topic, "SELECT key_text FROM messages")
            },
        );
        assert_eq!(rows.len(), 10);
        assert_eq!(progress.scanned, COUNT as u64, "the scan was not capped");
        assert!(progress.capped, "but the answer was truncated, and says so");

        // ...and a max_rows that is not reached is not a cap.
        let (rows, progress, _) = run_query(
            &conn,
            &SqlSpec {
                max_rows: 10,
                ..spec(&topic, "SELECT key_text FROM messages LIMIT 3")
            },
        );
        assert_eq!(rows.len(), 3);
        assert!(!progress.capped);

        let _ = crate::admin::delete_topic(&conn, &topic);
    }

    /// CANCELLATION, mid-scan. Stopped while it is still reading, a query has to
    /// stop *promptly*, report `done`, produce NO answer (see the module docs —
    /// a half-scanned aggregate is a confidently wrong number) — and leave no
    /// thread behind when the session is dropped.
    ///
    /// The fixture is 60,000 records rather than the few hundred that would
    /// prove the mechanism, because "this stopped part-way" is only meaningful
    /// while the scan is still running. If this ever fails on `scanned`, the
    /// machine got faster: raise `COUNT`.
    #[test]
    fn a_cancelled_query_stops_promptly_and_answers_nothing() {
        if !integration() {
            return;
        }
        const COUNT: usize = 60_000;
        let conn = connection();
        let topic = unique_topic("sql-cancel");
        crate::admin::create_topic(&conn, &topic, 6, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 6);
        seed_orders(&topic, COUNT, 6);

        let session = SqlSession::start(
            &conn,
            None,
            &spec(
                &topic,
                "SELECT partition, count(*) FROM messages GROUP BY partition",
            ),
        )
        .expect("start");

        // Cancelling a query that never started proves nothing, so wait for the
        // scan to actually be moving — and no longer than that.
        let deadline = Instant::now() + Duration::from_secs(30);
        while session.progress().scanned == 0 {
            assert!(Instant::now() < deadline, "the scan never got going");
            std::thread::sleep(Duration::from_millis(1));
        }

        let stopped_at = Instant::now();
        session.stop();
        session.stop(); // idempotent

        let mut progress = session.progress();
        while !progress.done {
            assert!(
                stopped_at.elapsed() < Duration::from_secs(10),
                "stop() did not end the query: {progress:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
            progress = session.progress();
        }
        let took = stopped_at.elapsed();
        eprintln!(
            "cancelled after {} of {COUNT} scanned in {took:?}",
            progress.scanned
        );
        assert!(
            took < Duration::from_secs(5),
            "cancellation is noticed within a poll, not at a deadline: {took:?}"
        );
        assert!(progress.error.is_none(), "cancelling is not a failure");
        assert!(
            progress.scanned > 0 && progress.scanned < COUNT as u64,
            "a cancelled scan is partial, got {} of {COUNT}",
            progress.scanned
        );
        assert_eq!(
            progress.produced_rows, 0,
            "a query cancelled mid-scan answers nothing rather than answering wrongly"
        );
        assert!(
            session.next_rows(Duration::from_millis(50)).is_none(),
            "and there is nothing queued behind it"
        );

        // The thread-leak check. `Drop` joins the worker, so a worker that
        // outlived its session would block here.
        let dropped_at = Instant::now();
        drop(session);
        assert!(
            dropped_at.elapsed() < Duration::from_secs(5),
            "dropping the session had to join a worker that was still running"
        );

        let _ = crate::admin::delete_topic(&conn, &topic);
    }

    /// A query whose window is empty still answers, and a seek that lands past
    /// the end is such a window. The alternative — an error — would make "no
    /// orders yesterday" indistinguishable from "the query broke".
    #[test]
    fn a_query_over_an_empty_window_answers_zero() {
        if !integration() {
            return;
        }
        let conn = connection();
        let topic = unique_topic("sql-empty");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);

        let (rows, progress, _) = run_query(&conn, &spec(&topic, "SELECT count(*) FROM messages"));
        assert_eq!(number(&rows[0][0]), 0);
        assert_eq!(progress.scanned, 0);
        assert!(!progress.capped, "nothing was truncated; there was nothing");

        let _ = crate::admin::delete_topic(&conn, &topic);
    }

    /// THE FOURTH WAY AN ANSWER GOES PARTIAL, and the one the flag used to miss.
    ///
    /// A committed transaction leaves a control record on the last offset of the
    /// partition: the end watermark is one past anything a consumer will ever be
    /// given, so the scan finishes on its quiet deadline with that partition
    /// short of where it was told to stop. Nothing in the poll loop can tell
    /// that apart from a broker that stopped answering half way through — so the
    /// answer is a partial one and has to say so, exactly as it does when the
    /// record or row ceilings bite.
    ///
    /// The `count(*)` is still exact for what was read: `capped` is a caveat on
    /// the scope, never an admission that the arithmetic is wrong.
    #[test]
    fn a_partition_that_goes_quiet_short_of_its_end_caps_the_answer() {
        if !integration() {
            return;
        }
        const COUNT: usize = 6;
        let conn = connection();
        let topic = unique_topic("sql-quiet");
        crate::admin::create_topic(&conn, &topic, 1, 1, &[]).expect("create topic");
        await_topic(&conn, &topic, 1);
        seed_transactional(&topic, COUNT);
        // A cold client is not what this test is about; see the helper.
        warm_the_consumer_path(&conn, &topic, COUNT);

        // Bounded, and it retries ONE thing: a scan that read nothing at all,
        // which on a loaded runner means the client was still connecting and not
        // that the flag under test is wrong. Every other outcome — including a
        // scan that read some records — falls straight through to the assertions
        // below, so a `capped` that stopped being set fails on the first attempt
        // and a scan that reads the wrong number fails on it too.
        let mut attempts = 0;
        let (rows, progress) = loop {
            attempts += 1;
            let (rows, progress, _) =
                run_query(&conn, &spec(&topic, "SELECT count(*) FROM messages"));
            if progress.scanned > 0 || attempts == 3 {
                break (rows, progress);
            }
        };
        let _ = crate::admin::delete_topic(&conn, &topic);

        assert_eq!(
            number(&rows[0][0]),
            COUNT as i64,
            "every readable record was scanned (attempt {attempts})"
        );
        assert_eq!(progress.scanned, COUNT as u64);
        assert!(
            progress.capped,
            "the commit marker's offset was never read, so the scan is short of its \
             window and the answer is partial: {progress:?}"
        );
        // Still not a failure: a capped answer is an answer.
        assert_eq!(progress.error, None);
    }

    /// An unknown topic fails from `start`, like every other deterministic
    /// mistake — not as a query that runs and finds nothing.
    #[test]
    fn an_unknown_topic_is_an_error_from_start() {
        if !integration() {
            return;
        }
        let conn = connection();
        let error = SqlSession::start(
            &conn,
            None,
            &spec(
                "kavka-it-no-such-topic-at-all",
                "SELECT count(*) FROM messages",
            ),
        )
        .expect_err("no such topic")
        .to_string();
        assert!(error.contains("no topic called"), "got {error}");
    }
}
