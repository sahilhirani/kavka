import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  MAX_FETCH_MESSAGES,
  errorMessage,
  messagesFetch,
  tailStart,
  tailStop,
  tailSubscribe,
  type ConnectionProfile,
  type FetchSpec,
  type MessageRecord,
  type PartitionDetail,
  type SeekSpec,
  type TailPayload,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { formatClock, fromDatetimeLocal, groupDigits, toDatetimeLocal } from "./format";
import { Term } from "./Glossary";
import MessageInspector from "./MessageInspector";
import { previewText } from "./payload";
import { ErrorBanner } from "./ProfileEditor";
import {
  isPinnedToBottom,
  scrollIndexIntoView,
  useRowHeightAssertion,
  useVirtualRows,
} from "./virtual";

/**
 * THE MESSAGE BROWSER
 *
 * Two things are worth reading before changing anything here.
 *
 * 1. THE TABLE IS VIRTUALIZED, so it carries `role="grid"`, `aria-rowcount`
 *    (the TOTAL, including the header) and `aria-rowindex` on every row —
 *    docs/DESIGN.md §5.2 says all three land in the same change as the
 *    virtualizer and not before, because the rendered count no longer matches
 *    the real one. `role="grid"` is a promise of an interactive widget, so
 *    j/k, the arrows, Home/End and ⏎ actually walk and open rows.
 *
 * 2. THE TAIL STATE MODEL. One session at a time, owned by one effect keyed on
 *    `tailing` plus the identity of what is being tailed. The effect starts
 *    the session, subscribes, and returns a cleanup that unsubscribes and
 *    stops it — so toggling off, navigating away and unmounting are the SAME
 *    path, and there is no fourth place to forget. `tailSubscribe` closes the
 *    unmount-before-registered race; `tailStop` is idempotent, so the extra
 *    call after an `ended` payload is free.
 *
 *    Rows are appended chronologically and the view stays pinned to the bottom
 *    until the user scrolls up, at which point new arrivals are counted into a
 *    "N new messages" chip instead of yanking the viewport. `dropped` is
 *    cumulative and is stated out loud in the status line: a tail that quietly
 *    loses messages is a tail that lies.
 */

/** How many live rows Kavka keeps. Beyond this the oldest are dropped — and
    the status line says so, because a silent buffer cap is a silent lie. */
const TAIL_BUFFER = 5000;

/** A tail with nothing arriving for this long is "quiet", and says so. */
const QUIET_AFTER_MS = 30_000;

type SeekMode = "earliest" | "latest" | "offset" | "timestamp";

type SeekField = "count" | "offset" | "timestamp" | "filter";

interface SeekError {
  field: SeekField;
  message: string;
}

function rowKey(record: MessageRecord): string {
  return `${record.partition}:${record.offset}`;
}

/**
 * The DOM id `aria-activedescendant` points at.
 *
 * Keyed by the record's address rather than its row index: a live tail appends
 * rows and trims the head, so an index-keyed id would name a different record
 * from one frame to the next and the announced row would drift.
 */
function rowDomId(record: MessageRecord): string {
  return `mv-row-${record.partition}-${record.offset}`;
}

/** `0, 3, 7` → [0,3,7]; empty or "all" → null (every partition). */
function parsePartitionFilter(
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

interface MessagesViewProps {
  profile: ConnectionProfile;
  topic: string;
  /** From topic_detail — the partition list, watermarks and "is it empty". */
  partitions: PartitionDetail[];
  onBack: () => void;
  onDanger: DangerReport;
}

export default function MessagesView({
  profile,
  topic,
  partitions,
  onBack,
  onDanger,
}: MessagesViewProps) {
  // ── Seek bar ───────────────────────────────────────────────────────────
  const [mode, setMode] = useState<SeekMode>("latest");
  const [count, setCount] = useState("100");
  const [seekPartition, setSeekPartition] = useState("0");
  const [seekOffset, setSeekOffset] = useState("0");
  const [when, setWhen] = useState(() => toDatetimeLocal(Date.now() - 3_600_000));
  const [filter, setFilter] = useState("");
  const [seekError, setSeekError] = useState<SeekError | null>(null);

  // ── Results ────────────────────────────────────────────────────────────
  const [rows, setRows] = useState<MessageRecord[]>([]);
  const [fetching, setFetching] = useState(false);
  const [fetched, setFetched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const fetchSeq = useRef(0);

  // ── Tail ───────────────────────────────────────────────────────────────
  const [tailing, setTailing] = useState(false);
  const [dropped, setDropped] = useState(0);
  const [trimmed, setTrimmed] = useState(false);
  const [tailEnded, setTailEnded] = useState(false);
  const [tailError, setTailError] = useState<string | null>(null);
  const [unseen, setUnseen] = useState(0);
  const [quiet, setQuiet] = useState(false);
  const pinnedRef = useRef(true);
  const tailingRef = useRef(false);
  const lastRecordAt = useRef(0);
  /** Total records this tail session has delivered, buffer cap included. */
  const received = useRef(0);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const firstRowRef = useRef<HTMLTableRowElement | null>(null);

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

  const { win, onScroll, remeasure } = useVirtualRows(scrollRef, rows.length);
  useRowHeightAssertion(firstRowRef, win.rowH, rows.length > 0);
  useDangerSignal(error !== null || tailError !== null, onDanger);

  const selectedIndex = useMemo(
    () =>
      selectedKey === null
        ? -1
        : rows.findIndex((r) => rowKey(r) === selectedKey),
    [rows, selectedKey],
  );
  const selected = selectedIndex >= 0 ? rows[selectedIndex] : null;

  // ── Fetching ───────────────────────────────────────────────────────────

  const buildSpec = useCallback((): FetchSpec | SeekError => {
    const parsedFilter = parsePartitionFilter(filter, known);
    if ("message" in parsedFilter)
      return { field: "filter", message: parsedFilter.message };

    const n = Number.parseInt(count, 10);
    if (!Number.isFinite(n) || n < 1)
      return { field: "count", message: "Ask for at least one message." };
    const capped = Math.min(n, MAX_FETCH_MESSAGES);

    let seek: SeekSpec;
    if (mode === "earliest") {
      seek = { kind: "earliest" };
    } else if (mode === "latest") {
      seek = { kind: "latest", last_n: capped };
    } else if (mode === "offset") {
      const p = Number.parseInt(seekPartition, 10);
      const o = Number.parseInt(seekOffset, 10);
      if (!Number.isInteger(p) || !known.has(p))
        return {
          field: "offset",
          message: `Pick a partition this topic has — 0 to ${
            Math.max(1, known.size) - 1
          }.`,
        };
      if (!Number.isInteger(o) || o < 0)
        return {
          field: "offset",
          message: "An offset counts from 0 — e.g. 8412",
        };
      seek = { kind: "offset", partition: p, offset: o };
    } else {
      const ms = fromDatetimeLocal(when);
      if (ms === null)
        return {
          field: "timestamp",
          message: "Pick a date and time to start from.",
        };
      seek = { kind: "timestamp", timestamp_ms: ms };
    }

    return {
      topic,
      seek,
      partitions: parsedFilter.partitions,
      max_messages: capped,
      // null = the core's own display cap. The inspector says when it bit.
      max_value_bytes: null,
    };
  }, [filter, known, count, mode, seekPartition, seekOffset, when, topic]);

  const runFetch = useCallback(async () => {
    const spec = buildSpec();
    if ("field" in spec) {
      setSeekError(spec);
      return;
    }
    setSeekError(null);
    const seq = ++fetchSeq.current;
    setFetching(true);
    setError(null);
    try {
      const records = await messagesFetch(profile.id, spec);
      if (fetchSeq.current !== seq) return;
      setRows(records);
      setSelectedKey(null);
      setTrimmed(false);
      setFetched(true);
      // A fetch is a range, not a stream: start the user at its beginning.
      if (scrollRef.current) scrollRef.current.scrollTop = 0;
    } catch (err) {
      if (fetchSeq.current === seq) setError(errorMessage(err));
    } finally {
      if (fetchSeq.current === seq) setFetching(false);
    }
  }, [buildSpec, profile.id]);

  // First paint fetches the default range rather than showing an empty table
  // with a button: the user asked for this topic's messages by clicking
  // "Browse messages", and asking twice is a dead end.
  const firstFetch = useRef(false);
  useEffect(() => {
    if (firstFetch.current) return;
    firstFetch.current = true;
    void runFetch();
    // Deliberately not re-run when runFetch's identity changes — the seek bar
    // is the user's, and re-fetching on every keystroke would fight them.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Invalidate any in-flight fetch when the topic (or connection) changes.
  useEffect(
    () => () => {
      fetchSeq.current += 1;
    },
    [profile.id, topic],
  );

  // ── Tail ───────────────────────────────────────────────────────────────

  const onBatch = useCallback((payload: TailPayload) => {
    setDropped(payload.dropped);
    if (payload.records.length > 0) {
      lastRecordAt.current = Date.now();
      setQuiet(false);
      // Counted outside the updater: a state setter called from inside another
      // setter's updater runs during render, which React is right to complain
      // about and StrictMode would run twice.
      received.current += payload.records.length;
      if (received.current > TAIL_BUFFER) setTrimmed(true);
      setRows((prev) => {
        const next = prev.concat(payload.records);
        return next.length > TAIL_BUFFER
          ? next.slice(next.length - TAIL_BUFFER)
          : next;
      });
      if (!pinnedRef.current) {
        setUnseen((prev) => prev + payload.records.length);
      }
    }
    if (payload.ended === true) {
      setTailEnded(true);
      setTailing(false);
    }
  }, []);

  const filterKey = filter.trim();
  useEffect(() => {
    if (!tailing) return;
    tailingRef.current = true;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    let started: string | null = null;

    const parsed = parsePartitionFilter(filterKey, known);
    const wanted = "message" in parsed ? null : parsed.partitions;

    void (async () => {
      try {
        const id = await tailStart(profile.id, topic, wanted);
        if (cancelled) {
          void tailStop(id);
          return;
        }
        started = id;
        unsubscribe = tailSubscribe(id, onBatch, (message) =>
          setTailError(message),
        );
      } catch (err) {
        if (cancelled) return;
        setTailError(errorMessage(err));
        setTailing(false);
      }
    })();

    return () => {
      cancelled = true;
      tailingRef.current = false;
      unsubscribe?.();
      // Idempotent on the Rust side, so the `ended` case costs nothing.
      if (started !== null) void tailStop(started);
    };
  }, [tailing, profile.id, topic, filterKey, known, onBatch]);

  const startTail = useCallback(() => {
    // The partition filter applies to the tail too, so a filter Kavka can't
    // parse must stop here — starting a tail on EVERY partition when the user
    // asked for three is the quiet kind of wrong.
    const parsed = parsePartitionFilter(filter, known);
    if ("message" in parsed) {
      setSeekError({ field: "filter", message: parsed.message });
      return;
    }
    setSeekError(null);
    // A tail starts at the live end, so keeping the fetched range on screen
    // would put an invisible gap in the middle of the table. Clear it and say
    // what the table now holds.
    setRows([]);
    setSelectedKey(null);
    setTrimmed(false);
    setUnseen(0);
    setDropped(0);
    setTailEnded(false);
    setTailError(null);
    setQuiet(false);
    lastRecordAt.current = Date.now();
    received.current = 0;
    pinnedRef.current = true;
    setTailing(true);
  }, [filter, known]);

  const stopTail = useCallback(() => setTailing(false), []);

  // "Listening. Nothing has been produced in the last 30 seconds." Silence is
  // a state; saying so stops people wondering whether the app is broken.
  useEffect(() => {
    if (!tailing) {
      setQuiet(false);
      return;
    }
    const timer = window.setInterval(() => {
      setQuiet(Date.now() - lastRecordAt.current > QUIET_AFTER_MS);
    }, 5000);
    return () => window.clearInterval(timer);
  }, [tailing]);

  // Pinned-to-bottom, and only while tailing: a fetched range must not scroll
  // itself away from the row the user is reading.
  useLayoutEffect(() => {
    if (!tailingRef.current || !pinnedRef.current) return;
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    // Re-window in the same commit. The spacers always add up to the full
    // list height, so scrollHeight is already correct — but the RENDERED
    // slice is still the one from before the jump, and waiting for the
    // scroll event to arrive would paint one frame of blank rows.
    onScroll();
  }, [rows, onScroll]);

  const handleScroll = useCallback(() => {
    onScroll();
    const atBottom = isPinnedToBottom(scrollRef.current);
    if (atBottom !== pinnedRef.current) pinnedRef.current = atBottom;
    if (atBottom && unseen !== 0) setUnseen(0);
  }, [onScroll, unseen]);

  const jumpToNewest = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    pinnedRef.current = true;
    setUnseen(0);
    remeasure();
  }, [remeasure]);

  // ── Keyboard: the promise `role="grid"` makes ──────────────────────────

  const moveTo = useCallback(
    (index: number) => {
      const clamped = Math.max(0, Math.min(rows.length - 1, index));
      const record = rows[clamped];
      if (!record) return;
      setSelectedKey(rowKey(record));
      scrollIndexIntoView(scrollRef.current, clamped, win);
      // Re-window in this same commit rather than waiting for the scroll event
      // to come back around. `aria-activedescendant` may only name a row that
      // is actually in the DOM, and the row we just scrolled to is outside the
      // rendered slice until the window catches up.
      onScroll();
      // Walking rows means the user is reading, not following the stream.
      pinnedRef.current = isPinnedToBottom(scrollRef.current);
    },
    [rows, win, onScroll],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (rows.length === 0) return;
      const cur = selectedIndex;
      switch (e.key) {
        case "ArrowDown":
        case "j":
          e.preventDefault();
          moveTo(cur < 0 ? win.start : cur + 1);
          break;
        case "ArrowUp":
        case "k":
          e.preventDefault();
          moveTo(cur < 0 ? win.start : cur - 1);
          break;
        case "Home":
          e.preventDefault();
          moveTo(0);
          break;
        case "End":
          e.preventDefault();
          moveTo(rows.length - 1);
          break;
        case "Enter":
          e.preventDefault();
          if (cur < 0) moveTo(win.start);
          break;
        case "Escape":
          if (selectedKey !== null) {
            e.preventDefault();
            setSelectedKey(null);
          }
          break;
        default:
          break;
      }
    },
    [rows.length, selectedIndex, selectedKey, moveTo, win.start],
  );

  // ── Render ─────────────────────────────────────────────────────────────

  const visible = rows.slice(win.start, win.end);
  /**
   * The row `aria-activedescendant` names, or nothing.
   *
   * Keyboard navigation is invisible to assistive technology without it: focus
   * never leaves the scrollport, so a screen reader has no way to know which
   * row `j`/`k` just moved to. `moveTo` scrolls the active row into view and
   * re-windows in the same commit, so on the keyboard path the id is always
   * rendered. It is dropped when the user scrolls the selected row out of the
   * window with the mouse — pointing at an element that is not in the DOM is
   * worse than pointing at nothing, and the selection is still announced by
   * `aria-selected` when the row comes back.
   */
  const activeDescendant =
    selected !== null && selectedIndex >= win.start && selectedIndex < win.end
      ? rowDomId(selected)
      : undefined;
  const topicIsEmpty = partitions.length > 0 && totalMessages === 0;
  const seekMessage = (field: SeekField) =>
    seekError?.field === field ? (
      <span className="field-error" id={`mv-${field}-error`}>
        {seekError.message}
      </span>
    ) : null;

  const busyReason = fetching
    ? "Kavka is asking the cluster for messages"
    : tailing
      ? "Stop the live tail to fetch a different range"
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
              {partitions.length}{" "}
              {partitions.length === 1 ? "partition" : "partitions"}
            </span>
          </h2>
          <div className="panel-tools">
            {/* A latched toggle is a physical switch: it has to read as ON
                across the room (§5.5). */}
            <button
              type="button"
              className={`btn${tailing ? " btn-latched" : ""}`}
              aria-pressed={tailing}
              onClick={() => (tailing ? stopTail() : startTail())}
              title={
                tailing
                  ? "Stop watching for new messages"
                  : "Watch messages arrive as they are produced"
              }
            >
              {/* No <Term> inside a button: the gloss is focusable, and a tab
                  stop nested in a control is a second thing to land on that
                  does nothing. The live-tail gloss lives in the empty state
                  below, where it is plain text. */}
              Live tail
            </button>
          </div>
        </div>

        {/* The seek bar. Plain-language modes; the Kafka word is never hidden
            — it is in the option text and in the hint under the control. */}
        <div className="seekbar" role="group" aria-label="Where to read from">
          <label className="seekbar-field">
            <span className="seekbar-label">Read from</span>
            <select
              value={mode}
              onChange={(e) => {
                setMode(e.target.value as SeekMode);
                setSeekError(null);
              }}
            >
              <option value="latest">The newest messages</option>
              <option value="earliest">The beginning of the topic</option>
              <option value="offset">A specific offset</option>
              <option value="timestamp">A point in time</option>
            </select>
          </label>

          {mode === "offset" && (
            <>
              <label className="seekbar-field">
                <span className="seekbar-label">
                  <Term name="partition">Partition</Term>
                </span>
                <select
                  value={seekPartition}
                  onChange={(e) => {
                    setSeekPartition(e.target.value);
                    setSeekError(null);
                  }}
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
                    seekError?.field === "offset" ? " input-invalid" : ""
                  }`}
                  value={seekOffset}
                  aria-invalid={seekError?.field === "offset" ? true : undefined}
                  aria-describedby={
                    seekError?.field === "offset" ? "mv-offset-error" : undefined
                  }
                  onChange={(e) => {
                    setSeekOffset(e.target.value);
                    setSeekError((prev) =>
                      prev?.field === "offset" ? null : prev,
                    );
                  }}
                />
              </label>
            </>
          )}

          {mode === "timestamp" && (
            <label className="seekbar-field">
              <span className="seekbar-label">From</span>
              <input
                type="datetime-local"
                step={1}
                className={`input-time${
                  seekError?.field === "timestamp" ? " input-invalid" : ""
                }`}
                value={when}
                aria-invalid={
                  seekError?.field === "timestamp" ? true : undefined
                }
                aria-describedby={
                  seekError?.field === "timestamp"
                    ? "mv-timestamp-error"
                    : undefined
                }
                onChange={(e) => {
                  setWhen(e.target.value);
                  setSeekError((prev) =>
                    prev?.field === "timestamp" ? null : prev,
                  );
                }}
              />
            </label>
          )}

          <label className="seekbar-field">
            <span className="seekbar-label">
              {mode === "latest" ? "How many" : "At most"}
            </span>
            <input
              type="number"
              min={1}
              max={MAX_FETCH_MESSAGES}
              step={1}
              className={`input-num${
                seekError?.field === "count" ? " input-invalid" : ""
              }`}
              value={count}
              aria-invalid={seekError?.field === "count" ? true : undefined}
              aria-describedby={
                seekError?.field === "count" ? "mv-count-error" : undefined
              }
              onChange={(e) => {
                setCount(e.target.value);
                setSeekError((prev) => (prev?.field === "count" ? null : prev));
              }}
            />
          </label>

          <label className="seekbar-field seekbar-field-wide">
            <span className="seekbar-label">Partitions</span>
            <input
              type="text"
              className={`input-mono${
                seekError?.field === "filter" ? " input-invalid" : ""
              }`}
              value={filter}
              placeholder="all"
              autoComplete="off"
              spellCheck={false}
              aria-invalid={seekError?.field === "filter" ? true : undefined}
              aria-describedby={
                seekError?.field === "filter" ? "mv-filter-error" : undefined
              }
              onChange={(e) => {
                setFilter(e.target.value);
                setSeekError((prev) => (prev?.field === "filter" ? null : prev));
              }}
            />
          </label>

          <button
            type="button"
            className="btn btn-primary"
            disabled={fetching || tailing}
            aria-busy={fetching || undefined}
            title={busyReason}
            onClick={() => void runFetch()}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {fetching ? <span className="spinner" /> : null}
            </span>
            Fetch
          </button>
        </div>

        {seekMessage("count")}
        {seekMessage("offset")}
        {seekMessage("timestamp")}
        {seekMessage("filter")}

        {error !== null && (
          <ErrorBanner raw={error} onDismiss={() => setError(null)} />
        )}
        {tailError !== null && (
          <ErrorBanner raw={tailError} onDismiss={() => setTailError(null)} />
        )}
        {tailEnded && (
          <div className="banner banner-warn" role="status">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">The live tail stopped.</p>
              <p className="banner-detail">
                The session ended on the Rust side — usually the connection
                dropped. The {groupDigits(rows.length)} messages already on
                screen are still here. Start it again to keep watching.
              </p>
            </div>
            <div className="banner-actions">
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => setTailEnded(false)}
              >
                Dismiss
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="messages-main">
        <div className="table-wrap messages-table-wrap">
          {fetching && <div className="table-loading" role="presentation" />}

          {/* THE VIRTUALIZED GRID. Two spacer rows carry the height of
              everything not rendered, so the scrollbar, the keyboard
              navigation and aria-rowindex all describe the same list. */}
          <div
            className="messages-scroll"
            ref={scrollRef}
            // The grid's focus target. role="group" so the label is actually
            // exposed — aria-label on a generic element is not guaranteed to
            // reach assistive technology.
            role="group"
            aria-label={`Messages in ${topic}`}
            tabIndex={0}
            aria-activedescendant={activeDescendant}
            onScroll={handleScroll}
            onKeyDown={onKeyDown}
          >
            <table
              className="data-table messages-table"
              role="grid"
              aria-rowcount={rows.length + 1}
            >
              <caption className="sr-only">
                Messages in {topic}
                {tailing ? ", live" : ""}
              </caption>
              <colgroup>
                <col className="mcol-offset" />
                <col className="mcol-part" />
                <col className="mcol-ts" />
                <col className="mcol-key" />
                <col className="mcol-value" />
              </colgroup>
              <thead>
                <tr aria-rowindex={1}>
                  <th scope="col" className="ledger-gutter">
                    <Term name="offset">Offset</Term>
                  </th>
                  <th scope="col" className="col-num">
                    Part.
                  </th>
                  <th scope="col">Time</th>
                  <th scope="col">Key</th>
                  <th scope="col">Value</th>
                </tr>
              </thead>
              <tbody>
                {win.padTop > 0 && (
                  <tr aria-hidden="true" className="row-pad">
                    <td colSpan={5} style={{ height: win.padTop, padding: 0 }} />
                  </tr>
                )}
                {visible.map((record, i) => {
                  const index = win.start + i;
                  const key = rowKey(record);
                  const tombstone = record.value === null;
                  return (
                    <tr
                      key={key}
                      // The id is what `aria-activedescendant` points at; the
                      // rowindex is the row's place in the WHOLE list, not in
                      // the rendered slice.
                      id={rowDomId(record)}
                      ref={i === 0 ? firstRowRef : undefined}
                      aria-rowindex={index + 2}
                      aria-selected={key === selectedKey}
                      className={[
                        key === selectedKey ? "row-selected" : "",
                        tombstone ? "row-tombstone" : "",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                      onClick={() => setSelectedKey(key)}
                    >
                      <td className="ledger-gutter">
                        {tombstone && (
                          <span className="tomb-tick" aria-hidden="true">
                            •
                          </span>
                        )}
                        {groupDigits(record.offset)}
                      </td>
                      <td className="col-num cell-num">{record.partition}</td>
                      <td className="cell-mono">
                        {record.timestamp_ms === null ? (
                          <span
                            className="absent"
                            title="This message carries no timestamp."
                          >
                            ∅
                          </span>
                        ) : (
                          formatClock(record.timestamp_ms)
                        )}
                      </td>
                      <td className="cell-mono cell-preview">
                        {record.key === null ? (
                          <span
                            className="absent"
                            title="No key — Kafka spread this message across partitions."
                          >
                            ∅
                          </span>
                        ) : (
                          previewText(record.key)
                        )}
                      </td>
                      <td className="cell-mono cell-preview">
                        {tombstone ? (
                          <>
                            <span className="absent">∅</span>
                            <span className="cell-tag"> tombstone</span>
                          </>
                        ) : (
                          previewText(record.value)
                        )}
                      </td>
                    </tr>
                  );
                })}
                {win.padBottom > 0 && (
                  <tr aria-hidden="true" className="row-pad">
                    <td
                      colSpan={5}
                      style={{ height: win.padBottom, padding: 0 }}
                    />
                  </tr>
                )}
              </tbody>
            </table>

            {rows.length === 0 && !fetching && (
              <div className="messages-empty">
                {!fetched ? (
                  <p className="empty-hint">
                    Asking the cluster for messages in{" "}
                    <code>{topic}</code>…
                  </p>
                ) : tailing ? (
                  <p className="empty-hint">
                    Listening. Nothing has been produced to{" "}
                    <code>{topic}</code> since the tail started.
                  </p>
                ) : topicIsEmpty ? (
                  <>
                    <p className="empty-hint">
                      No messages in <code>{topic}</code> yet. Start{" "}
                      <Term name="live-tail">live tail</Term> and Kavka will
                      show them as they arrive.
                    </p>
                    <div className="empty-actions">
                      <button type="button" className="btn" onClick={startTail}>
                        Start live tail
                      </button>
                    </div>
                  </>
                ) : (
                  <>
                    <p className="empty-hint">
                      Nothing in that range. <code>{topic}</code> holds about{" "}
                      {groupDigits(totalMessages)} messages — try reading from
                      the beginning, or widen the partition filter.
                    </p>
                    <div className="empty-actions">
                      <button
                        type="button"
                        className="btn"
                        onClick={() => {
                          setMode("earliest");
                          setFilter("");
                        }}
                      >
                        Read from the beginning
                      </button>
                    </div>
                  </>
                )}
              </div>
            )}
          </div>

          {/* The chip only exists while the user is behind the stream — it is
              an offer to catch up, never a thing that moves the view for them. */}
          {unseen > 0 && (
            <button
              type="button"
              className="newmsg-chip"
              onClick={jumpToNewest}
            >
              {groupDigits(unseen)} new{" "}
              {unseen === 1 ? "message" : "messages"} — jump to newest
            </button>
          )}
        </div>

        {selected !== null && (
          <MessageInspector
            record={selected}
            topic={topic}
            onClose={() => setSelectedKey(null)}
          />
        )}
      </div>

      {/* The status line: counts, the tail's honesty, and the two shortcuts
          that matter here. Mirrors the app status bar directly below it. */}
      <div className="view-statusline">
        <span className="statusbar-item">
          {groupDigits(rows.length)} {rows.length === 1 ? "message" : "messages"}
          {trimmed ? ` · showing the last ${groupDigits(TAIL_BUFFER)}` : ""}
        </span>
        {tailing && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            <span className="statusbar-item statusline-live">
              <span className="status-dot status-connected" aria-hidden="true" />
              {quiet ? "Listening — quiet for 30s" : "Live"}
            </span>
          </>
        )}
        {dropped > 0 && (
          <>
            <span className="statusbar-sep" aria-hidden="true">
              ·
            </span>
            {/* Stated, never hidden: a tail that silently loses messages is a
                tail that lies about what the topic contains. */}
            <span
              className="statusbar-item statusline-dropped"
              title="Messages arrived faster than Kavka could hand them to the window, so the session dropped these rather than falling behind."
            >
              {groupDigits(dropped)} dropped
            </span>
          </>
        )}
        <span className="statusbar-right">
          <span className="statusbar-item">
            <span className="kbd">j</span>
            <span className="kbd">k</span> walk rows
          </span>
          <span className="statusbar-sep" aria-hidden="true">
            ·
          </span>
          <span className="statusbar-item">
            <span className="kbd">Esc</span> close inspector
          </span>
        </span>
      </div>
    </section>
  );
}
