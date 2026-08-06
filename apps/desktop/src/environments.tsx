/**
 * The environment registry, as one module-level store.
 *
 * WHY A STORE AND NOT A CONTEXT. Every guardrail in the app asks the same
 * question — *is this connection's environment protected?* — and it asks it
 * from about twenty components, most of them deep views that receive nothing
 * but a `ConnectionProfile`. Threading a provider through all of them would
 * put a prop on every intermediate component for a value none of them care
 * about, and the answer is genuinely global: there is one registry per
 * machine, not one per subtree. So this is the same shape `i18n/index.ts`
 * uses — a module-level snapshot plus `useSyncExternalStore` — and for the
 * same reason.
 *
 * IT STARTS FULL, NOT EMPTY. The initial snapshot is `DEFAULT_ENVIRONMENTS`,
 * so every surface renders correct colours and correct guardrails on the very
 * first paint, before the IPC has answered — and, more importantly, *if it
 * never answers*. A registry that starts empty would render every existing
 * connection as an unknown environment for one frame, which means prod would
 * briefly render unprotected. The guardrail must never be the thing that is
 * late.
 *
 * NOTHING HERE RUNS AT IMPORT TIME. `load()` is called once from `App`'s
 * effect. A module-level `await environmentsList()` would make the whole
 * bundle depend on a command being registered, which is exactly the coupling
 * this file exists to avoid.
 */

import { useSyncExternalStore } from "react";
import {
  DEFAULT_ENVIRONMENTS,
  ENV_COLORS,
  environmentsList,
  type EnvColor,
  type Environment,
  type EnvironmentDef,
} from "./api";

/**
 * Force a definition's colour into the closed set.
 *
 * The Rust side stores `color` as a `String`, so the union in `api.ts` is a
 * claim about the data rather than a guarantee from the compiler — and a
 * definition hand-edited into `environments.json`, or written by a newer build
 * with an eighth token, would otherwise reach CSS as a `data-env-color` no
 * rule matches and paint an env chip with no colour at all. Slate is the right
 * landing place: it is already what "Kavka doesn't know this one" looks like.
 */
function normalize(def: EnvironmentDef): EnvironmentDef {
  return ENV_COLORS.includes(def.color) ? def : { ...def, color: "slate" };
}

let snapshot: EnvironmentDef[] = [...DEFAULT_ENVIRONMENTS];
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The current registry. Reference-stable until something actually changes. */
export function getEnvironments(): EnvironmentDef[] {
  return snapshot;
}

/**
 * Replace the registry.
 *
 * Called by `load` and by the manager after a save or a delete, so the chip in
 * the sidebar recolours in the same tick as the one in the manager's list.
 */
export function setEnvironments(next: EnvironmentDef[]): void {
  snapshot = next;
  emit();
}

/**
 * Read the registry from the backend, keeping the defaults on failure.
 *
 * A failure here is not worth a banner: either the command is not registered
 * yet (the shell stage has not landed) or the file could not be read, and in
 * both cases the honest fallback is the three defaults — which is precisely
 * what the backend itself writes when `environments.json` is absent. Returns
 * whether it succeeded, for a caller that wants to know.
 */
export async function loadEnvironments(): Promise<boolean> {
  try {
    const defs = await environmentsList();
    // An EXPLICIT empty registry is honoured, not overruled. It is a state the
    // user can reach — delete the last definition through the manager, or hand
    // the app an `environments.json` holding `[]` — and it means exactly one
    // thing: nothing is defined, so every connection resolves unknown and no
    // guardrail is armed. Substituting the defaults here would make the desktop
    // app the only surface that disagrees: `kavka profiles list` and the MCP
    // server both read the same file and both answer all-unknown, and a
    // guardrail that is on in one window and off in the terminal is worse than
    // one that is honestly off in both. The fallback that DOES apply is the
    // `catch` below — a registry that could not be read is not a registry that
    // is empty.
    setEnvironments(defs.map(normalize));
    return true;
  } catch {
    return false;
  }
}

/** Case-insensitive name equality — the store's one uniqueness rule. */
export function sameEnvironmentName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/**
 * What an environment Kavka has never heard of looks like.
 *
 * Slate and unprotected: no colour claim, and — deliberately — no guardrail.
 * The alternative, "unknown means protected", sounds safer and is worse: it
 * would arm type-to-confirm on every connection the moment the registry failed
 * to load, teaching people to type past the friction. The name is kept exactly
 * as the profile spells it, because that is the string the user has to
 * recognise when the hint tells them to define it.
 */
export function unknownEnvironment(name: string): EnvironmentDef {
  return { name, color: "slate", protected: false };
}

/**
 * The definition for a profile's `environment` string, or the slate stand-in.
 *
 * Never throws and never returns null: a profile referencing a deleted or
 * never-defined environment is a state the UI renders, not an error it
 * reports.
 */
export function resolveEnvironment(
  name: string,
  defs: EnvironmentDef[] = snapshot,
): EnvironmentDef {
  return defs.find((d) => sameEnvironmentName(d.name, name)) ?? unknownEnvironment(name);
}

/** Whether the registry actually holds this name. Drives the "unknown" hint. */
export function isKnownEnvironment(
  name: string,
  defs: EnvironmentDef[] = snapshot,
): boolean {
  return defs.some((d) => sameEnvironmentName(d.name, name));
}

/**
 * Whether writing to a connection in this environment should be gated.
 *
 * THE ONE PREDICATE the whole guardrail hangs off. Every `environment ===
 * "prod"` in the app is now a call to this, so the rule changes in one place —
 * and so "prod" stops being a magic string that a company running `production`
 * or `PRD` silently fails to match.
 */
export function isProtectedEnvironment(
  name: string,
  defs: EnvironmentDef[] = snapshot,
): boolean {
  return resolveEnvironment(name, defs).protected;
}

/** The registry, re-rendering the caller when it changes. */
export function useEnvironments(): EnvironmentDef[] {
  return useSyncExternalStore(subscribe, getEnvironments, getEnvironments);
}

/** One profile's environment definition, resolved. */
export function useEnvironment(name: string): EnvironmentDef {
  const defs = useEnvironments();
  return resolveEnvironment(name, defs);
}

/**
 * The guardrail predicate as a hook — the deep views' entire interface to this
 * module. `const isProtected = useIsProtected(profile.environment);`
 *
 * Built on `isProtectedEnvironment` rather than repeating it, so there is
 * exactly one definition of "is this gated" and a React component and a plain
 * function can never disagree about it.
 */
export function useIsProtected(name: string): boolean {
  return isProtectedEnvironment(name, useEnvironments());
}

/**
 * The DOM attributes that carry an environment into CSS.
 *
 * `data-env` used to carry both meanings in one token, which is exactly what
 * stopped environments being user-definable: `[data-env="prod"]` is a name
 * check, and a name is now user data. The two attributes below are the split —
 * `data-env-color` is identity (chip, ledger rule, accents), and
 * `data-env-protected` is the guardrail (warm substrate, top wire, danger
 * damper). Spread onto `.app`, the editor `<form>`, transfer dialog bodies and
 * every chip.
 *
 * `protected` is written as the string `"true"` or left off entirely — never
 * `"false"` — so the CSS can key on the attribute's presence and so a
 * screenshot of the DOM says what it means.
 */
export interface EnvAttrs {
  "data-env-color": EnvColor;
  "data-env-protected"?: "true";
}

export function envAttrs(def: EnvironmentDef): EnvAttrs {
  return {
    "data-env-color": def.color,
    ...(def.protected ? ({ "data-env-protected": "true" } as const) : {}),
  };
}

/**
 * The word the forced-colors wire prints, or undefined outside a protected
 * environment.
 *
 * Uppercased rather than translated: this is the same string the CLI's refusal
 * and the window title carry, and a guardrail that reads differently in
 * different locales is two signals where the design specifies one (DESIGN.md
 * §6, §10).
 */
export function envWireLabel(def: EnvironmentDef): string | undefined {
  return def.protected ? def.name.toUpperCase() : undefined;
}

/**
 * THE ENVIRONMENT CHIP — identity, in one pill.
 *
 * It lived in `Sidebar.tsx` until the sidebar was deleted (DESIGN.md §5.1).
 * This is where it belongs: the module that already owns `resolveEnvironment`,
 * `envAttrs` and the protection predicate now also owns the one component that
 * renders them, so a chip can never disagree with the guardrail beside it.
 * That is also why this file is `.tsx` — no import specifier changed.
 *
 * The NAME IS NEVER TRANSLATED and never sentence-cased in the DOM: it is user
 * data now, and it is the same string as the `data-env-color` sibling, the
 * forced-colors wire label, the CLI's refusal and the window title (§6, §10).
 * The chip uppercases in CSS so the word stays readable to anyone reading the
 * DOM or copying it into a bug report.
 *
 * Colour is identity; the FILLED form is the guardrail. An unprotected
 * environment is a tint-on-tag, a protected one a solid badge — the treatment
 * prod had, re-keyed onto the flag rather than onto the name, so a company
 * whose production environment is called `PRD` gets the badge too.
 */
export function EnvChip({ env }: { env: Environment }) {
  const def = useEnvironment(env);
  return (
    <span className="env-chip" {...envAttrs(def)}>
      {def.name}
    </span>
  );
}

/**
 * THE PADLOCK — the GLYPH channel of the protected signal.
 *
 * §6 spells production three ways or it has spelled it twice: the warm ground,
 * this glyph, and the WORD beside it. It lives here rather than in
 * `EnvironmentsManager` (where it was first drawn) because three surfaces need
 * it now — the environments list, the switcher menu's rows and the Connections
 * screen's saved-cluster rows — and a guardrail drawn twice is a guardrail that
 * can come to disagree with itself.
 *
 * It inherits `currentColor` and sizes to the text it sits beside, so it is
 * legible on the filled protected badge and on a warm row in both themes
 * without a literal that would only be right in one of them. It is always
 * `aria-hidden`: every caller pairs it with the word, and a screen reader that
 * says "protected" twice per row is a screen reader being shouted at.
 */
export function PadLock() {
  return (
    <svg
      className="env-lock"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      aria-hidden="true"
      focusable="false"
    >
      <rect x="5" y="11" width="14" height="9" rx="2" />
      <path d="M8 11V8a4 4 0 0 1 8 0v3" />
    </svg>
  );
}
