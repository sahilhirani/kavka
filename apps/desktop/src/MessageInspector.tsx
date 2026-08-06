import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  DEFAULT_MAX_VALUE_BYTES,
  type DecodedPayload,
  type DlqMeta,
  type MessageRecord,
} from "./api";
import { copyText } from "./clipboard";
import { conventionWord, droppedBinaryCount, droppedHeaderCount } from "./dlq";
import { formatBytes, formatStamp, groupDigits } from "./format";
import { Term } from "./Glossary";
import {
  PRETTY_LIMIT,
  RENDER_LIMIT,
  plainLines,
  prettyJsonLines,
  provenance,
  type Tok,
} from "./payload";

type InspectorTab = "value" | "key" | "headers" | "raw";

/**
 * The four tabs, in order, so the strip's rendering and its arrow keys read
 * from one list. `Headers` gains its count at the call site — the label here
 * is the stable name the keyboard model walks.
 */
const TABS: ReadonlyArray<readonly [InspectorTab, string]> = [
  ["value", "Value"],
  ["key", "Key"],
  ["headers", "Headers"],
  ["raw", "Raw"],
];

/**
 * Above this many lines the inspector stops rendering and says so. DESIGN
 * §5.10 asks for line-level virtualization at the cut; this is the honest
 * interim: a hard stop with a sentence, never a silently shortened payload.
 */
const MAX_LINES = 4000;

/**
 * HOW THESE BYTES WERE READ, SAID BEFORE THEY ARE SHOWN.
 *
 * The order is the whole claim. Kavka's pretty-printed object is Kavka's
 * GUESS — the bytes on the broker are bytes — and a reader who meets the
 * prettiness first has already believed it by the time a footnote under a
 * button row tells them where it came from. The fidelity audit found this
 * sentence rendered last, unstyled, below Copy value; the mockup opens every
 * tabpanel with it, inset, on an `--info` edge, with an info circle.
 *
 * It is deliberately not dismissible and deliberately not a disclosure: a
 * qualification one click away is a qualification that gets quoted without it.
 */
function Provenance({ children }: { children: React.ReactNode }) {
  return (
    <p className="provenance" role="note">
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        aria-hidden="true"
        focusable="false"
      >
        <circle cx="12" cy="12" r="9" />
        <path d="M12 11v5M12 7.6v.9" />
      </svg>
      <span>{children}</span>
    </p>
  );
}

interface MessageInspectorProps {
  record: MessageRecord;
  topic: string;
  onClose: () => void;
  /**
   * Walk to the previous (-1) or next (+1) row. Absent where the inspector has
   * no list behind it. The caller hands this to the grid's own `step`, so the
   * buttons and `j`/`k` are one code path.
   */
  onStep?: (delta: -1 | 1) => void;
  /** False at the ends of the list — the buttons disable rather than vanish. */
  canPrev?: boolean;
  canNext?: boolean;
  /**
   * Open the topic this record originally failed on, at the exact record.
   * Absent where there is nowhere to navigate to (search results live inside
   * one topic's screen, and a jump out of them would lose the search).
   */
  onBrowseOriginal?: (topic: string, partition: number, offset: number) => void;
  /** Send this record back to the topic it came from — see ProducePanel. */
  onReproduce?: (record: MessageRecord) => void;
  /** Set = re-producing is unavailable, and this is the sentence saying why. */
  reproduceBlocked?: string;
}

/**
 * The payload inspector (docs/DESIGN.md §5.10).
 *
 * Right-docked, `--bg-sunken` body, mono, and line numbers in a
 * `--gutter-w-code` gutter carrying the same `border-right: 1px solid
 * var(--rule)` as every table — the ledger device at document scale, so prod
 * is coral here too.
 *
 * Pretty JSON is the landing state; Raw is one click away and never the
 * default. §5.10's fifth tab — Hex — is NOT here yet, and deliberately not a
 * disabled stub: the IPC contract hands the UI a decoded `text`, never the
 * bytes, so anything labelled Hex today would be a hex dump of someone's lossy
 * UTF-8 rather than of the record. It lands with the raw-bytes field in Phase
 * 2; see docs/DESIGN.md §11.
 */
export default function MessageInspector({
  record,
  topic,
  onClose,
  onStep,
  canPrev = false,
  canNext = false,
  onBrowseOriginal,
  onReproduce,
  reproduceBlocked,
}: MessageInspectorProps) {
  const [tab, setTab] = useState<InspectorTab>("value");
  const [raw, setRaw] = useState(false);
  const [forced, setForced] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<number | null>(null);
  const tabRefs = useRef<Partial<Record<InspectorTab, HTMLButtonElement | null>>>(
    {},
  );

  /** Arrow keys walk the strip; selection follows focus, as in every other
      tablist in the app (ClusterView's is the reference implementation). */
  const onTabKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
      let next: number | null = null;
      if (e.key === "ArrowRight") next = (index + 1) % TABS.length;
      else if (e.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = TABS.length - 1;
      if (next === null) return;
      e.preventDefault();
      const [key] = TABS[next];
      setTab(key);
      tabRefs.current[key]?.focus();
    },
    [],
  );

  // A new record is a new payload: every per-payload toggle resets, or the
  // user lands on "Raw" for a message they never asked to see raw.
  const recordKey = `${record.partition}:${record.offset}`;
  useEffect(() => {
    setRaw(false);
    setForced(false);
    setExpanded(false);
  }, [recordKey]);

  useEffect(
    () => () => {
      if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    },
    [],
  );

  const flash = useCallback((what: string, ok: boolean) => {
    setCopied(ok ? `${what} copied` : `Kavka couldn't reach the clipboard`);
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => setCopied(null), 2400);
  }, []);

  const copy = useCallback(
    async (what: string, text: string) => {
      flash(what, await copyText(text));
    },
    [flash],
  );

  const payload: DecodedPayload | null =
    tab === "key" ? record.key : tab === "headers" ? null : record.value;

  const tooBigToPretty =
    payload !== null && payload.text.length > PRETTY_LIMIT && !forced;
  const usePretty =
    payload !== null &&
    payload.json !== null &&
    tab !== "raw" &&
    !raw &&
    !tooBigToPretty;

  const visibleText = useMemo(() => {
    if (payload === null) return "";
    if (payload.text.length <= RENDER_LIMIT || expanded) return payload.text;
    return payload.text.slice(0, RENDER_LIMIT);
  }, [payload, expanded]);

  const lines: Tok[][] = useMemo(() => {
    if (payload === null) return [];
    if (usePretty && payload.json !== null) {
      try {
        return prettyJsonLines(payload.json);
      } catch {
        // A pathological value (a cycle can't come out of serde, but a
        // 50-deep array can blow the stack) falls back to the text we were
        // handed rather than taking the panel down.
        return plainLines(payload.text);
      }
    }
    return plainLines(visibleText);
  }, [payload, usePretty, visibleText]);

  const clipped = lines.length > MAX_LINES;
  const shownLines = clipped ? lines.slice(0, MAX_LINES) : lines;

  const stamp =
    record.timestamp_ms === null ? null : formatStamp(record.timestamp_ms);

  const recordJson = useMemo(
    () =>
      JSON.stringify(
        {
          topic,
          partition: record.partition,
          offset: record.offset,
          timestamp_ms: record.timestamp_ms,
          key: record.key?.json ?? record.key?.text ?? null,
          value: record.value?.json ?? record.value?.text ?? null,
          headers: record.headers.map((h) => ({ [h.key]: h.value })),
        },
        null,
        2,
      ),
    [record, topic],
  );

  const masked = record.masked === true;

  return (
    <aside
      className={`inspector${masked ? " inspector-masked" : ""}`}
      aria-label={`Message at offset ${record.offset}, partition ${record.partition}${
        masked ? ", masked" : ""
      }`}
    >
      <header className="inspector-head">
        {/* No gloss here: this line is the record's address in mono, and a
            dotted underline through it reads as damage. `offset` is glossed
            on the table's column head, one view over. */}
        <span className="inspector-address">
          Partition {record.partition} · Offset {groupDigits(record.offset)}
          {stamp !== null ? ` · ${stamp}` : ""}
        </span>
        {/* Walking the list from the panel head, as the mockup has it. The
            keycaps in the status line stay: they are the faster path and this
            is the discoverable one, and both call the grid's `step` so they
            cannot disagree about what "next" means. `.btn-sm` is the floor —
            24px, SC 2.5.8 — and the arrow is captioned for anyone who cannot
            see it. */}
        {onStep !== undefined && (
          <span className="inspector-step">
            <button
              type="button"
              className="btn btn-sm"
              disabled={!canPrev}
              onClick={() => onStep(-1)}
              // A disabled control always says why (§5.5), and at the top of
              // the list "there is nothing above this" is the why.
              title={
                canPrev
                  ? "The message above this one in the table"
                  : "This is the first message in the table."
              }
            >
              <span aria-hidden="true">↑</span>
              <span className="sr-only">Previous message</span>
            </button>
            <button
              type="button"
              className="btn btn-sm"
              disabled={!canNext}
              onClick={() => onStep(1)}
              title={
                canNext
                  ? "The message below this one in the table"
                  : "This is the last message in the table."
              }
            >
              <span aria-hidden="true">↓</span>
              <span className="sr-only">Next message</span>
            </button>
          </span>
        )}
        <button
          type="button"
          className="btn btn-ghost inspector-close"
          onClick={onClose}
          title="Close the inspector"
        >
          Close
        </button>
      </header>

      {/* THE DEAD LETTER SECTION.
          Above the tabs, and that is deliberate: §5.10 keeps chrome off the
          top of this panel because the payload is the loudest thing here — but
          on a dead letter the payload is not what the user came for. They came
          for what killed it and where it came from, and burying that under a
          tab would make the one screen that explains a failure the one screen
          you have to go looking in. It is only ever rendered when the core
          actually matched a convention. */}
      {record.dlq != null && (
        <DeadLetterSection
          dlq={record.dlq}
          record={record}
          onBrowseOriginal={onBrowseOriginal}
          onReproduce={onReproduce}
          reproduceBlocked={reproduceBlocked}
        />
      )}

      {/* MASKED, AND SAID BEFORE THE PAYLOAD IS READ. The replacement text is
          what arrived — the core rewrote it on the decoded record before it
          crossed IPC, so nothing in this window ever held the original and
          there is deliberately no reveal control. Above the tabs for the same
          reason the dead-letter section is: it changes what everything below
          it means. */}
      {masked && (
        <p className="mask-notice" role="note">
          <span className="mask-notice-mark" aria-hidden="true">
            •••
          </span>
          A masking rule rewrote part of this record on its way here. What you
          are reading is not verbatim, and copies and exports carry the same
          replacement — Kavka masks in its core, so the original bytes are not
          in this window. Turn the rule off in the Masking tab and fetch again
          to see the real values.
        </p>
      )}

      {/* A tablist is one tab stop walked with the arrows, and every tab
          points at the panel it opens. Both halves were missing: four tabs
          all in the tab order, arrow keys that did nothing, and a body that
          was never announced as the tab's panel (SC 2.1.1, SC 4.1.2). */}
      <div className="modal-tabs inspector-tabs" role="tablist" aria-label="Payload">
        {TABS.map(([key, label], index) => (
          <button
            key={key}
            type="button"
            role="tab"
            id={`insp-tab-${key}`}
            aria-selected={tab === key}
            aria-controls="insp-panel"
            tabIndex={tab === key ? 0 : -1}
            ref={(el) => {
              tabRefs.current[key] = el;
            }}
            className={`tab${tab === key ? " tab-active" : ""}`}
            onClick={() => setTab(key)}
            onKeyDown={(e) => onTabKeyDown(e, index)}
          >
            {/* The count is muted and inside the label, as the mockup draws
                it: it qualifies the word rather than competing with it. It is
                still part of the tab's accessible name, which is what a screen
                reader announces when the arrow keys land here. */}
            {key === "headers" && record.headers.length > 0 ? (
              <>
                {label} <span className="tab-count">{record.headers.length}</span>
              </>
            ) : (
              label
            )}
          </button>
        ))}
      </div>

      {/* One panel element for all four tabs — its contents swap, its
          identity does not, so `aria-controls` always resolves. */}
      <div
        className="inspector-panel"
        id="insp-panel"
        role="tabpanel"
        aria-labelledby={`insp-tab-${tab}`}
      >
      {tab === "headers" ? (
        <div className="inspector-body inspector-body-plain">
          {record.headers.length === 0 ? (
            <p className="inspector-note">
              This message carries no headers. Producers use them for tracing
              ids, content types and routing hints — this one sent none.
            </p>
          ) : (
            <>
              <Provenance>
                Headers are bytes too. Everything shown as text decoded as text
                on its own; anything that did not is Kavka's hex rendering of
                the bytes, tagged <code>bytes</code>, and a header with no value
                at all shows ∅ rather than an empty string.
              </Provenance>
              <table className="data-table data-table-flush">
              <caption className="sr-only">Headers on this message</caption>
              <thead>
                <tr>
                  <th scope="col">Key</th>
                  <th scope="col">Value</th>
                </tr>
              </thead>
              <tbody>
                {record.headers.map((h, i) => (
                  <tr key={`${h.key}:${i}`}>
                    <td className="cell-mono">{h.key}</td>
                    <td className="cell-mono">
                      {h.value === null ? (
                        <span className="absent" title="This header has no value.">
                          ∅
                        </span>
                      ) : h.is_text ? (
                        h.value
                      ) : (
                        <>
                          {h.value}
                          <span className="cell-tag"> bytes</span>
                        </>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
              </table>
            </>
          )}
        </div>
      ) : payload === null ? (
        <div className="inspector-body inspector-body-plain">
          {tab === "key" ? (
            <p className="inspector-note">
              <span className="absent">∅</span> This message has no key, so
              Kafka spread it across partitions rather than pinning it to one.
            </p>
          ) : (
            <p className="inspector-note">
              <span className="absent">∅</span> This is a{" "}
              <Term name="tombstone">tombstone</Term> — a record with no value
              at all. On a compacted topic it is how a producer says this key is
              deleted.
            </p>
          )}
        </div>
      ) : (
        <>
          {/* PROVENANCE FIRST, THEN THE PAYLOAD. The one thing that goes above
              the body, and it is not chrome — it is the sentence that decides
              what everything under it means. See `Provenance`. */}
          <Provenance>
            {tab === "raw" || raw ? (
              <>
                The text Kavka decoded, with no JSON layout applied — line
                breaks and spacing are the producer's, not Kavka's. It is not a
                byte dump: the core decoded these bytes to text before they
                crossed into this window, so anything that was not valid UTF-8
                was replaced on the way.
              </>
            ) : (
              <>
                {provenance(payload)}
                {provenance(payload).endsWith(".") ? "" : "."}{" "}
                {formatBytes(payload.raw_len)} on the broker.
              </>
            )}
          </Provenance>

          {/* Body next, controls under it: the payload is the loudest thing on
              this panel and nothing else chrome-shaped goes above it. */}
          <div className="inspector-body">
            {tooBigToPretty && (
              <p className="inspector-note">
                Shown as text — this value is {formatBytes(payload.raw_len)}, and
                laying it out as JSON would block the window.{" "}
                <button
                  type="button"
                  className="btn btn-ghost inspector-inline-btn"
                  onClick={() => setForced(true)}
                >
                  Format JSON ({formatBytes(payload.raw_len)} — may take a moment)
                </button>
              </p>
            )}
            {shownLines.map((toks, i) => (
              <div className="code-line" key={i}>
                <span className="code-gutter" aria-hidden="true">
                  {i + 1}
                </span>
                <span className="code-text">
                  {toks.map((tok, j) =>
                    tok.cls === "plain" ? (
                      <span key={j}>{tok.text}</span>
                    ) : (
                      <span key={j} className={`syn-${tok.cls}`}>
                        {tok.text}
                      </span>
                    ),
                  )}
                </span>
              </div>
            ))}
            {clipped && (
              <p className="inspector-note">
                Showing the first {groupDigits(MAX_LINES)} lines of{" "}
                {groupDigits(lines.length)}. Copy the value to read the rest
                somewhere with more room.
              </p>
            )}
            {!clipped &&
              !expanded &&
              payload.text.length > RENDER_LIMIT &&
              !usePretty && (
                <p className="inspector-note">
                  Showing the first {formatBytes(RENDER_LIMIT)} of{" "}
                  {formatBytes(payload.raw_len)}.{" "}
                  <button
                    type="button"
                    className="btn btn-ghost inspector-inline-btn"
                    onClick={() => setExpanded(true)}
                  >
                    Load the rest
                  </button>
                </p>
              )}
          </div>

          <div className="inspector-foot">
            <div className="inspector-tools">
              {payload.json !== null && !tooBigToPretty && (
                <button
                  type="button"
                  className={`btn${raw ? " btn-latched" : ""}`}
                  aria-pressed={raw}
                  onClick={() => setRaw((prev) => !prev)}
                  title={
                    raw
                      ? "Back to formatted JSON"
                      : "Show the exact text Kavka decoded, unformatted"
                  }
                >
                  Raw text
                </button>
              )}
              <button
                type="button"
                className="btn"
                onClick={() => void copy("Value", record.value?.text ?? "")}
                disabled={record.value === null}
                title={
                  record.value === null
                    ? "This message is a tombstone — there is no value to copy."
                    : undefined
                }
              >
                Copy value
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => void copy("Key", record.key?.text ?? "")}
                disabled={record.key === null}
                title={
                  record.key === null
                    ? "This message has no key to copy."
                    : undefined
                }
              >
                Copy key
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => void copy("Record", recordJson)}
              >
                Copy as JSON
              </button>
            </div>

            {/* role="status": a copy is something you did, finished — it is
                announced, and it never steals focus. */}
            <span className="inspector-copied" role="status">
              {copied ?? ""}
            </span>

            {/* Both numbers, in that order: the cut and the whole. The old
                wording said Kavka "cut this value at 1.4 MB" using `raw_len`,
                which is the FULL size — so the one figure on screen was the
                one place the cut is not. The cut itself is the core's default,
                because the browser always fetches with `max_value_bytes:
                null`, which is what asks for it. */}
            {payload.truncated && (
              <p className="inspector-note">
                Kavka is showing the first {formatBytes(DEFAULT_MAX_VALUE_BYTES)}{" "}
                of {formatBytes(payload.raw_len)}. The record on the broker is
                unchanged.
              </p>
            )}

          </div>
        </>
      )}
      </div>

      {/* THE PANEL'S FOOT — the mockup's, and the one fact about reading that
          nobody thinks to ask until they have already worried about it.
          `enable.auto.commit` is false on every consumer the core builds
          (crates/kavka-core/src/connection.rs) and nothing here ever commits,
          so opening a message cannot move a group along. Stated on every tab
          because it is true of the whole panel, not of one payload. */}
      <p className="inspector-caveat">
        <span className="status-dot status-connected" aria-hidden="true" />
        Reading this committed nothing. Kavka never commits an offset, so this
        message is still unread as far as every consumer group is concerned.
      </p>
    </aside>
  );
}

/**
 * Where this record failed, what killed it, and the two ways back.
 *
 * THE STACK TRACE IS BEHIND A DISCLOSURE, exactly like the raw broker string
 * under every error banner (§7): the class and the message are what a person
 * reads, and forty frames of JVM internals is what an expert opens when those
 * two are not enough. Rendering it inline would push everything else — the
 * original coordinates, the actions — off the panel.
 *
 * EVERY FIELD IS OPTIONAL, so every field is conditional. A dead letter with an
 * original topic and no exception is a normal record (the framework was
 * configured that way), and inventing "unknown" rows for the missing ones would
 * turn a complete answer into a broken-looking one.
 */
function DeadLetterSection({
  dlq,
  record,
  onBrowseOriginal,
  onReproduce,
  reproduceBlocked,
}: {
  dlq: DlqMeta;
  record: MessageRecord;
  onBrowseOriginal?: (topic: string, partition: number, offset: number) => void;
  onReproduce?: (record: MessageRecord) => void;
  reproduceBlocked?: string;
}) {
  const [trace, setTrace] = useState(false);
  const canJump =
    onBrowseOriginal !== undefined &&
    dlq.original_topic !== null &&
    dlq.original_partition !== null &&
    dlq.original_offset !== null;

  return (
    <section className="dlq-section" aria-label="Dead letter">
      <p className="dlq-eyebrow">Dead letter</p>

      <dl className="dlq-facts">
        {dlq.original_topic !== null && (
          <div className="dlq-fact">
            <dt>Came from</dt>
            <dd className="cell-mono">
              {dlq.original_topic}
              {dlq.original_partition !== null && (
                <> · partition {dlq.original_partition}</>
              )}
              {dlq.original_offset !== null && (
                <> · offset {groupDigits(dlq.original_offset)}</>
              )}
            </dd>
          </div>
        )}
        {dlq.exception_class !== null && (
          <div className="dlq-fact">
            <dt>Failed with</dt>
            <dd className="cell-mono">{dlq.exception_class}</dd>
          </div>
        )}
        {dlq.exception_message !== null && (
          <div className="dlq-fact">
            <dt>Because</dt>
            <dd className="dlq-message">{dlq.exception_message}</dd>
          </div>
        )}
      </dl>

      {dlq.stacktrace !== null && (
        <div className="dlq-trace">
          <button
            type="button"
            className="btn btn-ghost inspector-inline-btn"
            aria-expanded={trace}
            onClick={() => setTrace((prev) => !prev)}
          >
            {trace ? "Hide details" : "Show details"}
          </button>
          {trace && <pre className="banner-raw dlq-stack">{dlq.stacktrace}</pre>}
        </div>
      )}

      <div className="dlq-actions">
        {canJump && (
          <button
            type="button"
            className="btn"
            title={`Open ${dlq.original_topic} at the record this one is about`}
            onClick={() =>
              onBrowseOriginal?.(
                dlq.original_topic as string,
                dlq.original_partition as number,
                dlq.original_offset as number,
              )
            }
          >
            Browse the original
          </button>
        )}
        {onReproduce !== undefined && dlq.original_topic !== null && (
          <button
            type="button"
            className="btn"
            disabled={reproduceBlocked !== undefined}
            title={
              reproduceBlocked ??
              `Send this record back to ${dlq.original_topic}, without the framework's dead-letter headers${
                droppedHeaderCount(record) > 0
                  ? ` (${droppedHeaderCount(record)} of them)`
                  : ""
              } and with Kavka's own kavka.dlq.replayed.from.* provenance.${
                // The other thing the replay loses, in the same breath: produce
                // writes text, and a binary header's `value` is Kavka's hex
                // rendering rather than the bytes themselves.
                droppedBinaryCount(record) > 0
                  ? ` ${droppedBinaryCount(record)} binary header${
                      droppedBinaryCount(record) === 1 ? "" : "s"
                    } can't be re-produced as text and were left off.`
                  : ""
              }`
            }
            onClick={() => onReproduce(record)}
          >
            Re-produce to {dlq.original_topic}
          </button>
        )}
      </div>

      <p className="dlq-convention">
        Read using the {conventionWord(dlq.convention)} convention — Kavka
        matched that framework's headers on this record.
      </p>
    </section>
  );
}
