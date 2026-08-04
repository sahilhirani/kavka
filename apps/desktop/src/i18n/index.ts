/**
 * Kavka's localization framework. Hand-rolled, no dependencies, ~200 lines.
 *
 * WHY HAND-ROLLED. The whole product is a single-binary desktop app whose
 * pitch is that nothing about your clusters leaves your machine; a runtime
 * that pulls a message-format compiler and a polyfill chain into the bundle to
 * do what `Intl.PluralRules` already does is not a trade this app should make.
 * The grammar it accepts is a strict subset of ICU (see `format.ts`), so a
 * catalog written for Kavka can be handed to a real ICU tool later without a
 * rewrite. That direction is the one that matters.
 *
 * THE SHAPE
 *
 *   const { t, tx, locale, setLocale } = useI18n();
 *   t("sidebar.title")                        → "Clusters"
 *   t("palette.goTo", { name: p.name })       → "Go to orders — local"
 *   tx("app.firstRun.example", {              → a ReactNode with the <code>
 *     example: <code>kafka-1.internal:9092</code>,   elements spliced into
 *     local: <code>localhost:9092</code>,            the translated sentence
 *   })
 *
 * `t` returns a string and is what an attribute (`title`, `placeholder`,
 * `aria-label`) takes. `tx` returns a ReactNode and is what a sentence with a
 * `<code>` or a `<strong>` in it takes — the element goes in as a PARAM, so
 * the translator never sees markup and never has to preserve word order that
 * a concatenation would have destroyed.
 *
 * THE IDENTITY RULE. `useI18n()` returns functions memoized on the locale, so
 * putting `t` in a `useMemo`/`useCallback` dependency list is both correct and
 * cheap: stable while the language is, and a new identity the moment it isn't.
 * A component that closes over `t` inside a memo MUST list it, or its labels
 * freeze in the old language. That is the one way to get this wrong.
 *
 * COVERAGE is the shell only. See docs/I18N.md.
 */

import {
  createElement,
  Fragment,
  useMemo,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { formatParts, type MessagePart, type Params } from "./format";
import en, { type Catalog, type MessageKey } from "./catalogs/en";
import de from "./catalogs/de";
import es from "./catalogs/es";
import fr from "./catalogs/fr";
import ja from "./catalogs/ja";
import ptBR from "./catalogs/pt-BR";

export type { MessageKey, Catalog } from "./catalogs/en";
export type { Params } from "./format";

export type Locale = "en" | "de" | "es" | "fr" | "ja" | "pt-BR";

export interface LocaleInfo {
  code: Locale;
  /** The language's own name for itself. Never translated — that is the point. */
  endonym: string;
  /** English name, for the docs and for anyone reading this file. */
  english: string;
  /**
   * True when the catalog came out of a machine and no native speaker has
   * been through it. Shown in the picker, because a user deciding whether to
   * trust a translation deserves to know. This flag is the contract.
   */
  machine: boolean;
}

/** Order is the order of the picker: English first, then alphabetical by endonym. */
export const LOCALES: readonly LocaleInfo[] = [
  { code: "en", endonym: "English", english: "English", machine: false },
  { code: "de", endonym: "Deutsch", english: "German", machine: true },
  { code: "es", endonym: "Español", english: "Spanish", machine: true },
  { code: "fr", endonym: "Français", english: "French", machine: true },
  {
    code: "pt-BR",
    endonym: "Português (Brasil)",
    english: "Portuguese (Brazil)",
    machine: true,
  },
  { code: "ja", endonym: "日本語", english: "Japanese", machine: true },
];

const CATALOGS: Record<Locale, Catalog> = {
  en,
  de,
  es,
  fr,
  ja,
  "pt-BR": ptBR,
};

/** Same namespace as `kavka.selectedProfileId` — one prefix for all app state. */
const STORAGE_KEY = "kavka.locale";

// localStorage throws in a browser with storage disabled, and a language
// preference is not worth a white screen. Same best-effort helpers as App's.
function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function lsSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* best-effort */
  }
}

function isLocale(value: string | null): value is Locale {
  return value !== null && LOCALES.some((l) => l.code === value);
}

/**
 * A saved choice always wins. Otherwise walk the browser's ordered preference
 * list, taking an exact tag first (`pt-BR`) and then the primary subtag
 * (`de-AT` → `de`, `pt-PT` → `pt-BR`). English is the floor.
 *
 * `pt-PT → pt-BR` is a deliberate approximation: Brazilian is the only
 * Portuguese catalog that exists, and a European Portuguese speaker reading
 * Brazilian is better served than one reading English. Both are one click
 * from the picker either way.
 */
function detect(): Locale {
  const saved = lsGet(STORAGE_KEY);
  if (isLocale(saved)) return saved;
  const nav = typeof navigator === "undefined" ? undefined : navigator;
  const tags: readonly string[] =
    nav?.languages && nav.languages.length > 0
      ? nav.languages
      : nav?.language
        ? [nav.language]
        : [];
  for (const tag of tags) {
    const lower = tag.toLowerCase();
    const exact = LOCALES.find((l) => l.code.toLowerCase() === lower);
    if (exact) return exact.code;
    const base = lower.split("-")[0];
    const byBase = LOCALES.find(
      (l) => l.code.toLowerCase().split("-")[0] === base,
    );
    if (byBase) return byBase.code;
  }
  return "en";
}

let current: Locale = detect();
const listeners = new Set<() => void>();

function applyDocumentLang(): void {
  // Screen readers pick pronunciation from this, and `:lang()` selectors and
  // hyphenation read it too. Set it here so no component has to remember.
  if (typeof document !== "undefined") document.documentElement.lang = current;
}
applyDocumentLang();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getLocale(): Locale {
  return current;
}

/** Persisted immediately: a language chosen once should survive the restart. */
export function setLocale(next: Locale): void {
  if (next === current) return;
  current = next;
  lsSet(STORAGE_KEY, next);
  applyDocumentLang();
  for (const listener of [...listeners]) listener();
}

// ── Missing-key reporting ──────────────────────────────────────────────────
// Once per message, in dev only. A warning that fires on every render is a
// warning nobody reads.
const warned = new Set<string>();

function devWarn(message: string): void {
  if (!import.meta.env.DEV) return;
  if (warned.has(message)) return;
  warned.add(message);
  console.warn(`[i18n] ${message}`);
}

function lookup(key: MessageKey, locale: Locale): string {
  const hit = CATALOGS[locale][key];
  if (typeof hit === "string") return hit;
  if (locale !== "en") devWarn(`${locale} has no "${key}" — using English`);
  const fallback: string | undefined = en[key];
  if (typeof fallback === "string") return fallback;
  // Only reachable if a catalog key is deleted from `en` while a call site
  // survives, which the compiler catches first. Showing the key beats blank.
  devWarn(`no message anywhere for "${key}"`);
  return key;
}

function resolve(
  key: MessageKey,
  locale: Locale,
  params: Params,
): MessagePart[] {
  return formatParts(lookup(key, locale), params, locale, (problem) =>
    devWarn(`${locale} "${key}": ${problem}`),
  );
}

export type TFunction = (key: MessageKey, params?: Params) => string;
export type TxFunction = (key: MessageKey, params?: Params) => ReactNode;

function makeT(locale: Locale): TFunction {
  return (key, params) => {
    let out = "";
    for (const part of resolve(key, locale, params ?? {})) {
      if (typeof part === "string") {
        out += part;
      } else {
        devWarn(`"${key}" got a non-text value — that call site needs tx()`);
        out += String(part.node);
      }
    }
    return out;
  };
}

function makeTx(locale: Locale): TxFunction {
  return (key, params) => {
    const parts = resolve(key, locale, params ?? {});
    // Spread, not an array: children passed as separate arguments need no
    // keys, and the alternative is a wrapper element around every sentence.
    return createElement(
      Fragment,
      null,
      ...parts.map((part) =>
        typeof part === "string" ? part : (part.node as ReactNode),
      ),
    );
  };
}

/**
 * Translate outside React — a module-level helper, a store, an error mapper.
 * Reads the locale at call time, so it is always current, but nothing
 * re-renders because of it. Inside a component use `useI18n`.
 */
export function t(key: MessageKey, params?: Params): string {
  return makeT(current)(key, params);
}

export interface I18n {
  locale: Locale;
  t: TFunction;
  tx: TxFunction;
  setLocale: (next: Locale) => void;
}

/**
 * Subscribe a component to the language. Re-renders on change, and hands back
 * `t`/`tx` whose identity changes with the locale — see THE IDENTITY RULE.
 */
export function useI18n(): I18n {
  const locale = useSyncExternalStore(subscribe, getLocale, getLocale);
  return useMemo(
    () => ({ locale, t: makeT(locale), tx: makeTx(locale), setLocale }),
    [locale],
  );
}

// ── Dev-time completeness audit ────────────────────────────────────────────
// English is complete by construction. Every other catalog is allowed to be
// short — it falls back — but a contributor should be told, once, at startup,
// exactly how short and where to look.
if (import.meta.env.DEV) {
  const keys = Object.keys(en) as MessageKey[];
  for (const info of LOCALES) {
    if (info.code === "en") continue;
    const catalog = CATALOGS[info.code];
    const missing = keys.filter((key) => catalog[key] === undefined);
    if (missing.length > 0) {
      console.warn(
        `[i18n] ${info.code} (${info.english}) has ${missing.length} of ${keys.length} keys missing — those render in English. First few: ${missing
          .slice(0, 5)
          .join(", ")}`,
      );
    }
  }
}
