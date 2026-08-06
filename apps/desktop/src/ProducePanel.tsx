import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  MAX_BULK_COUNT,
  bulkStop,
  bulkSubscribe,
  errorMessage,
  produceBulk,
  produceSend,
  type BulkPayload,
  type BulkSpec,
  type ConnectionProfile,
  type JsonValue,
  type PartitionDetail,
  type ProduceHeaderInput,
  type ProduceRecordSpec,
  type ProduceValueSpec,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useIsProtected } from "./environments";
import { classifyError } from "./errors";
import { approxCount, groupDigits } from "./format";
import { Term } from "./Glossary";
import HelpPopover from "./HelpPopover";
import Overlay from "./Overlay";
import { ErrorBanner } from "./ProfileEditor";
import { PLACEHOLDERS, previewRows, unknownPlaceholders } from "./template";
import type { ToastSpec } from "./Toast";

/**
 * PRODUCE — the app's first write path into a topic.
 *
 * Everything here is shaped by §6's guardrails rather than by the form:
 *
 *  - Layer 7: on prod every write control is danger-outlined even when
 *    routine, and the panel carries an UNDISMISSABLE warning banner.
 *  - Layer 4: type-to-confirm is environment-gated, not action-gated. Prod
 *    always asks and restates the cluster; dev never asks at all.
 *  - Read-only mode disables, never hides, and every disabled control says
 *    why — the same sentence the status bar and the palette use.
 *
 * The bulk run follows the session pattern: `produce_bulk` hands back an id,
 * one effect owns the subscription, and its cleanup stops the run — so closing
 * this panel mid-run stops it, exactly like navigating away from a live tail.
 *
 * WITH ONE ADDITION, WHICH IS THE WHOLE OF THE BULK STATE MODEL: STOPPING IS NOT
 * ENDING. A run that is asked to stop keeps its subscription, because the true
 * `sent` and `failed` only exist *after* the core has flushed what librdkafka
 * already accepted — that flush is what makes the counts describe the cluster
 * rather than our intentions, and it finishes some tens of milliseconds after
 * the click. Tearing the subscription down on the click throws that payload
 * away and leaves the last thing the user saw — a mid-run guess — as the final
 * word on how many real messages went into a real topic. So `Stop` sets a
 * `stopping` flag, the button says so, and the arriving `done` payload is what
 * finalizes and toasts. Closing the panel mid-run does the same thing from
 * outside the component: see `reportFinalCounts`.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/**
 * How long a detached reporter waits for the final counts before saying what it
 * last knew. Generous next to the core's own cancel-flush, and finite because a
 * listener with no owner must not be able to live for the life of the window.
 */
const FINAL_COUNT_WAIT_MS = 30_000;

/**
 * A BULK RUN OUTLIVES THE PANEL THAT STARTED IT, BY EXACTLY ONE PAYLOAD.
 *
 * Closing the panel stops the run — but the records already accepted by the
 * broker are still being flushed, and the counts that describe them arrive
 * after the component is gone. A listener that dies with the panel therefore
 * loses the one number that matters about a write: how much of it happened.
 *
 * So the close path hands the id to this, which keeps a listener alive just
 * long enough to hear `done`, toasts the real counts (toasts belong to the
 * topic view, which is still mounted) and unsubscribes itself. It is bounded
 * twice — by that payload and by `FINAL_COUNT_WAIT_MS` — so it can neither leak
 * nor go quiet: if the payload never comes it says what it last knew, marked as
 * a floor rather than a total.
 *
 * The alternative was to block the panel's Close while stopping. It was
 * rejected: `Esc` closes every overlay in Kavka, so blocking would have to
 * fight the shortcut too, and a modal that refuses to close is a worse failure
 * than a toast that arrives a moment later.
 */
function reportFinalCounts(
  bulkId: string,
  topic: string,
  expected: number,
  lastSeen: BulkPayload | null,
  push: (spec: ToastSpec) => void,
): void {
  let latest = lastSeen;
  let settled = false;
  let unsubscribe: (() => void) | null = null;
  let timer = 0;
  const close = () => {
    settled = true;
    window.clearTimeout(timer);
    unsubscribe?.();
    unsubscribe = null;
  };

  const sentence = (payload: BulkPayload, exact: boolean) => {
    const count = groupDigits(payload.sent);
    const noun = payload.sent === 1 ? "message" : "messages";
    const title = !exact
      ? `Stopped the bulk run to ${topic} — at least ${count} ${noun} were sent`
      : payload.sent >= expected
        ? `Sent ${count} ${noun} to ${topic}`
        : `Stopped after sending ${count} ${noun} to ${topic}`;
    const detail =
      payload.failed > 0
        ? `${groupDigits(payload.failed)} were refused by the broker.`
        : !exact
          ? "The panel was closed before the broker finished acknowledging them, so this is a floor rather than a total."
          : undefined;
    return { title, detail };
  };

  unsubscribe = bulkSubscribe(bulkId, (payload) => {
    latest = payload;
    if (!payload.done || settled) return;
    close();
    // Nothing was written, so there is nothing to say: a toast is the past
    // tense of a button, and this button did not do anything.
    if (payload.sent === 0 && payload.failed === 0 && payload.error === null) {
      return;
    }
    if (payload.error !== null) {
      const { title, detail } = classifyError(payload.error);
      push({ kind: "danger", title, detail });
      return;
    }
    const { title, detail } = sentence(payload, true);
    push({ kind: payload.failed > 0 ? "warn" : "ok", title, detail });
  });
  if (settled) unsubscribe?.();

  timer = window.setTimeout(() => {
    if (settled) return;
    close();
    if (latest === null || (latest.sent === 0 && latest.failed === 0)) return;
    const { title, detail } = sentence(latest, false);
    push({ kind: "warn", title, detail });
  }, FINAL_COUNT_WAIT_MS);

  // Idempotent, and the reason there is anything to report: the core flushes
  // what the broker already has before it calls the run done.
  void bulkStop(bulkId);
}

type Tab = "one" | "bulk";

/** The strip, in order, so its rendering and its arrow keys read one list. */
const PRODUCE_TABS: ReadonlyArray<readonly [Tab, string]> = [
  ["one", "Send one"],
  ["bulk", "Bulk"],
];
type ValueKind = "text" | "json" | "avro";

interface HeaderRow {
  /** Stable across removals so React never reuses one row's input for another. */
  id: number;
  key: string;
  value: string;
}

type FieldKey =
  | "value"
  | "subject"
  | "headers"
  | "count"
  | "interval"
  | "template";

interface FieldError {
  field: FieldKey;
  message: string;
}

let nextHeaderId = 1;

/**
 * A record on its way INTO the form rather than out of it.
 *
 * Phase 5a's DLQ replay is the first caller: it hands over the dead-lettered
 * record's key, value and headers — minus the framework's own dead-letter
 * bookkeeping, plus Kavka's provenance — with the destination set to the topic
 * it originally failed on. The form is otherwise unchanged, and everything the
 * user does to it afterwards is theirs: this is an initial value, never a lock.
 */
export interface ProducePrefill {
  key: string;
  /** The value's text. `null` = a tombstone, which is a real thing to replay. */
  value: string | null;
  /** JSON gets the parse-on-blur treatment; text does not. */
  valueKind?: "text" | "json";
  headers: ProduceHeaderInput[];
  /** One sentence saying where this came from, shown above the form. */
  note?: React.ReactNode;
}

interface ProducePanelProps {
  profile: ConnectionProfile;
  topic: string;
  partitions: PartitionDetail[];
  push: (spec: ToastSpec) => void;
  /** Jump the message browser to the record that was just written. */
  onViewRecord: (partition: number, offset: number) => void;
  onClose: () => void;
  /** Open with these values already in the form. See `ProducePrefill`. */
  initial?: ProducePrefill | null;
}

export default function ProducePanel({
  profile,
  topic,
  partitions,
  push,
  onViewRecord,
  onClose,
  initial = null,
}: ProducePanelProps) {
  const [tab, setTab] = useState<Tab>("one");
  const tabRefs = useRef<Partial<Record<Tab, HTMLButtonElement | null>>>({});

  /** Arrow keys walk the strip; selection follows focus, as everywhere else. */
  const onTabKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
      let next: number | null = null;
      if (e.key === "ArrowRight") next = (index + 1) % PRODUCE_TABS.length;
      else if (e.key === "ArrowLeft")
        next = (index - 1 + PRODUCE_TABS.length) % PRODUCE_TABS.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = PRODUCE_TABS.length - 1;
      if (next === null) return;
      e.preventDefault();
      const [key] = PRODUCE_TABS[next];
      setTab(key);
      tabRefs.current[key]?.focus();
    },
    [],
  );

  // ── One message ────────────────────────────────────────────────────────
  const [key, setKey] = useState(initial?.key ?? "");
  const [valueKind, setValueKind] = useState<ValueKind>(
    initial === null ? "json" : (initial.valueKind ?? "text"),
  );
  const [value, setValue] = useState(
    initial === null ? "{\n  \n}" : (initial.value ?? ""),
  );
  const [subject, setSubject] = useState(`${topic}-value`);
  const [tombstone, setTombstone] = useState(
    initial !== null && initial.value === null,
  );
  const [headers, setHeaders] = useState<HeaderRow[]>(() =>
    (initial?.headers ?? []).map((h) => ({
      id: nextHeaderId++,
      key: h.key,
      value: h.value,
    })),
  );
  const [partition, setPartition] = useState("auto");

  // ── Bulk ───────────────────────────────────────────────────────────────
  const [keyTemplate, setKeyTemplate] = useState("{{uuid}}");
  const [valueTemplate, setValueTemplate] = useState(
    '{"id": {{seq}}, "status": "{{choice new|paid|shipped}}", "at": "{{now_iso}}"}',
  );
  const [count, setCount] = useState("100");
  const [interval, setIntervalMs] = useState("10");
  const [bulkPartition, setBulkPartition] = useState("auto");
  const [bulkRun, setBulkRun] = useState<number | null>(null);
  const [bulkProgress, setBulkProgress] = useState<BulkPayload | null>(null);
  /** Stop has been asked for; the run is still owed its final counts. */
  const [stopping, setStopping] = useState(false);
  const bulkId = useRef<string | null>(null);
  const bulkSpec = useRef<BulkSpec | null>(null);
  /** The `done` payload has landed, so nobody else has to go looking for it. */
  const finalized = useRef(false);
  /** The latest payload, for a reporter that has to carry on without React. */
  const latestBulk = useRef<BulkPayload | null>(null);

  // ── Shared ─────────────────────────────────────────────────────────────
  const [fieldError, setFieldError] = useState<FieldError | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const firstRef = useRef<HTMLInputElement | null>(null);

  const isProtected = useIsProtected(profile.environment);
  const readOnly = profile.read_only;
  const hasRegistry =
    profile.schema_registry !== null && profile.schema_registry !== undefined;

  const clear = (field: FieldKey) =>
    setFieldError((prev) => (prev?.field === field ? null : prev));

  // ── Validation ─────────────────────────────────────────────────────────

  /**
   * A RESULT, not a union with JsonValue: `{"field": "x"}` is perfectly good
   * JSON, and a bare union would have this function's success case shadow its
   * own error shape for exactly one payload nobody would ever think to test.
   */
  const parsedJson = useCallback(():
    | { ok: true; json: JsonValue }
    | { ok: false; error: FieldError } => {
    try {
      return { ok: true, json: JSON.parse(value) as JsonValue };
    } catch (err) {
      return {
        ok: false,
        error: {
          field: "value",
          message: `Kavka couldn't read that as JSON — ${
            err instanceof Error ? err.message : "check the quotes and commas"
          }. Switch the value to Plain text if it isn't meant to be JSON.`,
        },
      };
    }
  }, [value]);

  const buildRecord = useCallback((): ProduceRecordSpec | FieldError => {
    let spec: ProduceValueSpec | null = null;
    if (!tombstone) {
      if (valueKind === "text") {
        spec = { kind: "text", text: value };
      } else {
        const parsed = parsedJson();
        if (!parsed.ok) return parsed.error;
        const json = parsed.json;
        if (valueKind === "json") {
          spec = { kind: "json", json };
        } else {
          const s = subject.trim();
          if (s.length === 0)
            return {
              field: "subject",
              message:
                "Name the Schema Registry subject to encode against — usually the topic plus -value, e.g. orders.v2-value",
            };
          spec = { kind: "avro", subject: s, json };
        }
      }
    }

    const rows = headers.filter(
      (h) => h.key.trim().length > 0 || h.value.length > 0,
    );
    const nameless = rows.find((h) => h.key.trim().length === 0);
    if (nameless !== undefined)
      return {
        field: "headers",
        message: "Give every header a name, or remove the empty row.",
      };

    return {
      // An empty box is NO key, which is what ∅ means in the table — not a key
      // that happens to be the empty string.
      key: key.length === 0 ? null : key,
      value: spec,
      headers: rows.map((h) => ({ key: h.key.trim(), value: h.value })),
      partition: partition === "auto" ? null : Number.parseInt(partition, 10),
    };
  }, [tombstone, valueKind, value, parsedJson, subject, headers, key, partition]);

  const validateBulk = useCallback(():
    | {
        count: number;
        interval_ms: number;
        key_template: string | null;
        value_template: string;
        partition: number | null;
      }
    | FieldError => {
    const n = Number.parseInt(count, 10);
    if (!Number.isFinite(n) || n < 1)
      return { field: "count", message: "Send at least one message." };
    if (n > MAX_BULK_COUNT)
      return {
        field: "count",
        message: `Kavka's core caps one run at ${groupDigits(
          MAX_BULK_COUNT,
        )} messages. Run it again for more.`,
      };
    const ms = Number.parseInt(interval, 10);
    if (!Number.isFinite(ms) || ms < 0)
      return {
        field: "interval",
        message: "Use 0 for as fast as the broker will take them.",
      };
    if (valueTemplate.trim().length === 0)
      return {
        field: "template",
        message: "Write the message body — it can be plain text or JSON.",
      };
    return {
      count: n,
      interval_ms: ms,
      key_template: keyTemplate.trim().length === 0 ? null : keyTemplate,
      value_template: valueTemplate,
      partition:
        bulkPartition === "auto" ? null : Number.parseInt(bulkPartition, 10),
    };
  }, [count, interval, valueTemplate, keyTemplate, bulkPartition]);

  // ── Sending ────────────────────────────────────────────────────────────

  const sendOne = useCallback(async () => {
    const record = buildRecord();
    if ("field" in record) {
      setFieldError(record);
      return;
    }
    setFieldError(null);
    setFailure(null);
    setBusy(true);
    try {
      const ack = await produceSend(profile.id, topic, record);
      push({
        kind: "ok",
        title: `Sent to ${topic}`,
        detail: `Partition ${ack.partition} · offset ${groupDigits(ack.offset)}`,
        action: {
          label: "View it",
          run: () => onViewRecord(ack.partition, ack.offset),
        },
      });
      onClose();
    } catch (err) {
      const raw = errorMessage(err);
      const { cause, title, detail } = classifyError(raw, {
        environmentProtected: isProtected,
      });
      // The read-only refusal is §7's own toast row: role="alert", no
      // auto-dismiss, and it says plainly that nothing was written.
      if (cause === "read-only") push({ kind: "danger", title, detail });
      else setFailure(raw);
    } finally {
      setBusy(false);
    }
  }, [buildRecord, profile.id, isProtected, topic, push, onViewRecord, onClose]);

  const startBulk = useCallback(() => {
    const spec = validateBulk();
    if ("field" in spec) {
      setFieldError(spec);
      return;
    }
    setFieldError(null);
    setFailure(null);
    setBulkProgress(null);
    setStopping(false);
    finalized.current = false;
    latestBulk.current = null;
    bulkSpec.current = spec;
    setBulkRun((prev) => (prev ?? 0) + 1);
  }, [validateBulk]);

  /**
   * Ask the run to stop — and KEEP the subscription. The counts that describe
   * what actually reached the topic are settled by the core's flush, which
   * happens after this returns; the `done` payload that follows is what ends
   * the run in the UI. See the note at the top of the file.
   */
  const stopBulk = useCallback(() => {
    const id = bulkId.current;
    if (id === null) {
      setBulkRun(null);
      return;
    }
    setStopping(true);
    void bulkStop(id);
  }, []);

  /**
   * The bulk session. One effect owns it, and its cleanup stops the run — so
   * closing the panel, switching tabs away and unmounting are the same path.
   * `bulk_stop` is idempotent, so the extra call after `done` is free.
   *
   * The effect is keyed on the run generation, NOT on whether the user has
   * pressed Stop: stopping leaves the subscription in place so the final counts
   * can arrive, and the `done` payload is the only thing that clears `bulkRun`.
   */
  useEffect(() => {
    if (bulkRun === null) return;
    const spec = bulkSpec.current;
    if (spec === null) return;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    let started: string | null = null;
    const expected = spec.count;

    void (async () => {
      try {
        const id = await produceBulk(profile.id, topic, spec);
        if (cancelled) {
          // The panel went away (or Stop landed) while `produce_bulk` was in
          // flight, so this component will never see this run's events — and
          // records may already be on their way. Same answer as the close
          // path: stop it, and stay around for the counts.
          reportFinalCounts(id, topic, expected, null, push);
          return;
        }
        started = id;
        bulkId.current = id;
        unsubscribe = bulkSubscribe(
          id,
          (payload) => {
            latestBulk.current = payload;
            setBulkProgress(payload);
            if (!payload.done) return;
            // THE FINAL WORD. `done` is set after the core has flushed what
            // librdkafka already accepted, so these counts describe the topic
            // rather than what we asked for — including after a Stop.
            finalized.current = true;
            setStopping(false);
            setBulkRun(null);
            if (payload.error !== null) {
              setFailure(payload.error);
              return;
            }
            const count = groupDigits(payload.sent);
            const noun = payload.sent === 1 ? "message" : "messages";
            push({
              kind: payload.failed > 0 ? "warn" : "ok",
              title:
                payload.sent >= expected
                  ? `Sent ${count} ${noun} to ${topic}`
                  : `Stopped after sending ${count} ${noun} to ${topic}`,
              detail:
                payload.failed > 0
                  ? `${groupDigits(payload.failed)} were refused by the broker.`
                  : undefined,
            });
          },
          (message) => {
            setFailure(message);
            finalized.current = true;
            setStopping(false);
            setBulkRun(null);
          },
        );
      } catch (err) {
        if (cancelled) return;
        const raw = errorMessage(err);
        const { cause, title, detail } = classifyError(raw, {
          environmentProtected: isProtected,
        });
        if (cause === "read-only") push({ kind: "danger", title, detail });
        else setFailure(raw);
        finalized.current = true;
        setStopping(false);
        setBulkRun(null);
      }
    })();

    return () => {
      cancelled = true;
      unsubscribe?.();
      if (started !== null) {
        if (finalized.current) {
          // Already over: stopping it again is the free, idempotent call.
          void bulkStop(started);
        } else {
          // The panel is going away mid-run. The run stops either way — but
          // the counts it is about to settle are the only honest answer to
          // "what did that write", so something has to stay behind to hear it.
          reportFinalCounts(
            started,
            topic,
            expected,
            latestBulk.current,
            push,
          );
        }
      }
      bulkId.current = null;
    };
  }, [bulkRun, profile.id, isProtected, topic, push]);

  const bulkRunning = bulkRun !== null;

  // ── Preview ────────────────────────────────────────────────────────────

  const preview = useMemo(() => {
    const now = Date.now();
    const keys =
      keyTemplate.trim().length === 0 ? null : previewRows(keyTemplate, 3, now);
    return { keys, values: previewRows(valueTemplate, 3, now) };
  }, [keyTemplate, valueTemplate]);

  const unknown = useMemo(
    () =>
      Array.from(
        new Set([
          ...unknownPlaceholders(keyTemplate),
          ...unknownPlaceholders(valueTemplate),
        ]),
      ),
    [keyTemplate, valueTemplate],
  );

  const message = (field: FieldKey) =>
    fieldError?.field === field ? (
      <span className="field-error" id={`pp-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;

  const sendReason = readOnly
    ? READ_ONLY_WHY
    : busy
      ? "Kavka is sending the message"
      : undefined;

  /** What the button (or the prod confirmation) actually runs. */
  const submit = useCallback(() => {
    if (tab === "one") void sendOne();
    else startBulk();
  }, [tab, sendOne, startBulk]);

  const act = useCallback(() => {
    // Prod always asks; dev never does. Friction where the stakes are.
    if (isProtected) setConfirming(true);
    else submit();
  }, [isProtected, submit]);

  /**
   * The confirmation REPLACES the panel rather than stacking on it.
   *
   * Two overlays at once are two focus traps: `Overlay` walks the focusable
   * children of its own surface, and the confirmation's surface is a
   * descendant of this one, so a Tab off the end of the confirmation would be
   * caught twice and land in the form behind the scrim. Only the tree changes
   * here — every field's state lives in this component, so Cancel comes back
   * to exactly what the user had typed.
   */
  if (confirming) {
    return (
      <ConfirmModal
        title={`Send to ${topic} on ${profile.name}?`}
        body={
          tab === "one" && tombstone ? (
            // §7 rule 8: destructive copy states the blast radius BEFORE the
            // button. A tombstone is not "one more message" — on a compacted
            // topic it is how a producer says this key is deleted, and the key
            // it names is the whole of the blast radius.
            <>
              This writes a tombstone for the key{" "}
              <code>{key.length === 0 ? "∅ (no key)" : key}</code> to{" "}
              <code>{topic}</code> — a record with no value at all. On a
              compacted topic that is how a producer says this key is deleted,
              so consumers see the key disappear and compaction eventually
              removes its history. Every consumer group reading{" "}
              <code>{topic}</code> sees it immediately, and it can't be unsent.
            </>
          ) : tab === "one" ? (
            <>
              This writes one real message to <code>{topic}</code>. Every
              consumer group reading it sees it immediately, and a message can't
              be unsent.
            </>
          ) : (
            <>
              This writes about {approxCount(Number.parseInt(count, 10) || 0)}{" "}
              real messages to <code>{topic}</code>, one every {interval} ms.
              Every consumer group reading it sees them immediately, and they
              can't be unsent.
            </>
          )
        }
        confirmLabel={tab === "one" ? "Send message" : "Send messages"}
        // Environment-gated, not action-gated: this modal only ever exists on
        // prod (§6 layer 4).
        typeToConfirm={topic}
        busy={busy}
        busyLabel="Kavka is sending"
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          submit();
        }}
      />
    );
  }

  return (
    <Overlay
      surfaceClass={`modal modal-wide produce-modal${
        isProtected ? " modal-destructive" : ""
      }`}
      labelledBy="produce-title"
      initialFocus={firstRef}
      onClose={onClose}
    >
      <h2 className="modal-title" id="produce-title">
        Produce to {topic}
      </h2>

      {/* §6 layer 7: on prod the produce form carries an undismissable
          warning. No Dismiss button, on purpose. */}
      {isProtected && (
        <div className="banner banner-danger" role="alert">
          <span className="banner-glyph" aria-hidden="true">
            !
          </span>
          <div className="banner-body">
            <p className="banner-title">
              {profile.name} is a production cluster.
            </p>
            <p className="banner-detail">
              Anything sent here is read by whatever is consuming{" "}
              <code>{topic}</code> right now, and a message can't be unsent.
            </p>
          </div>
        </div>
      )}

      {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

      {/* Where the values in this form came from, when they did not come from
          the user. A prefilled form that says nothing about why it is
          prefilled is a form nobody trusts enough to press. */}
      {initial?.note !== undefined && (
        <p className="dialog-note">{initial.note}</p>
      )}

      {/* One tab stop walked with the arrows, and each tab points at the
          panel it opens — the same contract every other tablist in the app
          keeps (SC 2.1.1, SC 4.1.2). */}
      <div className="modal-tabs" role="tablist" aria-label="Produce">
        {PRODUCE_TABS.map(([k, label], index) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`pp-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={tab === k ? `pp-panel-${k}` : undefined}
            tabIndex={tab === k ? 0 : -1}
            ref={(el) => {
              tabRefs.current[k] = el;
            }}
            className={`tab${tab === k ? " tab-active" : ""}`}
            onClick={() => setTab(k)}
            onKeyDown={(e) => onTabKeyDown(e, index)}
          >
            {label}
          </button>
        ))}
      </div>

      {failure !== null && (
        <ErrorBanner raw={failure} onDismiss={() => setFailure(null)} />
      )}

      {tab === "one" ? (
        <div
          className="modal-panel"
          id="pp-panel-one"
          role="tabpanel"
          aria-labelledby="pp-tab-one"
        >
          <div className="field">
            <label className="field-label" htmlFor="pp-key">
              Key
            </label>
            <input
              id="pp-key"
              ref={firstRef}
              type="text"
              className="input-mono"
              value={key}
              placeholder="A-102"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setKey(e.target.value)}
            />
            <span className="field-hint">
              Kafka sends every message with the same key to the same{" "}
              <Term name="partition">partition</Term>, in order. Leave it empty
              and the message is spread across them — the table shows that as ∅.
            </span>
          </div>

          <div className="check-field">
            <input
              id="pp-tombstone"
              type="checkbox"
              checked={tombstone}
              aria-describedby="pp-tombstone-hint"
              onChange={(e) => setTombstone(e.target.checked)}
            />
            <label className="check-label" htmlFor="pp-tombstone">
              Send a <Term name="tombstone">tombstone</Term> (∅ — no value)
            </label>
            <span className="field-hint" id="pp-tombstone-hint">
              A record with no value at all. On a compacted topic it is how a
              producer says this key is deleted; the value box below is switched
              off while it is on.
            </span>
          </div>

          <div className="field">
            <label className="field-label" htmlFor="pp-valuekind">
              Value
            </label>
            <select
              id="pp-valuekind"
              value={valueKind}
              disabled={tombstone}
              title={
                tombstone
                  ? "A tombstone has no value, so there is nothing to encode"
                  : undefined
              }
              onChange={(e) => {
                setValueKind(e.target.value as ValueKind);
                clear("value");
              }}
            >
              <option value="json">JSON</option>
              <option value="text">Plain text</option>
              <option value="avro" disabled={!hasRegistry}>
                Avro, against a Schema Registry subject
                {hasRegistry ? "" : " — no registry on this connection"}
              </option>
            </select>
            <textarea
              className={`produce-value${
                fieldError?.field === "value" ? " input-invalid" : ""
              }`}
              rows={7}
              value={tombstone ? "" : value}
              disabled={tombstone}
              spellCheck={false}
              aria-label="Message value"
              aria-invalid={fieldError?.field === "value" ? true : undefined}
              aria-describedby={
                fieldError?.field === "value" ? "pp-value-error" : undefined
              }
              onChange={(e) => {
                setValue(e.target.value);
                clear("value");
              }}
              // Validate on blur, never on keystroke (§5.3).
              onBlur={() => {
                if (tombstone || valueKind === "text") return;
                const parsed = parsedJson();
                if (!parsed.ok) setFieldError(parsed.error);
              }}
            />
            {message("value")}
            {!hasRegistry && (
              <span className="field-hint">
                Avro needs a Schema Registry. Add one in this connection's
                settings and Kavka will encode the JSON above against the
                subject's latest schema before sending.
              </span>
            )}
          </div>

          {valueKind === "avro" && !tombstone && (
            <div className="field">
              <label className="field-label" htmlFor="pp-subject">
                Schema Registry subject
              </label>
              <input
                id="pp-subject"
                type="text"
                className={`input-mono${
                  fieldError?.field === "subject" ? " input-invalid" : ""
                }`}
                value={subject}
                autoComplete="off"
                spellCheck={false}
                aria-invalid={fieldError?.field === "subject" ? true : undefined}
                aria-describedby={
                  fieldError?.field === "subject" ? "pp-subject-error" : undefined
                }
                onChange={(e) => {
                  setSubject(e.target.value);
                  clear("subject");
                }}
              />
              <span className="field-hint">
                Kavka encodes against this subject's latest schema. The usual
                name is the topic plus <code>-value</code>.
              </span>
              {message("subject")}
            </div>
          )}

          <div className="field">
            <span className="field-label">Headers</span>
            {headers.length === 0 ? (
              <span className="field-hint">
                None. Producers use headers for tracing ids, content types and
                routing hints.
              </span>
            ) : (
              <div className="header-rows">
                {headers.map((row) => (
                  <div className="header-row" key={row.id}>
                    <input
                      type="text"
                      className="input-mono"
                      value={row.key}
                      placeholder="trace-id"
                      aria-label="Header name"
                      autoComplete="off"
                      spellCheck={false}
                      onChange={(e) => {
                        const next = e.target.value;
                        setHeaders((prev) =>
                          prev.map((h) =>
                            h.id === row.id ? { ...h, key: next } : h,
                          ),
                        );
                        clear("headers");
                      }}
                    />
                    <input
                      type="text"
                      className="input-mono"
                      value={row.value}
                      placeholder="7f3c…"
                      aria-label="Header value"
                      autoComplete="off"
                      spellCheck={false}
                      onChange={(e) => {
                        const next = e.target.value;
                        setHeaders((prev) =>
                          prev.map((h) =>
                            h.id === row.id ? { ...h, value: next } : h,
                          ),
                        );
                      }}
                    />
                    <button
                      type="button"
                      className="btn btn-ghost"
                      title={`Remove the header “${row.key || "unnamed"}”`}
                      onClick={() =>
                        setHeaders((prev) => prev.filter((h) => h.id !== row.id))
                      }
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}
            {message("headers")}
            <div className="dialog-inline-action">
              <button
                type="button"
                className="btn"
                onClick={() =>
                  setHeaders((prev) =>
                    prev.concat({ id: nextHeaderId++, key: "", value: "" }),
                  )
                }
              >
                Add header
              </button>
            </div>
          </div>

          <div className="field">
            <label className="field-label" htmlFor="pp-partition">
              Partition
            </label>
            <select
              id="pp-partition"
              value={partition}
              onChange={(e) => setPartition(e.target.value)}
            >
              <option value="auto">
                Let Kafka choose (by key, or spread without one)
              </option>
              {partitions.map((p) => (
                <option key={p.partition} value={String(p.partition)}>
                  Partition {p.partition}
                </option>
              ))}
            </select>
          </div>
        </div>
      ) : (
        <div
          className="modal-panel"
          id="pp-panel-bulk"
          role="tabpanel"
          aria-labelledby="pp-tab-bulk"
        >
          <p className="dialog-note">
            Bulk sends the same template over and over, rendering the
            placeholders fresh each time. Kavka's core does the rendering — the
            preview below is this window's own reading of the same rules, so the
            shapes match and the random values do not.
          </p>

          <div className="field">
            <div className="field-label-row">
              <label className="field-label" htmlFor="pp-value-template">
                Message body
              </label>
              <HelpPopover
                label="Placeholders"
                buttonTitle="What Kavka substitutes into a template"
                title="Template placeholders"
              >
                <dl className="help-list">
                  {PLACEHOLDERS.map((p) => (
                    <div className="help-row" key={p.syntax}>
                      <dt className="help-syntax">{p.syntax}</dt>
                      <dd className="help-what">
                        {p.what} <span className="help-eg">→ {p.example}</span>
                      </dd>
                    </div>
                  ))}
                </dl>
              </HelpPopover>
            </div>
            <textarea
              id="pp-value-template"
              className={`produce-value${
                fieldError?.field === "template" ? " input-invalid" : ""
              }`}
              rows={4}
              value={valueTemplate}
              spellCheck={false}
              disabled={bulkRunning}
              aria-invalid={fieldError?.field === "template" ? true : undefined}
              aria-describedby={
                fieldError?.field === "template" ? "pp-template-error" : undefined
              }
              onChange={(e) => {
                setValueTemplate(e.target.value);
                clear("template");
              }}
            />
            {message("template")}
          </div>

          <div className="field">
            <label className="field-label" htmlFor="pp-key-template">
              Key template
            </label>
            <input
              id="pp-key-template"
              type="text"
              className="input-mono"
              value={keyTemplate}
              placeholder="Leave empty to send without a key"
              autoComplete="off"
              spellCheck={false}
              disabled={bulkRunning}
              onChange={(e) => setKeyTemplate(e.target.value)}
            />
          </div>

          <div className="reset-row">
            <label className="seekbar-field">
              <span className="seekbar-label">How many</span>
              <input
                type="number"
                min={1}
                max={MAX_BULK_COUNT}
                step={1}
                className={`input-num${
                  fieldError?.field === "count" ? " input-invalid" : ""
                }`}
                value={count}
                disabled={bulkRunning}
                aria-invalid={fieldError?.field === "count" ? true : undefined}
                aria-describedby={
                  fieldError?.field === "count" ? "pp-count-error" : undefined
                }
                onChange={(e) => {
                  setCount(e.target.value);
                  clear("count");
                }}
              />
            </label>
            <label className="seekbar-field">
              <span className="seekbar-label">Wait between (ms)</span>
              <input
                type="number"
                min={0}
                step={1}
                className={`input-num${
                  fieldError?.field === "interval" ? " input-invalid" : ""
                }`}
                value={interval}
                disabled={bulkRunning}
                aria-invalid={fieldError?.field === "interval" ? true : undefined}
                aria-describedby={
                  fieldError?.field === "interval" ? "pp-interval-error" : undefined
                }
                onChange={(e) => {
                  setIntervalMs(e.target.value);
                  clear("interval");
                }}
              />
            </label>
            <label className="seekbar-field">
              <span className="seekbar-label">Partition</span>
              <select
                value={bulkPartition}
                disabled={bulkRunning}
                onChange={(e) => setBulkPartition(e.target.value)}
              >
                <option value="auto">Let Kafka choose</option>
                {partitions.map((p) => (
                  <option key={p.partition} value={String(p.partition)}>
                    Partition {p.partition}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {message("count")}
          {message("interval")}

          {unknown.length > 0 && (
            // The old copy here said the core might still know these. It does
            // not: an unrecognised placeholder makes `Template::parse` refuse,
            // so the run never starts — which is the right behaviour (a typo'd
            // `{{sq}}` must not reach 100 000 records) and the wrong thing to
            // describe as "it'll probably work".
            <p className="dialog-note">
              Kavka doesn't know{" "}
              {unknown.map((u, i) => (
                <span key={u}>
                  {i > 0 ? ", " : ""}
                  <code>{`{{${u}}}`}</code>
                </span>
              ))}
              , so the preview leaves {unknown.length === 1 ? "it" : "them"} as
              written — and the run will refuse to start until{" "}
              {unknown.length === 1 ? "it is" : "they are"} fixed. Kavka knows{" "}
              {PLACEHOLDERS.map((p, i) => (
                <span key={p.syntax}>
                  {i > 0 ? ", " : ""}
                  <code>{p.syntax}</code>
                </span>
              ))}
              .
            </p>
          )}

          <div className="field">
            <span className="field-label">The first three, as Kavka reads them</span>
            <ol className="preview-list">
              {preview.values.map((v, i) => (
                <li className="preview-row" key={i}>
                  <span className="preview-index" aria-hidden="true">
                    {i + 1}
                  </span>
                  <span className="preview-body">
                    {preview.keys !== null && (
                      <span className="preview-key">{preview.keys[i]} · </span>
                    )}
                    {v}
                  </span>
                </li>
              ))}
            </ol>
          </div>

          {bulkProgress !== null && (
            <p className="bulk-progress" role="status">
              Sent {groupDigits(bulkProgress.sent)} of {groupDigits(
                Number.parseInt(count, 10) || 0,
              )}
              {bulkProgress.failed > 0
                ? ` · ${groupDigits(bulkProgress.failed)} refused`
                : ""}
              {/* Stopping is its own state, and it is not "finished": the
                  broker is still acknowledging what it already has, and these
                  counts are not final until it has. */}
              {bulkProgress.done
                ? " · finished"
                : stopping
                  ? " · stopping — waiting for the broker's final count"
                  : ""}
            </p>
          )}
        </div>
      )}

      {/* THE CAVEAT ABOVE THE ACTION, which is the only place it can do any
          good — after the send it is an excuse. What "sent" proves is exactly
          one thing: a broker took the bytes and returned a partition and an
          offset. People read it as proof that the pipeline works, and a topic
          with nothing consuming it accepts messages exactly as happily as one
          with ten consumers. */}
      <p className="dialog-note produce-caveat">
        A successful send means a broker acknowledged the write and told Kavka
        where it landed. It is not evidence that anything read it — the topic's
        consumer groups are where that question is answered.
      </p>

      <div className="modal-actions">
        <button
          type="button"
          className="btn"
          onClick={onClose}
          title={
            bulkRunning
              ? "Stops the run — Kavka reports what actually reached the topic in a message"
              : undefined
          }
        >
          {tab === "bulk" && bulkRunning ? "Close" : "Cancel"}
        </button>
        {tab === "bulk" && bulkRunning ? (
          // The label survives the whole flow (§5.5): it stays `Stop` and takes
          // the busy slot rather than resizing mid-click. The word "Stopping"
          // is in the progress line above, where it costs no layout.
          <button
            type="button"
            className="btn btn-latched btn-swap"
            onClick={stopBulk}
            disabled={stopping}
            aria-busy={stopping || undefined}
            title={
              stopping
                ? "Kavka is stopping the run and counting what the broker took"
                : "Stop sending — Kavka reports what actually reached the topic"
            }
          >
            <span className="btn-swap-face">
              Stop
            </span>
            <span className="btn-swap-face btn-swap-busy">
              <span className="spinner" aria-hidden="true" />
              Stop
            </span>
          </button>
        ) : (
          // §6 layer 7: a write action renders as danger-outlined on prod even
          // when it is routine. Off prod it is the surface's one primary.
          <button
            type="button"
            className={`btn ${isProtected ? "btn-danger" : "btn-primary"} btn-swap`}
            disabled={readOnly || busy}
            aria-busy={busy || undefined}
            title={sendReason}
            onClick={act}
          >
            <span className="btn-swap-face">
              {tab === "one" ? "Send message" : "Send messages"}
            </span>
            <span className="btn-swap-face btn-swap-busy">
              <span className="spinner" aria-hidden="true" />
              {tab === "one" ? "Send message" : "Send messages"}
            </span>
          </button>
        )}
      </div>

    </Overlay>
  );
}
