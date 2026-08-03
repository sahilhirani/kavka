/**
 * Payload rendering: a JSON value → lines of classed tokens.
 *
 * The inspector paints those tokens with the `--syn-*` palette and puts a line
 * number in a `--gutter-w-code` gutter beside them — the ledger device at
 * document scale (docs/DESIGN.md §5.10). Pure and total: no React, no DOM, so
 * a payload that would crash the pretty-printer crashes here, in a function
 * that can be called with the offending bytes, and not inside a render.
 */

import type { DecodedPayload, JsonValue } from "./api";

export type SynClass = "key" | "string" | "number" | "atom" | "punct" | "plain";

export interface Tok {
  text: string;
  cls: SynClass;
}

/** One indent level. Two spaces — mono at 12px, four is a lot of gutter. */
const INDENT = "  ";

/**
 * Above this many characters Kavka does not pretty-print on its own: parsing
 * plus tokenising a megabyte blocks the webview's only thread, and DESIGN's
 * perf guard says to render raw text with a "Format JSON" action instead.
 */
export const PRETTY_LIMIT = 262144;

/** Above this, the inspector shows a prefix and offers the rest on demand. */
export const RENDER_LIMIT = 204800;

function push(lines: Tok[][], depth: number, toks: Tok[]): void {
  lines.push(
    depth > 0
      ? [{ text: INDENT.repeat(depth), cls: "plain" }, ...toks]
      : [...toks],
  );
}

function scalarTok(value: string | number | boolean | null): Tok {
  if (value === null) return { text: "null", cls: "atom" };
  switch (typeof value) {
    case "string":
      return { text: JSON.stringify(value), cls: "string" };
    case "number":
      return { text: Number.isFinite(value) ? String(value) : "null", cls: "number" };
    default:
      return { text: value ? "true" : "false", cls: "atom" };
  }
}

/**
 * Render one value at `depth`. `lead` is whatever must sit before it on its
 * opening line (an object key and its colon); `tail` is the comma that follows
 * its closing line.
 */
function render(
  value: JsonValue,
  depth: number,
  lead: Tok[],
  tail: string,
  lines: Tok[][],
): void {
  const suffix: Tok[] = tail ? [{ text: tail, cls: "punct" }] : [];

  if (Array.isArray(value)) {
    if (value.length === 0) {
      push(lines, depth, [...lead, { text: "[]", cls: "punct" }, ...suffix]);
      return;
    }
    push(lines, depth, [...lead, { text: "[", cls: "punct" }]);
    value.forEach((item, i) => {
      render(item, depth + 1, [], i === value.length - 1 ? "" : ",", lines);
    });
    push(lines, depth, [{ text: "]", cls: "punct" }, ...suffix]);
    return;
  }

  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value);
    if (keys.length === 0) {
      push(lines, depth, [...lead, { text: "{}", cls: "punct" }, ...suffix]);
      return;
    }
    push(lines, depth, [...lead, { text: "{", cls: "punct" }]);
    keys.forEach((key, i) => {
      render(
        value[key],
        depth + 1,
        [
          { text: JSON.stringify(key), cls: "key" },
          { text: ": ", cls: "punct" },
        ],
        i === keys.length - 1 ? "" : ",",
        lines,
      );
    });
    push(lines, depth, [{ text: "}", cls: "punct" }, ...suffix]);
    return;
  }

  push(lines, depth, [...lead, scalarTok(value), ...suffix]);
}

/** Pretty-print a decoded JSON value into classed lines. */
export function prettyJsonLines(value: JsonValue): Tok[][] {
  const lines: Tok[][] = [];
  render(value, 0, [], "", lines);
  return lines;
}

/** Plain text, one token per line — the Raw tab, and every non-JSON payload. */
export function plainLines(text: string): Tok[][] {
  return text
    .split("\n")
    .map((line) => [{ text: line.replace(/\r$/, ""), cls: "plain" as SynClass }]);
}

/**
 * The one-line preview a message table cell shows.
 *
 * Collapses whitespace so a pretty-printed payload doesn't render as one blank
 * cell, and hard-caps the length — a 256 KB string in a `text-overflow:
 * ellipsis` cell still costs a full layout pass per row.
 */
export function previewText(payload: DecodedPayload | null, max = 240): string {
  if (payload === null) return "";
  const flat = payload.text.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}

/**
 * How the inspector describes where a payload came from.
 * DESIGN §5.10: provenance is pinned at the bottom, and it never claims a
 * schema that did not match.
 */
export function provenance(payload: DecodedPayload): string {
  const schema = payload.schema;
  if (schema === null) {
    return payload.encoding === "json"
      ? "Read as JSON — these bytes parsed on their own, with no schema involved."
      : `Shown as ${encodingWord(payload.encoding)} — no schema matched these bytes.`;
  }
  const parts = [`Decoded as ${encodingWord(payload.encoding)}`];
  if (schema.subject !== null) {
    parts.push(
      schema.version !== null
        ? `${schema.subject} v${schema.version}`
        : schema.subject,
    );
  }
  parts.push(`id ${schema.schema_id}`);
  return parts.join(" · ");
}

export function encodingWord(encoding: string): string {
  switch (encoding) {
    case "json":
      return "JSON";
    case "utf8":
      return "plain text";
    case "avro":
      return "Avro";
    case "msgpack":
      return "MessagePack";
    case "cbor":
      return "CBOR";
    case "hex":
      return "raw bytes";
    default:
      return encoding;
  }
}
