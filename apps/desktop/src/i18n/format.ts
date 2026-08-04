/**
 * ICU-lite message formatting — the whole of Kavka's translation grammar.
 *
 * No dependencies. This file is pure: strings and params in, parts out. It
 * knows nothing about React, nothing about catalogs and nothing about which
 * language is current, which is what makes the grammar testable by calling it.
 *
 * THE GRAMMAR, in full. If it isn't here, it isn't supported — a translator
 * reading docs/I18N.md must never meet a construct this file can't parse.
 *
 *   {name}                       an interpolation. The value is stringified;
 *                                a number is grouped with Intl.NumberFormat
 *                                for the active locale (§7 rule 5), and a
 *                                React element is passed through untouched by
 *                                `tx` (see index.ts).
 *
 *   {name, number}               the same, said out loud.
 *
 *   {n, plural, one {# thing}    CLDR plural categories via Intl.PluralRules:
 *               other {# things}} zero one two few many other, plus exact
 *                                `=0` / `=1` selectors which win over the
 *                                category. `#` is the number, grouped.
 *
 *   {kind, select, prod {…}      an exact string match with an `other` arm.
 *                  other {…}}
 *
 *   {{  }}                       a literal brace. There is NO apostrophe
 *                                escaping: real ICU treats `'` as a quote
 *                                character, and this product's voice is full
 *                                of "Kavka couldn't", "you'll" and "doesn't".
 *                                A grammar that mangles the house style is
 *                                the wrong grammar.
 *
 * Anything malformed degrades to text rather than throwing. A translation is
 * data a stranger wrote; it must not be able to blank a dialog.
 */

/** Values a message can interpolate. Non-text values survive `tx` intact. */
export type Params = Record<string, unknown>;

/** A param that is not text — a React element, in practice. */
export interface OpaqueParam {
  readonly node: unknown;
}

export type MessagePart = string | OpaqueParam;

export type Warn = (message: string) => void;

export function isOpaque(part: MessagePart): part is OpaqueParam {
  return typeof part !== "string";
}

const NUMBERS = new Map<string, Intl.NumberFormat>();
const PLURALS = new Map<string, Intl.PluralRules>();

function numbers(locale: string): Intl.NumberFormat {
  let f = NUMBERS.get(locale);
  if (f === undefined) {
    // A bad tag would throw and take the render with it; English grouping is
    // wrong-but-readable, a blank screen is neither.
    try {
      f = new Intl.NumberFormat(locale);
    } catch {
      f = new Intl.NumberFormat("en");
    }
    NUMBERS.set(locale, f);
  }
  return f;
}

function plurals(locale: string): Intl.PluralRules {
  let r = PLURALS.get(locale);
  if (r === undefined) {
    try {
      r = new Intl.PluralRules(locale);
    } catch {
      r = new Intl.PluralRules("en");
    }
    PLURALS.set(locale, r);
  }
  return r;
}

/**
 * The balanced block starting at `i` (where `src[i]` is `{`), or null when it
 * never closes. Doubled braces balance each other, so an escape inside an
 * argument body is counted symmetrically and cannot unbalance the scan.
 */
function readBlock(src: string, i: number): { inner: string; end: number } | null {
  let depth = 0;
  for (let j = i; j < src.length; j += 1) {
    const c = src[j];
    if (c === "{") depth += 1;
    else if (c === "}") {
      depth -= 1;
      if (depth === 0) return { inner: src.slice(i + 1, j), end: j + 1 };
    }
  }
  return null;
}

/**
 * `one {…} other {…}` → `[["one", "…"], ["other", "…"]]`.
 *
 * Selectors are whatever precedes the brace, so `=0`, `one` and a `select`
 * arm named after an environment all parse the same way.
 */
function readOptions(src: string): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  let i = 0;
  while (i < src.length) {
    while (i < src.length && /\s/.test(src[i])) i += 1;
    if (i >= src.length) break;
    let j = i;
    while (j < src.length && src[j] !== "{" && !/\s/.test(src[j])) j += 1;
    const selector = src.slice(i, j);
    while (j < src.length && /\s/.test(src[j])) j += 1;
    if (src[j] !== "{") break;
    const block = readBlock(src, j);
    if (block === null) break;
    out.push([selector, block.inner]);
    i = block.end;
  }
  return out;
}

function pick(
  options: Array<[string, string]>,
  selector: string,
): string | undefined {
  const hit = options.find(([s]) => s === selector);
  if (hit !== undefined) return hit[1];
  return options.find(([s]) => s === "other")?.[1];
}

function emitArg(
  inner: string,
  params: Params,
  locale: string,
  hash: number | null,
  out: MessagePart[],
  warn: Warn,
): void {
  const comma = inner.indexOf(",");
  const name = (comma === -1 ? inner : inner.slice(0, comma)).trim();
  const value = params[name];

  if (comma === -1) {
    if (value === undefined || value === null) {
      warn(`no value for {${name}}`);
      return;
    }
    if (typeof value === "string") out.push(value);
    else if (typeof value === "number") out.push(numbers(locale).format(value));
    else if (typeof value === "boolean") out.push(String(value));
    else out.push({ node: value });
    return;
  }

  const rest = inner.slice(comma + 1);
  const secondComma = rest.indexOf(",");
  const kind = (secondComma === -1 ? rest : rest.slice(0, secondComma)).trim();
  const body = secondComma === -1 ? "" : rest.slice(secondComma + 1);

  if (kind === "number") {
    const n = Number(value);
    if (!Number.isFinite(n)) {
      warn(`{${name}, number} got ${String(value)}`);
      return;
    }
    out.push(numbers(locale).format(n));
    return;
  }

  if (kind === "plural") {
    const n = Number(value);
    if (!Number.isFinite(n)) {
      warn(`{${name}, plural} got ${String(value)}`);
      return;
    }
    const options = readOptions(body);
    const chosen =
      options.find(([s]) => s === `=${n}`)?.[1] ??
      pick(options, plurals(locale).select(n));
    if (chosen === undefined) {
      warn(`{${name}, plural} has no arm for ${n} and no "other"`);
      return;
    }
    // `#` inside the arm is THIS plural's number, so the recursion carries it.
    emit(chosen, params, locale, n, out, warn);
    return;
  }

  if (kind === "select") {
    const options = readOptions(body);
    const chosen = pick(options, String(value ?? ""));
    if (chosen === undefined) {
      warn(`{${name}, select} has no arm for ${String(value)} and no "other"`);
      return;
    }
    emit(chosen, params, locale, hash, out, warn);
    return;
  }

  warn(`unknown argument type "${kind}" in {${name}}`);
}

function emit(
  msg: string,
  params: Params,
  locale: string,
  hash: number | null,
  out: MessagePart[],
  warn: Warn,
): void {
  let text = "";
  let i = 0;
  const flush = () => {
    if (text.length > 0) {
      out.push(text);
      text = "";
    }
  };
  while (i < msg.length) {
    const c = msg[i];
    if (c === "{" && msg[i + 1] === "{") {
      text += "{";
      i += 2;
      continue;
    }
    if (c === "}" && msg[i + 1] === "}") {
      text += "}";
      i += 2;
      continue;
    }
    if (c === "#" && hash !== null) {
      text += numbers(locale).format(hash);
      i += 1;
      continue;
    }
    if (c !== "{") {
      text += c;
      i += 1;
      continue;
    }
    const block = readBlock(msg, i);
    if (block === null) {
      // Unbalanced. Show the rest verbatim rather than losing the sentence.
      warn("unbalanced { — the rest of the message is shown as written");
      text += msg.slice(i);
      break;
    }
    flush();
    emitArg(block.inner, params, locale, hash, out, warn);
    i = block.end;
  }
  flush();
}

/**
 * Format one message into its parts. Adjacent text collapses into one part;
 * every non-text param becomes its own `OpaqueParam`, which is what lets `tx`
 * put a `<code>` element inside a translated sentence without the translator
 * ever seeing markup.
 */
export function formatParts(
  msg: string,
  params: Params,
  locale: string,
  warn: Warn,
): MessagePart[] {
  const out: MessagePart[] = [];
  emit(msg, params, locale, null, out, warn);
  return out;
}
