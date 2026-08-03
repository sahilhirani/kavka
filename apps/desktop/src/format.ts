/**
 * Numbers and times, per docs/DESIGN.md §7 rule 5:
 *
 *   "Numbers are grouped and honest. Prose rounds: about 4.2M messages.
 *    Tables are exact: 4 218 907. Never round a lag figure someone is about
 *    to act on."
 *
 * So there are two functions and they are not interchangeable. `groupDigits`
 * is for cells; `approxCount` is for sentences. Pure, no React, no I/O.
 */

/**
 * U+202F NARROW NO-BREAK SPACE — the typographic digit separator. A normal
 * space would let `4 218 907` wrap mid-number at a column boundary.
 */
const GROUP_SEP = " ";

/** Exact, space-grouped: `4 218 907`. What every table cell uses. */
export function groupDigits(value: number): string {
  if (!Number.isFinite(value)) return "—";
  const negative = value < 0;
  const digits = Math.abs(Math.trunc(value)).toString();
  let out = "";
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += GROUP_SEP;
    out += digits[i];
  }
  return negative ? `-${out}` : out;
}

/**
 * Rounded, for prose: `about 4.2M`. Only ever used inside a sentence, and
 * never for a lag figure the user is about to act on.
 */
export function approxCount(value: number): string {
  if (!Number.isFinite(value)) return "an unknown number of";
  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  if (abs >= 1e9) return `${sign}${trimZero(abs / 1e9)}B`;
  if (abs >= 1e6) return `${sign}${trimZero(abs / 1e6)}M`;
  if (abs >= 10_000) return `${sign}${groupDigits(Math.round(abs / 1000))}k`;
  return groupDigits(value);
}

function trimZero(n: number): string {
  const s = n.toFixed(1);
  return s.endsWith(".0") ? s.slice(0, -2) : s;
}

function pad(n: number, width = 2): string {
  return n.toString().padStart(width, "0");
}

/** `14:02:11.482` — the message table's timestamp column. */
export function formatClock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(
    d.getSeconds(),
  )}.${pad(d.getMilliseconds(), 3)}`;
}

/** `2026-08-03 14:02:11.482` — the inspector header, where space allows. */
export function formatStamp(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(
    d.getDate(),
  )} ${formatClock(ms)}`;
}

/** Epoch millis → the value an `<input type="datetime-local" step="1">` wants. */
export function toDatetimeLocal(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/**
 * The inverse. A datetime-local string carries no offset, so it is parsed as
 * LOCAL time — which is what the user typed and what the picker showed them.
 */
export function fromDatetimeLocal(value: string): number | null {
  if (value.trim().length === 0) return null;
  const ms = new Date(value).getTime();
  return Number.isFinite(ms) ? ms : null;
}

/**
 * A duration the BROKER measured, in prose: `3s ago`, `4m ago`, `2h 10m ago`.
 *
 * Kafka reports quorum fetch ages as "how long ago", not as a timestamp, so
 * nothing here touches `Date.now()` — this machine's clock is not involved and
 * must not be, or a laptop with a skewed clock invents a replica outage.
 *
 * Under a second reads "just now" rather than "0s ago": a healthy replica
 * fetches every few hundred milliseconds, and a column of `0s ago` says
 * nothing while looking like a measurement.
 */
export function formatAge(ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  // A negative age is a clock disagreement inside the cluster, not a fact
  // about the future. Clamp rather than print `-2s ago`.
  const v = Math.max(0, ms);
  if (v < 1000) return "just now";
  if (v < 60_000) return `${Math.round(v / 1000)}s ago`;
  if (v < 3_600_000) return `${Math.floor(v / 60_000)}m ago`;
  const hours = Math.floor(v / 3_600_000);
  const minutes = Math.floor((v % 3_600_000) / 60_000);
  return minutes === 0 ? `${hours}h ago` : `${hours}h ${minutes}m ago`;
}

/** Bytes, for the payload-size guards: `1.4 MB`. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${trimZero(bytes / 1024)} KB`;
  return `${trimZero(bytes / (1024 * 1024))} MB`;
}
