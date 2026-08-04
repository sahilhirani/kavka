import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  SQL_MAX_ROWS,
  SQL_SCAN_CAP,
  SQL_SURFACE,
  SQL_TABLE_COLUMNS,
  SQL_TABLE_NAME,
  errorMessage,
  sqlStart,
  sqlStop,
  sqlSubscribe,
  type ConnectionProfile,
  type JsonValue,
  type PartitionDetail,
  type SqlColumn,
  type SqlProgress,
  type SqlSpec,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import ExportButton from "./ExportButton";
import { approxCount, groupDigits } from "./format";
import HelpPopover from "./HelpPopover";
import { useMasking } from "./masking";
import NlQueryBar from "./NlQueryBar";
import { ErrorBanner } from "./ProfileEditor";
import ResultGrid, { type ResultGridHandle } from "./ResultGrid";
import SeekBar, {
  buildSeek,
  initialSeekState,
  type SeekError,
  type SeekField,
  type SeekState,
} from "./SeekBar";
import type { ToastSpec } from "./Toast";

/**
 * SQL OVER A TOPIC — Phase 5a, and the same three honesty rules as search,
 * because it is the same problem wearing a different grammar.
 *
 * 1. A QUERY OVER A CAPPED SCAN IS AN ANSWER ABOUT THE SCAN, NOT ABOUT THE
 *    TOPIC. This is the whole reason `SqlProgress.capped` is in the contract:
 *    `SELECT count(*)` that read 100 000 of 4.2M records returns 100 000, and
 *    a number like that with no sentence beside it is not a truncated result —
 *    it is a WRONG one, stated confidently. So `capped` raises a banner, sits
 *    in the status line, and rides into the export's toast. It is never folded
 *    into "showing the first N rows".
 *
 * 2. WHILE IT IS SCANNING, "no rows" IS A LIE. The empty state counts what has
 *    been read so far and offers Stop, exactly like search, and only becomes
 *    "nothing matched" once the query is actually done.
 *
 * 3. THE SESSION IS OWNED BY ONE EFFECT — start, subscribe, and a cleanup that
 *    unsubscribes and stops — so stopping, navigating away and unmounting are
 *    the same path. `stopRequested` carries a Stop pressed while `sql_start` is
 *    still in flight, because there is no id to stop until it resolves.
 *
 * And one rule that is this view's own: THE SUPPORTED SURFACE IS RENDERED FROM
 * THE CONTRACT, not written here. `SQL_SURFACE` and `SQL_TABLE_COLUMNS` live in
 * api.ts beside the types they describe, so the note under the editor and the
 * cheatsheet cannot drift from the engine — a documented function that the
 * build does not have is worse than no documentation, because the user spends
 * their time believing the tool is broken.
 */

/** The same default scope as search: the last hour, said out loud. */
const DEFAULT_WINDOW_MS = 3_600_000;

/** What a first-time user finds in the editor. It runs as it stands. */
const STARTER_QUERY = `SELECT partition, offset, key_text, value_text
FROM messages
ORDER BY offset DESC
LIMIT 100`;

interface RunSpec {
  spec: SqlSpec;
  /** Bumped per run so an identical query still restarts the effect. */
  nonce: number;
}

interface SqlViewProps {
  profile: ConnectionProfile;
  topic: string;
  partitions: PartitionDetail[];
  onBack: () => void;
  /** Swap to the message browser — one topic, three questions. */
  onBrowse: () => void;
  onSearch: () => void;
  onDanger: DangerReport;
  push: (spec: ToastSpec) => void;
}

export default function SqlView({
  profile,
  topic,
  partitions,
  onBack,
  onBrowse,
  onSearch,
  onDanger,
  push,
}: SqlViewProps) {
  const [query, setQuery] = useState(STARTER_QUERY);

  // ── Scope ──────────────────────────────────────────────────────────────
  const [seek, setSeek] = useState<SeekState>(() =>
    initialSeekState("timestamp", "1000", DEFAULT_WINDOW_MS),
  );
  const [seekError, setSeekError] = useState<SeekError | null>(null);
  const [scanCap, setScanCap] = useState(String(SQL_SCAN_CAP));
  const [capError, setCapError] = useState<string | null>(null);
  /** A condition of ONE CONTROL, so it renders under that control (§5.3). */
  const [queryError, setQueryError] = useState<string | null>(null);

  // ── The run ────────────────────────────────────────────────────────────
  const [run, setRun] = useState<RunSpec | null>(null);
  const [running, setRunning] = useState(false);
  const [stopped, setStopped] = useState(false);
  /** Stop was pressed before there was an id to stop. */
  const [stopping, setStopping] = useState(false);
  const [columns, setColumns] = useState<SqlColumn[]>([]);
  const [rows, setRows] = useState<JsonValue[][]>([]);
  const [progress, setProgress] = useState<SqlProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sqlId = useRef<string | null>(null);
  const stopRequested = useRef(false);
  const gridRef = useRef<ResultGridHandle | null>(null);
  const nonce = useRef(0);
  const editorRef = useRef<HTMLTextAreaElement | null>(null);

  useDangerSignal(error !== null, onDanger);

  /**
   * WHETHER THIS RESULT SET CARRIES MASKED TEXT — and why it is the rule count
   * rather than a flag off the rows.
   *
   * The shell masks SQL rows on their way to this window, cell by cell
   * (`mask_rows`), and a projected row has nowhere to carry a `masked` flag the
   * way a `MessageRecord` does: what arrives is `[3, "•••"]`, with the
   * rewriting already done and unmarked. So the honest thing this view CAN say
   * is that rules were in force while the query ran — which is exactly what an
   * exported file's notice claims ("some values here are not the values on the
   * topic"), and it is the same direction of error as the message export: it
   * over-warns rather than under-warns, and a file that says it might be
   * redacted when it is not is a footnote, while the reverse is evidence
   * somebody trusted.
   */
  const masking = useMasking(profile.id);
  const rowsMayBeMasked = masking.enabled > 0;

  const known = useMemo(
    () => new Set(partitions.map((p) => p.partition)),
    [partitions],
  );
  const totalMessages = useMemo(
    () =>
      partitions.reduce(
        (sum, p) => sum + Math.max(0, p.latest_offset - p.earliest_offset),
        0,
      ),
    [partitions],
  );

  // ── Start / stop ───────────────────────────────────────────────────────

  const start = useCallback(() => {
    if (query.trim().length === 0) {
      // A failed submit focuses the offending control and says what to do
      // there, rather than raising a banner about a control 400px away.
      setQueryError(
        `Write a query first — ${STARTER_QUERY.split("\n")[0]} … is a good place to start.`,
      );
      editorRef.current?.focus();
      return;
    }
    setQueryError(null);
    const built = buildSeek(seek, known, { maxCount: SQL_SCAN_CAP });
    if ("field" in built) {
      setSeekError(built);
      return;
    }
    setSeekError(null);

    const cap = Number.parseInt(scanCap, 10);
    if (!Number.isFinite(cap) || cap < 1) {
      setCapError("Read at least one record — that is the scan, not the result.");
      return;
    }
    setCapError(null);

    setColumns([]);
    setRows([]);
    setProgress(null);
    setError(null);
    setStopped(false);
    setStopping(false);
    setRunning(true);
    stopRequested.current = false;
    nonce.current += 1;
    setRun({
      spec: {
        topic,
        query: query.trim(),
        seek: built.seek,
        partitions: built.partitions,
        scan_cap: Math.min(cap, SQL_SCAN_CAP),
        max_rows: SQL_MAX_ROWS,
      },
      nonce: nonce.current,
    });
  }, [query, seek, known, scanCap, topic]);

  const stop = useCallback(() => {
    // Recorded FIRST and in a ref: the id may not exist yet, and this is the
    // only thing that survives the `sql_start` round trip.
    stopRequested.current = true;
    const id = sqlId.current;
    setRunning(false);
    setStopped(true);
    if (id !== null) {
      void sqlStop(id);
      return;
    }
    setStopping(true);
  }, []);

  /** The session. One effect owns it end to end — see rule 3 at the top. */
  useEffect(() => {
    if (run === null) return;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    let started: string | null = null;

    void (async () => {
      try {
        const id = await sqlStart(profile.id, run.spec);
        if (cancelled) {
          void sqlStop(id);
          return;
        }
        started = id;
        sqlId.current = id;
        unsubscribe = sqlSubscribe(
          id,
          {
            onSchema: (payload) => setColumns(payload.columns),
            onRows: (payload) => {
              if (payload.rows.length === 0) return;
              setRows((prev) =>
                prev.length >= SQL_MAX_ROWS
                  ? prev
                  : prev.concat(payload.rows).slice(0, SQL_MAX_ROWS),
              );
            },
            onProgress: (next) => {
              setProgress(next);
              if (next.done) {
                setRunning(false);
                setStopping(false);
                if (next.error !== null) setError(next.error);
              }
            },
          },
          (message) => {
            setError(message);
            setRunning(false);
            setStopping(false);
          },
        );
        if (stopRequested.current) {
          void sqlStop(id);
          setStopping(false);
          setRunning(false);
          setStopped(true);
        }
      } catch (err) {
        if (cancelled) return;
        setRunning(false);
        setStopping(false);
        setError(errorMessage(err));
      }
    })();

    return () => {
      cancelled = true;
      unsubscribe?.();
      if (started !== null) void sqlStop(started);
      sqlId.current = null;
    };
  }, [run, profile.id]);

  // `Esc` cancels the running query, as it does a running search. It yields to
  // anything that already handled the key.
  useEffect(() => {
    if (!running) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      stop();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [running, stop]);

  // ── Derived ────────────────────────────────────────────────────────────

  const scanned = progress?.scanned ?? 0;
  const produced = progress?.produced_rows ?? rows.length;
  const capped = progress?.capped === true;
  const hitRowCap = rows.length >= SQL_MAX_ROWS;
  /**
   * `capped` has FOUR causes and the banner may only name the one that
   * happened. The row cap is visible from the row count; the record ceiling is
   * visible from `scanned` reaching it. What is left — the scan's memory
   * ceiling, and a partition that went quiet before its end offset — cannot be
   * told apart from here, so the third branch names both rather than asserting
   * a ceiling the scan never got near. Claiming "the scan stopped at its
   * ceiling of 100,000 records" over a scan of 4,000 would be exactly the kind
   * of confident wrong sentence this flag exists to prevent.
   */
  const hitScanCap = scanned >= (run?.spec.scan_cap ?? SQL_SCAN_CAP);
  const finished = progress?.done === true;

  const busyWhy = stopping
    ? "Kavka is stopping the last query"
    : running
      ? "Kavka is already reading — stop it to run a different query"
      : undefined;

  return (
    <section className="messages-view">
      <div className="messages-head">
        <div className="panel-head messages-panel-head">
          <h2 className="panel-title">
            <button
              type="button"
              className="btn btn-ghost crumb-btn"
              onClick={onBack}
            >
              ← {topic}
            </button>
            <span className="panel-count">
              SQL · {partitions.length}{" "}
              {partitions.length === 1 ? "partition" : "partitions"}
            </span>
          </h2>
          <div className="panel-tools">
            <button type="button" className="btn" onClick={onBrowse}>
              Browse messages
            </button>
            <button type="button" className="btn" onClick={onSearch}>
              Search
            </button>
            <ExportButton
              table={{ columns, rows }}
              topic={topic}
              capped={
                capped ? { shown: rows.length, total: scanned, kind: "sql" } : null
              }
              masked={rowsMayBeMasked}
              push={push}
            />
          </div>
        </div>

        {/* PLAIN ENGLISH, ABOVE THE EDITOR IT WRITES INTO — and it only ever
            writes. The query lands in the box and waits for Run, because a
            grammar that misreads a sentence must not be able to start a scan.
            See NlQueryBar. */}
        <NlQueryBar
          mode="sql"
          profileId={profile.id}
          topic={topic}
          idPrefix="ql"
          disabled={running || stopping}
          disabledReason={busyWhy}
          onFill={(next) => {
            setQuery(next);
            setQueryError(null);
          }}
        />

        {/* THE EDITOR. Mono, because a query is a literal you could paste into
            any other SQL client; Ctrl/Cmd+Enter runs it, which is the one
            shortcut every SQL tool shares. */}
        <div className="sql-editor">
          <div className="sql-editor-head">
            <label className="seekbar-label" htmlFor="sql-query">
              Query
            </label>
            <HelpPopover
              className="btn btn-ghost"
              label="Examples"
              buttonTitle="Three queries that work on this topic"
              title="Three queries against messages"
            >
              <p className="help-pop-lead">
                Every query reads one table, <code>{SQL_TABLE_NAME}</code>,
                holding the records this scan read — the scope above decides
                which ones. It has {SQL_TABLE_COLUMNS.length} columns:
              </p>
              <dl className="help-list">
                {SQL_TABLE_COLUMNS.map((col) => (
                  <div className="help-row" key={col.name}>
                    <dt className="help-syntax">
                      {col.name} {col.data_type}
                      {col.nullable ? " NULL" : ""}
                    </dt>
                    <dd className="help-what">{col.what}</dd>
                  </div>
                ))}
              </dl>
              <p className="help-pop-lead">
                Three that run as they stand:
              </p>
              <dl className="help-list">
                <div className="help-row">
                  <dt className="help-syntax">
                    SELECT partition, count(*) AS n FROM messages GROUP BY
                    partition ORDER BY n DESC
                  </dt>
                  <dd className="help-what">
                    How the records in this scan are spread across partitions.
                    Counts describe what was read, not the whole topic.
                  </dd>
                </div>
                <div className="help-row">
                  <dt className="help-syntax">
                    SELECT offset, key_text, value_text FROM messages WHERE
                    value_text LIKE '%timeout%' LIMIT 50
                  </dt>
                  <dd className="help-what">
                    Anywhere in the body, whatever shape it is — the same job
                    the search bar does, with the offsets in the answer.
                  </dd>
                </div>
                <div className="help-row">
                  <dt className="help-syntax">
                    SELECT key_text, count(*) AS n, max(offset) AS newest FROM
                    messages WHERE key_text IS NOT NULL GROUP BY key_text HAVING
                    count(*) &gt; 1
                  </dt>
                  <dd className="help-what">
                    Keys that appear more than once, and where the newest of
                    each one is — the shape of a compaction or duplicate
                    question.
                  </dd>
                </div>
              </dl>
            </HelpPopover>
          </div>
          <textarea
            id="sql-query"
            ref={editorRef}
            className={`sql-input${queryError !== null ? " input-invalid" : ""}`}
            rows={6}
            value={query}
            spellCheck={false}
            disabled={running || stopping}
            aria-label="SQL query"
            aria-invalid={queryError !== null ? true : undefined}
            aria-describedby={queryError !== null ? "ql-query-error" : undefined}
            onChange={(e) => {
              setQuery(e.target.value);
              // Editing a field clears its own message; nothing ever adds one
              // mid-keystroke (§5.3).
              setQueryError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                if (!running && !stopping) start();
              }
            }}
          />
          {queryError !== null && (
            <span className="field-error" id="ql-query-error">
              {queryError}
            </span>
          )}
          {/* Rendered from the contract's own constant — see the note at the
              top of this file. */}
          <details className="sql-surface">
            <summary>What this engine understands</summary>
            <ul className="sql-surface-list">
              {SQL_SURFACE.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </details>
        </div>

        <SeekBar
          idPrefix="ql"
          label="What to read"
          state={seek}
          onChange={(patch) => {
            setSeek((prev) => ({ ...prev, ...patch }));
            if (patch.mode !== undefined) setSeekError(null);
          }}
          partitions={partitions}
          error={seekError}
          onClearError={(field: SeekField) =>
            setSeekError((prev) => (prev?.field === field ? null : prev))
          }
          showCount={seek.mode === "latest"}
          countLabel="Newest per partition"
          disabled={running || stopping}
        >
          <label className="seekbar-field">
            <span className="seekbar-label">Read at most</span>
            <input
              type="number"
              min={1}
              max={SQL_SCAN_CAP}
              step={1}
              className={`input-num${capError !== null ? " input-invalid" : ""}`}
              value={scanCap}
              disabled={running || stopping}
              aria-invalid={capError !== null ? true : undefined}
              aria-describedby={capError !== null ? "ql-cap-error" : undefined}
              onChange={(e) => {
                setScanCap(e.target.value);
                setCapError(null);
              }}
            />
          </label>
          <div className="seekbar-field seekbar-actions">
            <span className="seekbar-label" aria-hidden="true">
              &nbsp;
            </span>
            {running || stopping ? (
              <button
                type="button"
                className="btn btn-latched"
                onClick={stop}
                disabled={stopping}
                aria-busy={stopping || undefined}
                title={
                  stopping
                    ? "Kavka is stopping the query — it is still starting up, so there is nothing to stop yet"
                    : "Stop reading and keep the rows already produced"
                }
              >
                <span className="btn-busy-slot" aria-hidden="true">
                  {stopping ? <span className="spinner" /> : null}
                </span>
                Stop
              </button>
            ) : (
              <button
                type="button"
                className="btn btn-primary"
                title={busyWhy}
                onClick={start}
              >
                <span className="btn-busy-slot" aria-hidden="true" />
                Run
              </button>
            )}
          </div>
        </SeekBar>

        {capError !== null && (
          <span className="field-error" id="ql-cap-error">
            {capError}
          </span>
        )}

        <p className="sql-scope-note">
          Kavka reads up to {groupDigits(Math.min(Number.parseInt(scanCap, 10) || 0, SQL_SCAN_CAP))}{" "}
          records from the scope above and runs the query over those.{" "}
          <code>{topic}</code> holds about {approxCount(totalMessages)} messages,
          so a query is an answer about the slice you chose — the engine never
          sees the rest, and a count over it is a count of the slice.
        </p>

        {error !== null && (
          <SqlErrorBanner raw={error} onDismiss={() => setError(null)} />
        )}

        {/* THE HONESTY BANNER. Never silent, and never folded into a row
            count: a capped scan changes what an aggregate MEANS. */}
        {capped && (
          <div className="banner banner-warn" role="status">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                {hitRowCap
                  ? `This answer stops at ${groupDigits(SQL_MAX_ROWS)} rows.`
                  : `This answer covers the ${groupDigits(scanned)} records Kavka read, not the whole topic.`}
              </p>
              <p className="banner-detail">
                {hitRowCap
                  ? `The query produced more rows than Kavka holds. Add a LIMIT, or narrow the scope, so the rows you see are the rows you meant.`
                  : hitScanCap
                    ? `The scan stopped at its ceiling of ${groupDigits(
                        run?.spec.scan_cap ?? SQL_SCAN_CAP,
                      )} records. Anything aggregated — a count, a sum, a max — describes those records only. Narrow the time range or the partitions and the same query answers about a slice you can name.`
                    : `The scan didn't reach the end of its scope: it either filled its memory ceiling, or a partition stopped answering before its end offset. Anything aggregated — a count, a sum, a max — describes only what was read. Narrow the time range or the partitions and the same query answers about a slice you can name.`}
              </p>
            </div>
          </div>
        )}

        {progress !== null && (
          <p className="search-progress-line" role="status">
            {running ? "Reading" : stopped ? "Stopped after" : "Read"}{" "}
            {groupDigits(scanned)} {scanned === 1 ? "record" : "records"} ·{" "}
            {groupDigits(produced)} {produced === 1 ? "row" : "rows"}
            {finished && !capped ? " · complete over this scope" : ""}
          </p>
        )}
      </div>

      <div className="messages-main">
        <ResultGrid
          ref={gridRef}
          columns={columns}
          rows={rows}
          label={`Query results from ${topic}`}
          idPrefix="ql"
          loading={running}
          empty={
            run === null ? (
              <>
                <p className="empty-hint">
                  Nothing is read until you run the query. Kavka consumes the
                  records in the scope above, builds the{" "}
                  <code>{SQL_TABLE_NAME}</code> table from them and answers over
                  that — there is no index and nothing is cached.
                </p>
                <p className="empty-hint">
                  <span className="kbd">Ctrl</span>
                  <span className="kbd">⏎</span> runs the query.
                </p>
              </>
            ) : running ? (
              <p className="empty-hint">
                Read {groupDigits(scanned)} of at most{" "}
                {groupDigits(run.spec.scan_cap)} records · no rows yet.
              </p>
            ) : error !== null ? (
              <p className="empty-hint">
                The query stopped after {groupDigits(scanned)}{" "}
                {scanned === 1 ? "record" : "records"}. What went wrong is in the
                message above.
              </p>
            ) : stopped ? (
              <p className="empty-hint">
                You stopped the query after {groupDigits(scanned)}{" "}
                {scanned === 1 ? "record" : "records"}, and it had produced no
                rows yet. Run it again to start from the beginning of the scope.
              </p>
            ) : columns.length === 0 ? (
              <p className="empty-hint">
                The query returned no columns at all. That usually means the
                engine accepted it but it selects nothing — check the SELECT
                list.
              </p>
            ) : (
              <>
                <p className="empty-hint">
                  No rows matched. Kavka read {groupDigits(scanned)}{" "}
                  {scanned === 1 ? "record" : "records"} from this scope and the
                  query kept none of them.
                </p>
                <div className="empty-actions">
                  <button
                    type="button"
                    className="btn"
                    onClick={() => {
                      setSeek((prev) => ({ ...prev, mode: "earliest" }));
                      setSeekError(null);
                    }}
                  >
                    Read from the beginning
                  </button>
                </div>
              </>
            )
          }
        />
      </div>

      <div className="view-statusline">
        <span className="statusbar-item">
          {groupDigits(rows.length)} {rows.length === 1 ? "row" : "rows"}
          {columns.length > 0
            ? ` · ${columns.length} ${columns.length === 1 ? "column" : "columns"}`
            : ""}
        </span>
        {progress !== null && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="statusbar-item">{groupDigits(scanned)} scanned</span>
          </>
        )}
        {capped && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span
              className="statusbar-item statusline-dropped"
              title="A ceiling truncated this answer, so anything aggregated describes what was read rather than the topic."
            >
              capped
            </span>
          </>
        )}
        {running && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="statusbar-item statusline-live">
              <span className="status-dot status-connected" aria-hidden="true" />
              Reading
            </span>
          </>
        )}
        {stopping && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="statusbar-item" role="status">
              Stopping…
            </span>
          </>
        )}
        <span className="statusbar-right">
          <span className="statusbar-item">
            <span className="kbd">Ctrl</span>
            <span className="kbd">⏎</span> run
          </span>
          <span className="statusbar-sep" aria-hidden="true">
            ·
          </span>
          <span className="statusbar-item">
            <span className="kbd">Esc</span> stop
          </span>
        </span>
      </div>
    </section>
  );
}

/**
 * A query failure is not a cluster failure, and the §7 error library is about
 * clusters.
 *
 * `classifyError` is written against librdkafka strings; handed a DataFusion
 * planning error it correctly reports that it does not recognise it, which
 * makes the engine's own message the banner TITLE — a paragraph of type names
 * where the plain-language first line belongs. So a query problem gets its own
 * three layers: what happened in Kavka's words, the engine's first line as the
 * detail, and the verbatim text under Show details like everywhere else. A
 * failure that is NOT about the query (a broker that went away mid-scan) falls
 * through to the shared banner, which is the one that knows about brokers.
 */
function SqlErrorBanner({
  raw,
  onDismiss,
}: {
  raw: string;
  onDismiss: () => void;
}) {
  if (!looksLikeQueryProblem(raw))
    return <ErrorBanner raw={raw} onDismiss={onDismiss} />;
  // The engine's first line names the fault; the rest is usually a plan dump.
  const first = raw.split("\n")[0].trim();
  return (
    <div className="banner banner-danger" role="alert">
      <span className="banner-glyph" aria-hidden="true">
        !
      </span>
      <div className="banner-body">
        <p className="banner-title">Kavka couldn't run that query.</p>
        <p className="banner-detail">
          The engine refused it before reading anything, so nothing was
          consumed. It said: {first}
        </p>
        <details className="banner-details">
          <summary>Show details</summary>
          <pre className="banner-raw">{raw}</pre>
        </details>
      </div>
      <div className="banner-actions">
        <button type="button" className="btn btn-ghost" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}

/**
 * Is this failure about the SQL rather than about the cluster?
 *
 * Kept deliberately narrow, for the same reason `looksLikeCelProblem` is in
 * SearchView: a wrong guess tells someone their broker outage is a syntax
 * error. Anything unrecognised goes to the shared banner.
 */
export function looksLikeQueryProblem(raw: string): boolean {
  const s = raw.toLowerCase();
  return (
    s.includes("sql") ||
    s.includes("parser error") ||
    s.includes("error during planning") ||
    s.includes("schema error") ||
    s.includes("no field named") ||
    s.includes("invalid function") ||
    s.includes("datafusion")
  );
}
