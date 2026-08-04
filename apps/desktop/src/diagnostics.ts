/**
 * CRASH CAPTURE — the webview half.
 *
 * The Rust shell owns the panic hook and the files (see the "Diagnostics"
 * section of `src-tauri/src/lib.rs`). This is the other half of the same
 * feature: a React app that throws during render, or a promise nobody caught,
 * dies in the webview where no Rust panic hook can see it — and that is by far
 * the more common way a desktop app "just goes blank".
 *
 * THREE RULES, and they are the whole file:
 *
 * 1. **It is silent when diagnostics is off**, which is the shipped state. The
 *    command returns `false` and nothing is written. This module never decides
 *    that for itself and never caches the answer: the toggle can change while
 *    the app is running, and a cached "off" would quietly keep it off.
 * 2. **It never swallows an error.** Both handlers are additive listeners that
 *    do not call `preventDefault`, so the console still gets everything it
 *    would have got. A diagnostics feature that hides errors from a developer
 *    is a net loss.
 * 3. **It cannot itself throw.** Every call is fire-and-forget with the
 *    rejection swallowed: an IPC failure while recording a crash must not
 *    become a second crash, and there is nowhere useful to report it to.
 */

import { diagnosticsRecord } from "./api";

/** Longest message forwarded. The shell truncates too; this saves the IPC. */
const MAX_MESSAGE = 2000;

/**
 * How many entries one session may write.
 *
 * A render loop that throws on every frame would otherwise write a thousand
 * identical lines a second and rotate every genuinely useful file out of
 * existence within one second — the exact five-file history somebody turned
 * this on to keep. After the cap the app is still perfectly usable; it has just
 * stopped writing, and the first line is the one that mattered anyway.
 */
const MAX_PER_SESSION = 100;

let written = 0;
let installed = false;

function record(kind: "error" | "rejection" | "note", message: string): void {
  if (written >= MAX_PER_SESSION) return;
  written += 1;
  // Deliberately not awaited and deliberately swallowed — see rule 3.
  void diagnosticsRecord(kind, message.slice(0, MAX_MESSAGE)).catch(() => {
    /* nowhere to report a failure to report a failure */
  });
}

/** An Error's name, message and stack as one line, or whatever we were given. */
function describe(value: unknown): string {
  if (value instanceof Error) {
    // The stack usually already begins with "Name: message"; when it doesn't
    // (Safari), the prefix is what makes the line readable at all.
    return value.stack?.includes(value.message)
      ? value.stack
      : `${value.name}: ${value.message}\n${value.stack ?? "(no stack)"}`;
  }
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    // A value with a circular reference or a throwing getter. String() is the
    // last thing that can still say something about it.
    return String(value);
  }
}

/**
 * Installs the two global handlers. Idempotent — React 19's StrictMode mounts
 * the tree twice in development, and two listeners would double every entry.
 *
 * Called from `main.tsx` BEFORE the tree renders, so a throw in the very first
 * render is covered.
 */
export function installCrashCapture(): void {
  if (installed) return;
  installed = true;

  // `error` rather than `window.onerror`: assigning the property would stomp
  // on anything else that had claimed it, and React 19 reports uncaught render
  // errors through `reportError`, which dispatches exactly this event.
  window.addEventListener("error", (event) => {
    const where =
      event.filename !== "" ? ` (${event.filename}:${event.lineno}:${event.colno})` : "";
    record("error", `${describe(event.error ?? event.message)}${where}`);
  });

  window.addEventListener("unhandledrejection", (event) => {
    record("rejection", describe(event.reason));
  });
}

/**
 * Writes one line the app chose to record — used by the diagnostics panel to
 * mark the moment somebody turned it on and by nothing else.
 *
 * Exported rather than inlined so that every write in the app goes through the
 * same cap and the same swallowed rejection.
 */
export function noteToLog(message: string): void {
  record("note", message);
}
