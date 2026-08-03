import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  type TailPayload,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { looksLikeDlqTopic, replayBlockedWhy } from "./dlq";
import ExportButton from "./ExportButton";
import { groupDigits } from "./format";
import { Term } from "./Glossary";
import MessageGrid, { rowKey, type MessageGridHandle } from "./MessageGrid";
import MessageInspector from "./MessageInspector";
import {
  maskingChipLabel,
  maskingChipTitle,
  noteMaskedRecords,
  useMasking,
} from "./masking";
import { noteJsonFields } from "./nl";
import { ErrorBanner } from "./ProfileEditor";
import SeekBar, {
  buildSeek,
  initialSeekState,
  parsePartitionFilter,
  type SeekError,
  type SeekField,
  type SeekState,
} from "./SeekBar";
import type { ToastSpec } from "./Toast";

/**
 * THE MESSAGE BROWSER.
 *
 * Two things are worth reading before changing anything here.
 *
 * 1. THE TABLE AND THE SEEK BAR ARE NOT THIS FILE'S ANY MORE. Phase 2 gave
 *    search the same grid and the same "where to read from" controls, so both
 *    moved to MessageGrid and SeekBar — including the virtualization, the
 *    `role="grid"` keyboard contract and the partition-filter parsing. What is
 *    left here is what is genuinely the BROWSER's: a fetch, a live tail, and
 *    the honesty the tail owes about what it dropped.
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

interface MessagesViewProps {
  profile: ConnectionProfile;
  topic: string;
  /** From topic_detail — the partition list, watermarks and "is it empty". */
  partitions: PartitionDetail[];
  onBack: () => void;
  onDanger: DangerReport;
  /** Toasts belong to the topic view, so one stack serves every child. */
  push: (spec: ToastSpec) => void;
  onSearch: () => void;
  onProduce: () => void;
  /**
   * Land on a specific record instead of the newest — "View it" on the toast
   * a produce just raised. Read once, on mount, by the keyed remount that
   * brings it in.
   */
  initialSeek?: { partition: number; offset: number } | null;
  /** Open the topic a dead letter originally failed on, at that record. */
  onBrowseOriginal?: (topic: string, partition: number, offset: number) => void;
  /** Send a dead-lettered record back to the topic it came from. */
  onReproduce?: (record: MessageRecord) => void;
}

export default function MessagesView({
  profile,
  topic,
  partitions,
  onBack,
  onDanger,
  push,
  onSearch,
  onProduce,
  initialSeek = null,
  onBrowseOriginal,
  onReproduce,
}: MessagesViewProps) {
  // ── Seek bar ───────────────────────────────────────────────────────────
  const [seek, setSeek] = useState<SeekState>(() => {
    const base = initialSeekState("latest", "100");
    if (initialSeek === null) return base;
    return {
      ...base,
      mode: "offset",
      partition: String(initialSeek.partition),
      offset: String(initialSeek.offset),
      count: "100",
    };
  });
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
  /** Records this tail session has delivered, whether or not they are still on
      screen — the denominator the export note needs. */
  const [seen, setSeen] = useState(0);
  const [tailEnded, setTailEnded] = useState(false);
  const [tailError, setTailError] = useState<string | null>(null);
  const [unseen, setUnseen] = useState(0);
  const [quiet, setQuiet] = useState(false);
  /** The grid owns the scrollport, so it owns this; here it is only mirrored. */
  const pinnedRef = useRef(true);
  const lastRecordAt = useRef(0);
  /** Total records this tail session has delivered, buffer cap included. */
  const received = useRef(0);

  const gridRef = useRef<MessageGridHandle | null>(null);

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

  useDangerSignal(error !== null || tailError !== null, onDanger);

  const selected = useMemo(
    () => rows.find((r) => rowKey(r) === selectedKey) ?? null,
    [rows, selectedKey],
  );

  // ── Fetching ───────────────────────────────────────────────────────────

  const buildSpec = useCallback((): FetchSpec | SeekError => {
    const built = buildSeek(seek, known, {
      maxCount: MAX_FETCH_MESSAGES,
      needCount: true,
    });
    if ("field" in built) return built;
    return {
      topic,
      seek: built.seek,
      partitions: built.partitions,
      max_messages: built.count,
      // null = the core's own display cap. The inspector says when it bit.
      max_value_bytes: null,
    };
  }, [seek, known, topic]);

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
      // What this batch teaches the rest of the window: the payload's field
      // names (the plain-English bar's only source for them) and whether the
      // core masked any of it (the status bar's chip, and the export note).
      noteJsonFields(profile.id, topic, records);
      noteMaskedRecords(profile.id, records);
      setRows(records);
      setSelectedKey(null);
      setTrimmed(false);
      // A fetched range replaces whatever a previous tail session showed;
      // its drop/seen counters must not haunt this complete result set.
      setDropped(0);
      setSeen(0);
      setFetched(true);
      // A fetch is a range, not a stream: start the user at its beginning.
      gridRef.current?.scrollToTop();
    } catch (err) {
      if (fetchSeq.current === seq) setError(errorMessage(err));
    } finally {
      if (fetchSeq.current === seq) setFetching(false);
    }
  }, [buildSpec, profile.id, topic]);

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

  const onBatch = useCallback(
    (payload: TailPayload) => {
      setDropped(payload.dropped);
      if (payload.records.length > 0) {
        lastRecordAt.current = Date.now();
        setQuiet(false);
        // Same two facts as a fetch, on every batch: a tail is where a masking
        // rule most often first bites, and where the payload's field names
        // arrive from on a topic nobody has browsed yet.
        noteJsonFields(profile.id, topic, payload.records);
        noteMaskedRecords(profile.id, payload.records);
        // Counted outside the updater: a state setter called from inside
        // another setter's updater runs during render, which React is right to
        // complain about and StrictMode would run twice.
        received.current += payload.records.length;
        if (received.current > TAIL_BUFFER) setTrimmed(true);
        setSeen(received.current);
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
    },
    [profile.id, topic],
  );

  const filterKey = seek.filter.trim();
  useEffect(() => {
    if (!tailing) return;
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
      unsubscribe?.();
      // Idempotent on the Rust side, so the `ended` case costs nothing.
      if (started !== null) void tailStop(started);
    };
  }, [tailing, profile.id, topic, filterKey, known, onBatch]);

  const startTail = useCallback(() => {
    // The partition filter applies to the tail too, so a filter Kavka can't
    // parse must stop here — starting a tail on EVERY partition when the user
    // asked for three is the quiet kind of wrong.
    const parsed = parsePartitionFilter(seek.filter, known);
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
    setSeen(0);
    setUnseen(0);
    setDropped(0);
    setTailEnded(false);
    setTailError(null);
    setQuiet(false);
    lastRecordAt.current = Date.now();
    received.current = 0;
    pinnedRef.current = true;
    setTailing(true);
  }, [seek.filter, known]);

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

  const onPinnedChange = useCallback((pinned: boolean) => {
    pinnedRef.current = pinned;
    if (pinned) setUnseen(0);
  }, []);

  const jumpToNewest = useCallback(() => {
    gridRef.current?.scrollToNewest();
    setUnseen(0);
  }, []);

  // ── Render ─────────────────────────────────────────────────────────────

  const topicIsEmpty = partitions.length > 0 && totalMessages === 0;

  /** The masking rules in force on this connection — see the status line. */
  const masking = useMasking(profile.id);

  /**
   * THE DLQ EMPTY STATE, which is a teaching state rather than an error.
   *
   * A topic called `orders.dlq` whose records carry no headers Kavka
   * recognises produces a question the user is about to ask out loud, and the
   * honest answer is specific: the two conventions Kavka knows, and the fact
   * that a home-grown one is not one of them. Naming a topic is not evidence
   * about its records, so this NEVER decides whether something is a dead
   * letter — `record.dlq` does — it only decides whether to say something.
   */
  const dlqExpected = looksLikeDlqTopic(topic);
  const sawDlq = rows.some((r) => r.dlq != null);
  const teachDlq = dlqExpected && rows.length > 0 && !sawDlq;

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
            <button
              type="button"
              className="btn"
              onClick={onSearch}
              title="Scan the whole topic for messages that match"
            >
              Search
            </button>
            <button
              type="button"
              className={`btn ${
                profile.environment === "prod" ? "btn-danger" : ""
              }`}
              disabled={profile.read_only}
              title={
                profile.read_only
                  ? "This connection is read-only. Turn that off in the connection's settings to produce or edit."
                  : "Send a message to this topic"
              }
              onClick={onProduce}
            >
              Produce
            </button>
            {/* A tail's window rolls: what is on screen can be less than what
                the session delivered, and a file that quietly holds the last
                5 000 of 40 000 is the same lie as a truncated search. */}
            <ExportButton
              records={rows}
              topic={topic}
              capped={
                trimmed || dropped > 0
                  ? { shown: rows.length, total: seen + dropped, kind: "tail" }
                  : null
              }
              push={push}
            />
          </div>
        </div>

        <SeekBar
          idPrefix="mv"
          label="Where to read from"
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
          countMax={MAX_FETCH_MESSAGES}
        >
          <div className="seekbar-field seekbar-actions">
            <span className="seekbar-label" aria-hidden="true">
              &nbsp;
            </span>
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
        </SeekBar>

        {error !== null && (
          <ErrorBanner raw={error} onDismiss={() => setError(null)} />
        )}
        {tailError !== null && (
          <ErrorBanner raw={tailError} onDismiss={() => setTailError(null)} />
        )}
        {teachDlq && (
          <p className="table-note" role="note">
            No recognised dead-letter headers on these {groupDigits(rows.length)}{" "}
            {rows.length === 1 ? "record" : "records"}. Kavka knows two
            conventions — Kafka Connect's <code>__connect.errors.*</code> and
            Spring for Apache Kafka's <code>kafka_dlt-*</code> — and shows a Dead
            letter column, the original coordinates and the exception whenever
            one of them matches. A dead-letter topic written by your own code
            carries whatever headers that code chose, and Kavka won't guess at
            them; the Headers tab in the inspector shows what is actually there.
          </p>
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
        <MessageGrid
          ref={gridRef}
          records={rows}
          label={`Messages in ${topic}${tailing ? ", live" : ""}`}
          idPrefix="mv"
          selectedKey={selectedKey}
          onSelect={setSelectedKey}
          follow={tailing}
          onPinnedChange={onPinnedChange}
          loading={fetching}
          empty={
            fetching ? null : !fetched ? (
              <p className="empty-hint">
                Asking the cluster for messages in <code>{topic}</code>…
              </p>
            ) : tailing ? (
              <p className="empty-hint">
                Listening. Nothing has been produced to <code>{topic}</code>{" "}
                since the tail started.
              </p>
            ) : topicIsEmpty ? (
              <>
                <p className="empty-hint">
                  No messages in <code>{topic}</code> yet. Start{" "}
                  <Term name="live-tail">live tail</Term> and Kavka will show
                  them as they arrive.
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
                  {groupDigits(totalMessages)} messages — try reading from the
                  beginning, or widen the partition filter.
                </p>
                <div className="empty-actions">
                  <button
                    type="button"
                    className="btn"
                    onClick={() =>
                      setSeek((prev) => ({
                        ...prev,
                        mode: "earliest",
                        filter: "",
                      }))
                    }
                  >
                    Read from the beginning
                  </button>
                </div>
              </>
            )
          }
        >
          {/* The chip only exists while the user is behind the stream — it is
              an offer to catch up, never a thing that moves the view for them. */}
          {unseen > 0 && (
            <button type="button" className="newmsg-chip" onClick={jumpToNewest}>
              {groupDigits(unseen)} new {unseen === 1 ? "message" : "messages"} —
              jump to newest
            </button>
          )}
        </MessageGrid>

        {selected !== null && (
          <MessageInspector
            record={selected}
            topic={topic}
            onClose={() => setSelectedKey(null)}
            onBrowseOriginal={onBrowseOriginal}
            onReproduce={onReproduce}
            // Read-only is a fact about the connection; masked is a fact about
            // THIS record — the core rewrote it before it crossed IPC, so the
            // original bytes are not in this window to send. Both are the same
            // kind of answer, so they share one function.
            reproduceBlocked={replayBlockedWhy(profile.read_only, selected)}
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
        {/* Masking belongs beside the rows it changed, not only in the app's
            status bar: this is the table someone is about to screenshot. */}
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
