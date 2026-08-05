/**
 * UPDATES — the preference, the throttle, the dismissal, and the failure voice.
 *
 * Everything in this file is about RESTRAINT. Kavka now makes one network
 * request nobody asked for, and each of the four things below exists to keep a
 * promise the About panel, the README and the website all make in the same
 * words:
 *
 *   1. `UpdatePrefs.auto` — on by default, and one switch turns the request
 *      off completely. Not "off for now", not "off until the next release
 *      reminds you": off.
 *   2. `dueForCheck` — AT MOST ONCE A DAY. The check is throttled by a stamp
 *      on disk rather than by a timer in memory, so relaunching the app forty
 *      times in an afternoon is still one request.
 *   3. `isDismissed` — "Not now" means this version, not this launch. A notice
 *      that comes back every boot is a notice the user learns to click through
 *      without reading, which is how a real one gets missed.
 *   4. `updateFailure` — a failed check never renders as a cheerful verdict.
 *
 * NOTHING HERE TALKS TO THE NETWORK. `api.ts` has the four commands; the
 * decision about which release is newer lives in Rust with its unit tests
 * (`src-tauri/src/update.rs`). This module is the window's memory of what it
 * has already asked and what the user has already said about the answer.
 *
 * STORAGE IS BEST-EFFORT, like every other preference (see `storage.ts`). A
 * machine with localStorage disabled degrades to "auto on, stable channel,
 * never checked" — which means it checks once per launch instead of once per
 * day. That is the right way round: the failure mode of a broken preference
 * store is a slightly chattier app, never a silently disabled safety notice.
 */

import { useSyncExternalStore } from "react";
import { lsGet, lsRemove, lsSet } from "./storage";
import { classifyError, type ErrorCause } from "./errors";
import type { TFunction } from "./i18n";
import type { UpdateChannel, UpdateCheck } from "./api";

export type { UpdateChannel } from "./api";

export interface UpdatePrefs {
  /**
   * Whether Kavka asks github.com on its own. Default TRUE, and that default
   * is a considered choice rather than an industry habit: this app ships
   * unsigned installers off a Releases page, so a user who never hears that a
   * fix exists keeps running the bug. The honesty is in the disclosure beside
   * the switch, not in shipping the feature off.
   */
  auto: boolean;
  channel: UpdateChannel;
}

/** Picker order, and the order the roving radio arrows walk. */
export const UPDATE_CHANNELS: readonly UpdateChannel[] = ["stable", "builds"];

export const DEFAULT_UPDATE_PREFS: UpdatePrefs = {
  auto: true,
  channel: "stable",
};

/** Same `kavka.` namespace as every other preference on this machine. */
const PREFS_KEY = "kavka.updates";
const LAST_CHECK_KEY = "kavka.updates.lastCheck";
const DISMISSED_KEY = "kavka.updates.dismissed";

/** At most once a day — the number the disclosure sentence promises. */
export const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/**
 * How long after boot the launch check waits.
 *
 * Not zero, and not "when idle". The first seconds of a launch belong to
 * reading `profiles.json`, resolving the theme and connecting to whatever
 * cluster the user came back for; a release lookup is the least urgent thing
 * this app will do all session, so it goes last and it goes quietly.
 */
export const LAUNCH_CHECK_DELAY_MS = 5_000;

/**
 * A release Kavka has decided to offer, with the channel that decided it.
 *
 * Derived from the pinned IPC contract rather than restated, so an arm that
 * changes shape in `api.ts` is a compile error here instead of a banner that
 * renders `undefined`. The channel travels with the offer because `install`
 * has to be run against the same channel the check was: a build offered by the
 * builds channel is not reachable from the stable endpoint.
 */
export type UpdateOffer = Extract<UpdateCheck, { status: "update" }> & {
  channel: UpdateChannel;
};

// ── Preferences ────────────────────────────────────────────────────────────

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
 * Read the stored preference. Never throws, never returns a partial record: a
 * channel name left over from a future build degrades to `stable` without
 * costing the user the auto switch, and vice versa.
 *
 * A NON-BOOLEAN `auto` FALLS TO TRUE, which is the default. Deliberate: the
 * only way to be off is to have said so, and a corrupt record must not be a
 * way to silence the notice without anyone choosing that.
 */
export function readUpdatePrefs(): UpdatePrefs {
  const raw = lsGet(PREFS_KEY);
  if (raw === null) return DEFAULT_UPDATE_PREFS;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return DEFAULT_UPDATE_PREFS;
    const v = parsed as Partial<Record<keyof UpdatePrefs, unknown>>;
    return {
      auto: typeof v.auto === "boolean" ? v.auto : DEFAULT_UPDATE_PREFS.auto,
      channel: oneOf(v.channel, UPDATE_CHANNELS, DEFAULT_UPDATE_PREFS.channel),
    };
  } catch {
    return DEFAULT_UPDATE_PREFS;
  }
}

// The same module-level store `appearance.ts` and `i18n` use, for the same
// reason: this is one global fact and every subscriber must see the same one.
// The record is replaced rather than mutated so `useSyncExternalStore` has a
// stable snapshot to compare.
let current: UpdatePrefs = readUpdatePrefs();
const listeners = new Set<() => void>();

export function getUpdatePrefs(): UpdatePrefs {
  return current;
}

/** Persists immediately. A switch you flipped once must survive the restart. */
export function setUpdatePrefs(next: Partial<UpdatePrefs>): void {
  const merged: UpdatePrefs = { ...current, ...next };
  if (merged.auto === current.auto && merged.channel === current.channel) return;
  current = merged;
  lsSet(PREFS_KEY, JSON.stringify(merged));
  for (const listener of [...listeners]) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useUpdatePrefs(): UpdatePrefs {
  return useSyncExternalStore(subscribe, getUpdatePrefs, getUpdatePrefs);
}

// ── The throttle ───────────────────────────────────────────────────────────

/**
 * When Kavka last ASKED, and whether it got an answer.
 *
 * `ok` is stored because "last checked 3 minutes ago" over a failed request is
 * the small dishonesty that makes the big claim worthless — Settings says
 * "tried to check and couldn't reach github.com" instead. The throttle counts
 * attempts either way: a machine that is offline all week must not retry
 * every launch and call that "at most once a day".
 */
export interface LastCheck {
  at: number;
  ok: boolean;
}

export function readLastCheck(): LastCheck | null {
  const raw = lsGet(LAST_CHECK_KEY);
  if (raw === null) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return null;
    const v = parsed as Partial<Record<keyof LastCheck, unknown>>;
    if (typeof v.at !== "number" || !Number.isFinite(v.at)) return null;
    return { at: v.at, ok: v.ok === true };
  } catch {
    return null;
  }
}

export function noteChecked(ok: boolean, at: number = Date.now()): void {
  lsSet(LAST_CHECK_KEY, JSON.stringify({ at, ok }));
}

/**
 * Is a launch check due?
 *
 * A STAMP IN THE FUTURE IS TREATED AS NO STAMP. Wall-clock time on a desktop
 * moves backwards more often than anyone expects — a VM resumed from a
 * snapshot, a laptop whose clock was wrong until NTP corrected it, a user
 * fixing the date. Subtracting from a future stamp would park the check for
 * however long the jump was, which for a clock that was set to 2038 is
 * forever. A stamp that cannot have happened is not information.
 */
export function dueForCheck(now: number = Date.now()): boolean {
  const last = readLastCheck();
  if (last === null) return true;
  if (last.at > now) return true;
  return now - last.at >= CHECK_INTERVAL_MS;
}

// ── The dismissal ──────────────────────────────────────────────────────────
//
// ONE VERSION, not a set. Only the newest release is ever offered, so a
// history of everything the user has waved away is a list nothing will ever
// read again — and an unbounded key in localStorage that grows for the
// lifetime of the install. The stored string is the version the user last
// said "Not now" to; anything else gets its own notice.

export function isDismissed(version: string): boolean {
  return lsGet(DISMISSED_KEY) === version;
}

export function dismissVersion(version: string): void {
  lsSet(DISMISSED_KEY, version);
}

/**
 * Forget it. Called when the user presses "Check now" — they have just asked
 * the question out loud, so an old "Not now" must not be what answers them.
 */
export function clearDismissal(): void {
  lsRemove(DISMISSED_KEY);
}

// ── The failure voice ──────────────────────────────────────────────────────

/**
 * An update failure in the three layers DESIGN.md §7 requires of every
 * failure: what happened, the next click, and the raw text verbatim.
 *
 * WHY `classifyError` IS CONSULTED RATHER THAN OBEYED. That table is the
 * librdkafka table. Its rows name "that broker", and the row that catches a
 * string it does not recognise points the reader at "the broker's full reply"
 * — and an update check does not go anywhere near a broker. Rendering those
 * sentences over a github.com failure would put a false statement on the one
 * surface whose entire job is being true.
 *
 * So the CLASSIFICATION still happens there, in the one place that owns it,
 * and only the classification is used: when the library recognises a genuine
 * reachability failure the wording comes from the updates catalog, where it
 * can say github.com out loud and can be translated. Everything the library
 * does not recognise keeps the core's own sentence as the title — the IPC
 * contract says that string is already human-readable, and it is still the
 * most informative thing anyone has.
 */
export interface UpdateFailure {
  /** Line 1 — what happened. */
  title: string;
  /** Line 2 — the next click. Never "try again" on its own. */
  detail: string;
  /** Line 3 — verbatim, always reachable, never the title. */
  raw: string;
}

/** The `classifyError` rows that are about reaching a host rather than about Kafka. */
const UNREACHABLE: readonly ErrorCause[] = [
  "dns",
  "refused",
  "no-answer",
  "transport",
  "timeout",
];

/** First line only, capped — a stack trace is not a title. */
function firstLine(text: string): string {
  const line = text.split("\n")[0].trim();
  return line.length > 160 ? `${line.slice(0, 157)}…` : line;
}

/**
 * `t` is a PARAMETER rather than an import so this function stays pure and so
 * a component that calls it during render re-renders in the new language —
 * see THE IDENTITY RULE in `i18n/index.ts`.
 */
export function updateFailure(raw: string, t: TFunction): UpdateFailure {
  const text = (raw ?? "").trim();
  const { cause } = classifyError(text);
  if (UNREACHABLE.includes(cause)) {
    return {
      title: t("updates.error.unreachable.title"),
      detail: t("updates.error.unreachable.detail"),
      raw: text,
    };
  }
  return {
    title: text.length > 0 ? firstLine(text) : t("updates.error.title"),
    detail: t("updates.error.detail"),
    raw: text,
  };
}
