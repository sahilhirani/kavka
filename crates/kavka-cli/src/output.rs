//! What goes on stdout, and in what shape.
//!
//! **stdout is the answer; stderr is everything Kavka has to say about it.**
//! Every command builds one [`Answer`] — rows, a document, a table and a list
//! of sentences — and [`emit`] writes whichever of them the [`Mode`] calls for.
//! One shape means the three renderings cannot disagree about what was
//! returned, and it means a command's own module contains no `println!`.
//!
//! # Why the default depends on where stdout points
//!
//! The format a person wants and the format a script wants are different, and
//! there is no third answer that serves both: a table piped into `jq` is
//! useless, and NDJSON on a terminal is unreadable. Every CLI that picks one
//! makes the other case pass a flag forever. So the default is
//! [`Mode::detect`]: a terminal gets the table, anything else gets NDJSON, and
//! `--output` overrides both. `std::io::IsTerminal` has been in the standard
//! library since Rust 1.70, so this costs no dependency.
//!
//! # The table is docs/DESIGN.md §5.2's, as far as a terminal can carry it
//!
//! No borders, no zebra, no corners — a header in micro-caps, one hairline
//! under it, and leading between the columns. Identifiers left and unpadded;
//! quantities right and space-grouped, exactly as §7 rule 5 requires of a
//! table. Absent is `∅` and never the word `null`, and every state that has a
//! colour in the app has a WORD here (Law 2), which is also why this program
//! needs no colour support at all.

use kavka_core::serdes::{DecodedPayload, MessageRecord};
use serde_json::{json, Map, Value};
use std::io::{self, IsTerminal, Write};

/// How stdout is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A human-readable table. The default when stdout is a terminal.
    Table,
    /// One JSON object per line — the rows and nothing else. The default when
    /// stdout is a pipe or a file.
    Ndjson,
    /// One pretty-printed document: the rows *and* the limits, progress and
    /// masking state that came with them.
    Json,
}

impl Mode {
    /// The mode a run uses: what was asked for, else what stdout is.
    pub fn detect(requested: Option<Mode>, stdout_is_terminal: bool) -> Self {
        match requested {
            Some(mode) => mode,
            None if stdout_is_terminal => Self::Table,
            None => Self::Ndjson,
        }
    }

    /// The mode for this process's actual stdout.
    pub fn for_stdout(requested: Option<Mode>) -> Self {
        Self::detect(requested, io::stdout().is_terminal())
    }
}

/// Which way a column's cells are pushed. §5.2: identifiers left, quantities
/// right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Column {
    /// Shown in micro-caps, which is one of the four places docs/DESIGN.md
    /// allows uppercase at all. A `String` rather than a `&'static str` because
    /// a SQL result's columns are named by the query.
    pub head: String,
    pub align: Align,
    /// Cells longer than this are cut with `…`. `None` never cuts — used for
    /// every column that carries an address (offset, partition, topic name),
    /// because a truncated identifier is worse than a wide table.
    pub max_width: Option<usize>,
}

impl Column {
    pub fn left(head: impl Into<String>) -> Self {
        Self {
            head: head.into(),
            align: Align::Left,
            max_width: None,
        }
    }

    pub fn right(head: impl Into<String>) -> Self {
        Self {
            head: head.into(),
            align: Align::Right,
            max_width: None,
        }
    }

    /// A payload column: left, and cut at `width`.
    pub fn payload(head: impl Into<String>, width: usize) -> Self {
        Self {
            head: head.into(),
            align: Align::Left,
            max_width: Some(width),
        }
    }
}

/// The table a terminal gets.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        debug_assert_eq!(
            row.len(),
            self.columns.len(),
            "a row must have one cell per column"
        );
        self.rows.push(row);
    }

    /// The table as text, header + hairline + rows. Empty when there are no
    /// rows: the empty state is a sentence, and it is the caller's.
    pub fn render(&self) -> String {
        if self.rows.is_empty() {
            return String::new();
        }
        let cut: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&self.columns)
                    .map(|(cell, column)| match column.max_width {
                        Some(width) => one_line(cell, width),
                        None => one_line(cell, usize::MAX),
                    })
                    .collect()
            })
            .collect();
        let widths: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                cut.iter()
                    .map(|row| width_of(&row[index]))
                    .chain(std::iter::once(width_of(&column.head)))
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let mut out = String::new();
        let head: Vec<String> = self
            .columns
            .iter()
            .zip(&widths)
            .map(|(column, width)| pad(&column.head, *width, column.align))
            .collect();
        out.push_str(head.join("  ").trim_end());
        out.push('\n');
        // The one hairline under the header — §5.2's `--hairline-strong`, and
        // the only rule this table draws.
        let rule: Vec<String> = widths.iter().map(|width| "─".repeat(*width)).collect();
        out.push_str(&rule.join("  "));
        out.push('\n');
        for row in &cut {
            let cells: Vec<String> = row
                .iter()
                .zip(&widths)
                .zip(&self.columns)
                .map(|((cell, width), column)| pad(cell, *width, column.align))
                .collect();
            out.push_str(cells.join("  ").trim_end());
            out.push('\n');
        }
        out
    }
}

/// What a terminal gets: rows, or a sentence.
///
/// A produced record is the case that needs the second one. `Sent to
/// dead-letter[2] at offset 51` is docs/DESIGN.md §7 rule 2 — the verb survives
/// the whole flow — and a one-row table with the headings TOPIC PARTITION
/// OFFSET says the same thing in a shape nobody reads aloud.
#[derive(Debug, Clone)]
pub enum Body {
    Rows(Table),
    Text(String),
}

/// One command's whole answer.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The rows, as JSON. One per line in [`Mode::Ndjson`].
    pub rows: Vec<Value>,
    /// The whole answer, including the caps and progress the rows do not carry.
    /// [`Mode::Json`] prints this and nothing else.
    pub document: Value,
    /// The same answer, for a terminal.
    pub body: Body,
    /// Further tables a terminal gets under the first — a topic's
    /// configuration, a group's members. They are already inside `document`, so
    /// no other mode has to render them.
    pub sections: Vec<(String, Table)>,
    /// The situation and the action, for when there are no rows at all
    /// (docs/DESIGN.md §7, "Empty states"). Goes to stderr, never stdout — an
    /// empty pipe is the correct answer to an empty result.
    pub empty: String,
    /// Sentences for stderr: caps hit, masking applied, prod. Written before
    /// the rows so a person reading a terminal sees the caveat first.
    pub notes: Vec<String>,
}

impl Answer {
    pub fn new(document: Value, rows: Vec<Value>, table: Table, empty: impl Into<String>) -> Self {
        Self {
            rows,
            document,
            body: Body::Rows(table),
            sections: Vec::new(),
            empty: empty.into(),
            notes: Vec::new(),
        }
    }

    /// A single-object answer — a produced record, one topic's detail. The
    /// "rows" are the object itself, so `--output ndjson` gives one line.
    pub fn single(document: Value, body: Body) -> Self {
        Self {
            rows: vec![document.clone()],
            document,
            body,
            sections: Vec::new(),
            empty: String::new(),
            notes: Vec::new(),
        }
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn add_note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    /// Puts `notes` in front of the ones already there. Masking and the prod
    /// banner are said BEFORE the numbers they qualify, which on a terminal
    /// means "before the rows scroll past".
    pub fn prepend_notes(&mut self, mut notes: Vec<String>) {
        notes.append(&mut self.notes);
        self.notes = notes;
    }

    pub fn section(mut self, title: impl Into<String>, table: Table) -> Self {
        self.sections.push((title.into(), table));
        self
    }
}

/// Writes one answer. Notes and the empty state go to `err`; rows go to `out`.
pub fn emit(
    answer: &Answer,
    mode: Mode,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<()> {
    for note in &answer.notes {
        writeln!(err, "{note}")?;
    }
    match mode {
        Mode::Table => {
            let text = match &answer.body {
                Body::Rows(table) => table.render(),
                Body::Text(text) => format!("{text}\n"),
            };
            if text.trim().is_empty() {
                if !answer.empty.is_empty() {
                    writeln!(err, "{}", answer.empty)?;
                }
            } else {
                write!(out, "{text}")?;
                for (title, table) in &answer.sections {
                    let rendered = table.render();
                    if !rendered.is_empty() {
                        writeln!(out)?;
                        writeln!(out, "{title}")?;
                        write!(out, "{rendered}")?;
                    }
                }
            }
        }
        Mode::Ndjson => {
            for row in &answer.rows {
                writeln!(out, "{}", serde_json::to_string(row).unwrap_or_default())?;
            }
            if answer.rows.is_empty() && !answer.empty.is_empty() {
                writeln!(err, "{}", answer.empty)?;
            }
        }
        Mode::Json => {
            writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&answer.document).unwrap_or_default()
            )?;
        }
    }
    out.flush()
}

// ---------------------------------------------------------------------------
// Cells
// ---------------------------------------------------------------------------

/// The glyph for a value that is not there. **Never the word `null`**
/// (docs/DESIGN.md §5.2) — and never alone where it means something specific:
/// a tombstone reads `∅ tombstone`, because a glyph is not a word.
pub const ABSENT: &str = "∅";

/// A number as a table shows it: exact, and grouped in threes with a space.
/// docs/DESIGN.md §7 rule 5 — prose rounds, tables do not.
pub fn grouped(value: i64) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(digit);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

/// One cell's worth of a payload: no newlines, no control characters, runs of
/// whitespace collapsed to one space, cut at `max` with an ellipsis.
///
/// The collapse is not cosmetic. kavka-core hands back the payload's **display
/// text**, which for JSON is pretty-printed — so a cell that only swapped
/// newlines for spaces would spend most of its width on indentation
/// (`{   "amount": 70.5,   "orderId": 10`) and cut the record off before the
/// field anybody was looking for. NDJSON and `--output json` are unaffected:
/// they carry the parsed value, not this rendering.
pub fn one_line(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max.saturating_add(1)));
    let mut count = 0usize;
    let mut in_space = false;
    for ch in text.chars() {
        let space = ch.is_whitespace() || ch.is_control();
        if space && in_space {
            continue;
        }
        if count == max {
            out.push('…');
            return out;
        }
        out.push(if space { ' ' } else { ch });
        in_space = space;
        count += 1;
    }
    out
}

fn width_of(text: &str) -> usize {
    text.chars().count()
}

fn pad(text: &str, width: usize, align: Align) -> String {
    let spaces = width.saturating_sub(width_of(text));
    match align {
        Align::Left => format!("{text}{}", " ".repeat(spaces)),
        Align::Right => format!("{}{text}", " ".repeat(spaces)),
    }
}

/// Epoch milliseconds as ISO-8601 UTC, to the millisecond.
///
/// Hand-rolled rather than a `chrono` dependency: this is the only date this
/// program formats, and it is Howard Hinnant's civil-from-days, which is exact
/// for every value it is given. The same call kavka-core's metrics module makes
/// about the Prometheus text format.
pub fn iso8601_utc(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rest = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second, milli) = (
        rest / 3_600_000,
        (rest / 60_000) % 60,
        (rest / 1_000) % 60,
        rest % 1_000,
    );
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
}

/// Days since 1970-01-01 → (year, month, day). Hinnant's algorithm, which is
/// correct for the whole proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// One record, in the shape a pipe reads best.
///
/// The same compact shape `kavka-mcp` returns, and for a related reason:
/// kavka-core's `MessageRecord` carries each payload as
/// `{encoding, text, json, raw_len, truncated, schema}`, and for a JSON payload
/// `text` and `json` are the same content twice. That is right for a UI with an
/// inspector and two tabs; here it doubles the size of every line in the pipe.
/// This keeps the value once — as JSON when it decoded to JSON, as text
/// otherwise — and keeps every fact that would otherwise be lost: the encoding,
/// the true byte length, whether the text was cut, the schema, whether a
/// masking rule rewrote it, and the difference between a tombstone and an empty
/// value. `--verbose` returns the full shape for anything this drops.
pub fn record_json(record: &MessageRecord, verbose: bool) -> Value {
    if verbose {
        return serde_json::to_value(record).unwrap_or(Value::Null);
    }
    let mut out = Map::new();
    out.insert("partition".into(), json!(record.partition));
    out.insert("offset".into(), json!(record.offset));
    out.insert("timestamp_ms".into(), json!(record.timestamp_ms));
    out.insert(
        "key".into(),
        record.key.as_ref().map_or(Value::Null, payload_value),
    );
    match &record.value {
        Some(value) => {
            out.insert("value".into(), payload_value(value));
            out.insert(
                "value_encoding".into(),
                serde_json::to_value(value.encoding).unwrap_or(Value::Null),
            );
            out.insert("value_bytes".into(), json!(value.raw_len));
            if value.truncated {
                out.insert("value_truncated".into(), json!(true));
            }
            if let Some(schema) = &value.schema {
                out.insert(
                    "schema".into(),
                    serde_json::to_value(schema).unwrap_or(Value::Null),
                );
            }
        }
        None => {
            // A null value is not an empty one: on a compacted topic it is a
            // deletion, and the difference is the whole reason the field exists.
            out.insert("value".into(), Value::Null);
            out.insert("tombstone".into(), json!(true));
        }
    }
    if !record.headers.is_empty() {
        // An array, not an object: Kafka allows repeated header keys, and a map
        // would silently keep one of them.
        out.insert(
            "headers".into(),
            Value::Array(
                record
                    .headers
                    .iter()
                    .map(|header| {
                        let mut entry = Map::new();
                        entry.insert("key".into(), json!(header.key));
                        // `None` is a genuinely null header value, which Kafka
                        // allows and which is not an empty string.
                        entry.insert("value".into(), json!(header.value));
                        // Carried under the core's own name, and only when
                        // false: a header whose bytes were not text is rendered
                        // as HEX, and a reader that mistakes that for the
                        // header's own text is reading a different value.
                        // Skipped when true, which is nearly every header, so
                        // the common line stays short.
                        if !header.is_text {
                            entry.insert("is_text".into(), json!(false));
                        }
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    if record.masked {
        out.insert("masked".into(), json!(true));
    }
    if let Some(dlq) = &record.dlq {
        out.insert(
            "dlq".into(),
            serde_json::to_value(dlq).unwrap_or(Value::Null),
        );
    }
    Value::Object(out)
}

fn payload_value(payload: &DecodedPayload) -> Value {
    payload
        .json
        .clone()
        .unwrap_or_else(|| Value::String(payload.text.clone()))
}

/// The columns a record table has. The ledger rule's gutter is the offset
/// column: first, right-aligned, space-grouped.
pub fn record_columns() -> Vec<Column> {
    vec![
        Column::right("OFFSET"),
        Column::right("PART"),
        Column::left("TIMESTAMP"),
        Column::payload("KEY", 24),
        Column::payload("VALUE", 68),
    ]
}

/// One row of that table.
pub fn record_row(record: &MessageRecord) -> Vec<String> {
    vec![
        grouped(record.offset),
        record.partition.to_string(),
        record
            .timestamp_ms
            .map_or_else(|| ABSENT.to_string(), iso8601_utc),
        record
            .key
            .as_ref()
            .map_or_else(|| ABSENT.to_string(), |key| key.text.clone()),
        match &record.value {
            // §5.2: a tombstone is `∅ tombstone` — the glyph says absent, the
            // word says which kind of absent.
            None => format!("{ABSENT} tombstone"),
            Some(value) => value.text.clone(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_gets_a_table_and_a_pipe_gets_ndjson() {
        assert_eq!(Mode::detect(None, true), Mode::Table);
        assert_eq!(Mode::detect(None, false), Mode::Ndjson);
        // …and an explicit choice wins over both, in both directions.
        assert_eq!(Mode::detect(Some(Mode::Ndjson), true), Mode::Ndjson);
        assert_eq!(Mode::detect(Some(Mode::Table), false), Mode::Table);
        assert_eq!(Mode::detect(Some(Mode::Json), true), Mode::Json);
    }

    fn answer() -> Answer {
        let mut table = Table::new(vec![Column::left("NAME"), Column::right("PARTITIONS")]);
        table.push(vec!["orders".into(), "6".into()]);
        table.push(vec!["dead-letter".into(), "12".into()]);
        Answer::new(
            json!({ "topics": [{ "name": "orders" }], "truncated": false }),
            vec![
                json!({ "name": "orders", "partitions": 6 }),
                json!({ "name": "dead-letter", "partitions": 12 }),
            ],
            table,
            "This cluster has no topics.",
        )
        .note("2 of 40 topics.")
    }

    fn emitted(mode: Mode) -> (String, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        emit(&answer(), mode, &mut out, &mut err).expect("writing to a Vec");
        (
            String::from_utf8(out).expect("utf-8"),
            String::from_utf8(err).expect("utf-8"),
        )
    }

    #[test]
    fn ndjson_is_one_row_per_line_and_nothing_else() {
        let (out, err) = emitted(Mode::Ndjson);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "{out}");
        for line in lines {
            let row: Value = serde_json::from_str(line).expect("every line is one JSON object");
            assert!(row["name"].is_string(), "{row}");
        }
        // The note went to stderr, so the pipe is clean.
        assert!(out.lines().all(|line| line.starts_with('{')), "{out}");
        assert!(err.contains("2 of 40 topics."), "{err}");
    }

    #[test]
    fn json_is_the_whole_document_including_what_the_rows_do_not_carry() {
        let (out, _) = emitted(Mode::Json);
        let document: Value = serde_json::from_str(&out).expect("one JSON document");
        assert_eq!(document["truncated"], json!(false));
    }

    #[test]
    fn the_table_has_a_head_a_hairline_and_no_borders() {
        let (out, _) = emitted(Mode::Table);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "{out}");
        // Head in micro-caps, then the one hairline, then rows. No corners, no
        // pipes, no zebra — §5.2's table is leading and one rule.
        assert!(lines[0].starts_with("NAME"), "{out}");
        assert!(lines[0].ends_with("PARTITIONS"), "{out}");
        assert!(
            lines[1].chars().all(|c| c == '─' || c == ' '),
            "the second line is the hairline: {out}"
        );
        assert!(!out.contains('|') && !out.contains('+'), "{out}");
        // Identifiers left, quantities right — both halves of §5.2's rule, and
        // the column is as wide as its widest cell.
        assert!(lines[2].starts_with("orders "), "{out}");
        assert!(lines[2].ends_with("  6"), "{out}");
        assert!(lines[3].starts_with("dead-letter "), "{out}");
        assert!(lines[3].ends_with(" 12"), "{out}");
        assert_eq!(
            lines[0].chars().count(),
            lines[3].chars().count(),
            "the head and a full row line up: {out}"
        );
    }

    /// An empty answer is not an error, and it is not an empty table either: a
    /// sentence on stderr, nothing on stdout, in every mode that streams rows.
    #[test]
    fn an_empty_answer_says_so_on_stderr_and_writes_nothing_to_the_pipe() {
        let empty = Answer::new(
            json!({ "topics": [] }),
            Vec::new(),
            Table::new(vec![Column::left("NAME")]),
            "This cluster has no topics. They appear here as soon as something creates one.",
        );
        for mode in [Mode::Table, Mode::Ndjson] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            emit(&empty, mode, &mut out, &mut err).expect("writing");
            assert!(out.is_empty(), "{mode:?} wrote to stdout");
            assert!(
                String::from_utf8_lossy(&err).contains("no topics"),
                "{mode:?} said nothing"
            );
        }
    }

    #[test]
    fn quantities_are_grouped_in_threes() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1 000");
        assert_eq!(grouped(4_218_907), "4 218 907");
        assert_eq!(grouped(-12_345), "-12 345");
    }

    #[test]
    fn a_cell_is_one_line_and_cut_with_an_ellipsis() {
        assert_eq!(one_line("a\nb\tc", 10), "a b c");
        assert_eq!(one_line("abcdef", 3), "abc…");
        assert_eq!(one_line("abc", 3), "abc");
        // A control character never reaches a terminal.
        assert_eq!(one_line("a\u{7}b", 10), "a b");
        // Pretty-printed JSON is what kavka-core hands back, and a cell that
        // spends its width on indentation shows nobody the field they wanted.
        assert_eq!(
            one_line("{\n  \"orderId\": 10,\n  \"status\": \"created\"\n}", 80),
            "{ \"orderId\": 10, \"status\": \"created\" }"
        );
    }

    #[test]
    fn timestamps_are_iso_8601_utc() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(1_000), "1970-01-01T00:00:01.000Z");
        // A leap day, and the millisecond.
        assert_eq!(iso8601_utc(1_709_209_845_123), "2024-02-29T12:30:45.123Z");
        // A year boundary, and one millisecond before it.
        assert_eq!(iso8601_utc(1_735_689_600_000), "2025-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(1_735_689_599_999), "2024-12-31T23:59:59.999Z");
        // Before the epoch, where a naive `/` and `%` produce nonsense.
        assert_eq!(iso8601_utc(-1), "1969-12-31T23:59:59.999Z");
    }

    /// The value column says which kind of absent, in a word. A cell that reads
    /// only `∅` cannot tell a tombstone from a record with no value.
    #[test]
    fn a_tombstone_reads_as_a_word_not_only_a_glyph() {
        let record: MessageRecord = serde_json::from_value(json!({
            "partition": 3,
            "offset": 8_412,
            "timestamp_ms": 0,
            "key": { "encoding": "utf8", "text": "A-102", "json": null, "raw_len": 5, "truncated": false, "schema": null },
            "value": null,
            "headers": [],
        }))
        .expect("a MessageRecord");
        let row = record_row(&record);
        assert_eq!(row[0], "8 412");
        assert_eq!(row[4], "∅ tombstone");
        // …and the JSON says it in a field a script can branch on.
        let json = record_json(&record, false);
        assert_eq!(json["tombstone"], json!(true));
        assert_eq!(json["value"], Value::Null);
    }

    #[test]
    fn a_json_payload_is_carried_once_as_json() {
        let record: MessageRecord = serde_json::from_value(json!({
            "partition": 0,
            "offset": 1,
            "timestamp_ms": 1_700_000_000_000i64,
            "key": null,
            "value": {
                "encoding": "json",
                "text": "{\n  \"orderId\": 7\n}",
                "json": { "orderId": 7 },
                "raw_len": 18,
                "truncated": true,
                "schema": null,
            },
            "headers": [
                { "key": "trace", "value": "abc", "is_text": true },
                { "key": "raw", "value": "00 ff", "is_text": false },
            ],
        }))
        .expect("a MessageRecord");
        let compact = record_json(&record, false);
        assert_eq!(compact["value"]["orderId"], json!(7));
        assert_eq!(compact["value_bytes"], json!(18));
        assert_eq!(compact["value_truncated"], json!(true));
        assert_eq!(compact["key"], Value::Null);
        assert_eq!(compact["headers"][0]["key"], "trace");
        // A text header says nothing extra; a hex-rendered one says so, because
        // a reader that takes "00 ff" for the header's own text is reading a
        // different value.
        assert!(compact["headers"][0]["is_text"].is_null(), "{compact}");
        assert_eq!(compact["headers"][1]["is_text"], json!(false));
        // The pretty text is NOT also in there — that is the halving.
        assert!(!compact.to_string().contains("\\n  "), "{compact}");
        // …and --verbose keeps everything, including the text.
        let full = record_json(&record, true);
        assert!(full["value"]["text"].is_string(), "{full}");
    }
}
