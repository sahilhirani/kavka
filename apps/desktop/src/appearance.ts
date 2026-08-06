/**
 * APPEARANCE — the theme runtime.
 *
 * One record, five independent axes, persisted as JSON under a single key. The
 * whole of the contract is that the resolved values live on `<html>` as data
 * attributes, because that is the only place a stylesheet, a pre-paint script
 * and a MutationObserver can all agree on:
 *
 *   data-theme="dark|light"            RESOLVED, never "system"
 *   data-density="comfortable|compact"
 *   data-accent="brass|moss|sky|plum"
 *   data-motion="system|reduce"
 *   data-fontsize="s|m|l"
 *
 * "system" is a PREFERENCE, not a value: it is resolved here through
 * `matchMedia` and re-resolved whenever the OS flips, so the attribute is
 * always one of the two real themes. A stylesheet that had to handle a third
 * theme value would need `:root:not([data-theme])` fallbacks on every token
 * block, and the one that got missed would be a half-themed control.
 *
 * THE PRE-PAINT DUPLICATE. `index.html` carries an inline copy of the read and
 * stamp below so the first paint is already correct. It is a duplicate on
 * purpose — a module import cannot run before first paint — and the two must
 * be changed together. The storage key, the attribute names and the defaults
 * are the shared surface; everything else here is React-side.
 *
 * `perch` IS THE ONE AXIS WITH NO ATTRIBUTE, and deliberately so. The other
 * five are answered by the stylesheet alone, which is why they have to be on
 * `<html>` before the first byte of CSS arrives. Perch visibility is answered
 * by `Perch.tsx`, because it is not absolute: a hidden Perch still renders
 * while a screen is LOADING and still renders when a read FAILED, and no
 * `[data-perch="hidden"] .perch { display: none }` can know which of those it
 * is looking at. So it is stored with the other five — one record, one key,
 * one Settings screen — and read in React. There is nothing for the pre-paint
 * script to stamp, and adding a sixth field to its DEFAULTS would be a
 * duplicate that does nothing.
 */

import { lsGet, lsSet } from "./storage";
import { useSyncExternalStore } from "react";

export type ThemePref = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type AccentId = "brass" | "moss" | "sky" | "plum";
export type Density = "comfortable" | "compact";
export type FontSize = "s" | "m" | "l";
export type MotionPref = "system" | "reduce";

/**
 * HOW MUCH OF THE PERCH TO DRAW — the mockup's Hide control, made durable.
 *
 * The mockup drew a "Hide" pill on every Perch and restored it with a "Show
 * the note for this screen" button; the app refused it, on the grounds that a
 * verdict the user can switch off is a verdict the app stops being accountable
 * for. Both are right about different halves, so the preference is three
 * states rather than a switch:
 *
 *   full    the Perch as designed — kicker, verdict, caveat. The default.
 *   line    one line: the kicker and the verdict, nothing wrapped. For
 *           somebody who has read the note on this screen forty times and
 *           still wants the state at a glance.
 *   hidden  no Perch on screens that have nothing to report.
 *
 * WHAT NONE OF THEM DO IS SUPPRESS A LOADING OR A FAILED READ. "Hidden" means
 * "do not tell me the cluster is fine"; it has never meant "do not tell me the
 * numbers are missing". `Perch.tsx` forces the full form whenever it is
 * loading or holding an error, in every mode, and the Hide control disappears
 * while it does — see the precedence block there. That is the whole reason
 * this is a React-side preference and not a CSS attribute.
 */
export type PerchMode = "full" | "line" | "hidden";

export interface Appearance {
  theme: ThemePref;
  accent: AccentId;
  density: Density;
  fontSize: FontSize;
  motion: MotionPref;
  perch: PerchMode;
}

/** Shared with the inline script in index.html. Changing it strands the file. */
export const APPEARANCE_KEY = "kavka.appearance";

/**
 * Jackdaw's own settings. Dark ground, brass accent, comfortable rows — the
 * direction as drawn. `theme: "system"` rather than `"dark"` because a desktop
 * app that ignores the OS on first launch is making a decision it was not
 * asked to make; the mockup's dark is what most machines resolve to anyway.
 */
export const DEFAULT_APPEARANCE: Appearance = {
  theme: "system",
  accent: "brass",
  density: "comfortable",
  fontSize: "m",
  motion: "system",
  // The direction as drawn. A first-run user has never seen a Perch and is
  // exactly the person its teaching half is for.
  perch: "full",
};

export const ACCENTS: readonly AccentId[] = ["brass", "moss", "sky", "plum"];
export const DENSITIES: readonly Density[] = ["comfortable", "compact"];
export const FONT_SIZES: readonly FontSize[] = ["s", "m", "l"];
export const THEMES: readonly ThemePref[] = ["system", "light", "dark"];
export const MOTIONS: readonly MotionPref[] = ["system", "reduce"];
/** Order is the order the Settings segmented control offers them. */
export const PERCH_MODES: readonly PerchMode[] = ["full", "line", "hidden"];

function oneOf<T extends string>(
  value: unknown,
  allowed: readonly T[],
  fallback: T,
): T {
  return typeof value === "string" && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

/**
 * Read the stored preference. Never throws and never returns a partial record:
 * an unknown accent left over from a future build, a hand-edited file, or no
 * storage at all all degrade field-by-field to the default, so one bad value
 * cannot cost the user the other four.
 */
export function readAppearance(): Appearance {
  const raw = lsGet(APPEARANCE_KEY);
  if (raw === null) return DEFAULT_APPEARANCE;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return DEFAULT_APPEARANCE;
    const v = parsed as Partial<Record<keyof Appearance, unknown>>;
    return {
      theme: oneOf(v.theme, THEMES, DEFAULT_APPEARANCE.theme),
      accent: oneOf(v.accent, ACCENTS, DEFAULT_APPEARANCE.accent),
      density: oneOf(v.density, DENSITIES, DEFAULT_APPEARANCE.density),
      fontSize: oneOf(v.fontSize, FONT_SIZES, DEFAULT_APPEARANCE.fontSize),
      motion: oneOf(v.motion, MOTIONS, DEFAULT_APPEARANCE.motion),
      // Absent in every record written before this axis existed, which reads
      // as "full" — the behaviour those users already have.
      perch: oneOf(v.perch, PERCH_MODES, DEFAULT_APPEARANCE.perch),
    };
  } catch {
    return DEFAULT_APPEARANCE;
  }
}

// The LIGHT side, not the dark one. `(prefers-color-scheme: dark)` fails to
// match in two different situations — the OS says light, and the OS says
// nothing — and those two must not resolve the same way. Asking the light
// question makes "no opinion" fall to dark, which is what the pre-paint script
// in index.html does; ask the dark question here and a machine with no opinion
// gets a dark first paint and a light second one.
const LIGHT_QUERY = "(prefers-color-scheme: light)";

function media(query: string): MediaQueryList | null {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return null;
  }
  try {
    return window.matchMedia(query);
  } catch {
    return null;
  }
}

/**
 * What "system" means right now. Dark when the OS says dark AND when it has no
 * opinion — Kavka's ground is dark, so an unknown answer should not flash a
 * user into a theme they never chose.
 */
export function systemTheme(): ResolvedTheme {
  const mq = media(LIGHT_QUERY);
  if (mq === null) return "dark";
  return mq.matches ? "light" : "dark";
}

export function resolveTheme(pref: ThemePref): ResolvedTheme {
  return pref === "system" ? systemTheme() : pref;
}

/**
 * Stamp the document. Also sets `<meta name="color-scheme">` to the RESOLVED
 * theme so form controls, scrollbars and the webview's own chrome — none of
 * which read our custom properties — follow the app instead of the OS.
 */
export function applyAppearance(a: Appearance): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const theme = resolveTheme(a.theme);
  root.setAttribute("data-theme", theme);
  root.setAttribute("data-accent", a.accent);
  root.setAttribute("data-density", a.density);
  root.setAttribute("data-fontsize", a.fontSize);
  root.setAttribute("data-motion", a.motion);
  // The tag is in index.html; create it only if a future edit drops it, so
  // this function is correct on its own terms rather than on a file's.
  let meta = document.querySelector<HTMLMetaElement>('meta[name="color-scheme"]');
  if (meta === null) {
    meta = document.createElement("meta");
    meta.name = "color-scheme";
    document.head.appendChild(meta);
  }
  meta.content = theme;
}

// ── The store ──────────────────────────────────────────────────────────────
//
// A module-level value plus a listener set, the same shape `i18n` uses, for
// the same reason: appearance is one global fact and every subscriber has to
// see the same one. `useSyncExternalStore` needs a stable snapshot, so the
// record is replaced rather than mutated.

let current: Appearance = readAppearance();
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of [...listeners]) listener();
}

export function getAppearance(): Appearance {
  return current;
}

/** Applies and persists immediately: a preference chosen once must survive a restart. */
export function setAppearance(next: Partial<Appearance>): void {
  const merged: Appearance = { ...current, ...next };
  if (
    merged.theme === current.theme &&
    merged.accent === current.accent &&
    merged.density === current.density &&
    merged.fontSize === current.fontSize &&
    merged.motion === current.motion &&
    merged.perch === current.perch
  ) {
    return;
  }
  current = merged;
  lsSet(APPEARANCE_KEY, JSON.stringify(merged));
  applyAppearance(merged);
  emit();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Install the runtime. Called once from `main.tsx`, before the first render.
 *
 * The OS listener is never removed: it is one listener for the lifetime of the
 * process, and the alternative — subscribing in a component — would stop
 * following the OS the moment that component unmounted.
 */
export function startAppearance(): void {
  applyAppearance(current);
  const mq = media(LIGHT_QUERY);
  if (mq === null) return;
  const onChange = () => {
    // Only "system" is listening. Re-stamping under an explicit preference
    // would be a no-op, but emitting would re-render the app for an event it
    // deliberately does not care about.
    if (current.theme !== "system") return;
    applyAppearance(current);
    emit();
  };
  if (typeof mq.addEventListener === "function") {
    mq.addEventListener("change", onChange);
  } else if (typeof (mq as MediaQueryList).addListener === "function") {
    // Safari 13 and the older WKWebView the macOS floor still includes.
    (mq as MediaQueryList).addListener(onChange);
  }
}

export interface AppearanceState {
  appearance: Appearance;
  /** What `data-theme` actually says — "system" resolved. */
  theme: ResolvedTheme;
  set: (next: Partial<Appearance>) => void;
}

export function useAppearance(): AppearanceState {
  const appearance = useSyncExternalStore(subscribe, getAppearance, getAppearance);
  return {
    appearance,
    theme: resolveTheme(appearance.theme),
    set: setAppearance,
  };
}
