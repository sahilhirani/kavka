//! Plain English -> CEL or SQL, by a **grammar**. Not an LLM.
//!
//! # What this is, plainly
//!
//! This module is a tokenizer and a table of curated patterns. It recognises
//! the sentences in [`NLQ_GRAMMAR`] and nothing else, it gives the same answer
//! for the same input forever, it makes no network call, and there is no model
//! anywhere in it. When it does not understand a phrase it says so — verbatim,
//! in [`Translation::unrecognized`] — rather than guessing.
//!
//! **The road to real AI assistance is the MCP server** (docs/ROADMAP.md Phase
//! 5b), where the user's own Claude or Cursor holds the model and Kavka holds
//! the cluster. Shipping a network model *inside* the app would mean the
//! payloads of a production topic leaving the machine, which is the one promise
//! docs/DESIGN.md §7 makes on the first screen a user ever sees ("Nothing about
//! your clusters leaves this machine"). A grammar is what can be offered
//! honestly in v1; it is small, it is fast, and every sentence it accepts is in
//! a doc the user can read.
//!
//! # It fills the editor. It never runs.
//!
//! The UI puts the result in the CEL or SQL editor with the explanation beside
//! it, and the user presses the button. That is a deliberate design rule and it
//! is what makes an imperfect translation harmless: the worst outcome is a
//! query the user reads and rewrites, never a query that scanned a production
//! topic because a pattern misfired.
//!
//! # Determinism, and the clock
//!
//! [`nl_to_query_at`] is the whole translator and it is pure — same input, same
//! `now_ms`, same output, byte for byte. [`nl_to_query`] is the two-line
//! wrapper that reads the system clock and calls it, which is the same split
//! `crate::protocol::quorum` uses for the same reason: a relative time
//! (`last hour`) has to be anchored to *something*, and a function that reads a
//! clock cannot be tested against a table of expected strings.
//!
//! Relative times are anchored **at translation time** and emitted as absolute
//! millisecond literals in both modes, rather than as `now()` arithmetic. Two
//! reasons: CEL's activation (`crate::search::CelFilter`) has no clock binding
//! at all, so there is no other option there; and a query the user re-runs
//! tomorrow should mean what it meant when they read it, not silently slide.
//! Day boundaries (`today`, `yesterday`) are **UTC** — said in the explanation,
//! every time, because a user in UTC+13 is entitled to know.
//!
//! # Nothing the user types is ever passed through
//!
//! Every value reaches the output through [`cel_string`] or [`sql_string`], and
//! every field name through [`cel_path`] or a regex/LIKE literal builder. A
//! query is assembled from a fixed set of shapes with encoded leaves — so
//! `status is '; DROP TABLE messages; --` produces a SQL string literal
//! containing that text, and `status is value_text.contains("x")` produces a
//! CEL string literal containing that text. There is no path by which the
//! user's characters become operators, and the tests say so.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// The IPC contract. Field names are mirrored by the TypeScript in
// apps/desktop/src, so renaming one is a breaking change on both sides.
// ---------------------------------------------------------------------------

/// Which editor the answer is going into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Cel,
    Sql,
}

impl std::str::FromStr for Mode {
    type Err = crate::Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        match s {
            "cel" => Ok(Self::Cel),
            "sql" => Ok(Self::Sql),
            other => Err(crate::Error::Other(format!(
                "unknown query mode {other:?}; expected \"cel\" or \"sql\""
            ))),
        }
    }
}

/// What the caller knows about the shape of this topic's payloads.
///
/// `json_fields` is a list of field names seen in the records currently on
/// screen — the UI already has them, because it renders them. It does two
/// things and neither is required: it lets `order id` find a field spelled
/// `orderId`, and it is what makes an unknown field name *lower the
/// confidence* rather than pass silently. An empty hint is normal and costs
/// nothing: with no list to check against, a field name is taken at its word.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaHint {
    #[serde(default)]
    pub json_fields: Vec<String>,
}

/// How sure the translator is that it understood the **English**.
///
/// It is not a claim about how precise the resulting query is: a translation
/// can be `high` and still carry a caveat about what the mode can express (SQL
/// has no JSON functions, so a number inside a payload is matched by text —
/// see [`sql_json_number`]). Those caveats live in
/// [`Translation::explanation`], where the user reads them next to the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Low,
}

/// One translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    pub query: String,
    pub confidence: Confidence,
    /// One or more sentences: what the query says, then any caveats. Always
    /// non-empty — a translation with nothing to say about itself is a
    /// translation the user cannot check.
    pub explanation: String,
    /// The phrases this grammar did not recognise, **verbatim** and in the
    /// order they appeared. Rendering these is not optional: a translator that
    /// silently drops half a sentence produces a query that looks like an
    /// answer to a question nobody asked.
    pub unrecognized: Vec<String>,
}

/// Every sentence this translator accepts, in one string the UI can render
/// beside the input.
///
/// **This doc is checked against the implementation**, not written from
/// memory: `every_documented_pattern_translates_in_both_modes` walks a table of
/// examples and a tripwire asserts each of them appears here, so the grammar
/// cannot grow a pattern the doc does not mention or promise one that does not
/// work.
pub const NLQ_GRAMMAR: &str = r#"Plain English -> query. A grammar, not an AI.

Kavka recognises the sentences below and nothing else — every accepted phrasing
is on this page. Anything not on it is listed back to you untouched rather than
guessed at. The result fills the editor; it is never run for you.

FIELDS IN THE PAYLOAD
  status is failed
  status equals failed
  status = failed
  status is not failed
  status isn't failed
  status != failed
  orderId over 100
  orderId above 100
  orderId greater than 100
  orderId more than 100
  orderId > 100
  orderId at least 100
  orderId under 100
  orderId below 100
  orderId less than 100
  orderId at most 100
  orderId between 100 and 200
  note contains timeout
  note includes timeout
  email starts with sales
  email begins with sales
  email ends with acme.com
  couponCode exists
  couponCode is present
  couponCode is set
  couponCode is missing
  couponCode is null
  couponCode is empty
  couponCode is absent

  A field is found however you spell it: `order id`, `order_id` and `orderId`
  all reach the same field. `>=` and `<=` are read as "at least" and "at most".

THE RECORD ITSELF
  key is A-102
  key starts with order-
  key contains 42
  no key
  keyless
  has a key
  partition 3
  partition is 3
  on partition 3
  partitions 1, 2 and 3
  offset over 5000
  offset is 8412
  offset between 100 and 200
  header trace-id is abc123
  header trace-id exists
  tombstones

ANYWHERE IN THE BODY
  contains timeout
  mentions timeout
  body contains timeout

TIME  (anchored when you translate; days are UTC)
  last hour
  past 2 days
  in the last 15 minutes
  since yesterday
  today
  yesterday

  Any of second, minute, hour, day and week, singular or plural, with or
  without a count.

JOINING THEM
  status is failed and orderId over 100
  status is failed or status is cancelled
  not tombstones
  without a key
  except partition 0

  A comma reads as `and`. Mixing `and` with `or` in one line is ambiguous, so
  Kavka parenthesises what it chose and lowers its confidence.

QUOTES
  Wrap a value in "..." or '...' to keep it exactly as typed, including the
  words `and`, `or` and `contains`."#;

/// See [`NLQ_GRAMMAR`]. A function so the UI has something to call over IPC
/// without the constant becoming part of the IPC contract's shape.
pub fn nlq_grammar() -> &'static str {
    NLQ_GRAMMAR
}

// ---------------------------------------------------------------------------
// The entry points.
// ---------------------------------------------------------------------------

/// Translates one plain-English line, anchoring relative times to the system
/// clock.
///
/// The clock is the only impure thing in this module and it is confined to this
/// function; [`nl_to_query_at`] is the translator.
pub fn nl_to_query(input: &str, mode: Mode, hint: &SchemaHint) -> Translation {
    nl_to_query_at(input, mode, hint, now_ms())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(0))
}

/// The translator: pure, total, and the thing every test drives.
///
/// `now_ms` anchors `last hour` and friends. Nothing else in here reads the
/// world.
pub fn nl_to_query_at(input: &str, mode: Mode, hint: &SchemaHint, now_ms: i64) -> Translation {
    let tokens = tokenize(input);
    let raw_clauses = split_clauses(&tokens);

    let mut groups: Vec<Vec<Clause>> = Vec::new();
    let mut unrecognized: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut guessed_fields: Vec<String> = Vec::new();
    let mut saw_and = false;
    let mut saw_or = false;

    for raw in &raw_clauses {
        match raw.join {
            Some(Join::And) => saw_and = true,
            Some(Join::Or) => saw_or = true,
            None => {}
        }
        let parsed = parse_clause(&raw.tokens, hint, now_ms);
        let Some(parsed) = parsed else {
            let text = input[raw.start..raw.end].trim();
            if !text.is_empty() {
                unrecognized.push(text.to_string());
            }
            continue;
        };
        for note in parsed.notes {
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        for field in parsed.guessed {
            if !guessed_fields.contains(&field) {
                guessed_fields.push(field);
            }
        }
        // An OR starts a new group; everything else joins the group being
        // built. Groups are OR-ed, members are AND-ed, which is the precedence
        // every language in this app already uses.
        if matches!(raw.join, Some(Join::Or)) || groups.is_empty() {
            groups.push(parsed.clauses);
        } else {
            groups
                .last_mut()
                .expect("groups is non-empty in this branch")
                .extend(parsed.clauses);
        }
    }
    groups.retain(|group| !group.is_empty());

    let mixed = saw_and && saw_or;
    if mixed {
        notes.push(
            "This mixes `and` with `or`; Kavka read it as `and` binding tighter, and \
             parenthesised the query so you can see the grouping it chose."
                .to_string(),
        );
    }
    for field in &guessed_fields {
        notes.push(format!(
            "{field} is not one of the fields Kavka has seen in this topic, so it was assumed to \
             be a field inside the payload."
        ));
    }
    if !unrecognized.is_empty() {
        notes.push(format!(
            "Kavka did not recognise {} — {} left out of the query entirely.",
            quote_list(&unrecognized),
            if unrecognized.len() == 1 {
                "it is"
            } else {
                "they are"
            }
        ));
    }

    let query = emit(&groups, mode);
    let explanation = explain(&groups, &notes, mode);
    let confidence =
        if groups.is_empty() || mixed || !unrecognized.is_empty() || !guessed_fields.is_empty() {
            Confidence::Low
        } else {
            Confidence::High
        };

    Translation {
        query,
        confidence,
        explanation,
        unrecognized,
    }
}

// ---------------------------------------------------------------------------
// Tokens.
// ---------------------------------------------------------------------------

/// One word, operator glyph, or quoted run, with its place in the original
/// input.
///
/// `raw` is what the user typed — the value literals are built from it, so
/// `status is Failed` produces `"Failed"` and not `"failed"`. `lower` is what
/// the matchers compare against.
#[derive(Debug, Clone)]
struct Token {
    lower: String,
    raw: String,
    start: usize,
    end: usize,
    /// Came out of `"..."` or `'...'`, so it is a value and never a keyword.
    quoted: bool,
}

impl Token {
    fn is(&self, word: &str) -> bool {
        !self.quoted && self.lower == word
    }

    fn is_any(&self, words: &[&str]) -> bool {
        words.iter().any(|word| self.is(word))
    }

    fn as_i64(&self) -> Option<i64> {
        self.lower.replace(['_', ','], "").parse().ok()
    }
}

/// Splits input into words, quoted runs and comparison glyphs.
///
/// Quoted runs are one token and are never keywords, which is the escape hatch
/// the grammar doc promises: `note contains "and then it failed"` keeps the
/// `and`. An unterminated quote runs to the end of the input rather than
/// failing — a half-typed sentence should translate as far as it can.
fn tokenize(input: &str) -> Vec<Token> {
    let bytes: Vec<char> = input.chars().collect();
    // char index -> byte offset, so spans point into the original string.
    let offsets: Vec<usize> = input
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(input.len()))
        .collect();

    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // An apostrophe only opens a quoted run at the START of a token, so
        // `isn't` stays one word. Without that rule the operator table's
        // contractions are unreachable — the tokenizer eats them and the
        // clause reads as prose.
        if c == '"' || (c == '\'' && (i == 0 || bytes[i - 1].is_whitespace())) {
            let start = i;
            i += 1;
            let text_start = i;
            while i < bytes.len() && bytes[i] != c {
                i += 1;
            }
            let text: String = bytes[text_start..i].iter().collect();
            if i < bytes.len() {
                i += 1; // the closing quote
            }
            tokens.push(Token {
                lower: text.to_lowercase(),
                raw: text,
                start: offsets[start],
                end: offsets[i],
                quoted: true,
            });
            continue;
        }
        // Comparison glyphs, longest first.
        let two: String = bytes[i..bytes.len().min(i + 2)].iter().collect();
        if matches!(two.as_str(), ">=" | "<=" | "!=" | "<>" | "==") {
            tokens.push(glyph(&two, offsets[i], offsets[i + 2]));
            i += 2;
            continue;
        }
        if matches!(c, '>' | '<' | '=' | ',') {
            tokens.push(glyph(&c.to_string(), offsets[i], offsets[i + 1]));
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_whitespace()
            && !matches!(bytes[i], '>' | '<' | '=' | ',' | '"')
        {
            i += 1;
        }
        let raw: String = bytes[start..i].iter().collect();
        tokens.push(Token {
            lower: raw.to_lowercase(),
            raw,
            start: offsets[start],
            end: offsets[i],
            quoted: false,
        });
    }
    tokens
}

fn glyph(text: &str, start: usize, end: usize) -> Token {
    Token {
        lower: text.to_string(),
        raw: text.to_string(),
        start,
        end,
        quoted: false,
    }
}

// ---------------------------------------------------------------------------
// Clause splitting.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Join {
    And,
    Or,
}

#[derive(Debug)]
struct RawClause {
    tokens: Vec<Token>,
    /// How this clause attaches to the one before it; `None` for the first.
    join: Option<Join>,
    start: usize,
    end: usize,
}

/// Splits a token stream on `and` / `or` / `,`, with the two exceptions that
/// make the grammar's own patterns survive splitting:
///
/// 1. **`between 100 and 200`** — the `and` belongs to the `between`, so a
///    clause holding an unsatisfied `between` swallows it.
/// 2. **`partitions 1, 2 and 3`** — a list head (`partition`, `offset`) makes
///    every following separator a list separator rather than a conjunction.
///
/// Both are decided by looking at the clause being built, never at the clause
/// ahead, so the rule is a state machine rather than a lookahead heuristic.
fn split_clauses(tokens: &[Token]) -> Vec<RawClause> {
    let mut clauses: Vec<RawClause> = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    let mut join: Option<Join> = None;
    let mut start = tokens.first().map_or(0, |t| t.start);

    for token in tokens {
        let separator = if token.is("or") {
            Some(Join::Or)
        } else if token.is("and") || token.is(",") {
            Some(Join::And)
        } else {
            None
        };
        let Some(kind) = separator else {
            current.push(token.clone());
            continue;
        };
        // Exception 1: a `between` still waiting for its upper bound.
        let pending_between = kind == Join::And
            && current.iter().any(|t| t.is("between"))
            && current
                .iter()
                .skip_while(|t| !t.is("between"))
                .skip(1)
                .filter(|t| t.as_i64().is_some())
                .count()
                == 1;
        // Exception 2: a list of numbers under a list head.
        let list_continuation = kind == Join::And
            && current
                .first()
                .is_some_and(|head| head.is_any(&["partition", "partitions", "offset", "offsets"]))
            && current.last().is_some_and(|last| last.as_i64().is_some());
        if pending_between || list_continuation {
            current.push(token.clone());
            continue;
        }
        if !current.is_empty() {
            let end = current.last().expect("non-empty").end;
            clauses.push(RawClause {
                tokens: std::mem::take(&mut current),
                join,
                start,
                end,
            });
        }
        join = Some(kind);
        start = token.end;
    }
    if !current.is_empty() {
        let end = current.last().expect("non-empty").end;
        clauses.push(RawClause {
            tokens: current,
            join,
            start,
            end,
        });
    }
    clauses
}

// ---------------------------------------------------------------------------
// The parsed form: one shape per thing a record has.
// ---------------------------------------------------------------------------

/// What a clause is about.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// A field inside the decoded value, as a dotted path.
    Json(String),
    Key,
    /// The whole body, as the text the message table is showing.
    ValueText,
    Partition,
    Offset,
    Timestamp,
    Header(String),
    /// A record with no value at all.
    Tombstone,
}

/// A number as the user wrote it. Integers stay integers so `partition = 3`
/// never emits `3.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn parse(text: &str) -> Option<Self> {
        let cleaned = text.replace(['_', ','], "");
        if let Ok(i) = cleaned.parse::<i64>() {
            return Some(Self::Int(i));
        }
        cleaned
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(Self::Float)
    }
}

impl std::fmt::Display for Num {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(x) => write!(f, "{x}"),
        }
    }
}

/// A comparison value.
#[derive(Debug, Clone, PartialEq)]
enum Literal {
    Text(String),
    Number(Num),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
}

impl Cmp {
    fn cel(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
        }
    }

    fn english(self) -> &'static str {
        match self {
            Self::Gt => "over",
            Self::Ge => "at least",
            Self::Lt => "under",
            Self::Le => "at most",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Predicate {
    Equals(Literal),
    NotEquals(Literal),
    Compare(Cmp, Num),
    OneOf(Vec<Num>),
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Exists,
    IsNull,
}

/// One recognised clause, with the sentence it will contribute to the
/// explanation. The description is built where the clause is recognised — the
/// time matcher knows it said "the last hour", and no generic renderer of
/// `timestamp_ms >= 1754179200000` ever could.
#[derive(Debug, Clone, PartialEq)]
struct Clause {
    target: Target,
    predicate: Predicate,
    negated: bool,
    description: String,
}

/// What one clause of input produced.
struct Parsed {
    clauses: Vec<Clause>,
    notes: Vec<String>,
    /// Field names taken on trust because the schema hint did not list them.
    guessed: Vec<String>,
}

impl Parsed {
    fn one(clause: Clause) -> Self {
        Self {
            clauses: vec![clause],
            notes: Vec::new(),
            guessed: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Matching one clause.
// ---------------------------------------------------------------------------

/// One clause matcher: tokens in, a parsed clause or "not mine" out. They are
/// tried in the order [`parse_clause`] lists them, and the first one that
/// answers wins — so the order is the grammar's precedence, and it is written
/// down in exactly one place.
type Matcher = fn(&[Token], &SchemaHint, i64) -> Option<Parsed>;

const MS_PER_SECOND: i64 = 1_000;
const MS_PER_MINUTE: i64 = 60 * MS_PER_SECOND;
const MS_PER_HOUR: i64 = 60 * MS_PER_MINUTE;
const MS_PER_DAY: i64 = 24 * MS_PER_HOUR;

/// The UTC-day note, said once per translation that needs it.
const UTC_NOTE: &str = "Day boundaries are UTC.";

fn parse_clause(tokens: &[Token], hint: &SchemaHint, now_ms: i64) -> Option<Parsed> {
    if tokens.is_empty() {
        return None;
    }
    // "no key" and "no value" are their own patterns; a leading `no` elsewhere
    // is a negation. Order matters here and only here.
    let (negated, rest) = if tokens[0].is_any(&["not", "without", "except", "excluding"])
        || (tokens[0].is("no") && !tokens.get(1).is_some_and(|t| t.is_any(&["key", "value"])))
    {
        (true, &tokens[1..])
    } else {
        (false, tokens)
    };
    if rest.is_empty() {
        return None;
    }
    // Articles and the one preposition the grammar's own examples use are
    // noise once the negation is off: `without a key`, `on partition 3`.
    let mut rest = rest;
    while rest.len() > 1 && rest[0].is_any(&["a", "an", "the", "on"]) {
        rest = &rest[1..];
    }
    if rest.is_empty() {
        return None;
    }

    const MATCHERS: [Matcher; 8] = [
        match_time,
        match_tombstone,
        match_partition,
        match_offset,
        match_key,
        match_header,
        match_body,
        match_field,
    ];
    let mut parsed = MATCHERS
        .iter()
        .find_map(|matcher| matcher(rest, hint, now_ms))?;
    if negated {
        for clause in &mut parsed.clauses {
            clause.negated = !clause.negated;
            clause.description = format!("NOT ({})", clause.description);
        }
    }
    Some(parsed)
}

/// `last hour`, `past 2 days`, `in the last 15 minutes`, `since yesterday`,
/// `today`, `yesterday`.
fn match_time(tokens: &[Token], _hint: &SchemaHint, now_ms: i64) -> Option<Parsed> {
    let mut i = 0;
    while i < tokens.len() && tokens[i].is_any(&["in", "the", "over", "from", "within", "during"]) {
        i += 1;
    }
    let rest = &tokens[i..];
    let first = rest.first()?;

    if first.is_any(&["today", "yesterday"]) || (first.is("since") && rest.len() == 2) {
        let (word, since) = if first.is("since") {
            (&rest[1], true)
        } else {
            (first, false)
        };
        let today = now_ms.div_euclid(MS_PER_DAY) * MS_PER_DAY;
        let (from, until, english) = if word.is("today") {
            (today, None, "today")
        } else if word.is("yesterday") {
            let start = today - MS_PER_DAY;
            (start, if since { None } else { Some(today) }, "yesterday")
        } else {
            return None;
        };
        let phrase = if since {
            format!("since the start of {english} (UTC)")
        } else {
            format!("{english} (UTC)")
        };
        let mut clauses = vec![Clause {
            target: Target::Timestamp,
            predicate: Predicate::Compare(Cmp::Ge, Num::Int(from)),
            negated: false,
            description: format!("the record was written {phrase}"),
        }];
        if let Some(until) = until {
            clauses.push(Clause {
                target: Target::Timestamp,
                predicate: Predicate::Compare(Cmp::Lt, Num::Int(until)),
                negated: false,
                description: "it was written before today started".to_string(),
            });
        }
        return Some(Parsed {
            clauses,
            notes: vec![UTC_NOTE.to_string(), anchor_note(now_ms)],
            guessed: Vec::new(),
        });
    }

    if !first.is_any(&["last", "past", "previous"]) {
        return None;
    }
    let after = &rest[1..];
    let (count, unit_at) = match after.first().and_then(Token::as_i64) {
        Some(n) if n >= 0 => (n, 1),
        _ => (1, 0),
    };
    let unit = after.get(unit_at)?;
    let (per, singular) = unit_ms(&unit.lower)?;
    // `last 15 minutes ago` and other trailing noise is not this grammar's.
    if after.len() > unit_at + 1 {
        return None;
    }
    let span = count.saturating_mul(per);
    let english = if count == 1 {
        format!("the last {singular}")
    } else {
        format!("the last {count} {singular}s")
    };
    Some(Parsed {
        clauses: vec![Clause {
            target: Target::Timestamp,
            predicate: Predicate::Compare(Cmp::Ge, Num::Int(now_ms - span)),
            negated: false,
            description: format!("the record was written in {english}"),
        }],
        notes: vec![anchor_note(now_ms)],
        guessed: Vec::new(),
    })
}

fn anchor_note(now_ms: i64) -> String {
    format!(
        "The time is anchored to the moment you translated this ({now_ms} ms since the epoch), \
         so it does not move when you re-run the query."
    )
}

fn unit_ms(word: &str) -> Option<(i64, &'static str)> {
    Some(match word {
        "second" | "seconds" | "sec" | "secs" => (MS_PER_SECOND, "second"),
        "minute" | "minutes" | "min" | "mins" => (MS_PER_MINUTE, "minute"),
        "hour" | "hours" | "hr" | "hrs" => (MS_PER_HOUR, "hour"),
        "day" | "days" => (MS_PER_DAY, "day"),
        "week" | "weeks" => (7 * MS_PER_DAY, "week"),
        _ => return None,
    })
}

fn match_tombstone(tokens: &[Token], _hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    let words: Vec<&str> = tokens.iter().map(|t| t.lower.as_str()).collect();
    let is_tombstone = matches!(
        words.as_slice(),
        ["tombstone"] | ["tombstones"] | ["is", "a", "tombstone"] | ["are", "tombstones"]
    );
    if !is_tombstone || tokens.iter().any(|t| t.quoted) {
        return None;
    }
    Some(Parsed::one(Clause {
        target: Target::Tombstone,
        predicate: Predicate::IsNull,
        negated: false,
        description: "the record is a tombstone (it has no value at all)".to_string(),
    }))
}

fn match_partition(tokens: &[Token], _hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    if !tokens[0].is_any(&["partition", "partitions"]) {
        return None;
    }
    // The separators the splitter deliberately left in a list clause.
    let items: Vec<&Token> = strip_filler(&tokens[1..])
        .iter()
        .filter(|t| !t.is(",") && !t.is("and"))
        .collect();
    let numbers: Vec<Num> = items.iter().filter_map(|t| Num::parse(&t.lower)).collect();
    // Every remaining token has to be a number, or this is a sentence about
    // partitions that only the operator table can read.
    if numbers.is_empty() || numbers.len() != items.len() {
        return comparison_clause(Target::Partition, "the partition", &tokens[1..]);
    }
    let description = if numbers.len() == 1 {
        format!("the partition is {}", numbers[0])
    } else {
        format!(
            "the partition is one of {}",
            numbers
                .iter()
                .map(Num::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Some(Parsed::one(Clause {
        target: Target::Partition,
        predicate: if numbers.len() == 1 {
            Predicate::Equals(Literal::Number(numbers[0]))
        } else {
            Predicate::OneOf(numbers)
        },
        negated: false,
        description,
    }))
}

fn match_offset(tokens: &[Token], _hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    if !tokens[0].is_any(&["offset", "offsets"]) {
        return None;
    }
    comparison_clause(Target::Offset, "the offset", &tokens[1..])
}

fn match_key(tokens: &[Token], _hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    let words: Vec<&str> = tokens
        .iter()
        .map(|t| if t.quoted { "\u{0}" } else { t.lower.as_str() })
        .collect();
    let presence = match words.as_slice() {
        // Bare `key` is "the record has one" — the shape `without a key`
        // negates into `no key`.
        ["key"] | ["has", "key"] | ["with", "key"] | ["has", "a", "key"] | ["with", "a", "key"] => {
            Some(true)
        }
        ["keyless"] | ["no", "key"] => Some(false),
        _ => None,
    };
    if let Some(has_key) = presence {
        return Some(Parsed::one(Clause {
            target: Target::Key,
            predicate: Predicate::IsNull,
            negated: has_key,
            description: if has_key {
                "the record has a key".to_string()
            } else {
                "the record has no key".to_string()
            },
        }));
    }
    if !tokens[0].is("key") {
        return None;
    }
    comparison_clause(Target::Key, "the key", &tokens[1..])
}

fn match_header(tokens: &[Token], _hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    if !tokens[0].is_any(&["header", "headers"]) {
        return None;
    }
    let name = tokens.get(1)?;
    if OPERATOR_WORDS.contains(&name.lower.as_str()) {
        return None;
    }
    comparison_clause(
        Target::Header(name.raw.clone()),
        &format!("the header {}", name.raw),
        &tokens[2..],
    )
}

/// `contains timeout`, `mentions timeout`, `body contains timeout`.
///
/// A body clause that turns out to be about *presence* is about the value as a
/// whole, and the value as a whole is a tombstone question — `value_text` is
/// bound to `""` for a tombstone and is never null, so asking it would be
/// asking the wrong binding (see `crate::search::CelFilter`).
fn match_body(tokens: &[Token], _hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    let named = tokens[0].is_any(&["body", "payload", "message", "value", "text"]);
    let rest = if named { &tokens[1..] } else { tokens };
    let first = rest.first()?;
    if first.is_any(&["mentions", "mentioning", "matching"]) {
        let value = literal_text(&rest[1..])?;
        return Some(Parsed::one(Clause {
            target: Target::ValueText,
            predicate: Predicate::Contains(value.clone()),
            negated: false,
            description: format!("the body contains {value:?}"),
        }));
    }
    if !named && !first.is("contains") {
        return None;
    }
    let mut parsed = comparison_clause(Target::ValueText, "the body", rest)?;
    for clause in &mut parsed.clauses {
        if matches!(clause.predicate, Predicate::IsNull | Predicate::Exists) {
            clause.target = Target::Tombstone;
        }
    }
    Some(parsed)
}

/// The general case: `<field> <operator> <value>`, where the field is a path
/// into the decoded payload.
fn match_field(tokens: &[Token], hint: &SchemaHint, _now: i64) -> Option<Parsed> {
    // The field name is everything before the operator, so `order id is 7`
    // finds `orderId` when the hint knows it.
    let operator_at = tokens
        .iter()
        .position(|t| !t.quoted && OPERATOR_WORDS.contains(&t.lower.as_str()))?;
    if operator_at == 0 {
        return None;
    }
    let words: Vec<&str> = tokens[..operator_at]
        .iter()
        .map(|t| t.raw.as_str())
        .collect();
    let typed = words.join(" ");
    let (field, guessed) = match resolve_field(&words, hint) {
        Some(known) => (known, Vec::new()),
        None => {
            if words.len() > 1 {
                // Several words and no field by that name: this is prose, not
                // a field, and guessing `order id` as a JSON key would be a
                // query that silently matches nothing.
                return None;
            }
            (typed.clone(), vec![typed])
        }
    };
    let description = format!("the payload field {field}");
    let mut parsed = comparison_clause(Target::Json(field), &description, &tokens[operator_at..])?;
    parsed.guessed = guessed;
    Some(parsed)
}

/// Every word that can start an operator phrase. Used to find where a field
/// name ends, and to stop a header name eating its own operator.
const OPERATOR_WORDS: &[&str] = &[
    "is",
    "isn't",
    "isnt",
    "are",
    "aren't",
    "arent",
    "was",
    "=",
    "==",
    "!=",
    "<>",
    "equals",
    "equal",
    "over",
    "above",
    ">",
    ">=",
    "under",
    "below",
    "<",
    "<=",
    "at",
    "greater",
    "more",
    "less",
    "fewer",
    "between",
    "contains",
    "containing",
    "includes",
    "including",
    "starts",
    "starting",
    "begins",
    "beginning",
    "ends",
    "ending",
    "exists",
    "missing",
    "absent",
    "present",
    "set",
    "in",
    "has",
    "no",
];

/// Reads `<operator> <value>` and builds the clause. `description` is the
/// English for the *target*; this appends the English for the comparison.
fn comparison_clause(target: Target, subject: &str, tokens: &[Token]) -> Option<Parsed> {
    let (op, rest) = read_operator(tokens)?;
    let numeric_target = matches!(
        target,
        Target::Partition | Target::Offset | Target::Timestamp
    );

    let clause = |predicate: Predicate, tail: String| Clause {
        target: target.clone(),
        predicate,
        negated: false,
        description: format!("{subject} {tail}"),
    };

    match op {
        Operator::Exists => {
            return Some(Parsed::one(clause(
                Predicate::Exists,
                "is present".to_string(),
            )))
        }
        Operator::IsNull => {
            return Some(Parsed::one(clause(
                Predicate::IsNull,
                "is missing".to_string(),
            )))
        }
        Operator::Between => {
            let numbers: Vec<Num> = rest.iter().filter_map(|t| Num::parse(&t.lower)).collect();
            if numbers.len() != 2 {
                return None;
            }
            return Some(Parsed {
                clauses: vec![
                    clause(
                        Predicate::Compare(Cmp::Ge, numbers[0]),
                        format!("is at least {}", numbers[0]),
                    ),
                    clause(
                        Predicate::Compare(Cmp::Le, numbers[1]),
                        format!("is at most {}", numbers[1]),
                    ),
                ],
                notes: Vec::new(),
                guessed: Vec::new(),
            });
        }
        _ => {}
    }

    let text = literal_text(rest)?;
    let quoted = rest.first().is_some_and(|t| t.quoted);
    match op {
        Operator::Eq | Operator::Ne => {
            // `is null` reached as a value rather than as an operator word.
            if !quoted && matches!(text.to_lowercase().as_str(), "null" | "nothing" | "empty") {
                let (predicate, tail) = if op == Operator::Eq {
                    (Predicate::IsNull, "is missing")
                } else {
                    (Predicate::Exists, "is present")
                };
                return Some(Parsed::one(clause(predicate, tail.to_string())));
            }
            let literal = literal_of(&text, quoted, numeric_target);
            let tail = match op {
                Operator::Eq => format!("is {}", english_literal(&literal)),
                _ => format!("is not {}", english_literal(&literal)),
            };
            let predicate = if op == Operator::Eq {
                Predicate::Equals(literal)
            } else {
                Predicate::NotEquals(literal)
            };
            Some(Parsed::one(clause(predicate, tail)))
        }
        Operator::Cmp(cmp) => {
            let number = Num::parse(&text)?;
            Some(Parsed::one(clause(
                Predicate::Compare(cmp, number),
                format!("is {} {number}", cmp.english()),
            )))
        }
        Operator::OneOf => {
            let numbers: Vec<Num> = rest.iter().filter_map(|t| Num::parse(&t.lower)).collect();
            if numbers.is_empty() || !numeric_target {
                return None;
            }
            Some(Parsed::one(clause(
                Predicate::OneOf(numbers.clone()),
                format!(
                    "is one of {}",
                    numbers
                        .iter()
                        .map(Num::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )))
        }
        Operator::Contains => Some(Parsed::one(clause(
            Predicate::Contains(text.clone()),
            format!("contains {text:?}"),
        ))),
        Operator::StartsWith => Some(Parsed::one(clause(
            Predicate::StartsWith(text.clone()),
            format!("starts with {text:?}"),
        ))),
        Operator::EndsWith => Some(Parsed::one(clause(
            Predicate::EndsWith(text.clone()),
            format!("ends with {text:?}"),
        ))),
        Operator::Exists | Operator::IsNull | Operator::Between => unreachable!("handled above"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Eq,
    Ne,
    Cmp(Cmp),
    Between,
    OneOf,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
    IsNull,
}

/// The operator table, longest phrase first so `is not` beats `is` and
/// `is missing` beats both.
const OPERATORS: &[(&[&str], Operator)] = &[
    (
        &["is", "greater", "than", "or", "equal", "to"],
        Operator::Cmp(Cmp::Ge),
    ),
    (
        &["is", "less", "than", "or", "equal", "to"],
        Operator::Cmp(Cmp::Le),
    ),
    (&["is", "not", "equal", "to"], Operator::Ne),
    (&["is", "no", "less", "than"], Operator::Cmp(Cmp::Ge)),
    (&["is", "no", "more", "than"], Operator::Cmp(Cmp::Le)),
    (&["is", "greater", "than"], Operator::Cmp(Cmp::Gt)),
    (&["is", "less", "than"], Operator::Cmp(Cmp::Lt)),
    (&["is", "more", "than"], Operator::Cmp(Cmp::Gt)),
    (&["is", "fewer", "than"], Operator::Cmp(Cmp::Lt)),
    (&["is", "at", "least"], Operator::Cmp(Cmp::Ge)),
    (&["is", "at", "most"], Operator::Cmp(Cmp::Le)),
    (&["is", "not", "present"], Operator::IsNull),
    (&["is", "not", "set"], Operator::IsNull),
    (&["is", "missing"], Operator::IsNull),
    (&["is", "absent"], Operator::IsNull),
    (&["is", "empty"], Operator::IsNull),
    (&["is", "null"], Operator::IsNull),
    (&["is", "present"], Operator::Exists),
    (&["is", "set"], Operator::Exists),
    (&["is", "one", "of"], Operator::OneOf),
    (&["is", "in"], Operator::OneOf),
    (&["is", "not"], Operator::Ne),
    (&["are", "not"], Operator::Ne),
    (&["was", "not"], Operator::Ne),
    (&["equal", "to"], Operator::Eq),
    (&["no", "less", "than"], Operator::Cmp(Cmp::Ge)),
    (&["no", "more", "than"], Operator::Cmp(Cmp::Le)),
    (&["greater", "than"], Operator::Cmp(Cmp::Gt)),
    (&["more", "than"], Operator::Cmp(Cmp::Gt)),
    (&["less", "than"], Operator::Cmp(Cmp::Lt)),
    (&["fewer", "than"], Operator::Cmp(Cmp::Lt)),
    (&["at", "least"], Operator::Cmp(Cmp::Ge)),
    (&["at", "most"], Operator::Cmp(Cmp::Le)),
    (&["starts", "with"], Operator::StartsWith),
    (&["starting", "with"], Operator::StartsWith),
    (&["begins", "with"], Operator::StartsWith),
    (&["beginning", "with"], Operator::StartsWith),
    (&["ends", "with"], Operator::EndsWith),
    (&["ending", "with"], Operator::EndsWith),
    (&["one", "of"], Operator::OneOf),
    (&["isn't"], Operator::Ne),
    (&["isnt"], Operator::Ne),
    (&["aren't"], Operator::Ne),
    (&["arent"], Operator::Ne),
    (&["!="], Operator::Ne),
    (&["<>"], Operator::Ne),
    (&[">="], Operator::Cmp(Cmp::Ge)),
    (&["<="], Operator::Cmp(Cmp::Le)),
    (&["=="], Operator::Eq),
    (&["="], Operator::Eq),
    (&[">"], Operator::Cmp(Cmp::Gt)),
    (&["<"], Operator::Cmp(Cmp::Lt)),
    (&["equals"], Operator::Eq),
    (&["is"], Operator::Eq),
    (&["are"], Operator::Eq),
    (&["was"], Operator::Eq),
    (&["over"], Operator::Cmp(Cmp::Gt)),
    (&["above"], Operator::Cmp(Cmp::Gt)),
    (&["under"], Operator::Cmp(Cmp::Lt)),
    (&["below"], Operator::Cmp(Cmp::Lt)),
    (&["between"], Operator::Between),
    (&["contains"], Operator::Contains),
    (&["containing"], Operator::Contains),
    (&["includes"], Operator::Contains),
    (&["including"], Operator::Contains),
    (&["has"], Operator::Contains),
    (&["in"], Operator::OneOf),
    (&["exists"], Operator::Exists),
    (&["missing"], Operator::IsNull),
    (&["absent"], Operator::IsNull),
];

/// The longest operator phrase at the head of `tokens`, and what follows it.
fn read_operator(tokens: &[Token]) -> Option<(Operator, &[Token])> {
    OPERATORS
        .iter()
        .find(|(phrase, _)| {
            phrase.len() <= tokens.len()
                && phrase
                    .iter()
                    .zip(tokens)
                    .all(|(word, token)| token.is(word))
        })
        .map(|(phrase, op)| (*op, &tokens[phrase.len()..]))
}

/// The value, as the user typed it: original case, original spacing between
/// words, quotes stripped. `None` when there is nothing left.
fn literal_text(tokens: &[Token]) -> Option<String> {
    let tokens = strip_filler(tokens);
    let first = tokens.first()?;
    if tokens.len() == 1 || first.quoted {
        return Some(first.raw.clone());
    }
    Some(
        tokens
            .iter()
            .map(|t| t.raw.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Drops the words that carry no meaning in a value position.
fn strip_filler(tokens: &[Token]) -> &[Token] {
    let mut start = 0;
    while start < tokens.len()
        && tokens[start].is_any(&["a", "an", "the", "to", "than", "of", "value", "and", ","])
    {
        start += 1;
    }
    &tokens[start..]
}

fn literal_of(text: &str, quoted: bool, numeric_target: bool) -> Literal {
    if quoted {
        return Literal::Text(text.to_string());
    }
    if let Some(number) = Num::parse(text) {
        return Literal::Number(number);
    }
    if !numeric_target {
        match text.to_lowercase().as_str() {
            "true" | "yes" => return Literal::Bool(true),
            "false" | "no" => return Literal::Bool(false),
            _ => {}
        }
    }
    Literal::Text(text.to_string())
}

fn english_literal(literal: &Literal) -> String {
    match literal {
        Literal::Text(text) => format!("{text:?}"),
        Literal::Number(n) => n.to_string(),
        Literal::Bool(b) => b.to_string(),
    }
}

/// Finds the field the user meant. Exact first, then a normalised comparison so
/// `order id`, `order_id` and `orderId` all find `orderId`.
fn resolve_field(words: &[&str], hint: &SchemaHint) -> Option<String> {
    let typed = words.join(" ");
    if let Some(exact) = hint.json_fields.iter().find(|known| **known == typed) {
        return Some(exact.clone());
    }
    let wanted = normalize_field(&typed);
    if let Some(close) = hint
        .json_fields
        .iter()
        .find(|known| normalize_field(known) == wanted)
    {
        return Some(close.clone());
    }
    // No hint to check against, or nothing matched: a single word is taken at
    // its word (the caller records the guess), several words are not.
    if hint.json_fields.is_empty() && words.len() == 1 {
        return Some(typed);
    }
    None
}

fn normalize_field(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

// ---------------------------------------------------------------------------
// Emitting CEL.
// ---------------------------------------------------------------------------

fn emit(groups: &[Vec<Clause>], mode: Mode) -> String {
    match mode {
        Mode::Cel => emit_cel(groups),
        Mode::Sql => emit_sql(groups),
    }
}

/// `true` is the honest CEL for "nothing was recognised": it is a valid
/// expression, it matches every record, and the explanation says so. An empty
/// string would also match everything (`crate::search` treats it as no filter)
/// while looking like the translator had simply failed to write anything.
const CEL_EVERYTHING: &str = "true";
const SQL_EVERYTHING: &str = "SELECT * FROM messages";

fn emit_cel(groups: &[Vec<Clause>]) -> String {
    if groups.is_empty() {
        return CEL_EVERYTHING.to_string();
    }
    let parenthesise = groups.len() > 1;
    groups
        .iter()
        .map(|group| {
            let joined = group
                .iter()
                .map(cel_clause)
                .collect::<Vec<_>>()
                .join(" && ");
            if parenthesise && group.len() > 1 {
                format!("({joined})")
            } else {
                joined
            }
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

fn cel_clause(clause: &Clause) -> String {
    let body = cel_predicate(clause);
    if clause.negated {
        format!("!({body})")
    } else {
        body
    }
}

fn cel_predicate(clause: &Clause) -> String {
    let subject = match &clause.target {
        Target::Json(path) => cel_path(path),
        Target::Key => "key".to_string(),
        Target::ValueText => "value_text".to_string(),
        Target::Partition => "partition".to_string(),
        Target::Offset => "offset".to_string(),
        Target::Timestamp => "timestamp_ms".to_string(),
        Target::Header(name) => format!("headers[{}]", cel_string(name)),
        Target::Tombstone => "value".to_string(),
    };
    match &clause.predicate {
        Predicate::Equals(literal) => format!("{subject} == {}", cel_literal(literal)),
        Predicate::NotEquals(literal) => format!("{subject} != {}", cel_literal(literal)),
        Predicate::Compare(cmp, number) => format!("{subject} {} {number}", cmp.cel()),
        Predicate::OneOf(numbers) => format!(
            "{subject} in [{}]",
            numbers
                .iter()
                .map(Num::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Predicate::Contains(text) => format!("{subject}.contains({})", cel_string(text)),
        Predicate::StartsWith(text) => format!("{subject}.startsWith({})", cel_string(text)),
        Predicate::EndsWith(text) => format!("{subject}.endsWith({})", cel_string(text)),
        // `has()` is a CEL macro over a field selection, which is exactly what
        // a payload field is; a header is a map entry, and `in` is the map
        // question. Everything else answers presence as "not null".
        Predicate::Exists => match &clause.target {
            Target::Json(path) => format!("has({})", cel_path(path)),
            Target::Header(name) => format!("{} in headers", cel_string(name)),
            _ => format!("{subject} != null"),
        },
        Predicate::IsNull => match &clause.target {
            Target::Json(path) => format!("!has({})", cel_path(path)),
            Target::Header(name) => format!("!({} in headers)", cel_string(name)),
            _ => format!("{subject} == null"),
        },
    }
}

/// `value.a.b`, dropping to index syntax for any segment that is not a bare
/// CEL identifier — which is also what keeps a field name from becoming
/// syntax.
fn cel_path(path: &str) -> String {
    let mut out = "value".to_string();
    for segment in path.split('.') {
        if is_cel_identifier(segment) {
            out.push('.');
            out.push_str(segment);
        } else {
            out.push('[');
            out.push_str(&cel_string(segment));
            out.push(']');
        }
    }
    out
}

/// CEL's reserved words (cel-spec, "Syntax"). A field genuinely called `in`
/// reaches the expression as `value["in"]`.
const CEL_RESERVED: &[&str] = &[
    "as",
    "break",
    "const",
    "continue",
    "else",
    "false",
    "for",
    "function",
    "if",
    "import",
    "in",
    "let",
    "loop",
    "package",
    "namespace",
    "null",
    "return",
    "true",
    "var",
    "void",
    "while",
];

fn is_cel_identifier(name: &str) -> bool {
    !name.is_empty()
        && !CEL_RESERVED.contains(&name)
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A CEL string literal. Everything the user typed becomes character data:
/// backslashes and quotes are escaped, and every control character goes out as
/// a `\uXXXX` escape rather than as a raw byte that would end the literal or
/// smuggle a newline into the expression.
fn cel_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn cel_literal(literal: &Literal) -> String {
    match literal {
        Literal::Text(text) => cel_string(text),
        Literal::Number(n) => n.to_string(),
        Literal::Bool(b) => b.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Emitting SQL.
// ---------------------------------------------------------------------------

fn emit_sql(groups: &[Vec<Clause>]) -> String {
    if groups.is_empty() {
        return SQL_EVERYTHING.to_string();
    }
    let parenthesise = groups.len() > 1;
    let predicate = groups
        .iter()
        .map(|group| {
            let joined = group
                .iter()
                .map(sql_clause)
                .collect::<Vec<_>>()
                .join(" AND ");
            if parenthesise && group.len() > 1 {
                format!("({joined})")
            } else {
                joined
            }
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    format!("{SQL_EVERYTHING} WHERE {predicate}")
}

fn sql_clause(clause: &Clause) -> String {
    let body = sql_predicate(clause);
    if clause.negated {
        format!("NOT ({body})")
    } else {
        body
    }
}

fn sql_predicate(clause: &Clause) -> String {
    match &clause.target {
        Target::Json(path) => sql_json(path, &clause.predicate),
        Target::Header(name) => sql_json_in("headers_json", name, &clause.predicate),
        Target::Key => sql_column("key_text", &clause.predicate),
        Target::ValueText => sql_column("value_text", &clause.predicate),
        Target::Partition => sql_column("partition", &clause.predicate),
        // `offset` is a SQL keyword; the surface doc says quote it, so this
        // does, everywhere, rather than only where the parser would trip.
        Target::Offset => sql_column("\"offset\"", &clause.predicate),
        Target::Timestamp => sql_column("timestamp_ms", &clause.predicate),
        Target::Tombstone => match &clause.predicate {
            Predicate::Exists => "value_text IS NOT NULL".to_string(),
            _ => "value_text IS NULL".to_string(),
        },
    }
}

/// A real column: ordinary SQL, with the literal encoded.
fn sql_column(column: &str, predicate: &Predicate) -> String {
    match predicate {
        Predicate::Equals(literal) => format!("{column} = {}", sql_literal(literal)),
        Predicate::NotEquals(literal) => format!("{column} <> {}", sql_literal(literal)),
        Predicate::Compare(cmp, number) => format!("{column} {} {number}", cmp.cel()),
        Predicate::OneOf(numbers) => format!(
            "{column} IN ({})",
            numbers
                .iter()
                .map(Num::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Predicate::Contains(text) => sql_like(column, &format!("%{}%", like_escape(text)), text),
        Predicate::StartsWith(text) => sql_like(column, &format!("{}%", like_escape(text)), text),
        Predicate::EndsWith(text) => sql_like(column, &format!("%{}", like_escape(text)), text),
        Predicate::Exists => format!("{column} IS NOT NULL"),
        Predicate::IsNull => format!("{column} IS NULL"),
    }
}

/// A field inside the payload. **Kavka SQL has no JSON functions** (see
/// `crate::sql::SQL_SURFACE`), so `value_json` is text and every question about
/// a field inside it is a question about that text.
fn sql_json(path: &str, predicate: &Predicate) -> String {
    // Only the last segment is used: compact JSON nests, and `"email":` finds
    // the field wherever it sits. The explanation says so.
    let leaf = path.rsplit('.').next().unwrap_or(path);
    sql_json_in("value_json", leaf, predicate)
}

/// The shared shape for the two text columns that hold JSON: `value_json` and
/// `headers_json`.
fn sql_json_in(column: &str, field: &str, predicate: &Predicate) -> String {
    let key = format!("\"{}\":", json_key_escape(field));
    match predicate {
        Predicate::Equals(Literal::Text(text)) => sql_like(
            column,
            &format!("%{}\"{}\"%", like_escape(&key), like_escape(text)),
            &format!("{key}\"{text}\""),
        ),
        Predicate::NotEquals(Literal::Text(text)) => format!(
            "NOT ({})",
            sql_like(
                column,
                &format!("%{}\"{}\"%", like_escape(&key), like_escape(text)),
                &format!("{key}\"{text}\""),
            )
        ),
        Predicate::Equals(literal) => sql_like(
            column,
            &format!(
                "%{}{}%",
                like_escape(&key),
                like_escape(&json_scalar(literal))
            ),
            &format!("{key}{}", json_scalar(literal)),
        ),
        Predicate::NotEquals(literal) => format!(
            "NOT ({})",
            sql_like(
                column,
                &format!(
                    "%{}{}%",
                    like_escape(&key),
                    like_escape(&json_scalar(literal))
                ),
                &format!("{key}{}", json_scalar(literal)),
            )
        ),
        Predicate::Compare(cmp, number) => {
            format!("{} {} {number}", sql_json_number(column, field), cmp.cel())
        }
        Predicate::OneOf(numbers) => format!(
            "{} IN ({})",
            sql_json_number(column, field),
            numbers
                .iter()
                .map(Num::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Predicate::Contains(text) => sql_regex(
            column,
            &format!("{}\"[^\"]*{}", regex_escape(&key), regex_escape(text)),
        ),
        Predicate::StartsWith(text) => sql_regex(
            column,
            &format!("{}\"{}", regex_escape(&key), regex_escape(text)),
        ),
        Predicate::EndsWith(text) => sql_regex(
            column,
            &format!("{}\"[^\"]*{}\"", regex_escape(&key), regex_escape(text)),
        ),
        Predicate::Exists => sql_like(column, &format!("%{}%", like_escape(&key)), &key),
        Predicate::IsNull => format!(
            "NOT ({})",
            sql_like(column, &format!("%{}%", like_escape(&key)), &key)
        ),
    }
}

/// The one genuinely lossy shape in this module: a **number** inside a JSON
/// payload, compared numerically, with no JSON function to reach it.
///
/// `regexp_replace` pulls the first `"field": <number>` out of the compact JSON
/// text and `TRY_CAST` turns it into a double — `TRY_CAST`, not `CAST`, so a
/// record without the field yields NULL (the pattern does not match, the
/// replace returns the whole document, the cast fails, the row is excluded)
/// rather than failing the whole query. Both functions are in
/// `crate::sql::SQL_SURFACE`, and the emitted SQL is asserted to plan.
///
/// What it cannot do: tell one `"amount"` from another when the payload nests
/// two of them. The explanation says this whenever it is emitted.
fn sql_json_number(column: &str, field: &str) -> String {
    let pattern = format!(
        "^.*?\"{}\":(-?[0-9]+(?:\\.[0-9]+)?(?:[eE][-+]?[0-9]+)?).*$",
        regex_escape(&json_key_escape(field))
    );
    format!(
        "TRY_CAST(regexp_replace({column}, {}, '$1') AS DOUBLE)",
        sql_string(&pattern)
    )
}

/// LIKE when the pattern holds no LIKE metacharacter, `regexp_like` when it
/// does.
///
/// The two are the same question; the switch exists because `%` and `_` in a
/// user's value would otherwise become wildcards, and an `ESCAPE` clause is one
/// more thing for a reader of the generated SQL to decode. `human` is the same
/// pattern without escaping, for the regex path.
fn sql_like(column: &str, pattern: &str, human: &str) -> String {
    if pattern.contains('\u{0}') {
        // A NUL cannot ride in a SQL literal; fall through to the regex form,
        // which encodes it.
        return sql_regex(column, &regex_escape(human));
    }
    format!("{column} LIKE {}", sql_string(pattern))
}

fn sql_regex(column: &str, pattern: &str) -> String {
    format!("regexp_like({column}, {})", sql_string(pattern))
}

/// Escapes a value for use inside a LIKE pattern. The `\` escape character is
/// not used (no `ESCAPE` clause is emitted), so a value carrying a LIKE
/// metacharacter is routed through the regex form instead — see [`sql_like`]'s
/// callers, which pre-encode with this and then check.
fn like_escape(text: &str) -> String {
    // `%` and `_` are the metacharacters; there is no way to write them
    // literally without an ESCAPE clause, so they are replaced by `_`, the
    // single-character wildcard, which matches themselves and is never wrong
    // in the "did I find this substring" direction this module uses LIKE for.
    text.replace(['%', '_'], "_")
}

/// A SQL string literal: single-quoted, with `''` for an embedded quote. That
/// is the whole of the standard's escaping, and it is why nothing a user types
/// can leave the literal.
fn sql_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn sql_literal(literal: &Literal) -> String {
    match literal {
        Literal::Text(text) => sql_string(text),
        Literal::Number(n) => n.to_string(),
        Literal::Bool(b) => b.to_string().to_uppercase(),
    }
}

fn json_scalar(literal: &Literal) -> String {
    match literal {
        Literal::Text(text) => text.clone(),
        Literal::Number(n) => n.to_string(),
        Literal::Bool(b) => b.to_string(),
    }
}

/// How `serde_json` would have written this key, so the pattern matches the
/// text `crate::sql` actually put in the column.
fn json_key_escape(field: &str) -> String {
    let encoded = serde_json::Value::String(field.to_string()).to_string();
    encoded[1..encoded.len() - 1].to_string()
}

/// Regex-escapes a literal. Hand-written rather than `regex::escape` because
/// the output goes into a **DataFusion** regex (the same syntax, but this
/// module must not gain a dependency on the regex crate to build a string).
fn regex_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// The explanation.
// ---------------------------------------------------------------------------

fn explain(groups: &[Vec<Clause>], notes: &[String], mode: Mode) -> String {
    let mut sentences: Vec<String> = Vec::new();
    if groups.is_empty() {
        sentences.push(
            "Nothing in this was recognised, so the query matches every record in the scan \
             window."
                .to_string(),
        );
    } else {
        let described = groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|clause| clause.description.clone())
                    .collect::<Vec<_>>()
                    .join(", and ")
            })
            .collect::<Vec<_>>()
            .join("; or ");
        sentences.push(format!("Matches records where {described}."));
    }
    if mode == Mode::Sql && groups.iter().flatten().any(is_json_number) {
        sentences.push(
            "Kavka SQL has no JSON functions, so a number inside the payload is pulled out of the \
             JSON text with a regular expression: it reads the first field of that name in the \
             record, which is the wrong one if the payload nests two."
                .to_string(),
        );
    }
    if mode == Mode::Sql
        && groups
            .iter()
            .flatten()
            .any(|c| matches!(c.target, Target::Json(ref p) if p.contains('.')))
    {
        sentences.push(
            "A nested field is matched by its last name only, because the payload is matched as \
             text."
                .to_string(),
        );
    }
    sentences.extend(notes.iter().cloned());
    sentences.join(" ")
}

fn is_json_number(clause: &Clause) -> bool {
    matches!(clause.target, Target::Json(_))
        && matches!(
            clause.predicate,
            Predicate::Compare(_, _) | Predicate::OneOf(_)
        )
}

fn quote_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("{item:?}")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

// ---------------------------------------------------------------------------
// The fixture table, shared by the two test modules below: everything runs on
// the bare tier except `every_emitted_query_plans`, which needs DataFusion and
// therefore the `kafka` feature.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod fixtures {
    use super::SchemaHint;

    /// A fixed anchor: **2026-08-03T12:00:00Z**. Every time assertion is
    /// written against this, so the tests are a table rather than an
    /// approximation of one.
    pub const NOW: i64 = 1_785_758_400_000;

    /// The fields the table's examples talk about, so none of them takes the
    /// unknown-field path — that path has its own test, because it is the one
    /// that lowers confidence.
    pub const TABLE_FIELDS: &[&str] = &["status", "orderId", "note", "email", "couponCode"];

    /// The SQL shape for a number inside a payload, spelled once. It appears in
    /// six table rows and a typo in any of them would be a test that agrees
    /// with the bug.
    pub fn json_number(field: &str) -> String {
        format!(
            "TRY_CAST(regexp_replace(value_json, \
             '^.*?\"{field}\":(-?[0-9]+(?:\\.[0-9]+)?(?:[eE][-+]?[0-9]+)?).*$', '$1') AS DOUBLE)"
        )
    }

    pub fn hint(fields: &[&str]) -> SchemaHint {
        SchemaHint {
            json_fields: fields.iter().map(|f| (*f).to_string()).collect(),
        }
    }

    pub fn table_hint() -> SchemaHint {
        hint(TABLE_FIELDS)
    }

    /// **The exhaustive table.** Every pattern in [`super::NLQ_GRAMMAR`]
    /// appears here with its CEL and its SQL, so a change to either emitter is
    /// a change to this table, and a pattern in the grammar doc with no entry
    /// here fails `the_grammar_doc_and_the_table_agree`.
    pub fn table() -> Vec<(&'static str, String, String)> {
        let order_id = json_number("orderId");
        let select = "SELECT * FROM messages";
        let mut rows: Vec<(&'static str, String, String)> = Vec::new();
        let mut row = |input: &'static str, cel: String, sql: String| rows.push((input, cel, sql));

        // --- fields in the payload
        for input in [
            "status is failed",
            "status equals failed",
            "status = failed",
        ] {
            row(
                input,
                r#"value.status == "failed""#.to_string(),
                format!(r#"{select} WHERE value_json LIKE '%"status":"failed"%'"#),
            );
        }
        for input in [
            "status is not failed",
            "status isn't failed",
            "status != failed",
        ] {
            row(
                input,
                r#"value.status != "failed""#.to_string(),
                format!(r#"{select} WHERE NOT (value_json LIKE '%"status":"failed"%')"#),
            );
        }
        for input in [
            "orderId over 100",
            "orderId above 100",
            "orderId greater than 100",
            "orderId more than 100",
            "orderId > 100",
        ] {
            row(
                input,
                "value.orderId > 100".to_string(),
                format!("{select} WHERE {order_id} > 100"),
            );
        }
        row(
            "orderId at least 100",
            "value.orderId >= 100".to_string(),
            format!("{select} WHERE {order_id} >= 100"),
        );
        for input in [
            "orderId under 100",
            "orderId below 100",
            "orderId less than 100",
        ] {
            row(
                input,
                "value.orderId < 100".to_string(),
                format!("{select} WHERE {order_id} < 100"),
            );
        }
        row(
            "orderId at most 100",
            "value.orderId <= 100".to_string(),
            format!("{select} WHERE {order_id} <= 100"),
        );
        row(
            "orderId between 100 and 200",
            "value.orderId >= 100 && value.orderId <= 200".to_string(),
            format!("{select} WHERE {order_id} >= 100 AND {order_id} <= 200"),
        );
        for input in ["note contains timeout", "note includes timeout"] {
            row(
                input,
                r#"value.note.contains("timeout")"#.to_string(),
                format!(r#"{select} WHERE regexp_like(value_json, '"note":"[^"]*timeout')"#),
            );
        }
        for input in ["email starts with sales", "email begins with sales"] {
            row(
                input,
                r#"value.email.startsWith("sales")"#.to_string(),
                format!(r#"{select} WHERE regexp_like(value_json, '"email":"sales')"#),
            );
        }
        row(
            "email ends with acme.com",
            r#"value.email.endsWith("acme.com")"#.to_string(),
            format!(r#"{select} WHERE regexp_like(value_json, '"email":"[^"]*acme\.com"')"#),
        );
        for input in [
            "couponCode exists",
            "couponCode is present",
            "couponCode is set",
        ] {
            row(
                input,
                "has(value.couponCode)".to_string(),
                format!(r#"{select} WHERE value_json LIKE '%"couponCode":%'"#),
            );
        }
        for input in [
            "couponCode is missing",
            "couponCode is null",
            "couponCode is empty",
            "couponCode is absent",
        ] {
            row(
                input,
                "!has(value.couponCode)".to_string(),
                format!(r#"{select} WHERE NOT (value_json LIKE '%"couponCode":%')"#),
            );
        }

        // --- the record itself
        row(
            "key is A-102",
            r#"key == "A-102""#.to_string(),
            format!("{select} WHERE key_text = 'A-102'"),
        );
        row(
            "key starts with order-",
            r#"key.startsWith("order-")"#.to_string(),
            format!("{select} WHERE key_text LIKE 'order-%'"),
        );
        row(
            "key contains 42",
            r#"key.contains("42")"#.to_string(),
            format!("{select} WHERE key_text LIKE '%42%'"),
        );
        for input in ["no key", "keyless"] {
            row(
                input,
                "key == null".to_string(),
                format!("{select} WHERE key_text IS NULL"),
            );
        }
        row(
            "has a key",
            "!(key == null)".to_string(),
            format!("{select} WHERE NOT (key_text IS NULL)"),
        );
        for input in ["partition 3", "partition is 3", "on partition 3"] {
            row(
                input,
                "partition == 3".to_string(),
                format!("{select} WHERE partition = 3"),
            );
        }
        row(
            "partitions 1, 2 and 3",
            "partition in [1, 2, 3]".to_string(),
            format!("{select} WHERE partition IN (1, 2, 3)"),
        );
        row(
            "offset over 5000",
            "offset > 5000".to_string(),
            format!("{select} WHERE \"offset\" > 5000"),
        );
        row(
            "offset is 8412",
            "offset == 8412".to_string(),
            format!("{select} WHERE \"offset\" = 8412"),
        );
        row(
            "offset between 100 and 200",
            "offset >= 100 && offset <= 200".to_string(),
            format!("{select} WHERE \"offset\" >= 100 AND \"offset\" <= 200"),
        );
        row(
            "header trace-id is abc123",
            r#"headers["trace-id"] == "abc123""#.to_string(),
            format!(r#"{select} WHERE headers_json LIKE '%"trace-id":"abc123"%'"#),
        );
        row(
            "header trace-id exists",
            r#""trace-id" in headers"#.to_string(),
            format!(r#"{select} WHERE headers_json LIKE '%"trace-id":%'"#),
        );
        row(
            "tombstones",
            "value == null".to_string(),
            format!("{select} WHERE value_text IS NULL"),
        );

        // --- anywhere in the body
        for input in [
            "contains timeout",
            "mentions timeout",
            "body contains timeout",
        ] {
            row(
                input,
                r#"value_text.contains("timeout")"#.to_string(),
                format!("{select} WHERE value_text LIKE '%timeout%'"),
            );
        }

        // --- time, all anchored to NOW
        row(
            "last hour",
            "timestamp_ms >= 1785754800000".to_string(),
            format!("{select} WHERE timestamp_ms >= 1785754800000"),
        );
        row(
            "past 2 days",
            "timestamp_ms >= 1785585600000".to_string(),
            format!("{select} WHERE timestamp_ms >= 1785585600000"),
        );
        row(
            "in the last 15 minutes",
            "timestamp_ms >= 1785757500000".to_string(),
            format!("{select} WHERE timestamp_ms >= 1785757500000"),
        );
        row(
            "since yesterday",
            "timestamp_ms >= 1785628800000".to_string(),
            format!("{select} WHERE timestamp_ms >= 1785628800000"),
        );
        row(
            "today",
            "timestamp_ms >= 1785715200000".to_string(),
            format!("{select} WHERE timestamp_ms >= 1785715200000"),
        );
        row(
            "yesterday",
            "timestamp_ms >= 1785628800000 && timestamp_ms < 1785715200000".to_string(),
            format!(
                "{select} WHERE timestamp_ms >= 1785628800000 AND timestamp_ms < 1785715200000"
            ),
        );

        // --- joining
        row(
            "status is failed and orderId over 100",
            r#"value.status == "failed" && value.orderId > 100"#.to_string(),
            format!(r#"{select} WHERE value_json LIKE '%"status":"failed"%' AND {order_id} > 100"#),
        );
        row(
            "status is failed or status is cancelled",
            r#"value.status == "failed" || value.status == "cancelled""#.to_string(),
            format!(
                r#"{select} WHERE value_json LIKE '%"status":"failed"%' OR value_json LIKE '%"status":"cancelled"%'"#
            ),
        );
        row(
            "not tombstones",
            "!(value == null)".to_string(),
            format!("{select} WHERE NOT (value_text IS NULL)"),
        );
        row(
            "without a key",
            "key == null".to_string(),
            format!("{select} WHERE key_text IS NULL"),
        );
        row(
            "except partition 0",
            "!(partition == 0)".to_string(),
            format!("{select} WHERE NOT (partition = 0)"),
        );
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn cel(input: &str) -> String {
        nl_to_query_at(input, Mode::Cel, &hint(&[]), NOW).query
    }

    fn sql(input: &str) -> String {
        nl_to_query_at(input, Mode::Sql, &hint(&[]), NOW).query
    }

    fn cel_known(input: &str, fields: &[&str]) -> Translation {
        nl_to_query_at(input, Mode::Cel, &hint(fields), NOW)
    }

    fn sql_known(input: &str, fields: &[&str]) -> String {
        nl_to_query_at(input, Mode::Sql, &hint(fields), NOW).query
    }

    #[test]
    fn every_documented_pattern_translates_in_both_modes() {
        for (input, expected_cel, expected_sql) in table() {
            let cel = nl_to_query_at(input, Mode::Cel, &table_hint(), NOW);
            assert_eq!(cel.query, expected_cel, "CEL for {input:?}");
            assert!(
                cel.unrecognized.is_empty(),
                "{input:?} left {:?} unrecognised",
                cel.unrecognized
            );
            assert_eq!(
                cel.confidence,
                Confidence::High,
                "{input:?} is a documented pattern and should be confident: {}",
                cel.explanation
            );
            assert!(!cel.explanation.is_empty(), "{input:?} explained nothing");

            let sql = nl_to_query_at(input, Mode::Sql, &table_hint(), NOW);
            assert_eq!(sql.query, expected_sql, "SQL for {input:?}");
            assert!(sql.unrecognized.is_empty(), "SQL for {input:?}");
            assert_eq!(sql.confidence, Confidence::High, "SQL for {input:?}");
        }
    }

    /// Every example sentence in the grammar doc translates, and every table
    /// entry is in the doc. Without this the doc and the grammar drift the
    /// first time either is edited alone.
    #[test]
    fn the_grammar_doc_and_the_table_agree() {
        for (input, _, _) in table() {
            assert!(
                NLQ_GRAMMAR.contains(input),
                "{input:?} translates but is not in NLQ_GRAMMAR"
            );
        }
        assert_eq!(nlq_grammar(), NLQ_GRAMMAR);
    }

    /// Nothing a user types becomes syntax. The values below are the exact
    /// strings that would break a naive string-concatenating translator.
    #[test]
    fn query_syntax_in_the_input_is_data() {
        let attacks = [
            "'; DROP TABLE messages; --",
            "\" || true || \"",
            "x\\\") && (true",
            "%_%",
            "a\nb",
            "value_text.contains(\"x\")",
            "1=1",
        ];
        for attack in attacks {
            let cel = cel(&format!("status is {attack}"));
            let sql = sql(&format!("status is {attack}"));

            // CEL: one comparison against exactly one string literal, whatever
            // the value turned out to be.
            let literal = cel
                .strip_prefix("value.status == ")
                .unwrap_or_else(|| panic!("{attack:?} changed the expression shape: {cel}"));
            assert!(
                is_one_cel_string(literal),
                "{attack:?} escaped its literal: {cel}"
            );

            // SQL: with the literals blanked out, nothing of the input
            // survives — no terminator, no comment, no second statement, and
            // the same skeleton every time.
            assert_eq!(
                outside_literals(&sql),
                "SELECT * FROM messages WHERE value_json LIKE ?",
                "{attack:?} escaped its literal: {sql}"
            );
        }

        // `1' OR '1'='1` is the interesting near-miss, and it is not an escape:
        // `OR` is a word this grammar owns, so the line reads as two clauses —
        // and both halves are still string literals. The classic payload
        // produces a query about two nonsense values, never a tautology.
        assert_eq!(
            cel("status is 1' OR '1'='1"),
            r#"value.status == "1'" || value["1"] == "'1""#
        );
    }

    /// Whether `text` is one complete double-quoted CEL string literal — the
    /// question "did the value stay inside the quotes", asked precisely.
    fn is_one_cel_string(text: &str) -> bool {
        let mut chars = text.chars();
        if chars.next() != Some('"') {
            return false;
        }
        let mut escaped = false;
        for c in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' => escaped = true,
                // The closing quote has to be the last character.
                '"' => return chars.next().is_none(),
                // A raw newline would end nothing in CEL but would break the
                // one-line editor the result is pasted into.
                '\n' | '\r' => return false,
                _ => {}
            }
        }
        false
    }

    /// Blanks out every single-quoted SQL literal (`''` is an escaped quote,
    /// not a boundary), leaving the statement's skeleton. If a value ever
    /// escaped its literal, it shows up here.
    fn outside_literals(sql: &str) -> String {
        let mut out = String::with_capacity(sql.len());
        let mut chars = sql.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\'' {
                out.push(c);
                continue;
            }
            while let Some(inner) = chars.next() {
                if inner == '\'' {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                        continue;
                    }
                    break;
                }
            }
            out.push('?');
        }
        out
    }

    /// A field name is not a hole in the fence either: one that is not a bare
    /// CEL identifier goes out as an index, not as a selector.
    #[test]
    fn a_hostile_field_name_is_data_too() {
        // Spaces, a dash and a nested path: none of them is a bare CEL
        // identifier, so none of them is emitted as a selector.
        assert_eq!(
            cel_known("weird name.with-dash is x", &["weird name.with-dash"]).query,
            r#"value["weird name"]["with-dash"] == "x""#
        );
        // A field that collides with a CEL keyword.
        assert_eq!(
            cel_known("package is 3", &["package"]).query,
            r#"value["package"] == 3"#
        );
        // A field that starts with a digit is not an identifier either.
        assert_eq!(
            cel_known("2fa is on", &["2fa"]).query,
            r#"value["2fa"] == "on""#
        );
        // The escaper is not vacuous: a quote in a segment is escaped rather
        // than ending the literal. (No tokenizer path produces this name, so it
        // is checked at the emitter.)
        assert_eq!(cel_path("a\"b"), r#"value["a\"b"]"#);
    }

    #[test]
    fn a_value_keeps_the_case_the_user_typed() {
        assert_eq!(cel("status is Failed"), r#"value.status == "Failed""#);
        assert_eq!(
            cel(r#"note contains "Timed Out""#),
            r#"value.note.contains("Timed Out")"#
        );
    }

    /// Quoting is the escape hatch the grammar doc promises: the connective
    /// words inside a quoted value stay in the value.
    #[test]
    fn a_quoted_value_keeps_its_keywords() {
        assert_eq!(
            cel(r#"note contains "failed and cancelled""#),
            r#"value.note.contains("failed and cancelled")"#
        );
        assert_eq!(cel(r#"status is "or""#), r#"value.status == "or""#);
    }

    #[test]
    fn a_field_name_is_found_however_it_is_spelled() {
        for spelling in [
            "orderId is 7",
            "order id is 7",
            "order_id is 7",
            "OrderID is 7",
        ] {
            let translation = cel_known(spelling, &["orderId"]);
            assert_eq!(translation.query, "value.orderId == 7", "{spelling}");
            assert_eq!(translation.confidence, Confidence::High, "{spelling}");
        }
    }

    /// A field nobody has seen is still translated — it may simply not have
    /// appeared in the records on screen — but the confidence drops and the
    /// explanation names it.
    #[test]
    fn an_unknown_field_lowers_confidence_and_says_so() {
        let translation = cel_known("mystery is 7", &["orderId", "status"]);
        assert_eq!(translation.query, "value.mystery == 7");
        assert_eq!(translation.confidence, Confidence::Low);
        assert!(
            translation
                .explanation
                .contains("mystery is not one of the fields"),
            "{}",
            translation.explanation
        );
        assert!(translation.unrecognized.is_empty());

        // With no hint at all there is nothing to be unsure about.
        assert_eq!(cel_known("mystery is 7", &[]).confidence, Confidence::High);
    }

    #[test]
    fn unrecognised_phrases_come_back_verbatim_and_are_left_out() {
        let translation = nl_to_query_at(
            "status is failed and make it snappy please",
            Mode::Cel,
            &hint(&["status"]),
            NOW,
        );
        assert_eq!(translation.query, r#"value.status == "failed""#);
        assert_eq!(
            translation.unrecognized,
            vec!["make it snappy please".to_string()]
        );
        assert_eq!(translation.confidence, Confidence::Low);
        assert!(
            translation
                .explanation
                .contains("did not recognise \"make it snappy please\""),
            "{}",
            translation.explanation
        );
    }

    #[test]
    fn nothing_recognised_matches_everything_and_says_so() {
        for (mode, expected) in [(Mode::Cel, "true"), (Mode::Sql, "SELECT * FROM messages")] {
            let translation = nl_to_query_at("please help me", mode, &hint(&[]), NOW);
            assert_eq!(translation.query, expected);
            assert_eq!(translation.confidence, Confidence::Low);
            assert_eq!(translation.unrecognized, vec!["please help me".to_string()]);
            assert!(
                translation.explanation.contains("matches every record"),
                "{}",
                translation.explanation
            );
        }
        // Empty input is the same answer with nothing to report back.
        let empty = nl_to_query_at("   ", Mode::Cel, &hint(&[]), NOW);
        assert_eq!(empty.query, "true");
        assert!(empty.unrecognized.is_empty());
    }

    /// Mixing `and` with `or` is the one genuinely ambiguous thing an English
    /// sentence can do here, so it is `low` and the grouping is parenthesised
    /// where it can be seen.
    #[test]
    fn mixing_and_with_or_is_low_confidence_and_parenthesised() {
        let input = "status is failed and orderId over 100 or partition 3";
        let translation = nl_to_query_at(input, Mode::Cel, &table_hint(), NOW);
        assert_eq!(
            translation.query,
            r#"(value.status == "failed" && value.orderId > 100) || partition == 3"#
        );
        assert_eq!(translation.confidence, Confidence::Low);
        assert!(
            translation.explanation.contains("`and` binding tighter"),
            "{}",
            translation.explanation
        );

        let as_sql = nl_to_query_at(input, Mode::Sql, &table_hint(), NOW);
        assert!(
            as_sql.query.contains(") OR partition = 3"),
            "{}",
            as_sql.query
        );
    }

    /// A comma is an `and`, and an `and` between two clauses that are not a
    /// list is still an `and`.
    #[test]
    fn a_comma_joins_like_and() {
        assert_eq!(
            cel_known("status is failed, partition 3", &["status"]).query,
            r#"value.status == "failed" && partition == 3"#
        );
    }

    /// The two splitter exceptions, which are the only reason `between` and a
    /// partition list survive a naive split on `and`.
    #[test]
    fn between_and_partition_lists_survive_the_splitter() {
        assert_eq!(
            cel_known("orderId between 100 and 200", &["orderId"]).query,
            "value.orderId >= 100 && value.orderId <= 200"
        );
        assert_eq!(cel("partitions 0, 1 and 2"), "partition in [0, 1, 2]");
        // …and an `and` that is genuinely a conjunction is not swallowed.
        assert_eq!(
            cel_known("orderId between 100 and 200 and partition 3", &["orderId"]).query,
            "value.orderId >= 100 && value.orderId <= 200 && partition == 3"
        );
    }

    #[test]
    fn negation_wraps_the_clause_it_was_typed_in_front_of() {
        assert_eq!(
            cel_known("not status is failed", &["status"]).query,
            r#"!(value.status == "failed")"#
        );
        assert_eq!(
            sql("except partition 0"),
            "SELECT * FROM messages WHERE NOT (partition = 0)"
        );
        // Two negations do not cancel into a lie: the query says both.
        assert_eq!(
            cel_known("not couponCode is missing", &["couponCode"]).query,
            "!(!has(value.couponCode))"
        );
    }

    /// The anchor is stated in the explanation, in milliseconds, because a
    /// relative time that has become an absolute number has to say what it was
    /// anchored to or it cannot be checked.
    #[test]
    fn a_relative_time_says_what_it_was_anchored_to() {
        let translation = nl_to_query_at("last hour", Mode::Cel, &hint(&[]), NOW);
        assert!(
            translation.explanation.contains(&NOW.to_string()),
            "{}",
            translation.explanation
        );
        assert!(
            translation.explanation.contains("the last hour"),
            "{}",
            translation.explanation
        );
        // A day boundary says it is UTC.
        let day = nl_to_query_at("today", Mode::Cel, &hint(&[]), NOW);
        assert!(day.explanation.contains("UTC"), "{}", day.explanation);
    }

    /// `nl_to_query` is `nl_to_query_at` plus a clock, and nothing else — a
    /// query with no time in it is identical either way.
    #[test]
    fn the_clock_wrapper_changes_nothing_but_the_anchor() {
        assert_eq!(
            nl_to_query("status is failed", Mode::Cel, &hint(&["status"])),
            nl_to_query_at("status is failed", Mode::Cel, &hint(&["status"]), NOW)
        );
    }

    #[test]
    fn the_same_input_always_gives_the_same_answer() {
        for (input, _, _) in table() {
            let first = nl_to_query_at(input, Mode::Sql, &table_hint(), NOW);
            let second = nl_to_query_at(input, Mode::Sql, &table_hint(), NOW);
            assert_eq!(first, second, "{input:?} is not deterministic");
        }
    }

    /// A LIKE metacharacter in a value would otherwise become a wildcard. It is
    /// replaced by `_`, which matches itself, so the query is never *narrower*
    /// than the user asked for and never picks up an unbounded wildcard.
    #[test]
    fn like_metacharacters_do_not_become_wildcards() {
        assert_eq!(
            sql("key contains 50%"),
            "SELECT * FROM messages WHERE key_text LIKE '%50_%'"
        );
        // The regex path carries the character itself, escaped for regex.
        let regex = sql_known("note contains 50%", &["note"]);
        assert!(regex.contains("50%"), "{regex}");
    }

    #[test]
    fn booleans_and_numbers_keep_their_types() {
        assert_eq!(
            cel_known("paid is true", &["paid"]).query,
            "value.paid == true"
        );
        assert_eq!(
            sql_known("paid is true", &["paid"]),
            r#"SELECT * FROM messages WHERE value_json LIKE '%"paid":true%'"#
        );
        assert_eq!(
            cel_known("amount is 12.5", &["amount"]).query,
            "value.amount == 12.5"
        );
        // A partition is a number, never a bool and never a string.
        assert_eq!(cel("partition is 0"), "partition == 0");
    }

    #[test]
    fn the_ipc_wire_form_is_fixed() {
        let raw = serde_json::json!({
            "query": "value.status == \"failed\"",
            "confidence": "high",
            "explanation": "Matches records where the payload field status is \"failed\".",
            "unrecognized": [],
        });
        let translation: Translation = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(translation.confidence, Confidence::High);
        assert_eq!(serde_json::to_value(&translation).unwrap(), raw);

        assert_eq!(
            serde_json::to_value(Mode::Cel).unwrap(),
            serde_json::json!("cel")
        );
        assert_eq!(
            serde_json::to_value(Mode::Sql).unwrap(),
            serde_json::json!("sql")
        );
        assert_eq!("cel".parse::<Mode>().unwrap(), Mode::Cel);
        assert!("prolog".parse::<Mode>().is_err());

        let parsed: SchemaHint =
            serde_json::from_value(serde_json::json!({"json_fields": ["a", "b"]})).unwrap();
        assert_eq!(parsed.json_fields, vec!["a".to_string(), "b".to_string()]);
        // The hint is optional in both directions.
        let empty: SchemaHint = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.json_fields.is_empty());
    }

    /// Unicode in a value survives, and is not mangled into escapes it does not
    /// need. The spans the tokenizer hands back are byte offsets into a UTF-8
    /// string, so this is also the test that a multi-byte character never
    /// splits one.
    #[test]
    fn unicode_values_pass_through_as_themselves() {
        assert_eq!(
            cel_known("city is Köln", &["city"]).query,
            r#"value.city == "Köln""#
        );
        assert_eq!(
            sql_known("city is 東京", &["city"]),
            r#"SELECT * FROM messages WHERE value_json LIKE '%"city":"東京"%'"#
        );
        // …and an unrecognised phrase with a multi-byte character comes back
        // whole rather than sliced.
        let translation = nl_to_query_at("сделай красиво", Mode::Cel, &hint(&[]), NOW);
        assert_eq!(translation.unrecognized, vec!["сделай красиво".to_string()]);
    }

    /// The CEL this module emits has to compile — the one check that says the
    /// emitter speaks the language rather than something that looks like it.
    #[test]
    fn every_emitted_cel_expression_compiles() {
        let compile = |query: &str| {
            crate::search::CompiledQuery::compile(&crate::search::SearchQuery {
                substring: None,
                cel: Some(query.to_string()),
            })
        };
        for (input, _, _) in table() {
            let query = nl_to_query_at(input, Mode::Cel, &table_hint(), NOW).query;
            compile(&query)
                .unwrap_or_else(|e| panic!("{input:?} emitted CEL that does not compile: {e}"));
        }
        // Including the hostile inputs, which is where an escaping bug shows.
        for attack in ["'; DROP--", "\" || true || \"", "a\nb", "\\", "\"\""] {
            let query = cel(&format!("status is \"{attack}\""));
            compile(&query)
                .unwrap_or_else(|e| panic!("{attack:?} emitted CEL that does not compile: {e}"));
        }
    }
}

/// The SQL half of the same promise: every query this module emits **plans**
/// against the real `messages` schema, which is what proves the columns exist,
/// the functions exist in this build, and the statement is a read.
///
/// Gated on `kafka` because `datafusion` is (see Cargo.toml); the CEL half runs
/// on the bare tier.
#[cfg(all(test, feature = "kafka"))]
mod sql_plans {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn every_emitted_query_plans() {
        for (input, _, expected_sql) in table() {
            let translation = nl_to_query_at(input, Mode::Sql, &table_hint(), NOW);
            assert_eq!(translation.query, expected_sql);
            crate::sql::plan_columns(&translation.query)
                .unwrap_or_else(|e| panic!("{input:?} emitted SQL that does not plan: {e}"));
        }
        for attack in ["'; DROP TABLE messages; --", "100%", "a'b", "_"] {
            let query = nl_to_query_at(
                &format!("status is \"{attack}\""),
                Mode::Sql,
                &table_hint(),
                NOW,
            )
            .query;
            crate::sql::plan_columns(&query)
                .unwrap_or_else(|e| panic!("{attack:?} emitted SQL that does not plan: {e}"));
        }
    }
}
