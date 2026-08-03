/**
 * Bulk-template placeholders, re-implemented in TypeScript FOR PREVIEW ONLY.
 *
 * ⚠ THE CORE IS THE AUTHORITY. `kavka-core` (crates/kavka-core/src/produce.rs,
 * `Template`) renders every message that is actually produced; this file exists
 * so the produce panel can show the first three rows before anything is sent,
 * which is the difference between "I think this template is right" and "I can
 * see it is". The two are expected to agree on SHAPE, never on VALUES:
 * `{{uuid}}`, `{{rand_int}}` and `{{choice}}` are random and `{{now_ms}}` is
 * read at send time, so a preview that matched the sent message exactly would
 * mean one of the two was lying about being random.
 *
 * # The rules, which are the core's rules
 *
 * They are restated here because this file used to hold its own approximation
 * of them, and every difference was a preview that quietly disagreed with what
 * the broker would receive:
 *
 *  1. A placeholder is `{{`, a LOWERCASE IDENTIFIER (`[a-z][a-z0-9_]*`),
 *     optional arguments, `}}`. Anything else between doubled braces is
 *     literal text — which is what keeps a JSON body like `{{"nested": 1}}`
 *     working instead of being read as a broken placeholder.
 *  2. `{{choice a|b|c}}` TRIMS its options and KEEPS THE EMPTY ONES, so
 *     `{{choice a||b}}` is a one-in-three chance of an empty string. Dropping
 *     empties would make the preview show a distribution the run does not have.
 *  3. `{{rand_int A B}}` takes exactly two whole numbers, low first. Reversed
 *     bounds are an ERROR in the core, not a silent swap — so here they leave
 *     the token exactly as written, and `unknownPlaceholders` names it.
 *
 * # What a rejected placeholder looks like here
 *
 * Anything the core would REFUSE is left ON SCREEN VERBATIM and reported by
 * `unknownPlaceholders`, because refusal is the core's actual behaviour: an
 * unrecognised or malformed placeholder fails `Template::parse`, which happens
 * in `BulkSession::start`, so the run never begins. (A typo'd `{{sq}}` reaching
 * 100 000 records is the failure that rule exists to prevent.) The panel says
 * so in those words.
 *
 * The one core rule this file cannot reproduce is the UNCLOSED `{{`: with no
 * closing braces there is no token to name, so the text is previewed as written
 * and the core's own message ("unclosed {{ at position 6") is what the user
 * gets when they press Send. Everything else is checked by the parity table at
 * the bottom of this file, which runs in dev.
 *
 * Pure and total: no React, no DOM. `now` and `rand` are injected so a preview
 * can be made deterministic — which is what makes that table possible.
 */

export const PLACEHOLDERS: ReadonlyArray<{
  syntax: string;
  what: string;
  example: string;
}> = [
  { syntax: "{{seq}}", what: "The message's number in this run, from 1.", example: "1" },
  { syntax: "{{uuid}}", what: "A fresh random UUID for every message.", example: "6f1e…" },
  { syntax: "{{now_iso}}", what: "Send time, ISO-8601 UTC.", example: "2026-08-03T12:04:19.482Z" },
  { syntax: "{{now_ms}}", what: "Send time, epoch milliseconds.", example: "1785499459482" },
  {
    syntax: "{{rand_int 1 100}}",
    what: "A whole number between the two bounds, inclusive — low one first.",
    example: "42",
  },
  {
    syntax: "{{choice new|paid|shipped}}",
    what: "One of the pipe-separated options, at random. Options are trimmed, and an empty one counts.",
    example: "paid",
  },
];

/** One `{{…}}` the scanner recognised as a placeholder attempt. */
interface Token {
  /** The source text of the whole `{{…}}`, for anything Kavka would refuse. */
  raw: string;
  /** The text between the braces, trimmed — what the core parses. */
  body: string;
}

type Piece = { literal: string; token?: undefined } | { literal?: undefined; token: Token };

/**
 * Whether text between doubled braces looks like a placeholder AT ALL — the
 * core's `opens_a_placeholder`, character for character.
 *
 * This is the rule that keeps `{{"nested": 1}}` and `{{1}}` literal JSON while
 * still catching `{{sq}}` as a typo. Everything Kavka knows is lowercase, so
 * nothing legitimate is excluded by it.
 *
 * One honest asterisk on "character for character": JS `\s`/`trim()` and
 * Rust `char::is_whitespace` are different sets (NEL, BOM, and friends), so
 * a placeholder containing exotic invisible characters can preview
 * differently than the core renders it. The core is always the authority —
 * a divergence there costs a confusing preview, never a wrong record.
 */
function opensAPlaceholder(inner: string): boolean {
  const name = inner.trim().split(/\s+/)[0] ?? "";
  return /^[a-z][a-z0-9_]*$/.test(name);
}

/**
 * Splits a template into literals and placeholder tokens, following the core's
 * parse loop rather than a regex: the loop is what implements rule 1 above, and
 * a regex over `\{\{([^{}]*)\}\}` cannot express "…unless it doesn't look like
 * a placeholder, in which case consume ONE brace and try again".
 */
function scan(source: string): Piece[] {
  const pieces: Piece[] = [];
  let literal = "";
  let at = 0;

  const flush = () => {
    if (literal.length > 0) {
      pieces.push({ literal });
      literal = "";
    }
  };

  while (at < source.length) {
    if (!source.startsWith("{{", at)) {
      // By code point, not by UTF-16 unit: slicing a surrogate pair in half
      // would corrupt the first template anyone writes with an emoji in it.
      const next = String.fromCodePoint(source.codePointAt(at) as number);
      literal += next;
      at += next.length;
      continue;
    }
    const close = source.indexOf("}}", at + 2);
    if (close === -1) {
      // Unclosed. The core refuses; the preview can only show what is written.
      literal += "{";
      at += 1;
      continue;
    }
    const body = source.slice(at + 2, close).trim();
    if (!opensAPlaceholder(body)) {
      literal += "{";
      at += 1;
      continue;
    }
    flush();
    pieces.push({ token: { raw: source.slice(at, close + 2), body } });
    at = close + 2;
  }
  flush();
  return pieces;
}

/** Rust's `str::parse::<i64>()`, near enough: sign, digits, nothing else. */
function wholeNumber(text: string): number | null {
  if (!/^[+-]?\d+$/.test(text)) return null;
  // Beyond i64::MAX the CORE refuses the template, so the preview must refuse
  // too or it promises a run that won't start. BigInt because the boundary is
  // not representable as a float. Between 2^53 and i64::MAX the core accepts
  // and this preview draws a rounded number — accepted divergence for bounds
  // nobody writes.
  const I64_MAX = 9223372036854775807n;
  const I64_MIN = -9223372036854775808n;
  let big: bigint;
  try {
    big = BigInt(text);
  } catch {
    return null;
  }
  if (big > I64_MAX || big < I64_MIN) return null;
  return Number(text);
}

interface Substitution {
  seq: number;
  now: number;
  rand: () => number;
}

/**
 * One placeholder's value, or `null` when the core would refuse the template
 * outright. `null` is the single decision behind both behaviours the panel
 * needs: leave the token verbatim, and name it.
 */
function substitute(body: string, { seq, now, rand }: Substitution): string | null {
  const split = body.search(/\s/);
  const name = split === -1 ? body : body.slice(0, split);
  const rest = split === -1 ? "" : body.slice(split).trim();

  switch (name) {
    case "seq":
      return rest === "" ? String(seq) : null;
    case "uuid":
      return rest === "" ? uuid(rand) : null;
    case "now_iso":
      return rest === "" ? new Date(now).toISOString() : null;
    case "now_ms":
      return rest === "" ? String(now) : null;
    case "rand_int": {
      const bounds = rest.split(/\s+/).filter((b) => b.length > 0);
      if (bounds.length !== 2) return null;
      const low = wholeNumber(bounds[0]);
      const high = wholeNumber(bounds[1]);
      if (low === null || high === null) return null;
      // Reversed bounds are an error in the core — never a silent swap.
      if (low > high) return null;
      return String(low + Math.floor(rand() * (high - low + 1)));
    }
    case "choice": {
      if (rest === "") return null;
      // Trimmed, and empties KEPT: `a||b` is three options in the core.
      const options = rest.split("|").map((option) => option.trim());
      return options[Math.floor(rand() * options.length)];
    }
    default:
      return null;
  }
}

function uuid(rand: () => number): string {
  // crypto.randomUUID is present in every webview Kavka supports, but this is
  // a preview: a fallback that never throws beats one that takes the panel
  // down on a platform nobody tested.
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === "function") return c.randomUUID();
  const hex = "0123456789abcdef";
  let out = "";
  for (let i = 0; i < 36; i += 1) {
    if (i === 8 || i === 13 || i === 18 || i === 23) out += "-";
    else if (i === 14) out += "4";
    else if (i === 19) out += hex[8 + Math.floor(rand() * 4)];
    else out += hex[Math.floor(rand() * 16)];
  }
  return out;
}

export interface RenderOptions {
  /** 1-based position in the run. */
  seq: number;
  now?: number;
  rand?: () => number;
}

/** Render one message body. Anything Kavka would refuse survives verbatim. */
export function renderTemplate(
  template: string,
  { seq, now = Date.now(), rand = Math.random }: RenderOptions,
): string {
  let out = "";
  for (const piece of scan(template)) {
    if (piece.token === undefined) {
      out += piece.literal;
      continue;
    }
    out += substitute(piece.token.body, { seq, now, rand }) ?? piece.token.raw;
  }
  return out;
}

/**
 * The placeholders Kavka will REFUSE, in the order they appear — a typo'd name,
 * arguments where none are allowed, `{{rand_int 5 1}}`, an empty `{{choice}}`.
 *
 * These are not "the preview doesn't know these": the core parses the template
 * in `BulkSession::start`, so every name in this list stops the run from
 * starting. The panel says exactly that.
 */
export function unknownPlaceholders(template: string): string[] {
  const out: string[] = [];
  for (const piece of scan(template)) {
    if (piece.token === undefined) continue;
    const body = piece.token.body;
    // The arguments are what make some of these invalid, so the check is a real
    // substitution rather than a name lookup. Values are irrelevant here.
    if (substitute(body, { seq: 1, now: 0, rand: () => 0 }) !== null) continue;
    if (!out.includes(body)) out.push(body);
  }
  return out;
}

/** The first `count` rendered bodies, for the live preview. */
export function previewRows(
  template: string,
  count: number,
  now = Date.now(),
): string[] {
  const rows: string[] = [];
  for (let i = 0; i < count; i += 1) {
    rows.push(renderTemplate(template, { seq: i + 1, now }));
  }
  return rows;
}

// ---------------------------------------------------------------------------
// THE PARITY TABLE
//
// The repo has no TS test runner (see apps/desktop/package.json), and the same
// problem — an assertion with nowhere to live — was already answered once, by
// `useRowHeightAssertion` in virtual.ts: check it in dev, and fail loudly in
// the console. This is that pattern, applied to the one file whose whole job is
// to agree with a Rust module nobody edits at the same time.
//
// Every row below is ported from `mod templates` in
// crates/kavka-core/src/produce.rs. If a row here fails, the preview and the
// producer disagree, and the preview is the one that is wrong.
// ---------------------------------------------------------------------------

/** The core's own test clock: 2023-11-14T22:13:20.000Z. */
const PARITY_CLOCK = 1_700_000_000_000;

/** A "random" source that always picks the middle option — deterministic. */
const HALF = () => 0.5;

interface ParityRow {
  what: string;
  got: () => string;
  want: string;
}

function parityRows(): ParityRow[] {
  const render = (source: string, seq = 1, rand: () => number = HALF) =>
    renderTemplate(source, { seq, now: PARITY_CLOCK, rand });

  const rows: ParityRow[] = [
    // --- what renders (produce.rs: seq_counts_from_one, the clock, whitespace)
    { what: "plain text", got: () => render("plain text"), want: "plain text" },
    { what: "empty template", got: () => render(""), want: "" },
    { what: "{{seq}}", got: () => render("order-{{seq}}", 1), want: "order-1" },
    { what: "{{seq}} at 500", got: () => render("order-{{seq}}", 500), want: "order-500" },
    { what: "{{seq}} twice is one number", got: () => render("{{seq}}/{{seq}}", 7), want: "7/7" },
    { what: "{{now_ms}}", got: () => render("{{now_ms}}"), want: String(PARITY_CLOCK) },
    {
      what: "{{now_iso}}",
      got: () => render("{{now_iso}}"),
      want: "2023-11-14T22:13:20.000Z",
    },
    { what: "whitespace inside {{ seq }}", got: () => render("{{  seq  }}", 3), want: "3" },
    {
      what: "whitespace inside {{ rand_int 4 4 }}",
      got: () => render("{{ rand_int  4   4 }}"),
      want: "4",
    },
    { what: "{{rand_int 5 5}} is a constant", got: () => render("{{rand_int 5 5}}"), want: "5" },
    {
      what: "{{rand_int 1 3}} lower bound",
      got: () => render("{{rand_int 1 3}}", 1, () => 0),
      want: "1",
    },
    {
      what: "{{rand_int 1 3}} upper bound is inclusive",
      got: () => render("{{rand_int 1 3}}", 1, () => 0.999999),
      want: "3",
    },
    {
      what: "{{rand_int -3 -1}} takes negatives",
      got: () => render("{{rand_int -3 -1}}", 1, () => 0),
      want: "-3",
    },
    { what: "{{choice only}}", got: () => render("{{choice only}}"), want: "only" },
    {
      what: "{{choice}} trims its options",
      got: () => render("{{choice  padded  }}"),
      want: "padded",
    },
    {
      what: "{{choice}} keeps empty options",
      got: () => render("{{choice a||b}}", 1, HALF),
      want: "",
    },
    {
      what: "{{choice}} picks the last option",
      got: () => render("{{choice a|b|c}}", 1, () => 0.999999),
      want: "c",
    },

    // --- doubled braces that are NOT placeholders stay literal
    ...[
      '{{"nested": 1}}',
      "{{ }}",
      "{{1}}",
      "{{UPPER}}",
      "a { b } c",
      "{{",
      '{"id": {{seq}}}',
    ].map((literal) => ({
      what: `literal ${literal}`,
      got: () => render(literal, 1),
      want: literal === '{"id": {{seq}}}' ? '{"id": 1}' : literal,
    })),

    // --- everything the core REFUSES is left verbatim (malformed_templates…)
    ...[
      "order-{{seq",
      "{{sq}}",
      "ok {{now}} ok",
      "{{seq 3}}",
      "{{uuid extra}}",
      "{{rand_int}}",
      "x{{rand_int 1}}",
      "{{rand_int 1 2 3}}",
      "{{rand_int a b}}",
      "{{rand_int 5 1}}",
      "{{choice}}",
    ].map((source) => ({
      what: `refused, so verbatim: ${source}`,
      got: () => render(source, 1),
      want: source,
    })),

    // --- and every refusal except the unclosed one is NAMED
    {
      what: "unknownPlaceholders names the refusals",
      got: () =>
        unknownPlaceholders(
          "{{sq}} {{seq}} {{rand_int 5 1}} {{choice}} {{uuid extra}} {{sq}}",
        ).join(","),
      want: "sq,rand_int 5 1,choice,uuid extra",
    },
    {
      what: "unknownPlaceholders leaves JSON alone",
      got: () => unknownPlaceholders('{{"nested": 1}} {{1}} {{UPPER}}').join(","),
      want: "",
    },
    {
      what: "unknownPlaceholders is quiet about a good template",
      got: () =>
        unknownPlaceholders(
          '{"id": {{seq}}, "u": "{{uuid}}", "n": {{rand_int 1 9}}, "s": "{{choice a|b}}", "t": "{{now_iso}}", "ms": {{now_ms}}}',
        ).join(","),
      want: "",
    },
  ];

  // A UUID is random by definition, so it is checked by shape (RFC 9562 §5.4),
  // exactly as the core's `uuids_are_valid_version_4_uuids` does.
  const drawn = render("{{uuid}}");
  const groups = drawn.split("-");
  rows.push(
    { what: "{{uuid}} length", got: () => String(drawn.length), want: "36" },
    {
      what: "{{uuid}} grouping",
      got: () => groups.map((g) => g.length).join("-"),
      want: "8-4-4-4-12",
    },
    { what: "{{uuid}} version nibble", got: () => groups[2]?.slice(0, 1) ?? "", want: "4" },
    {
      what: "{{uuid}} variant nibble",
      got: () => String(["8", "9", "a", "b"].includes(groups[3]?.slice(0, 1) ?? "")),
      want: "true",
    },
    {
      what: "{{uuid}} is hexadecimal",
      got: () => String(/^[0-9a-f-]{36}$/.test(drawn)),
      want: "true",
    },
  );
  return rows;
}

/**
 * Runs the parity table and returns what disagreed. Exported so it can be
 * called from a console, or from a test runner the day this repo grows one.
 */
export function templateParityFailures(): string[] {
  return parityRows()
    .filter((row) => row.got() !== row.want)
    .map((row) => `${row.what}: expected ${JSON.stringify(row.want)}`);
}

if (import.meta.env.DEV) {
  const failures = templateParityFailures();
  if (failures.length > 0) {
    console.error(
      "[kavka] the bulk-template preview no longer matches kavka-core's own rules — " +
        "crates/kavka-core/src/produce.rs is the authority, so the preview is what is wrong:\n" +
        failures.join("\n"),
    );
  }
}
