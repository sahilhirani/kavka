import { useCallback, useRef, useState } from "react";
import {
  errorMessage,
  exportRecords,
  exportRows,
  saveDialog,
  type ExportFormat,
  type JsonValue,
  type MessageRecord,
  type SqlColumn,
} from "./api";
import { classifyError } from "./errors";
import { groupDigits } from "./format";
import type { ToastSpec } from "./Toast";

/**
 * EXPORT — the same button in the message browser and in search.
 *
 * The shell writes the file; this only picks the path. Two honesty rules ride
 * along, and they are the reason this is a component rather than three lines
 * at each call site:
 *
 *  - WHAT YOU EXPORT IS WHAT IS ON SCREEN. The records handed to the core are
 *    the ones in the table, so an export can never quietly contain more (or
 *    less) than the view it came from.
 *  - IF THE VIEW IS CAPPED, THE EXPORT SAYS SO — in the button's title before
 *    the click and in the toast after it. A file that looks complete but is not
 *    is the single worst thing this feature could produce.
 *
 * THERE ARE TWO WAYS A VIEW GETS CAPPED, and both of them end up here. A search
 * fills its result buffer and keeps matching past it; a live tail keeps a
 * rolling window and drops what falls out of the far end (or what it could not
 * hand to the window fast enough). The sentence differs because the reason
 * differs — "matched but not held" is not "seen and rolled off" — but the
 * obligation is identical, so `capped` carries which one it is.
 */

const FORMATS: ReadonlyArray<{ name: string; ext: ExportFormat }> = [
  { name: "CSV", ext: "csv" },
  { name: "JSON", ext: "json" },
  { name: "Newline-delimited JSON", ext: "ndjson" },
];

/** `orders.v2-2026-08-03.csv`. Kafka's own name rules make this path-safe. */
export function defaultExportName(topic: string, now = new Date()): string {
  const pad = (n: number) => n.toString().padStart(2, "0");
  const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  return `${topic}-${date}.csv`;
}

/**
 * The format comes from the extension the user actually chose, not from a
 * dropdown Kavka would then have to keep in sync with the dialog's own filter.
 * Anything unrecognised is written as CSV and the toast says which — silently
 * writing JSON into a `.txt` would be a file nobody can open twice.
 */
export function formatFromPath(path: string): ExportFormat {
  const lower = path.toLowerCase();
  if (lower.endsWith(".ndjson") || lower.endsWith(".jsonl")) return "ndjson";
  if (lower.endsWith(".json")) return "json";
  return "csv";
}

/**
 * The table is holding a slice of something bigger. `shown` is what goes into
 * the file; `total` is the honest number this view knows about — matches for a
 * search, records delivered for a tail.
 */
export interface CappedView {
  shown: number;
  total: number;
  kind: "search" | "tail" | "sql";
}

/**
 * A SQL result set on its way to a file. `columns` carries the header row, so
 * a query that matched nothing still exports its schema rather than an empty
 * file — see `exportRows` in api.ts, which the shell's `export_rows` command
 * serves with the same RFC 4180 quoting the message export uses.
 */
export interface ExportTable {
  columns: SqlColumn[];
  rows: JsonValue[][];
}

interface ExportButtonProps {
  /** The message list to write. Exactly one of this and `table` is set. */
  records?: MessageRecord[];
  /** The result set to write. Exactly one of this and `records` is set. */
  table?: ExportTable | null;
  topic: string;
  /** Set whenever the view is capped — see `CappedView`. */
  capped?: CappedView | null;
  push: (spec: ToastSpec) => void;
  /** Extra reason the control is unavailable — a running fetch, say. */
  disabledReason?: string;
}

/** The sentence the toast carries after a capped export. */
function cappedDetail(capped: CappedView, path: string): string {
  if (capped.kind === "search")
    return `${path} — these are the first ${groupDigits(
      capped.shown,
    )} matches Kavka is holding, of ${groupDigits(capped.total)} matched.`;
  if (capped.kind === "sql")
    return `${path} — the query was capped, so these ${groupDigits(
      capped.shown,
    )} rows are the answer over the ${groupDigits(
      capped.total,
    )} records it managed to read, not over the whole topic.`;
  return `${path} — these are the ${groupDigits(
    capped.shown,
  )} messages still on screen, of ${groupDigits(
    capped.total,
  )} the live tail has seen. A tail keeps a rolling window; the rest are gone from this view.`;
}

/** The same fact, in the button's title, before the click rather than after. */
function cappedReason(capped: CappedView): string {
  if (capped.kind === "search")
    return `Writes the ${groupDigits(capped.shown)} matches on screen — ${groupDigits(
      capped.total,
    )} matched in total`;
  if (capped.kind === "sql")
    return `Writes these ${groupDigits(
      capped.shown,
    )} rows — the query stopped at ${groupDigits(
      capped.total,
    )} records, so the answer covers those and no more`;
  return `Writes the ${groupDigits(
    capped.shown,
  )} messages on screen — the live tail has seen ${groupDigits(
    capped.total,
  )}, and keeps only the most recent`;
}

export default function ExportButton({
  records,
  table = null,
  topic,
  capped = null,
  push,
  disabledReason,
}: ExportButtonProps) {
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);

  const rows = table?.rows ?? null;
  const count = rows !== null ? rows.length : (records?.length ?? 0);
  const noun = rows !== null ? "row" : "message";

  const run = useCallback(async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const path = await saveDialog({
        title:
          table !== null
            ? `Export query results from ${topic}`
            : `Export messages from ${topic}`,
        defaultPath: defaultExportName(topic),
        filters: FORMATS.map((f) => ({ name: f.name, extensions: [f.ext] })),
      });
      // Cancelling is not an error and must raise nothing at all.
      if (path === null) return;
      const format = formatFromPath(path);
      if (table !== null) await exportRows(path, format, table.columns, table.rows);
      else await exportRecords(path, format, records ?? []);
      push({
        kind: "ok",
        title: `Exported ${groupDigits(count)} ${
          count === 1 ? noun : `${noun}s`
        } as ${format.toUpperCase()}`,
        detail: capped === null ? path : cappedDetail(capped, path),
        mono: capped === null,
      });
    } catch (err) {
      const { title, detail } = classifyError(errorMessage(err));
      // Danger, so it never auto-dismisses: a file the user believes exists
      // and does not is worse than one they know failed.
      push({ kind: "danger", title, detail });
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, [records, table, count, noun, topic, capped, push]);

  const nothing = count === 0;
  const reason = busy
    ? "Kavka is writing the file"
    : nothing
      ? "There is nothing on screen to export yet"
      : (disabledReason ??
        (capped === null
          ? `Write these ${groupDigits(count)} ${noun}s to a file`
          : cappedReason(capped)));

  return (
    <button
      type="button"
      className="btn"
      disabled={busy || nothing || disabledReason !== undefined}
      aria-busy={busy || undefined}
      title={reason}
      onClick={() => void run()}
    >
      <span className="btn-busy-slot" aria-hidden="true">
        {busy ? <span className="spinner" /> : null}
      </span>
      Export…
    </button>
  );
}
