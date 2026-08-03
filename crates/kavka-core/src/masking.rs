//! Display masking: rules that redact matching text **before a record crosses
//! IPC**, so the webview never holds what the rule hides.
//!
//! # Where it happens, and why there
//!
//! Masking runs at the end of the decode pipeline, on the
//! [`crate::serdes::DecodedPayload`] the shell is about to hand the UI, and
//! nowhere else. That is the one placement with a property worth having: while
//! a rule is on, the unmasked text does not exist on the webview side at all,
//! so a screen share, a devtools console, a copied cell, a `Copy row as JSON`
//! and an exported file all carry the same masked text. Masking in the UI —
//! CSS, a render-time replace, a "blur" class — would leave the real value one
//! inspector click away, which is not masking, it is a costume.
//!
//! It is a **display** rule, not an access control. Kavka is a client: the user
//! running it can read the topic. Masking is for the demo, the screenshot, the
//! pair-programming session and the support call — the moments when the person
//! looking at the screen is not the person holding the credentials.
//!
//! # It is independent of read-only mode, deliberately
//!
//! Read-only (docs/ARCHITECTURE.md D5) is about what leaves Kavka. Masking is
//! about what reaches the screen. A read-only connection can be unmasked and a
//! writable one can be masked; neither implies the other, and tying them
//! together would mean turning off a guardrail to get a redaction.
//!
//! # The performance contract
//!
//! Rules are compiled **once per session** into a [`MaskSet`], never per
//! record. A search buffering 10,000 records with three rules on runs three
//! compiled automata over each payload; compiling the same three regexes 30,000
//! times would cost more than the search. [`MaskSet::is_empty`] is the guard
//! every caller checks first, so a session with no rules pays one boolean per
//! record.
//!
//! # What a rule may do to a match
//!
//! Replace it, entirely, with the replacement text. Nothing of the match
//! survives — capture-group references in the replacement are **not** expanded
//! ([`regex::NoExpand`]), so `$1` in a replacement is three characters, not the
//! matched digits coming back out through the door they were shown out of.

use crate::serdes::{DecodedPayload, DlqMeta, HeaderEntry, MessageRecord};
use crate::{Error, Result};
use regex::{NoExpand, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// Envelope version for `masking.json`, versioned for the same reason
/// `alerts.json` and profile exports are: a future shape gets migrated or
/// refused, never half-parsed.
pub const MASKING_VERSION: u32 = 1;

/// What a rule writes over a match when it does not say.
///
/// Three bullets rather than `[REDACTED]` or `****`: it is short enough not to
/// reflow a table row, it is visibly *not* data (no producer emits U+2022), and
/// it does not leak the length of what it replaced the way a run of asterisks
/// matching the original does.
pub const DEFAULT_REPLACEMENT: &str = "•••";

/// The compiled size a single rule's automaton may occupy. A regex is user
/// input and `regex` has no backtracking to blow up on, but a pathological
/// pattern can still ask for an enormous DFA; this refuses it at save time with
/// a message rather than at decode time with a stall.
const MAX_REGEX_BYTES: usize = 1 << 20;

// ---------------------------------------------------------------------------
// The IPC contract. Field names are mirrored by the TypeScript in
// apps/desktop/src, so renaming one is a breaking change on both sides.
// ---------------------------------------------------------------------------

/// Which part of a record a rule reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskTarget {
    Value,
    Key,
    Headers,
    All,
}

impl MaskTarget {
    fn covers(self, field: MaskField) -> bool {
        match self {
            Self::All => true,
            Self::Value => field == MaskField::Value,
            Self::Key => field == MaskField::Key,
            Self::Headers => field == MaskField::Headers,
        }
    }
}

/// The part of a record being masked right now. Separate from [`MaskTarget`]
/// because `all` is a thing a *rule* can say and not a thing a *payload* can
/// be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskField {
    Value,
    Key,
    Headers,
}

/// One masking rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskRule {
    pub id: String,
    pub name: String,
    /// A regular expression in the `regex` crate's syntax — the same syntax
    /// CEL's `matches()` uses in a search filter, so a user learns one.
    pub pattern: String,
    #[serde(default = "default_replacement")]
    pub replacement: String,
    pub applies_to: MaskTarget,
    pub enabled: bool,
}

fn default_replacement() -> String {
    DEFAULT_REPLACEMENT.to_string()
}

impl MaskRule {
    /// A rule with the default replacement, enabled, over everything — the
    /// shape the editor starts from.
    pub fn new(id: impl Into<String>, name: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            pattern: pattern.into(),
            replacement: default_replacement(),
            applies_to: MaskTarget::All,
            enabled: true,
        }
    }
}

/// Why a rule was refused, with the **position** in the pattern.
///
/// The position is the whole point: a regex error rendered as a caret drawing
/// is unusable to a UI that wants to put a caret at the mistake, and
/// `regex::Error`'s `Display` is exactly that drawing. `regex_syntax` is asked
/// separately for the offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskRuleError {
    /// Byte offset into `pattern` where the parser gave up, when it named one.
    pub position: Option<usize>,
    /// One sentence, already fit to sit under the field.
    pub message: String,
}

impl std::fmt::Display for MaskRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.position {
            Some(at) => write!(f, "{} (at character {at})", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

// ---------------------------------------------------------------------------
// The two sentences the rest of the app says about masking. They live here so
// the status bar, the export writer and the clipboard cannot drift apart.
// ---------------------------------------------------------------------------

/// The marker every masked export and every masked copy carries, and the string
/// a reader can search a file for to find out whether it was masked.
///
/// **Contract with the UI:** the export writer prepends [`mask_notice`] as the
/// first line of a JSON/NDJSON/CSV export (as a comment line for CSV, as a
/// `_kavka_notice` field for JSON) and the clipboard writer prepends it to a
/// multi-record copy. Both use these functions rather than their own wording,
/// because a file that is masked and does not say so is a file someone will
/// later treat as evidence.
pub const MASK_NOTICE_MARKER: &str = "Masked by Kavka";

/// The sentence that rides on a masked export or copy.
pub fn mask_notice(rules_applied: usize) -> String {
    format!(
        "{MASK_NOTICE_MARKER} — {rules_applied} masking {} applied. Some values here are not the \
         values on the topic.",
        plural(rules_applied, "rule", "rules")
    )
}

/// The status-bar sentence (docs/DESIGN.md §5.1: the status bar is the
/// permanent home for session state).
///
/// **Mirrored in TypeScript**, by `maskingChipLabel` in
/// `apps/desktop/src/masking.ts` — the chip renders on every store change and
/// must not await an IPC round trip for six words. Changing the wording here is
/// a two-file edit; the test below pins this side of it so the drift fails a
/// build rather than reaching a screen.
pub fn masking_status(enabled_rules: usize) -> String {
    format!(
        "Masking on — {enabled_rules} {}",
        plural(enabled_rules, "rule", "rules")
    )
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}

// ---------------------------------------------------------------------------
// The compiled set.
// ---------------------------------------------------------------------------

struct CompiledRule {
    regex: Regex,
    replacement: String,
    applies_to: MaskTarget,
}

/// A session's enabled rules, compiled once.
///
/// Cheap to share (`Send + Sync`), and cheap to *not* use: [`Self::is_empty`]
/// is the first thing every caller asks, so an unmasked session pays a boolean
/// per record and nothing else.
pub struct MaskSet {
    rules: Vec<CompiledRule>,
}

impl std::fmt::Debug for MaskSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskSet")
            .field("rules", &self.rules.len())
            .finish()
    }
}

impl MaskSet {
    /// An empty set — the "masking off" state, and what every caller falls back
    /// to.
    pub fn none() -> Self {
        Self { rules: Vec::new() }
    }

    /// Compiles the **enabled** rules of a profile.
    ///
    /// A disabled rule is not compiled at all: toggling masking off has to cost
    /// nothing per record, and a compiled-but-skipped rule is a cost with no
    /// benefit.
    ///
    /// A rule that fails to compile here is skipped rather than fatal, and its
    /// name comes back in the second half of the answer — a stored rule can
    /// only be invalid if it was hand-edited or written by a future build, and
    /// refusing to open the topic at all would be a worse answer than masking
    /// with the rules that do work and saying which one did not. The save path
    /// ([`MaskStore::save_rule`]) is where an invalid pattern is *refused*.
    pub fn compile(rules: &[MaskRule]) -> (Self, Vec<String>) {
        let mut compiled = Vec::new();
        let mut refused = Vec::new();
        for rule in rules.iter().filter(|rule| rule.enabled) {
            match compile_pattern(&rule.pattern) {
                Ok(regex) => compiled.push(CompiledRule {
                    regex,
                    replacement: rule.replacement.clone(),
                    applies_to: rule.applies_to,
                }),
                Err(e) => refused.push(format!("the masking rule {:?} is off: {e}", rule.name)),
            }
        }
        (Self { rules: compiled }, refused)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// How many rules are live — the number [`masking_status`] and
    /// [`mask_notice`] quote.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Applies every rule that covers `field` to one decoded payload, in place.
    /// Answers whether anything changed.
    ///
    /// Both halves of the payload are masked and kept consistent: the JSON tree
    /// (string values, recursively) and the display `text`. When the payload
    /// has a tree, `text` is re-rendered from the masked tree rather than
    /// masked separately — otherwise a pattern that matches across JSON
    /// punctuation would produce a `text` and a `json` that disagree, and the
    /// inspector shows both.
    pub fn mask_payload(&self, decoded: &mut DecodedPayload, field: MaskField) -> bool {
        let rules: Vec<&CompiledRule> = self
            .rules
            .iter()
            .filter(|rule| rule.applies_to.covers(field))
            .collect();
        if rules.is_empty() {
            return false;
        }
        match decoded.json.as_mut() {
            Some(json) => {
                if !mask_json(json, &rules) {
                    return false;
                }
                decoded.text =
                    serde_json::to_string_pretty(json).unwrap_or_else(|_| json.to_string());
                true
            }
            None => {
                let masked = apply(&decoded.text, &rules);
                if masked == decoded.text {
                    return false;
                }
                decoded.text = masked;
                true
            }
        }
    }

    /// Masks a header entry's rendered value.
    pub fn mask_header(&self, header: &mut HeaderEntry) -> bool {
        let rules: Vec<&CompiledRule> = self
            .rules
            .iter()
            .filter(|rule| rule.applies_to.covers(MaskField::Headers))
            .collect();
        let Some(value) = header.value.as_mut() else {
            return false;
        };
        if rules.is_empty() {
            return false;
        }
        let masked = apply(value, &rules);
        if masked == *value {
            return false;
        }
        *value = masked;
        true
    }

    /// Masks a record's dead-letter metadata, under the **Headers** scope.
    ///
    /// [`DlqMeta`] is not a fifth part of a record — every string in it was
    /// read straight out of a header by [`crate::serdes::dlq_inspect`], and it
    /// is the *same text*, copied. So a Headers-scoped rule that redacts the
    /// `kafka_dlt-exception-message` header and leaves `dlq.exception_message`
    /// alone has not masked anything: the panel a dead letter is actually read
    /// in shows the copy, above the payload, in larger type than the headers
    /// tab it came from.
    ///
    /// **Four fields, and only four.** The topic, the exception's class and
    /// message, and the stack trace are the ones carrying text a producer
    /// wrote. `original_partition` and `original_offset` are numbers and are
    /// left alone for the reason [`mask_json`] leaves numbers alone, and
    /// `convention` is Kavka's own vocabulary rather than anything the record
    /// said.
    pub fn mask_dlq(&self, dlq: &mut DlqMeta) -> bool {
        let rules: Vec<&CompiledRule> = self
            .rules
            .iter()
            .filter(|rule| rule.applies_to.covers(MaskField::Headers))
            .collect();
        if rules.is_empty() {
            return false;
        }
        let mut changed = false;
        for field in [
            &mut dlq.original_topic,
            &mut dlq.exception_class,
            &mut dlq.exception_message,
            &mut dlq.stacktrace,
        ] {
            let Some(text) = field.as_mut() else {
                continue;
            };
            let masked = apply(text, &rules);
            if masked != *text {
                *text = masked;
                changed = true;
            }
        }
        changed
    }

    /// The whole record: key, value, headers and dead-letter metadata, with
    /// [`MessageRecord::masked`] set when anything changed.
    ///
    /// **This is what the shell calls**, once per record, on the way to IPC.
    pub fn mask_record(&self, record: &mut MessageRecord) -> bool {
        if self.is_empty() {
            return false;
        }
        let mut changed = false;
        if let Some(key) = record.key.as_mut() {
            changed |= self.mask_payload(key, MaskField::Key);
        }
        if let Some(value) = record.value.as_mut() {
            changed |= self.mask_payload(value, MaskField::Value);
        }
        for header in &mut record.headers {
            changed |= self.mask_header(header);
        }
        // The dead-letter copy of four of those headers — see `mask_dlq`.
        if let Some(dlq) = record.dlq.as_mut() {
            changed |= self.mask_dlq(dlq);
        }
        // The flag is sticky across a record that was already masked upstream,
        // so a re-mask never *clears* it.
        record.masked |= changed;
        changed
    }
}

/// The free function the IPC contract names. [`MaskSet::mask_payload`] is the
/// method behind it; this is the spelling the shell's call site reads best.
pub fn mask_payload(decoded: &mut DecodedPayload, rules: &MaskSet, field: MaskField) -> bool {
    rules.mask_payload(decoded, field)
}

/// Applies rules in order. Each rule sees the previous rule's output, which is
/// what makes a set of rules composable — a broad rule after a narrow one does
/// not un-mask what the narrow one hid.
fn apply(text: &str, rules: &[&CompiledRule]) -> String {
    let mut current = std::borrow::Cow::Borrowed(text);
    for rule in rules {
        // `NoExpand` is load-bearing: without it a replacement containing `$1`
        // would put the matched text back.
        if let std::borrow::Cow::Owned(replaced) = rule
            .regex
            .replace_all(&current, NoExpand(rule.replacement.as_str()))
        {
            current = std::borrow::Cow::Owned(replaced);
        }
    }
    current.into_owned()
}

/// Walks a JSON tree masking **string values**, at every depth, inside objects
/// and arrays alike.
///
/// Object *keys* are left alone, deliberately: a key is the shape of the
/// document rather than its content, masking it would break every downstream
/// reader of an export, and a secret stored in a field *name* is a different
/// problem from the one this feature solves. Numbers are also left alone — a
/// regex over a rendered number would mask an account id and a quantity with
/// equal enthusiasm, and the honest way to hide a number is to quote it in the
/// producer.
fn mask_json(value: &mut serde_json::Value, rules: &[&CompiledRule]) -> bool {
    match value {
        serde_json::Value::String(text) => {
            let masked = apply(text, rules);
            if masked == *text {
                return false;
            }
            *text = masked;
            true
        }
        // Written as loops rather than as `any`/`fold`: every element has to be
        // visited, and any short-circuiting form would stop masking the moment
        // it found its first match — leaving the rest of the array in the
        // clear. `|=` is the whole reason this is not one line.
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= mask_json(item, rules);
            }
            changed
        }
        serde_json::Value::Object(entries) => {
            let mut changed = false;
            for (_, item) in entries {
                changed |= mask_json(item, rules);
            }
            changed
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// SQL result sets.
// ---------------------------------------------------------------------------

/// Masks a batch of SQL result rows in place, by column name.
///
/// **SQL is the one surface where masking is approximate, and it says so.**
/// Every other path masks a [`MessageRecord`], where each string has a known
/// provenance; a result set is whatever columns the query projected, which may
/// be `value_text`, may be `substr(value_text, 1, 40)`, and may be a `count(*)`
/// that never touched a payload. So:
///
/// - The four columns of the `messages` table whose provenance IS known are
///   masked as that part of a record — `key_text` as a key, `headers_json` as
///   headers, `value_text`/`value_json` as a value — so a rule scoped to keys
///   behaves in a result set exactly as it does in a message grid.
/// - **Every other column is masked with every enabled rule**, whatever each
///   rule is scoped to. Nothing can know which part of the record
///   `upper(key_text) || value_text` came from, and under-masking is the
///   failure that matters here: a rule that redacts a card number should redact
///   it wherever the query put it.
/// - Numbers, booleans and nulls are left alone, exactly as they are inside a
///   payload's JSON — see [`mask_json`] for why.
///
/// Nested values (a DataFusion `array_agg`, say) are walked to their strings.
///
/// **It lives here rather than in a front end** because it is a policy, and
/// both front ends run it: the desktop shell masks the rows it emits to its
/// webview, and the MCP server masks the rows it returns to an agent. Two
/// copies of this reasoning would be two answers to the same question.
pub fn mask_sql_rows(
    rules: &MaskSet,
    column_names: &[&str],
    rows: &mut [Vec<serde_json::Value>],
) -> bool {
    if rules.is_empty() {
        return false;
    }
    let fields: Vec<Option<MaskField>> = column_names
        .iter()
        .map(|name| match *name {
            "key_text" => Some(MaskField::Key),
            "headers_json" => Some(MaskField::Headers),
            "value_text" | "value_json" => Some(MaskField::Value),
            // Not a column of the `messages` table: a projection, an
            // expression, an aggregate. Provenance unknown, so every rule runs.
            _ => None,
        })
        .collect();

    let mut masked = false;
    for row in rows.iter_mut() {
        for (index, cell) in row.iter_mut().enumerate() {
            let field = fields.get(index).copied().flatten();
            masked |= mask_sql_cell(rules, cell, field);
        }
    }
    masked
}

/// One result cell. `None` for the field means "provenance unknown" — see
/// [`mask_sql_rows`] — and applies every enabled rule.
fn mask_sql_cell(rules: &MaskSet, cell: &mut serde_json::Value, field: Option<MaskField>) -> bool {
    match cell {
        serde_json::Value::String(text) => {
            let fields: &[MaskField] = match field {
                Some(MaskField::Key) => &[MaskField::Key],
                Some(MaskField::Headers) => &[MaskField::Headers],
                Some(MaskField::Value) => &[MaskField::Value],
                None => &[MaskField::Value, MaskField::Key, MaskField::Headers],
            };
            // Routed through `mask_payload` rather than a second regex loop:
            // rule ordering, `NoExpand` (so a `$1` in a replacement cannot put
            // the match back) and the empty-match refusal are all decisions
            // that must have exactly one implementation.
            let mut payload = DecodedPayload {
                encoding: crate::serdes::Encoding::Utf8,
                text: std::mem::take(text),
                json: None,
                raw_len: 0,
                truncated: false,
                schema: None,
                decoded_by: None,
            };
            let mut masked = false;
            for field in fields {
                masked |= rules.mask_payload(&mut payload, *field);
            }
            *text = payload.text;
            masked
        }
        serde_json::Value::Array(items) => {
            let mut masked = false;
            for item in items {
                masked |= mask_sql_cell(rules, item, field);
            }
            masked
        }
        serde_json::Value::Object(entries) => {
            let mut masked = false;
            for (_, item) in entries {
                masked |= mask_sql_cell(rules, item, field);
            }
            masked
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Validation.
// ---------------------------------------------------------------------------

/// Compiles one pattern under this module's limits, or explains what is wrong
/// with it and where.
///
/// Two refusals beyond "it does not parse":
///
/// - **A pattern that matches the empty string** (`a*`, `^`, `x?`). `replace_all`
///   would insert the replacement between every character of every payload,
///   which reads as a broken app rather than as a masked one — and the user
///   who typed `\d*` meant `\d+`.
/// - **A pattern whose automaton exceeds [`MAX_REGEX_BYTES`]**, refused here
///   with a sentence rather than at decode time with a stall.
pub fn compile_pattern(pattern: &str) -> std::result::Result<Regex, MaskRuleError> {
    if pattern.is_empty() {
        return Err(MaskRuleError {
            position: Some(0),
            message: "a masking rule needs a pattern".to_string(),
        });
    }
    let regex = RegexBuilder::new(pattern)
        .size_limit(MAX_REGEX_BYTES)
        .dfa_size_limit(MAX_REGEX_BYTES)
        .build()
        .map_err(|e| describe(pattern, &e))?;
    if regex.is_match("") {
        return Err(MaskRuleError {
            position: None,
            message: "this pattern matches the empty string, so it would mask between every \
                      character — `*` or `?` on the whole pattern is usually meant to be `+`"
                .to_string(),
        });
    }
    Ok(regex)
}

/// Turns a `regex::Error` into a position and a sentence.
///
/// `regex::Error`'s own `Display` is a multi-line caret drawing; it is exactly
/// right in a terminal and unusable under a form field. `regex_syntax`'s parser
/// is asked the same question and answers with a span, which is the number the
/// UI needs; when the failure was in compilation rather than parsing there is
/// no span to have, and the message goes out without one.
fn describe(pattern: &str, error: &regex::Error) -> MaskRuleError {
    let position = regex_syntax::ast::parse::Parser::new()
        .parse(pattern)
        .err()
        .map(|e| e.span().start.offset);
    let message = match error {
        regex::Error::CompiledTooBig(_) => {
            "this pattern is too large to compile — narrow it, or split it into two rules"
                .to_string()
        }
        other => {
            // The last line of the caret drawing is the sentence; the drawing
            // itself is what the position replaces.
            let text = other.to_string();
            text.lines()
                .rev()
                .find_map(|line| line.strip_prefix("error: "))
                .unwrap_or("this is not a valid regular expression")
                .to_string()
        }
    };
    MaskRuleError { position, message }
}

// ---------------------------------------------------------------------------
// The store: `masking.json` beside `alerts.json` and `profiles.json`.
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfileMasks {
    #[serde(default)]
    rules: Vec<MaskRule>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    kavka_masking: u32,
    #[serde(default)]
    profiles: BTreeMap<String, ProfileMasks>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            kavka_masking: MASKING_VERSION,
            profiles: BTreeMap::new(),
        }
    }
}

/// Masking rules, in `masking.json` beside `profiles.json`.
///
/// Same discipline as [`crate::alerts::AlertStore`] and
/// [`crate::profiles::ProfileStore`], for the same reasons: write-tmp-then-
/// rename so a crash cannot leave a truncated file, and a write lock so two IPC
/// commands cannot lose each other's updates in a read-modify-write.
///
/// **Beside the profile rather than in it**, like alert rules: a profile export
/// is a thing users mail to each other, and one person's redaction policy is
/// not portable — a rule naming an internal customer-id format would travel to
/// someone who cannot read it, and a rule someone relies on would silently not
/// travel with the connection it was written for. Keeping them out of the
/// export means the export stays what it says it is: how to reach a cluster.
pub struct MaskStore {
    dir: PathBuf,
    write_lock: std::sync::Mutex<()>,
}

impl MaskStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            write_lock: std::sync::Mutex::new(()),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("masking.json")
    }

    pub fn rules(&self, profile_id: &str) -> Result<Vec<MaskRule>> {
        Ok(self
            .read()?
            .profiles
            .remove(profile_id)
            .unwrap_or_default()
            .rules)
    }

    /// The compiled set for a session, plus any stored rule that would not
    /// compile — see [`MaskSet::compile`].
    pub fn mask_set(&self, profile_id: &str) -> Result<(MaskSet, Vec<String>)> {
        Ok(MaskSet::compile(&self.rules(profile_id)?))
    }

    /// Upserts a rule by id, so the editor's "save" is one call whether the
    /// rule is new or not.
    ///
    /// **The pattern is validated here**, before anything is written: a stored
    /// rule that cannot compile is a rule that silently does not mask, which is
    /// the worst possible failure for this feature — the user believes the
    /// screen is redacted.
    pub fn save_rule(&self, profile_id: &str, rule: MaskRule) -> Result<()> {
        compile_pattern(&rule.pattern)
            .map_err(|e| Error::Other(format!("that pattern won't work: {e}")))?;
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        let profile = document.profiles.entry(profile_id.to_string()).or_default();
        match profile.rules.iter_mut().find(|kept| kept.id == rule.id) {
            Some(slot) => *slot = rule,
            None => profile.rules.push(rule),
        }
        self.write(&document)
    }

    /// Idempotent: deleting a rule that is not there is not an error — the
    /// other window already did it.
    pub fn delete_rule(&self, profile_id: &str, rule_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        if let Some(profile) = document.profiles.get_mut(profile_id) {
            profile.rules.retain(|rule| rule.id != rule_id);
        }
        self.write(&document)
    }

    /// Turns one rule on or off, answering the state it ended in.
    ///
    /// A separate call from [`Self::save_rule`] because the status-bar toggle
    /// is a different action from the editor's save, and routing it through
    /// save would mean the toggle re-validating (and potentially refusing) a
    /// pattern the user is not editing.
    pub fn set_enabled(&self, profile_id: &str, rule_id: &str, enabled: bool) -> Result<bool> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        let rule = document
            .profiles
            .get_mut(profile_id)
            .and_then(|profile| profile.rules.iter_mut().find(|rule| rule.id == rule_id))
            .ok_or_else(|| {
                Error::Other(format!(
                    "there is no masking rule {rule_id} on this connection any more — it may have \
                     been deleted in another window"
                ))
            })?;
        rule.enabled = enabled;
        self.write(&document)?;
        Ok(enabled)
    }

    /// Everything one profile owns here, dropped with the profile — the same
    /// rule as alert rules and keychain entries: state that outlives the
    /// connection it describes is state nobody can find again.
    pub fn forget_profile(&self, profile_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        if document.profiles.remove(profile_id).is_none() {
            return Ok(());
        }
        self.write(&document)
    }

    fn read(&self) -> Result<Document> {
        let path = self.file();
        if !path.exists() {
            return Ok(Document::default());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| Error::Other(format!("reading {}: {e}", path.display())))?;
        let document: Document = serde_json::from_str(&raw)
            .map_err(|e| Error::Other(format!("parsing {}: {e}", path.display())))?;
        if document.kavka_masking != MASKING_VERSION {
            return Err(Error::Other(format!(
                "{} was written by a different Kavka (version {}); this build reads version \
                 {MASKING_VERSION}",
                path.display(),
                document.kavka_masking
            )));
        }
        Ok(document)
    }

    fn write(&self, document: &Document) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| Error::Other(format!("creating {}: {e}", self.dir.display())))?;
        let json = serde_json::to_vec_pretty(document)
            .map_err(|e| Error::Other(format!("serializing masking rules: {e}")))?;
        let tmp = self.dir.join("masking.json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| Error::Other(format!("writing {}: {e}", tmp.display())))?;
        fs::rename(&tmp, self.file())
            .map_err(|e| Error::Other(format!("replacing masking.json: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serdes::{decode, DEFAULT_MAX_VALUE_BYTES};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Scratch config dir, removed on drop — keeps store tests dependency-free.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kavka-masking-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn store(&self) -> MaskStore {
            MaskStore::new(self.0.clone())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rule(id: &str, pattern: &str, applies_to: MaskTarget) -> MaskRule {
        MaskRule {
            applies_to,
            ..MaskRule::new(id, format!("rule {id}"), pattern)
        }
    }

    fn set(rules: &[MaskRule]) -> MaskSet {
        let (set, refused) = MaskSet::compile(rules);
        assert!(refused.is_empty(), "{refused:?}");
        set
    }

    fn payload(bytes: &[u8]) -> DecodedPayload {
        decode(bytes, None, DEFAULT_MAX_VALUE_BYTES)
    }

    /// A dead letter whose four text fields all hold the same probe string, so
    /// one assertion covers all four.
    fn dlq_meta(text: &str) -> DlqMeta {
        DlqMeta {
            convention: crate::serdes::DlqConvention::Spring,
            original_topic: Some(text.to_string()),
            original_partition: Some(3),
            original_offset: Some(8412),
            exception_class: Some(text.to_string()),
            exception_message: Some(text.to_string()),
            stacktrace: Some(text.to_string()),
        }
    }

    /// A 16-digit card number, the canonical rule.
    const CARD: &str = r"\b\d{4}[ -]?\d{4}[ -]?\d{4}[ -]?\d{4}\b";
    const EMAIL: &str = r"[\w.+-]+@[\w-]+\.[\w.]+";

    #[test]
    fn a_rule_replaces_the_whole_match_in_plain_text() {
        let rules = set(&[rule("r1", CARD, MaskTarget::All)]);
        let mut decoded = payload(b"card 4111 1111 1111 1111 charged");

        assert!(rules.mask_payload(&mut decoded, MaskField::Value));
        assert_eq!(decoded.text, "card ••• charged");
        // Nothing of the match survives — not one digit.
        assert!(!decoded.text.contains('4'), "{}", decoded.text);
    }

    /// The recursion is the point: a secret three objects down inside an array
    /// is the shape a "mask the top-level fields" implementation misses.
    #[test]
    fn json_string_values_are_masked_at_every_depth() {
        let rules = set(&[rule("r1", EMAIL, MaskTarget::Value)]);
        let mut decoded = payload(
            br#"{"customer":{"contacts":[{"email":"a@b.com"},{"email":"c@d.org"}]},"n":7}"#,
        );

        assert!(rules.mask_payload(&mut decoded, MaskField::Value));
        let json = decoded.json.as_ref().expect("still JSON");
        assert_eq!(json["customer"]["contacts"][0]["email"], "•••");
        assert_eq!(json["customer"]["contacts"][1]["email"], "•••");
        // Numbers and keys are untouched.
        assert_eq!(json["n"], 7);
        assert!(json["customer"]["contacts"][0].get("email").is_some());
        // The display text is re-rendered from the masked tree, so the two
        // halves of the payload agree.
        assert!(!decoded.text.contains("a@b.com"), "{}", decoded.text);
        assert!(decoded.text.contains("•••"), "{}", decoded.text);
    }

    /// A key that is a valid JSON *document* still masks, and its text stays
    /// consistent with its tree — the key path is the one most likely to be
    /// forgotten.
    #[test]
    fn the_key_and_the_headers_are_maskable_separately_from_the_value() {
        let value_only = set(&[rule("r1", EMAIL, MaskTarget::Value)]);
        let key_only = set(&[rule("r1", EMAIL, MaskTarget::Key)]);

        let mut key = payload(b"a@b.com");
        assert!(!value_only.mask_payload(&mut key, MaskField::Key));
        assert_eq!(key.text, "a@b.com");
        assert!(key_only.mask_payload(&mut key, MaskField::Key));
        assert_eq!(key.text, "•••");

        let headers_only = set(&[rule("r1", EMAIL, MaskTarget::Headers)]);
        let mut header = crate::serdes::decode_header("from", Some(b"a@b.com"));
        assert!(!value_only.mask_header(&mut header));
        assert!(headers_only.mask_header(&mut header));
        assert_eq!(header.value.as_deref(), Some("•••"));
    }

    /// The matrix, in one table: every target against every field — **and the
    /// dead-letter copy of the headers**, which is the field a rule can
    /// otherwise walk straight past.
    ///
    /// The `dlq` row is not a fifth column here on purpose: it follows the
    /// Headers column exactly, because every string in a `DlqMeta` came out of
    /// a header. If the two ever disagree, one of them is leaking.
    #[test]
    fn a_rules_target_decides_which_fields_it_reads() {
        for (target, value, key, headers) in [
            (MaskTarget::All, true, true, true),
            (MaskTarget::Value, true, false, false),
            (MaskTarget::Key, false, true, false),
            (MaskTarget::Headers, false, false, true),
        ] {
            let rules = set(&[rule("r1", "secret", target)]);
            let mut record = MessageRecord {
                partition: 0,
                offset: 1,
                timestamp_ms: None,
                key: Some(payload(b"secret")),
                value: Some(payload(b"secret")),
                headers: vec![crate::serdes::decode_header("h", Some(b"secret"))],
                dlq: Some(dlq_meta("secret")),
                masked: false,
            };
            assert!(rules.mask_record(&mut record));
            assert_eq!(
                record.value.as_ref().unwrap().text == "•••",
                value,
                "{target:?} value"
            );
            assert_eq!(
                record.key.as_ref().unwrap().text == "•••",
                key,
                "{target:?} key"
            );
            assert_eq!(
                record.headers[0].value.as_deref() == Some("•••"),
                headers,
                "{target:?} headers"
            );
            let dlq = record.dlq.as_ref().unwrap();
            for (field, what) in [
                (&dlq.original_topic, "original_topic"),
                (&dlq.exception_class, "exception_class"),
                (&dlq.exception_message, "exception_message"),
                (&dlq.stacktrace, "stacktrace"),
            ] {
                assert_eq!(
                    field.as_deref() == Some("•••"),
                    headers,
                    "{target:?} dlq.{what}"
                );
            }
            // The two fields that are numbers and the one that is Kavka's own
            // word survive every rule, whatever it is scoped to.
            assert_eq!(dlq.original_partition, Some(3));
            assert_eq!(dlq.original_offset, Some(8412));
            assert_eq!(dlq.convention, crate::serdes::DlqConvention::Spring);
            assert!(record.masked, "{target:?} did not set the flag");
        }
    }

    /// The probe from the other end: the real path, headers in, `DlqMeta` out,
    /// and the secret must not survive in the copy.
    ///
    /// `dlq_inspect` reads the metadata off the headers, so this is exactly the
    /// record a browse builds — and the assertion is that masking the record
    /// leaves the secret in neither place.
    #[test]
    fn the_dlq_copy_of_a_masked_header_is_masked_too() {
        let rules = set(&[rule("r1", CARD, MaskTarget::Headers)]);
        let headers = vec![
            crate::serdes::decode_header("kafka_dlt-original-topic", Some(b"orders")),
            crate::serdes::decode_header(
                "kafka_dlt-exception-message",
                Some(b"card 4111 1111 1111 1111 was declined"),
            ),
            crate::serdes::decode_header(
                "kafka_dlt-exception-stacktrace",
                Some(b"at Pay.charge(4111 1111 1111 1111)"),
            ),
        ];
        let dlq = crate::serdes::dlq_inspect(&headers).expect("a Spring dead letter");
        let mut record = MessageRecord {
            partition: 0,
            offset: 1,
            timestamp_ms: None,
            key: None,
            value: Some(payload(b"{}")),
            headers,
            dlq: Some(dlq),
            masked: false,
        };

        assert!(rules.mask_record(&mut record));
        let raw = serde_json::to_string(&record).expect("a record serializes");
        assert!(
            !raw.contains("4111"),
            "the card number crossed IPC inside the dlq metadata: {raw}"
        );
        let dlq = record.dlq.as_ref().unwrap();
        assert_eq!(
            dlq.exception_message.as_deref(),
            Some("card ••• was declined")
        );
        assert_eq!(dlq.stacktrace.as_deref(), Some("at Pay.charge(•••)"));
        // The topic matched nothing, so it is untouched — masking rewrites
        // matches, not fields.
        assert_eq!(dlq.original_topic.as_deref(), Some("orders"));
    }

    /// The flag is the wire's half of the contract: a UI that sees `masked` can
    /// say so, and a record nothing matched must not claim to be masked.
    #[test]
    fn the_masked_flag_follows_whether_anything_actually_changed() {
        let rules = set(&[rule("r1", CARD, MaskTarget::All)]);
        let mut untouched = MessageRecord {
            partition: 0,
            offset: 1,
            timestamp_ms: None,
            key: None,
            value: Some(payload(b"nothing sensitive here")),
            headers: Vec::new(),
            dlq: None,
            masked: false,
        };
        assert!(!rules.mask_record(&mut untouched));
        assert!(!untouched.masked);

        // …and an empty set is free and changes nothing.
        let none = MaskSet::none();
        assert!(none.is_empty());
        assert!(!none.mask_record(&mut untouched));
        assert!(!untouched.masked);
    }

    /// A masked record that goes through masking again does not lose the flag.
    #[test]
    fn the_masked_flag_is_sticky() {
        let rules = set(&[rule("r1", CARD, MaskTarget::All)]);
        let mut record = MessageRecord {
            partition: 0,
            offset: 1,
            timestamp_ms: None,
            key: None,
            value: Some(payload(b"nothing here")),
            headers: Vec::new(),
            dlq: None,
            masked: true,
        };
        assert!(!rules.mask_record(&mut record));
        assert!(record.masked, "a second pass cleared the flag");
    }

    /// Capture-group syntax in a replacement is text, not a way to get the
    /// match back.
    #[test]
    fn a_replacement_never_expands_the_match() {
        let rules = set(&[MaskRule {
            replacement: "[$0-$1]".to_string(),
            ..rule("r1", r"(\d{4})-(\d{4})", MaskTarget::All)
        }]);
        let mut decoded = payload(b"pin 1234-5678 ok");
        assert!(rules.mask_payload(&mut decoded, MaskField::Value));
        assert_eq!(decoded.text, "pin [$0-$1] ok");
    }

    /// Rules compose in order, and a later rule sees the earlier one's output —
    /// so a broad rule cannot un-mask what a narrow one hid.
    #[test]
    fn rules_apply_in_order_and_compose() {
        let rules = set(&[
            MaskRule {
                replacement: "<card>".to_string(),
                ..rule("r1", CARD, MaskTarget::All)
            },
            rule("r2", "<card>", MaskTarget::All),
        ]);
        let mut decoded = payload(b"4111 1111 1111 1111");
        assert!(rules.mask_payload(&mut decoded, MaskField::Value));
        assert_eq!(decoded.text, "•••");
    }

    /// Unicode: a pattern over non-ASCII text matches on characters, and the
    /// replacement never splits one.
    #[test]
    fn unicode_is_matched_and_replaced_by_character() {
        let rules = set(&[MaskRule {
            replacement: "▒".to_string(),
            ..rule("r1", "Müller|東京", MaskTarget::All)
        }]);
        let mut decoded = payload("Frau Müller lives in 東京".as_bytes());
        assert!(rules.mask_payload(&mut decoded, MaskField::Value));
        assert_eq!(decoded.text, "Frau ▒ lives in ▒");

        // A character class over a non-ASCII range still lands on boundaries.
        let cyrillic = set(&[rule("r1", r"\p{Cyrillic}+", MaskTarget::All)]);
        let mut decoded = payload("код: секрет".as_bytes());
        assert!(cyrillic.mask_payload(&mut decoded, MaskField::Value));
        assert_eq!(decoded.text, "•••: •••");
    }

    /// A payload no decoder could read renders as hex, and a rule that matches
    /// the hex *rendering* masks it. Stated as a test because the alternative
    /// reading — "hex payloads are exempt" — would be a hole.
    #[test]
    fn a_hex_rendering_is_text_like_any_other() {
        let rules = set(&[rule("r1", "de ad be ef", MaskTarget::All)]);
        let mut decoded = payload(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(decoded.text, "de ad be ef");
        assert!(rules.mask_payload(&mut decoded, MaskField::Value));
        assert_eq!(decoded.text, "•••");
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    #[test]
    fn an_invalid_pattern_is_refused_with_a_position() {
        let error = compile_pattern("a(b").expect_err("unclosed group");
        assert_eq!(error.position, Some(1), "{error}");
        assert!(error.message.contains("unclosed group"), "{error}");
        assert!(error.to_string().contains("at character 1"), "{error}");

        let class = compile_pattern(r"ok[a-").expect_err("unclosed class");
        assert_eq!(class.position, Some(2), "{class}");
    }

    /// The pattern that would turn every payload into a wall of bullets.
    #[test]
    fn a_pattern_matching_the_empty_string_is_refused() {
        for pattern in [r"\d*", "x?", "^", "(a)?"] {
            let error = compile_pattern(pattern).expect_err(pattern);
            assert!(
                error.message.contains("matches the empty string"),
                "{pattern} -> {error}"
            );
        }
        // …and the fix for it is accepted.
        assert!(compile_pattern(r"\d+").is_ok());
    }

    #[test]
    fn an_empty_pattern_is_refused() {
        let error = compile_pattern("").expect_err("empty");
        assert_eq!(error.position, Some(0));
        assert!(error.message.contains("needs a pattern"), "{error}");
    }

    /// Saving is where an invalid pattern is refused — a stored rule that
    /// cannot compile is a rule that silently does not mask, and the user
    /// believes the screen is redacted.
    #[test]
    fn saving_an_invalid_rule_is_refused_and_writes_nothing() {
        let dir = TempDir::new();
        let store = dir.store();
        let error = store
            .save_rule("p1", rule("r1", "a(b", MaskTarget::All))
            .expect_err("invalid");
        assert!(error.to_string().contains("at character 1"), "{error}");
        assert!(store.rules("p1").unwrap().is_empty());
        assert!(!dir.0.join("masking.json").exists());
    }

    /// A rule stored by a future build (or by hand) that does not compile is
    /// skipped with its name, rather than taking the whole session down.
    #[test]
    fn a_stored_rule_that_cannot_compile_is_skipped_by_name() {
        let (set, refused) = MaskSet::compile(&[
            rule("good", CARD, MaskTarget::All),
            MaskRule {
                name: "hand-edited".to_string(),
                ..rule("bad", "a(b", MaskTarget::All)
            },
        ]);
        assert_eq!(set.len(), 1);
        assert_eq!(refused.len(), 1);
        assert!(refused[0].contains("\"hand-edited\""), "{refused:?}");
        assert!(refused[0].contains("at character 1"), "{refused:?}");
    }

    #[test]
    fn a_disabled_rule_is_not_compiled_at_all() {
        let (set, refused) = MaskSet::compile(&[MaskRule {
            enabled: false,
            ..rule("r1", "a(b", MaskTarget::All)
        }]);
        assert!(set.is_empty());
        assert!(refused.is_empty(), "a disabled rule is not an error");
    }

    // -----------------------------------------------------------------------
    // The two sentences
    // -----------------------------------------------------------------------

    /// The notice and the status line are the contract with the UI, so they are
    /// pinned here rather than retyped in TypeScript.
    #[test]
    fn the_notice_and_the_status_line_are_fixed() {
        assert_eq!(
            mask_notice(3),
            "Masked by Kavka — 3 masking rules applied. Some values here are not the values on \
             the topic."
        );
        assert_eq!(
            mask_notice(1),
            "Masked by Kavka — 1 masking rule applied. Some values here are not the values on the \
             topic."
        );
        assert!(mask_notice(2).starts_with(MASK_NOTICE_MARKER));
        assert_eq!(masking_status(3), "Masking on — 3 rules");
        assert_eq!(masking_status(1), "Masking on — 1 rule");
    }

    /// An export of a masked session carries the masked text **and** the
    /// notice. This is the tripwire for the promise in the module docs: the
    /// unmasked value is not on the webview side to export.
    #[test]
    fn an_export_of_a_masked_session_carries_the_masked_text_and_a_notice() {
        let rules = set(&[rule("r1", CARD, MaskTarget::All)]);
        let mut record = MessageRecord {
            partition: 3,
            offset: 8412,
            timestamp_ms: Some(1_700_000_000_000),
            key: None,
            value: Some(payload(br#"{"card":"4111 1111 1111 1111"}"#)),
            headers: Vec::new(),
            dlq: None,
            masked: false,
        };
        assert!(rules.mask_record(&mut record));

        // What an exporter serializes is the record it was handed.
        let exported = serde_json::json!({
            "_kavka_notice": mask_notice(rules.len()),
            "records": [record],
        });
        let text = serde_json::to_string(&exported).unwrap();
        assert!(text.contains(MASK_NOTICE_MARKER), "{text}");
        assert!(
            !text.contains("4111"),
            "the raw value reached the export: {text}"
        );
        assert!(text.contains("•••"), "{text}");
        assert!(text.contains("\"masked\":true"), "{text}");
    }

    // -----------------------------------------------------------------------
    // The store
    // -----------------------------------------------------------------------

    #[test]
    fn rules_round_trip_and_upsert_by_id() {
        let dir = TempDir::new();
        let store = dir.store();
        assert!(store.rules("p1").unwrap().is_empty());

        store
            .save_rule("p1", rule("r1", CARD, MaskTarget::All))
            .unwrap();
        store
            .save_rule("p1", rule("r2", EMAIL, MaskTarget::Value))
            .unwrap();
        assert_eq!(store.rules("p1").unwrap().len(), 2);

        // Same id, new pattern: one rule, updated.
        store
            .save_rule("p1", rule("r1", r"\d{3}-\d{2}-\d{4}", MaskTarget::Key))
            .unwrap();
        let rules = store.rules("p1").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].applies_to, MaskTarget::Key);

        // Another profile's rules are its own.
        assert!(store.rules("p2").unwrap().is_empty());
    }

    #[test]
    fn toggling_a_rule_survives_a_reload_and_names_a_missing_one() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .save_rule("p1", rule("r1", CARD, MaskTarget::All))
            .unwrap();

        assert!(!store.set_enabled("p1", "r1", false).unwrap());
        assert!(!store.rules("p1").unwrap()[0].enabled);
        let (set, _) = store.mask_set("p1").unwrap();
        assert!(set.is_empty(), "a disabled rule does not mask");

        assert!(store.set_enabled("p1", "r1", true).unwrap());
        let (set, _) = store.mask_set("p1").unwrap();
        assert_eq!(set.len(), 1);

        let error = store.set_enabled("p1", "nope", true).unwrap_err();
        assert!(error.to_string().contains("another window"), "{error}");
    }

    #[test]
    fn deleting_is_idempotent_and_a_profile_can_be_forgotten() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .save_rule("p1", rule("r1", CARD, MaskTarget::All))
            .unwrap();

        store.delete_rule("p1", "r1").unwrap();
        assert!(store.rules("p1").unwrap().is_empty());
        // Twice is not an error — the other window already did it.
        store.delete_rule("p1", "r1").unwrap();
        store.delete_rule("nope", "r1").unwrap();

        store
            .save_rule("p1", rule("r1", CARD, MaskTarget::All))
            .unwrap();
        store.forget_profile("p1").unwrap();
        assert!(store.rules("p1").unwrap().is_empty());
        store.forget_profile("p1").unwrap();
    }

    #[test]
    fn the_file_is_written_atomically_and_versioned() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .save_rule("p1", rule("r1", CARD, MaskTarget::All))
            .unwrap();

        let raw = fs::read_to_string(dir.0.join("masking.json")).unwrap();
        let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(document["kavka_masking"], MASKING_VERSION);
        assert_eq!(document["profiles"]["p1"]["rules"][0]["id"], "r1");
        assert!(
            !dir.0.join("masking.json.tmp").exists(),
            "the temporary file outlived the rename"
        );
    }

    #[test]
    fn a_file_from_another_version_is_refused_rather_than_half_read() {
        let dir = TempDir::new();
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(
            dir.0.join("masking.json"),
            r#"{"kavka_masking": 99, "profiles": {}}"#,
        )
        .unwrap();
        let error = dir.store().rules("p1").unwrap_err().to_string();
        assert!(error.contains("version 99"), "{error}");
        assert!(error.contains("version 1"), "{error}");
    }

    #[test]
    fn the_ipc_wire_form_is_fixed() {
        let raw = serde_json::json!({
            "id": "r1",
            "name": "Card numbers",
            "pattern": r"\d{16}",
            "replacement": "•••",
            "applies_to": "all",
            "enabled": true,
        });
        let rule: MaskRule = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(rule.applies_to, MaskTarget::All);
        assert_eq!(serde_json::to_value(&rule).unwrap(), raw);

        // The replacement defaults, so an editor may omit it.
        let terse: MaskRule = serde_json::from_value(serde_json::json!({
            "id": "r2",
            "name": "Emails",
            "pattern": EMAIL,
            "applies_to": "value",
            "enabled": false,
        }))
        .expect("parses without a replacement");
        assert_eq!(terse.replacement, DEFAULT_REPLACEMENT);

        for (target, name) in [
            (MaskTarget::Value, "value"),
            (MaskTarget::Key, "key"),
            (MaskTarget::Headers, "headers"),
            (MaskTarget::All, "all"),
        ] {
            assert_eq!(
                serde_json::to_value(target).unwrap(),
                serde_json::json!(name)
            );
        }

        let error = MaskRuleError {
            position: Some(4),
            message: "unclosed group".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({"position": 4, "message": "unclosed group"})
        );
    }
}
