import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  MAX_SEARCH_BUFFERED,
  errorMessage,
  searchStart,
  searchStop,
  searchSubscribe,
  type ConnectionProfile,
  type MessageRecord,
  type PartitionDetail,
  type SearchProgress,
  type SearchSpec,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { replayBlockedWhy } from "./dlq";
import { classifyError } from "./errors";
import ExportButton from "./ExportButton";
import { approxCount, groupDigits } from "./format";
import HelpPopover from "./HelpPopover";
import MessageGrid, {
  rowKey,
  type MessageGridHandle,
} from "./MessageGrid";
import MessageInspector from "./MessageInspector";
import {
  maskingChipLabel,
  maskingChipTitle,
  noteMaskedRecords,
  useMasking,
} from "./masking";
import { noteJsonFields } from "./nl";
import NlQueryBar from "./NlQueryBar";
import { ErrorBanner } from "./ProfileEditor";
import SeekBar, {
  buildSeek,
  initialSeekState,
  type SeekError,
  type SeekField,
  type SeekState,
} from "./SeekBar";
import { readFilters, writeFilters, type SavedFilter } from "./storage";
import type { ToastSpec } from "./Toast";

/**
 * STREAMING SEARCH — Phase 2's crown jewel, from the UI side.
 *
 * Three rules run this screen, and all three are honesty rules.
 *
 * 1. SEARCH NEVER SILENTLY TRUNCATES. The core buffers at most
 *    MAX_SEARCH_BUFFERED results, but it keeps MATCHING past that and keeps
 *    counting — so whenever `matched` runs ahead of `buffered` the view says
 *    so, in a banner, in the export button's title and in the status line. A
 *    result list that stops at ten thousand with no sentence is the acceptance
 *    gate for this phase failing quietly.
 *
 * 2. WHILE IT IS SCANNING, "no results" IS A LIE (§7). The empty state reads
 *    "Scanned 412 000 of ~2.4M · 0 matches so far" with a Stop, and only
 *    becomes "nothing matched" once the search is actually done.
 *
 * 3. THE SESSION IS OWNED BY ONE EFFECT, exactly like the live tail: it
 *    starts, subscribes, and returns a cleanup that unsubscribes and stops —
 *    so stopping, navigating away and unmounting are the SAME path and there
 *    is no fourth place to forget. `search_stop` is idempotent, so the extra
 *    call after a finished search costs nothing.
 *
 *    THE ONE THING THAT PATH DOES NOT COVER is a Stop pressed while
 *    `search_start` is still in flight: there is no id yet, so there is
 *    nothing to stop, and the click would be swallowed while eight consumers
 *    spun up behind it. `stopRequested` is what carries the intent across that
 *    window — the moment the id exists it is stopped — and the button says
 *    "stopping" rather than pretending the click landed.
 *
 * Two more honesty rules arrived with the reviewed contract, and they are the
 * same rule as (1) wearing different hats:
 *
 * 4. A FILTER THAT COULD NOT JUDGE A RECORD DID NOT JUDGE IT. `unevaluated`
 *    counts those, and the view says so beside the match count — "0 matches"
 *    and "0 matches, 12 000 records this expression couldn't read" are
 *    different answers to the same question.
 * 5. A PARTITION THAT WENT QUIET BEFORE ITS END OFFSET IS NAMED.
 *    `assumed_complete` is how the core reports one, and a progress bar that
 *    reached 100% because nothing more arrived is not the same claim as one
 *    that reached the watermark.
 */

/** DESIGN's own default scope for a search: the last hour, said out loud. */
const DEFAULT_WINDOW_MS = 3_600_000;

interface RunSpec {
  spec: SearchSpec;
  /** Bumped per run so an identical spec still restarts the effect. */
  nonce: number;
  /** What the user typed, kept for the "nothing matched" sentence. */
  described: string;
  /** True when the scope was the default hour — the empty state offers wider. */
  windowed: boolean;
}

interface SearchViewProps {
  profile: ConnectionProfile;
  topic: string;
  partitions: PartitionDetail[];
  onBack: () => void;
  /** Swap to the message browser — the two are one topic, two questions. */
  onBrowse: () => void;
  onDanger: DangerReport;
  push: (spec: ToastSpec) => void;
  /**
   * The two dead-letter actions, threaded so a search for failures can act on
   * what it finds. Searching a DLQ for one exception class and re-producing
   * the matches is the whole workflow; an inspector that only offers it in the
   * browser would send the user back to look the record up again.
   */
  onBrowseOriginal?: (topic: string, partition: number, offset: number) => void;
  onReproduce?: (record: MessageRecord) => void;
}

export default function SearchView({
  profile,
  topic,
  partitions,
  onBack,
  onBrowse,
  onDanger,
  push,
  onBrowseOriginal,
  onReproduce,
}: SearchViewProps) {
  // ── The query ──────────────────────────────────────────────────────────
  const [cel, setCel] = useState(false);
  const [text, setText] = useState("");
  const [celText, setCelText] = useState("");
  const [celError, setCelError] = useState<string | null>(null);

  // ── Scope ──────────────────────────────────────────────────────────────
  // Defaults to the last hour, which is the scope §7's "search found nothing"
  // empty state is written against.
  const [seek, setSeek] = useState<SeekState>(() =>
    initialSeekState("timestamp", "1000", DEFAULT_WINDOW_MS),
  );
  const [seekError, setSeekError] = useState<SeekError | null>(null);

  // ── The run ────────────────────────────────────────────────────────────
  const [run, setRun] = useState<RunSpec | null>(null);
  const [running, setRunning] = useState(false);
  const [stopped, setStopped] = useState(false);
  /** Stop was pressed before there was an id to stop. */
  const [stopping, setStopping] = useState(false);
  const [rows, setRows] = useState<MessageRecord[]>([]);
  const [progress, setProgress] = useState<SearchProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const searchId = useRef<string | null>(null);
  /** Survives the `search_start` round trip — see rule 3 at the top. */
  const stopRequested = useRef(false);
  const gridRef = useRef<MessageGridHandle | null>(null);
  const nonce = useRef(0);

  // ── Saved filters ──────────────────────────────────────────────────────
  const [filters, setFilters] = useState<SavedFilter[]>(() =>
    readFilters(profile.id, topic),
  );
  const [naming, setNaming] = useState(false);
  const [filterName, setFilterName] = useState("");
  const nameRef = useRef<HTMLInputElement | null>(null);

  useDangerSignal(error !== null, onDanger);

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

  const selected = useMemo(
    () => rows.find((r) => rowKey(r) === selectedKey) ?? null,
    [rows, selectedKey],
  );

  // ── Start / stop ───────────────────────────────────────────────────────

  const start = useCallback(() => {
    const built = buildSeek(seek, known, { maxCount: 100000 });
    if ("field" in built) {
      setSeekError(built);
      return;
    }
    setSeekError(null);

    const substring = cel ? null : text.trim().length === 0 ? null : text;
    const program = cel && celText.trim().length > 0 ? celText.trim() : null;

    setRows([]);
    setSelectedKey(null);
    setProgress(null);
    setError(null);
    setCelError(null);
    setStopped(false);
    setStopping(false);
    setRunning(true);
    stopRequested.current = false;
    nonce.current += 1;
    setRun({
      spec: {
        topic,
        seek: built.seek,
        partitions: built.partitions,
        query: { substring, cel: program },
        max_buffered: MAX_SEARCH_BUFFERED,
        // null = the core's own display cap. The inspector says when it bit.
        max_value_bytes: null,
      },
      nonce: nonce.current,
      described: program ?? substring ?? "everything in this range",
      windowed: seek.mode === "timestamp",
    });
  }, [seek, known, cel, text, celText, topic]);

  const stop = useCallback(() => {
    // Recorded FIRST, and in a ref: the id may not exist yet, and this is the
    // only thing that survives the `search_start` round trip to stop the
    // session the moment it does.
    stopRequested.current = true;
    const id = searchId.current;
    setRunning(false);
    setStopped(true);
    if (id !== null) {
      // Optimistic, and idempotent on the Rust side: the final progress still
      // arrives and corrects the counts, but the button has to answer NOW.
      void searchStop(id);
      return;
    }
    // No id yet — `search_start` is still resolving. The click has landed, but
    // nothing has been stopped, so the control says exactly that rather than
    // going quiet and leaving a scan running behind a "Search" button.
    setStopping(true);
  }, []);

  /**
   * The session. One effect owns it end to end — see rule 3 at the top.
   *
   * The listener is registered AFTER `search_start` resolves, because the id is
   * what the events are addressed to. `searchSubscribe` confirms the
   * subscription to the shell (`session_ready`) the moment both listeners are
   * up, and the shell holds the session's first event until then — so the
   * window between the id existing and the UI listening no longer swallows
   * results or a `done`.
   */
  useEffect(() => {
    if (run === null) return;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    let started: string | null = null;

    void (async () => {
      try {
        const id = await searchStart(profile.id, run.spec);
        if (cancelled) {
          void searchStop(id);
          return;
        }
        started = id;
        searchId.current = id;
        unsubscribe = searchSubscribe(
          id,
          {
            onResults: (payload) => {
              if (payload.records.length === 0) return;
              // Two facts about a batch, recorded before it is rendered: what
              // the payload's fields are called (the plain-English bar's only
              // source of field names) and whether the core masked any of it
              // (the status bar's, and the export's).
              noteJsonFields(profile.id, topic, payload.records);
              noteMaskedRecords(profile.id, payload.records);
              setRows((prev) =>
                prev.length >= MAX_SEARCH_BUFFERED
                  ? prev
                  : prev.concat(payload.records).slice(0, MAX_SEARCH_BUFFERED),
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
        // The Stop the user pressed while this was in flight. It could not be
        // sent then; it is sent now, after subscribing, so the final progress —
        // what was scanned before it stopped — still arrives.
        if (stopRequested.current) {
          void searchStop(id);
          setStopping(false);
          setRunning(false);
          setStopped(true);
        }
      } catch (err) {
        if (cancelled) return;
        const raw = errorMessage(err);
        setRunning(false);
        setStopping(false);
        // A CEL program that doesn't compile is a condition of ONE CONTROL, so
        // it renders under that control (§5.3) — never in the banner, which is
        // for conditions the system is in.
        if (looksLikeCelProblem(raw)) setCelError(classifyError(raw).title);
        else setError(raw);
      }
    })();

    return () => {
      cancelled = true;
      unsubscribe?.();
      if (started !== null) void searchStop(started);
      searchId.current = null;
    };
    // `topic` only moves when this component is remounted (TopicsTab keys it by
    // topic), but it is read inside the effect now, so it is declared.
  }, [run, profile.id, topic]);

  // `Esc` cancels the running search (§7's expert shortcuts). It yields to
  // anything that already handled the key — the grid clears its selection with
  // preventDefault, and one press must never do two things.
  useEffect(() => {
    if (!running) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      stop();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [running, stop]);

  // ── Saved filters ──────────────────────────────────────────────────────

  const persist = useCallback(
    (next: SavedFilter[]) => {
      setFilters(next);
      writeFilters(profile.id, topic, next);
    },
    [profile.id, topic],
  );

  const applyFilter = useCallback((filter: SavedFilter) => {
    setText(filter.substring);
    setCelText(filter.cel);
    setCel(filter.cel.trim().length > 0);
    setCelError(null);
  }, []);

  const saveFilter = useCallback(() => {
    const name = filterName.trim();
    if (name.length === 0) return;
    const next = filters
      .filter((f) => f.name !== name)
      .concat({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        name,
        substring: text,
        cel: celText,
      });
    persist(next);
    setNaming(false);
    setFilterName("");
  }, [filterName, filters, text, celText, persist]);

  useEffect(() => {
    if (naming) nameRef.current?.focus();
  }, [naming]);

  // ── Derived ────────────────────────────────────────────────────────────

  const matched = progress?.matched ?? 0;
  const scanned = progress?.scanned ?? 0;
  const buffered = progress?.buffered ?? rows.length;
  const capped = matched > buffered && buffered > 0;
  /** Records the expression could not be evaluated against — read, not judged.
      (`assumed_complete` is read by SearchProgressBlock, beside the bars it is
      about.) */
  const unevaluated = progress?.unevaluated ?? 0;
  const queryIsEmpty = cel
    ? celText.trim().length === 0
    : text.trim().length === 0;
  const activeFilter = filters.find(
    (f) => f.substring === text && f.cel === celText,
  );

  // `value_text`, not `string(value)`: the value binding changes shape with the
  // payload, so `string(value)` is an ERROR on every JSON record — which is
  // most of them, and exactly the ones a text search was going to find.
  // `value_text` is the display text, always a string, always bound.
  const celHint = useMemo(() => {
    const q = text.trim();
    return q.length === 0 ? null : `value_text.contains(${JSON.stringify(q)})`;
  }, [text]);

  const searchDisabledWhy = stopping
    ? "Kavka is stopping the last search"
    : running
      ? "Kavka is already scanning — stop it to start a different search"
      : undefined;

  // Masking, in the view's own status line as well as the app's: this is where
  // the rewritten payloads actually are, and the export button beside them
  // carries the same fact into the file.
  const masking = useMasking(profile.id);

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
              Search · {partitions.length}{" "}
              {partitions.length === 1 ? "partition" : "partitions"}
            </span>
          </h2>
          <div className="panel-tools">
            <button type="button" className="btn" onClick={onBrowse}>
              Browse messages
            </button>
            <ExportButton
              records={rows}
              topic={topic}
              capped={
                capped
                  ? { shown: buffered, total: matched, kind: "search" }
                  : null
              }
              push={push}
            />
          </div>
        </div>

        {/* Saved filters — a question you asked before, parked where you can
            ask it again. Latched styling when the bar already holds it. */}
        <div className="savedfilters">
          <span className="savedfilters-label">Saved</span>
          {filters.length === 0 && !naming && (
            <span className="savedfilters-empty">
              Nothing saved for this topic yet. Write a query, then save it here.
            </span>
          )}
          {filters.map((filter) => {
            const on = activeFilter?.id === filter.id;
            return (
              <span
                key={filter.id}
                className={`saved-chip${on ? " saved-chip-active" : ""}`}
              >
                <button
                  type="button"
                  className="saved-chip-apply"
                  aria-pressed={on}
                  title={
                    filter.cel.trim().length > 0
                      ? `CEL — ${filter.cel}`
                      : `Text — ${filter.substring}`
                  }
                  onClick={() => applyFilter(filter)}
                >
                  {filter.name}
                </button>
                <button
                  type="button"
                  className="saved-chip-del"
                  title={`Forget the saved filter “${filter.name}”`}
                  onClick={() =>
                    persist(filters.filter((f) => f.id !== filter.id))
                  }
                >
                  <span aria-hidden="true">×</span>
                  <span className="sr-only">Delete {filter.name}</span>
                </button>
              </span>
            );
          })}
          {naming ? (
            <span className="savefilter-row">
              <input
                ref={nameRef}
                type="text"
                className="savefilter-input"
                value={filterName}
                placeholder="Failed orders"
                autoComplete="off"
                aria-label="Name for this filter"
                onChange={(e) => setFilterName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    saveFilter();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    e.stopPropagation();
                    setNaming(false);
                  }
                }}
              />
              <button
                type="button"
                className="btn"
                disabled={filterName.trim().length === 0}
                title={
                  filterName.trim().length === 0
                    ? "Give the filter a name so you can find it again"
                    : undefined
                }
                onClick={saveFilter}
              >
                Save
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => setNaming(false)}
              >
                Cancel
              </button>
            </span>
          ) : (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={queryIsEmpty}
              title={
                queryIsEmpty
                  ? "Write a query first — there is nothing to save yet"
                  : "Keep this query on this topic"
              }
              onClick={() => {
                setFilterName("");
                setNaming(true);
              }}
            >
              Save filter
            </button>
          )}
        </div>

        {/* PLAIN ENGLISH, ABOVE THE FILTER IT WRITES INTO. It fills the CEL box
            and turns ƒx on so the result is visible in the control that will
            run it — and it never starts a search itself. See NlQueryBar: it is
            a grammar in the core, not an assistant, and it says so. */}
        <NlQueryBar
          mode="cel"
          profileId={profile.id}
          topic={topic}
          idPrefix="sv"
          disabled={running || stopping}
          disabledReason={searchDisabledWhy}
          onFill={(query) => {
            setCelText(query);
            setCel(true);
            setCelError(null);
          }}
        />

        {/* THE SEARCH BAR (§5.7). Plain text is the landing state; the CEL
            editor lives behind the ƒx toggle at the right edge — visible so an
            expert finds it in one second, never the default so a junior never
            faces it. */}
        <div className="searchbar">
          <span className="searchbar-glyph" aria-hidden="true">
            ⌕
          </span>
          <input
            type="text"
            className={`searchbar-input${cel ? " input-mono" : ""}${
              celError !== null ? " input-invalid" : ""
            }`}
            value={cel ? celText : text}
            placeholder={
              cel
                ? 'value.status == "failed"'
                : "Find in key, value or headers"
            }
            autoComplete="off"
            spellCheck={false}
            aria-label={cel ? "CEL filter" : "Find in key, value or headers"}
            aria-invalid={celError !== null ? true : undefined}
            aria-describedby={celError !== null ? "sv-cel-error" : undefined}
            onChange={(e) => {
              if (cel) {
                setCelText(e.target.value);
                setCelError(null);
              } else {
                setText(e.target.value);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !running) {
                e.preventDefault();
                start();
              }
            }}
          />
          <button
            type="button"
            className={`btn searchbar-fx${cel ? " btn-latched" : ""}`}
            aria-pressed={cel}
            title={
              cel
                ? "Back to plain text — Kavka matches the raw bytes of the key, value and headers"
                : "Write the filter as a CEL expression instead"
            }
            onClick={() => {
              setCel((prev) => !prev);
              setCelError(null);
            }}
          >
            ƒx
          </button>
          <HelpPopover
            className="btn btn-ghost searchbar-help"
            label="CEL cheatsheet"
            buttonTitle="Five expressions that work on this topic"
            title="CEL cheatsheet"
          >
            <p className="help-pop-lead">
              A filter is one expression that has to come out true. It sees{" "}
              <code>key</code>, <code>value</code>, <code>value_text</code>,{" "}
              <code>headers</code>, <code>partition</code>, <code>offset</code>{" "}
              and <code>timestamp_ms</code>. <code>value</code> is the decoded
              message — parsed JSON where it parsed, text otherwise — so{" "}
              <code>value.status</code> works on JSON and nothing else does.{" "}
              <code>value_text</code> is the same payload as the text you see in
              the table, always a string, so it works whatever the shape.
            </p>
            <dl className="help-list">
              <div className="help-row">
                <dt className="help-syntax">value.status == "failed"</dt>
                <dd className="help-what">
                  A field of a JSON payload, compared exactly.
                </dd>
              </div>
              <div className="help-row">
                <dt className="help-syntax">
                  key != null &amp;&amp; key.startsWith("A-1")
                </dt>
                <dd className="help-what">
                  A keyed message. The null check matters — messages without a
                  key are spread across partitions.
                </dd>
              </div>
              <div className="help-row">
                <dt className="help-syntax">"trace-id" in headers</dt>
                <dd className="help-what">
                  Any message carrying a header, whatever its value.
                </dd>
              </div>
              <div className="help-row">
                <dt className="help-syntax">
                  partition == 3 &amp;&amp; offset &gt; 8412
                </dt>
                <dd className="help-what">
                  A window of one partition, by address rather than content.
                </dd>
              </div>
              <div className="help-row">
                <dt className="help-syntax">
                  value_text.contains("timeout")
                </dt>
                <dd className="help-what">
                  Anywhere in the body, whatever shape it is — JSON, plain text
                  or a tombstone's empty string. This is the one to reach for
                  when a topic holds more than one kind of message.
                </dd>
              </div>
            </dl>
          </HelpPopover>
        </div>

        {/* Raised by pressing ⏎ in the box or Start beside it, neither of
            which moves focus — so the message needs a live region to be
            heard at all (SC 4.1.3). The input keeps `aria-describedby` too. */}
        {celError !== null && (
          <span className="field-error" id="sv-cel-error" role="alert">
            {celError}
          </span>
        )}

        {/* A plain-text search shows the equivalent CEL underneath. That is how
            a support engineer accidentally learns CEL (§5.7).

            "Close", not "same": the text box is an ASCII case-INSENSITIVE
            match on the raw bytes, and CEL's contains() is case-sensitive. The
            hint teaches the shape; the title says where the two differ, rather
            than teaching an equivalence that isn't one. */}
        {!cel && celHint !== null && (
          <p className="searchbar-hint">
            Close in CEL:{" "}
            <code title="Two differences: the text box ignores case, and it reads the raw bytes of the key, the value AND the headers. This reads the value's text only, case-sensitively — add .lowerAscii() on both sides to match the case behaviour.">
              {celHint}
            </code>{" "}
            <button
              type="button"
              className="btn btn-ghost inspector-inline-btn"
              onClick={() => {
                setCelText(celHint);
                setCel(true);
              }}
            >
              Use this
            </button>
          </p>
        )}

        <SeekBar
          idPrefix="sv"
          label="What to search"
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
          <div className="seekbar-field seekbar-actions">
            <span className="seekbar-label" aria-hidden="true">
              &nbsp;
            </span>
            {running || stopping ? (
              // The label survives the whole flow and the button keeps its
              // width (§5.5): `Stop` stays `Stop`, takes the busy slot, and
              // the word "Stopping" is in the status line below.
              <button
                type="button"
                className="btn btn-latched"
                onClick={stop}
                disabled={stopping}
                aria-busy={stopping || undefined}
                title={
                  stopping
                    ? "Kavka is stopping the search — it is still starting up, so there is nothing to stop yet"
                    : "Stop scanning and keep what has matched so far"
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
                title={searchDisabledWhy}
                onClick={start}
              >
                <span className="btn-busy-slot" aria-hidden="true" />
                Search
              </button>
            )}
          </div>
        </SeekBar>

        {error !== null && (
          <ErrorBanner raw={error} onDismiss={() => setError(null)} />
        )}

        {/* THE HONESTY BANNER. The buffer stops; the matching does not. */}
        {capped && (
          <div className="banner banner-warn" role="status">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                Showing the first {groupDigits(buffered)} matches —{" "}
                {groupDigits(matched)} total matched.
              </p>
              <p className="banner-detail">
                Kavka keeps counting past what it can hold on screen. Narrow the
                query or the partitions to see the rest, or export these{" "}
                {groupDigits(buffered)} now — the file carries exactly what is
                in the table.
              </p>
            </div>
          </div>
        )}

        {/* A FILTER THAT COULD NOT READ A RECORD DID NOT JUDGE IT. The count is
            the point — "0 matches" and "0 matches, and 4 000 records this
            expression couldn't read" are different answers — and the first
            failure, verbatim, is what turns the count into something the user
            can act on. Held to the same three layers as every other error
            (§7): what happened, what to do, the raw text under Show details. */}
        {unevaluated > 0 && (
          <div className="banner banner-warn" role="status">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                {groupDigits(unevaluated)}{" "}
                {unevaluated === 1 ? "record" : "records"} couldn't be judged by
                this expression.
              </p>
              <p className="banner-detail">
                Kavka read {unevaluated === 1 ? "it" : "them"} and the
                expression asked for something{" "}
                {unevaluated === 1 ? "it" : "they"} doesn't have — a topic
                holding more than one shape of message is the usual reason.{" "}
                {unevaluated === 1 ? "It is" : "They are"} counted as scanned
                and {unevaluated === 1 ? "is" : "are"} not among the matches.
                Try <code>value_text</code>, which is bound for every record
                whatever its shape.
              </p>
              {progress?.filter_error != null && (
                <details className="banner-details">
                  <summary>Show details</summary>
                  <pre className="banner-raw">{progress.filter_error}</pre>
                </details>
              )}
            </div>
          </div>
        )}

        {progress !== null && (
          <SearchProgressBlock
            progress={progress}
            running={running}
            stopped={stopped}
          />
        )}
      </div>

      <div className="messages-main">
        <MessageGrid
          ref={gridRef}
          records={rows}
          label={`Search results in ${topic}`}
          idPrefix="sv"
          selectedKey={selectedKey}
          onSelect={setSelectedKey}
          loading={running}
          empty={
            run === null ? (
              <>
                <p className="empty-hint">
                  Search reads every message in the range and keeps the ones
                  that match — nothing is fetched until you press Search.{" "}
                  <code>{topic}</code> holds about {approxCount(totalMessages)}{" "}
                  messages.
                </p>
                <p className="empty-hint">
                  The scope starts at the last hour. Widen it above to search
                  further back.
                </p>
              </>
            ) : running ? (
              <p className="empty-hint">
                Scanned {groupDigits(scanned)} of about{" "}
                {approxCount(totalMessages)} · 0 matches so far.
              </p>
            ) : error !== null ? (
              // "Nothing matched" would be a lie about a search that never
              // finished. The banner above already says what went wrong.
              <p className="empty-hint">
                The search stopped after {groupDigits(scanned)}{" "}
                {scanned === 1 ? "message" : "messages"}. What went wrong is in
                the message above.
              </p>
            ) : stopped ? (
              // Same rule, other cause: a search the USER stopped did not
              // check the range, so it cannot report that nothing was in it.
              <p className="empty-hint">
                You stopped the search after {groupDigits(scanned)}{" "}
                {scanned === 1 ? "message" : "messages"}, and nothing had
                matched <code>{run.described}</code> yet. Search again to pick
                it up from the start of the range.
              </p>
            ) : (
              <>
                <p className="empty-hint">
                  No messages matched <code>{run.described}</code>. Kavka checked{" "}
                  {groupDigits(scanned)}{" "}
                  {scanned === 1 ? "message" : "messages"}
                  {run.windowed
                    ? " — the scope only looks back an hour."
                    : " in that range."}
                  {unevaluated > 0 && (
                    <>
                      {" "}
                      {groupDigits(unevaluated)} of{" "}
                      {unevaluated === 1 ? "them" : "those"} couldn't be judged
                      by this expression at all — see the note above.
                    </>
                  )}
                </p>
                <div className="empty-actions">
                  {run.windowed && (
                    <button
                      type="button"
                      className="btn"
                      onClick={() => {
                        setSeek((prev) => ({ ...prev, mode: "earliest" }));
                        setSeekError(null);
                      }}
                    >
                      Search all time
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={() => {
                      setText("");
                      setCelText("");
                      setCelError(null);
                    }}
                  >
                    Clear the query
                  </button>
                </div>
              </>
            )
          }
        />

        {selected !== null && (
          <MessageInspector
            record={selected}
            topic={topic}
            onClose={() => setSelectedKey(null)}
            onBrowseOriginal={onBrowseOriginal}
            onReproduce={onReproduce}
            // The same two answers the browser gives, from the same function:
            // a search result is the same record, and a masked one is just as
            // unsendable here as it is there.
            reproduceBlocked={replayBlockedWhy(profile.read_only, selected)}
          />
        )}
      </div>

      <div className="view-statusline">
        <span className="statusbar-item">
          {groupDigits(buffered)} {buffered === 1 ? "match" : "matches"}
          {capped ? ` of ${groupDigits(matched)} matched` : ""}
        </span>
        {progress !== null && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="statusbar-item">
              {groupDigits(scanned)} scanned
            </span>
          </>
        )}
        {unevaluated > 0 && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            {/* Never folded into "scanned": these were read and NOT judged. */}
            <span
              className="statusbar-item statusline-dropped"
              title="The expression asked these records for something they don't have, so Kavka could not decide either way. They are not counted as matches."
            >
              {groupDigits(unevaluated)} unjudged
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
              Scanning
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
        {stopped && !running && !stopping && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="statusbar-item">Stopped</span>
          </>
        )}
        {masking.enabled > 0 && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="mask-chip" title={maskingChipTitle(masking)}>
              {maskingChipLabel(masking)}
            </span>
          </>
        )}
        <span className="statusbar-right">
          <span className="statusbar-item">
            <span className="kbd">⏎</span> search
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
 * Progress: a sentence, then the per-partition bars.
 *
 * Law 2 applies to every bar here — each one carries its two offsets as digits
 * beside it, so a partition that is behind is legible without reading a
 * length, and the whole block survives greyscale.
 */
function SearchProgressBlock({
  progress,
  running,
  stopped,
}: {
  progress: SearchProgress;
  running: boolean;
  stopped: boolean;
}) {
  const rate = Math.round(progress.msgs_per_sec);
  const assumed = new Set(progress.assumed_complete);
  return (
    <div className="search-progress">
      <p className="search-progress-line" role="status">
        {running ? "Scanning" : stopped ? "Stopped after" : "Scanned"}{" "}
        {groupDigits(progress.scanned)}{" "}
        {progress.scanned === 1 ? "message" : "messages"} ·{" "}
        {groupDigits(progress.matched)}{" "}
        {progress.matched === 1 ? "match" : "matches"}
        {/* Read but not judged, so it is never folded into either of the two
            numbers beside it. */}
        {progress.unevaluated > 0
          ? ` · ${groupDigits(progress.unevaluated)} unjudged`
          : ""}
        {running && rate > 0 ? ` · ${groupDigits(rate)}/s` : ""}
      </p>

      {/* A BAR THAT REACHED 100% BECAUSE NOTHING MORE ARRIVED IS NOT THE SAME
          CLAIM AS ONE THAT REACHED THE WATERMARK. Only shown on a finished
          search: mid-scan it would fire and clear as partitions catch up. */}
      {progress.done && assumed.size > 0 && (
        <p className="search-progress-note" role="status">
          {assumed.size === 1 ? "Partition " : "Partitions "}
          {progress.assumed_complete.map((p, i) => (
            <span key={p}>
              {i > 0 ? ", " : ""}
              <code>P{p}</code>
            </span>
          ))}{" "}
          went quiet before reaching{" "}
          {assumed.size === 1 ? "its end offset" : "their end offsets"} —
          results there may be incomplete. Transactional markers usually explain
          this: they take offsets a consumer never receives, so the last one can
          never be read. A broker that stopped answering looks the same from
          here, which is why Kavka says so rather than assuming.
        </p>
      )}
      {progress.per_partition.length > 0 && (
        <ul className="pbars">
          {progress.per_partition.map((p) => {
            const span = Math.max(0, p.end_offset);
            // The fraction is against the partition's end offset, which is
            // where the search stops — not against its earliest, which a scan
            // that started mid-partition never intended to reach.
            const pct =
              span === 0
                ? 100
                : Math.max(
                    0,
                    Math.min(100, Math.round((p.current_offset / span) * 100)),
                  );
            return (
              <li className="pbar-row" key={p.partition}>
                <span className="pbar-label">P{p.partition}</span>
                <span className="pbar-track">
                  <span
                    className={`pbar-fill${running ? "" : " pbar-fill-done"}`}
                    style={{ width: `${pct}%` }}
                  />
                </span>
                <span className="pbar-num cell-num">
                  {groupDigits(p.current_offset)} / {groupDigits(p.end_offset)}
                </span>
                {/* Law 2: the state is a glyph plus words, never a colour or a
                    bar length — this bar looks identical to a finished one. */}
                {assumed.has(p.partition) && (
                  <span
                    className="pbar-assumed"
                    title="This partition stopped because nothing more arrived, not because it reached its end offset."
                  >
                    <span aria-hidden="true">≈</span>
                    <span className="sr-only">went quiet before its end</span>
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/**
 * Is this failure about the CEL program rather than about the cluster?
 *
 * Kept deliberately narrow. A wrong guess sends a broker failure to a field
 * error under the query box, where §5.3 says it does not belong, so anything
 * this does not clearly recognise goes to the banner.
 */
function looksLikeCelProblem(raw: string): boolean {
  const s = raw.toLowerCase();
  return (
    s.includes("cel") ||
    s.includes("undeclared") ||
    s.includes("unexpected token") ||
    s.includes("parse error") ||
    s.includes("failed to compile") ||
    s.includes("expression")
  );
}
