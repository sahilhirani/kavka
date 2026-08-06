import { useRef } from "react";
import type { PartitionDetail, SeekSpec } from "./api";
import { fromDatetimeLocal, toDatetimeLocal } from "./format";
import { Term } from "./Glossary";

/**
 * WHERE TO READ FROM — the seek bar, shared by the message browser and search.
 *
 * Extracted in Phase 2. Both views ask the same question ("start where, on
 * which partitions") and both send the same `SeekSpec`, so the parsing, the
 * validation messages and the plain-language option wording live once. The
 * state stays with the caller: the browser keeps its bar between fetches and
 * search keeps its own between runs, and neither should be able to move the
 * other's.
 *
 * Validation follows §5.3: a message is produced on SUBMIT, never on
 * keystroke, editing a field clears its own message, and a failed submit
 * focuses nothing here — the caller owns focus because the caller owns the
 * button that failed.
 *
 * ALL FOUR START POSITIONS ARE VISIBLE AT ONCE. This was a `<select>` until
 * the fidelity audit named it: a dropdown shows one of the four and gives a
 * beginner no reason to discover that reading from a timestamp is even
 * possible. The mockup draws a four-option segmented radiogroup for exactly
 * that teaching reason, and the sentence under it (`.seekbar-note`) is the
 * same idea again — it says what the chosen mode will actually do BEFORE the
 * user spends a fetch finding out.
 */

export type SeekMode = "earliest" | "latest" | "offset" | "timestamp";

/**
 * The order the segments are offered in, and the order the arrow keys walk.
 * Newest first because it is the answer to "what is happening right now",
 * which is why most people open this screen; the mockup orders it the same
 * way (`jackdaw.html:1713-1718`).
 */
export const SEEK_MODES: readonly SeekMode[] = [
  "latest",
  "earliest",
  "offset",
  "timestamp",
];

/** The word on each segment. Short, because four of them share one row. */
const MODE_LABEL: Record<SeekMode, string> = {
  latest: "Newest",
  earliest: "Oldest",
  offset: "An offset",
  timestamp: "A time",
};

/**
 * What the chosen mode will do, said before it is done.
 *
 * The BROWSER's wording, which is the default. A scan is a different promise —
 * it walks forward to the end of every partition rather than reading one
 * window — so SearchView passes its own set through `noteFor` rather than
 * letting this sentence be approximately true twice.
 */
export function browseSeekNote(mode: SeekMode): string {
  switch (mode) {
    case "latest":
      return "Starts at the newest message in each partition and walks backwards. Nothing produced after the fetch is in it — that is what live tail is for.";
    case "earliest":
      return "Starts at the oldest message Kafka still holds. Retention and compaction have already removed anything older, so this is not necessarily the first message ever written.";
    case "offset":
      return "Starts at the offset you type, in the one partition you pick. Offsets are per partition — the same number means a different message in each one.";
    case "timestamp":
      return "Kafka finds the first message written at or after this time, in each partition. A partition holding nothing that new is skipped, and it will be missing from the results rather than empty in them.";
  }
}

/** The same four answers for a scan, which reads forwards rather than back. */
export function scanSeekNote(mode: SeekMode): string {
  switch (mode) {
    case "latest":
      return "Scans the newest messages in each partition, the count below deciding how many. Anything older than that window is never read, so it can never match.";
    case "earliest":
      return "Scans from the oldest message Kafka still holds to the end of each partition — the widest scan this topic allows, and the slowest.";
    case "offset":
      return "Scans from the offset you type to the end of the one partition you pick. The other partitions are not read at all.";
    case "timestamp":
      return "Scans from the first message written at or after this time to the end of each partition. A partition holding nothing that new contributes nothing.";
  }
}

export type SeekField = "count" | "offset" | "timestamp" | "filter";

export interface SeekError {
  field: SeekField;
  message: string;
}

export interface SeekState {
  mode: SeekMode;
  /** Raw text, because "12x" has to survive long enough to be complained about. */
  count: string;
  partition: string;
  offset: string;
  /** `<input type="datetime-local">` value — local time, as typed. */
  when: string;
  /** Partition filter: `0, 3, 7`, or empty/"all" for every partition. */
  filter: string;
}

export function initialSeekState(
  mode: SeekMode = "latest",
  count = "100",
  sinceMs = 3_600_000,
): SeekState {
  return {
    mode,
    count,
    partition: "0",
    offset: "0",
    when: toDatetimeLocal(Date.now() - sinceMs),
    filter: "",
  };
}

/** `0, 3, 7` → [0,3,7]; empty or "all" → null (every partition). */
export function parsePartitionFilter(
  raw: string,
  known: Set<number>,
): { partitions: number[] | null } | { message: string } {
  const trimmed = raw.trim();
  if (trimmed.length === 0 || trimmed.toLowerCase() === "all")
    return { partitions: null };
  const out: number[] = [];
  for (const piece of trimmed.split(/[,\s]+/)) {
    if (piece.length === 0) continue;
    const n = Number.parseInt(piece, 10);
    if (!Number.isInteger(n) || n < 0 || String(n) !== piece)
      return {
        message: `“${piece}” isn't a partition number. List them with commas — e.g. 0, 3, 7`,
      };
    if (known.size > 0 && !known.has(n))
      return {
        message: `This topic has no partition ${n}. It has ${known.size}, numbered 0 to ${
          known.size - 1
        }.`,
      };
    if (!out.includes(n)) out.push(n);
  }
  return out.length === 0 ? { partitions: null } : { partitions: out };
}

export interface BuiltSeek {
  seek: SeekSpec;
  partitions: number[] | null;
  /** The validated count, capped. Only meaningful where the caller uses it. */
  count: number;
}

/**
 * Turn the bar's text into a `SeekSpec`, or into the one message that names
 * the fix. Pure — no React, no I/O — so the whole matrix can be checked by
 * calling it.
 *
 * `needCount` is for the browser, which spends the count on every mode as its
 * fetch limit. Search only needs it in `latest` mode, where it IS the seek.
 */
export function buildSeek(
  state: SeekState,
  known: Set<number>,
  opts: { maxCount?: number; needCount?: boolean } = {},
): BuiltSeek | SeekError {
  const parsedFilter = parsePartitionFilter(state.filter, known);
  if ("message" in parsedFilter)
    return { field: "filter", message: parsedFilter.message };

  const wantsCount = opts.needCount === true || state.mode === "latest";
  let capped = 0;
  if (wantsCount) {
    const n = Number.parseInt(state.count, 10);
    if (!Number.isFinite(n) || n < 1)
      return { field: "count", message: "Ask for at least one message." };
    capped = opts.maxCount === undefined ? n : Math.min(n, opts.maxCount);
  }

  let seek: SeekSpec;
  if (state.mode === "earliest") {
    seek = { kind: "earliest" };
  } else if (state.mode === "latest") {
    seek = { kind: "latest", last_n: capped };
  } else if (state.mode === "offset") {
    const p = Number.parseInt(state.partition, 10);
    const o = Number.parseInt(state.offset, 10);
    if (!Number.isInteger(p) || !known.has(p))
      return {
        field: "offset",
        message: `Pick a partition this topic has — 0 to ${
          Math.max(1, known.size) - 1
        }.`,
      };
    if (!Number.isInteger(o) || o < 0)
      return { field: "offset", message: "An offset counts from 0 — e.g. 8412" };
    seek = { kind: "offset", partition: p, offset: o };
  } else {
    const ms = fromDatetimeLocal(state.when);
    if (ms === null)
      return {
        field: "timestamp",
        message: "Pick a date and time to start from.",
      };
    seek = { kind: "timestamp", timestamp_ms: ms };
  }

  return { seek, partitions: parsedFilter.partitions, count: capped };
}

interface SeekBarProps {
  /** Namespaces the `aria-describedby` targets. `mv`, `sv`. */
  idPrefix: string;
  label: string;
  state: SeekState;
  onChange: (patch: Partial<SeekState>) => void;
  partitions: PartitionDetail[];
  error: SeekError | null;
  /** Editing a field clears its own message — the caller holds the error. */
  onClearError: (field: SeekField) => void;
  /** The count field, where the caller spends it. Hidden when it means nothing. */
  showCount?: boolean;
  countLabel?: string;
  countMax?: number;
  disabled?: boolean;
  /**
   * The sentence under the bar, per mode. Defaults to the browser's wording;
   * search passes `scanSeekNote`, because "walks backwards from the newest"
   * and "scans forwards to the end" are different promises.
   */
  noteFor?: (mode: SeekMode) => string;
  /** The bar's trailing actions: Fetch, Start, Stop. */
  children?: React.ReactNode;
}

/**
 * THE FOUR START POSITIONS, as a radiogroup rather than a dropdown.
 *
 * The keyboard contract is the one Settings' segmented controls already make
 * (`SettingsView.tsx`'s `useRovingRadio`): arrows and Home/End move selection
 * AND focus together, and only the checked segment is in the tab order, so the
 * group is one Tab stop rather than four. A `role="radiogroup"` announced as
 * "1 of 4" whose arrow keys do nothing is a promise broken on the first press.
 *
 * Selection follows focus, which is correct here for the same reason it is in
 * Settings: choosing a mode changes no data and reverses instantly — it only
 * changes which fields the bar offers and what the sentence under it says.
 */
function SeekModeSeg({
  value,
  onChange,
  disabled,
  labelledBy,
  describedBy,
}: {
  value: SeekMode;
  onChange: (next: SeekMode) => void;
  disabled: boolean;
  labelledBy: string;
  describedBy: string;
}) {
  const refs = useRef<Partial<Record<SeekMode, HTMLButtonElement | null>>>({});

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step =
      e.key === "ArrowRight" || e.key === "ArrowDown"
        ? 1
        : e.key === "ArrowLeft" || e.key === "ArrowUp"
          ? -1
          : 0;
    let next: SeekMode | undefined;
    if (step !== 0) {
      const from = SEEK_MODES.indexOf(value);
      next = SEEK_MODES[(from + step + SEEK_MODES.length) % SEEK_MODES.length];
    } else if (e.key === "Home") next = SEEK_MODES[0];
    else if (e.key === "End") next = SEEK_MODES[SEEK_MODES.length - 1];
    else return;
    if (next === undefined || disabled) return;
    e.preventDefault();
    onChange(next);
    refs.current[next]?.focus();
  };

  return (
    <div
      className="seg seekbar-seg"
      role="radiogroup"
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      onKeyDown={onKeyDown}
    >
      {SEEK_MODES.map((mode) => (
        <button
          key={mode}
          ref={(el) => {
            refs.current[mode] = el;
          }}
          type="button"
          role="radio"
          aria-checked={value === mode}
          tabIndex={value === mode ? 0 : -1}
          className="seg-btn"
          disabled={disabled}
          onClick={() => onChange(mode)}
        >
          {MODE_LABEL[mode]}
        </button>
      ))}
    </div>
  );
}

export default function SeekBar({
  idPrefix,
  label,
  state,
  onChange,
  partitions,
  error,
  onClearError,
  showCount = true,
  countLabel,
  countMax,
  disabled = false,
  noteFor = browseSeekNote,
  children,
}: SeekBarProps) {
  const noteId = `${idPrefix}-seek-note`;
  const labelId = `${idPrefix}-seek-label`;
  // `role="alert"`, because this is the one validation message in the app
  // that appears WITHOUT focus moving to the control it is about: the caller
  // owns the button that failed, and both callers leave the caret on it
  // (§5.3 puts focus on the offending control only where the form owns it).
  // Without a live region the message is silent for a screen reader —
  // SC 4.1.3. `aria-describedby` still carries it on the control itself, so
  // it is also there for anyone who tabs back into the field.
  const message = (field: SeekField) =>
    error?.field === field ? (
      <span
        className="field-error"
        id={`${idPrefix}-${field}-error`}
        role="alert"
      >
        {error.message}
      </span>
    ) : null;

  const described = (field: SeekField) =>
    error?.field === field ? `${idPrefix}-${field}-error` : undefined;

  return (
    <>
      {/* Plain-language modes; the Kafka word is never hidden — it is in the
          segment text and in the sentence under the control. All four are on
          screen at once, which is the whole point (see the file header). */}
      <div className="seekbar" role="group" aria-label={label}>
        <div className="seekbar-field">
          {/* The visible words ARE the group's accessible name (SC 2.5.3), so
              this is `aria-labelledby` and not a second, longer `aria-label`
              nobody can see. What the choice costs is in the note below,
              wired up as the group's description. */}
          <span className="seekbar-label" id={labelId}>
            Start from
          </span>
          <SeekModeSeg
            value={state.mode}
            disabled={disabled}
            labelledBy={labelId}
            describedBy={noteId}
            onChange={(mode) => onChange({ mode })}
          />
        </div>

        {state.mode === "offset" && (
          <>
            <label className="seekbar-field">
              <span className="seekbar-label">
                <Term name="partition">Partition</Term>
              </span>
              <select
                value={state.partition}
                disabled={disabled}
                onChange={(e) => onChange({ partition: e.target.value })}
              >
                {partitions.map((p) => (
                  <option key={p.partition} value={String(p.partition)}>
                    {p.partition}
                  </option>
                ))}
              </select>
            </label>
            <label className="seekbar-field">
              {/* The gloss for `offset` lives on the table's column head —
                  one gloss per term per view (§7). */}
              <span className="seekbar-label">Offset</span>
              <input
                type="number"
                min={0}
                step={1}
                className={`input-num${
                  error?.field === "offset" ? " input-invalid" : ""
                }`}
                value={state.offset}
                disabled={disabled}
                aria-invalid={error?.field === "offset" ? true : undefined}
                aria-describedby={described("offset")}
                onChange={(e) => {
                  onChange({ offset: e.target.value });
                  onClearError("offset");
                }}
              />
            </label>
          </>
        )}

        {state.mode === "timestamp" && (
          <label className="seekbar-field">
            <span className="seekbar-label">From</span>
            <input
              type="datetime-local"
              step={1}
              className={`input-time${
                error?.field === "timestamp" ? " input-invalid" : ""
              }`}
              value={state.when}
              disabled={disabled}
              aria-invalid={error?.field === "timestamp" ? true : undefined}
              aria-describedby={described("timestamp")}
              onChange={(e) => {
                onChange({ when: e.target.value });
                onClearError("timestamp");
              }}
            />
          </label>
        )}

        {showCount && (
          <label className="seekbar-field">
            <span className="seekbar-label">
              {countLabel ?? (state.mode === "latest" ? "How many" : "At most")}
            </span>
            <input
              type="number"
              min={1}
              max={countMax}
              step={1}
              className={`input-num${
                error?.field === "count" ? " input-invalid" : ""
              }`}
              value={state.count}
              disabled={disabled}
              aria-invalid={error?.field === "count" ? true : undefined}
              aria-describedby={described("count")}
              onChange={(e) => {
                onChange({ count: e.target.value });
                onClearError("count");
              }}
            />
          </label>
        )}

        <label className="seekbar-field seekbar-field-wide">
          <span className="seekbar-label">Partitions</span>
          <input
            type="text"
            className={`input-mono${
              error?.field === "filter" ? " input-invalid" : ""
            }`}
            value={state.filter}
            placeholder="all"
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            aria-invalid={error?.field === "filter" ? true : undefined}
            aria-describedby={described("filter")}
            onChange={(e) => {
              onChange({ filter: e.target.value });
              onClearError("filter");
            }}
          />
        </label>

        {children}
      </div>

      {/* The mockup's `#seekNote`, rewritten per mode. It is NOT a repeat of
          the segment's word — it is what that word costs: which partitions get
          read, what retention has already taken, and what a mode cannot see.
          Plain text and not a live region: it changes because the user just
          pressed the control it describes, and `aria-describedby` on the
          radiogroup means the new sentence is read out with the new
          selection. */}
      <p className="seekbar-note" id={noteId}>
        {noteFor(state.mode)}
      </p>

      {message("count")}
      {message("offset")}
      {message("timestamp")}
      {message("filter")}
    </>
  );
}
