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
 */

export type SeekMode = "earliest" | "latest" | "offset" | "timestamp";

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
  /** The bar's trailing actions: Fetch, Start, Stop. */
  children?: React.ReactNode;
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
  children,
}: SeekBarProps) {
  const message = (field: SeekField) =>
    error?.field === field ? (
      <span className="field-error" id={`${idPrefix}-${field}-error`}>
        {error.message}
      </span>
    ) : null;

  const described = (field: SeekField) =>
    error?.field === field ? `${idPrefix}-${field}-error` : undefined;

  return (
    <>
      {/* Plain-language modes; the Kafka word is never hidden — it is in the
          option text and in the hint under the control. */}
      <div className="seekbar" role="group" aria-label={label}>
        <label className="seekbar-field">
          <span className="seekbar-label">Read from</span>
          <select
            value={state.mode}
            disabled={disabled}
            onChange={(e) => onChange({ mode: e.target.value as SeekMode })}
          >
            <option value="latest">The newest messages</option>
            <option value="earliest">The beginning of the topic</option>
            <option value="offset">A specific offset</option>
            <option value="timestamp">A point in time</option>
          </select>
        </label>

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

      {message("count")}
      {message("offset")}
      {message("timestamp")}
      {message("filter")}
    </>
  );
}
