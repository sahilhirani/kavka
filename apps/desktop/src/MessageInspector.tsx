import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  DEFAULT_MAX_VALUE_BYTES,
  type DecodedPayload,
  type MessageRecord,
} from "./api";
import { copyText } from "./clipboard";
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
 * Above this many lines the inspector stops rendering and says so. DESIGN
 * §5.10 asks for line-level virtualization at the cut; this is the honest
 * interim: a hard stop with a sentence, never a silently shortened payload.
 */
const MAX_LINES = 4000;

interface MessageInspectorProps {
  record: MessageRecord;
  topic: string;
  onClose: () => void;
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
}: MessageInspectorProps) {
  const [tab, setTab] = useState<InspectorTab>("value");
  const [raw, setRaw] = useState(false);
  const [forced, setForced] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<number | null>(null);

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

  return (
    <aside
      className="inspector"
      aria-label={`Message at offset ${record.offset}, partition ${record.partition}`}
    >
      <header className="inspector-head">
        {/* No gloss here: this line is the record's address in mono, and a
            dotted underline through it reads as damage. `offset` is glossed
            on the table's column head, one view over. */}
        <span className="inspector-address">
          Partition {record.partition} · Offset {groupDigits(record.offset)}
          {stamp !== null ? ` · ${stamp}` : ""}
        </span>
        <button
          type="button"
          className="btn btn-ghost inspector-close"
          onClick={onClose}
          title="Close the inspector"
        >
          Close
        </button>
      </header>

      <div className="modal-tabs inspector-tabs" role="tablist" aria-label="Payload">
        {(
          [
            ["value", "Value"],
            ["key", "Key"],
            ["headers", `Headers${record.headers.length > 0 ? ` ${record.headers.length}` : ""}`],
            ["raw", "Raw"],
          ] as const
        ).map(([key, label]) => (
          <button
            key={key}
            type="button"
            role="tab"
            aria-selected={tab === key}
            className={`tab${tab === key ? " tab-active" : ""}`}
            onClick={() => setTab(key)}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === "headers" ? (
        <div className="inspector-body inspector-body-plain">
          {record.headers.length === 0 ? (
            <p className="inspector-note">
              This message carries no headers. Producers use them for tracing
              ids, content types and routing hints — this one sent none.
            </p>
          ) : (
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
          {/* Body first, controls under it: the payload is the loudest thing
              on this panel and nothing chrome-shaped goes above it. */}
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

            {/* Provenance, pinned at the bottom: what these bytes were read
                as, and which schema said so — or plainly that none did. It
                already carries subject · version · id, so there is no separate
                schema chip here to say the same thing twice. */}
            <p className="inspector-provenance">{provenance(payload)}</p>
          </div>
        </>
      )}
    </aside>
  );
}
