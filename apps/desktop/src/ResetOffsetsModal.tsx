import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  offsetsReset,
  topicDetail,
  type ConnectionProfile,
  type GroupDetail,
  type GroupOffset,
  type OffsetResetSpec,
  type ResetTarget,
} from "./api";
import { useIsProtected } from "./environments";
import { classifyError } from "./errors";
import { approxCount, fromDatetimeLocal, toDatetimeLocal } from "./format";
import { Term } from "./Glossary";
import Overlay from "./Overlay";

/**
 * THE MODAL THAT TEACHES (docs/DESIGN.md §7, "Destructive confirmations").
 *
 * Offset reset is the operation juniors get wrong, so the whole feature's UX
 * is one sentence: "checkout-service will re-process about 4.2M messages.
 * Anything the application does on each message will happen again." That line
 * is computed from committed vs the target and updates live as the mode
 * changes — which is why this modal loads the topic's watermarks: `earliest`
 * is not on the group's offsets, and DESIGN specifies the count as "computed
 * from committed vs earliest offsets".
 *
 * Two guards, in this order:
 *  - Type-to-confirm when the move is big (>10 000 messages) or the cluster is
 *    a protected environment. Friction where the stakes are.
 *  - `force` NEVER appears up front. The core refuses a reset on a live group,
 *    and the checkbox only exists after the broker has actually said no — with
 *    the risk restated in one sentence beside it.
 *
 * THE SCOPE IS ALWAYS AN EXPLICIT PARTITION LIST. `partitions: null` is a
 * legal `OffsetResetSpec`, and the core reads it as EVERY partition of the
 * topic — including ones this group has never read and has no committed offset
 * on. That is not what a checkbox labelled "every partition this group reads"
 * promises, and the preview sentence was computed from the group's own
 * partitions either way, so the modal was quietly resetting more than it said
 * and more than it counted. It now sends the list it shows, always.
 */

/** Above this many messages, dev clusters ask to have the group id typed too. */
const TYPE_CONFIRM_THRESHOLD = 10_000;

type TargetKind = ResetTarget["kind"];

interface Watermarks {
  earliest: Map<number, number>;
  latest: Map<number, number>;
}

interface ResetOffsetsModalProps {
  profile: ConnectionProfile;
  detail: GroupDetail;
  /** Which of the group's topics to start on. */
  initialTopic: string;
  onDone: (offsets: GroupOffset[]) => void;
  onClose: () => void;
}

export default function ResetOffsetsModal({
  profile,
  detail,
  initialTopic,
  onDone,
  onClose,
}: ResetOffsetsModalProps) {
  const cancelRef = useRef<HTMLButtonElement | null>(null);

  const topics = useMemo(
    () => Array.from(new Set(detail.offsets.map((o) => o.topic))).sort(),
    [detail.offsets],
  );
  const [topic, setTopic] = useState(
    () => (topics.includes(initialTopic) ? initialTopic : (topics[0] ?? initialTopic)),
  );
  const [kind, setKind] = useState<TargetKind>("earliest");
  const [offsetValue, setOffsetValue] = useState("0");
  const [shiftValue, setShiftValue] = useState("-100");
  const [when, setWhen] = useState(() =>
    toDatetimeLocal(Date.now() - 3_600_000),
  );
  const [allPartitions, setAllPartitions] = useState(true);
  const [partitionText, setPartitionText] = useState("");
  const [typed, setTyped] = useState("");
  const [force, setForce] = useState(false);
  const [refusedAsActive, setRefusedAsActive] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const [marks, setMarks] = useState<Watermarks | null>(null);
  const [marksFailed, setMarksFailed] = useState(false);
  const marksSeq = useRef(0);

  // The group's own offsets carry `committed` and `end`, but not `earliest` —
  // and "will re-process about N" is committed minus earliest. One extra read,
  // and the sentence stops being a guess.
  useEffect(() => {
    const seq = ++marksSeq.current;
    setMarks(null);
    setMarksFailed(false);
    void topicDetail(profile.id, topic)
      .then((t) => {
        if (marksSeq.current !== seq) return;
        setMarks({
          earliest: new Map(
            t.partitions.map((p) => [p.partition, p.earliest_offset]),
          ),
          latest: new Map(
            t.partitions.map((p) => [p.partition, p.latest_offset]),
          ),
        });
      })
      .catch(() => {
        if (marksSeq.current === seq) setMarksFailed(true);
      });
    return () => {
      marksSeq.current += 1;
    };
  }, [profile.id, topic]);

  const topicOffsets = useMemo(
    () =>
      detail.offsets
        .filter((o) => o.topic === topic)
        .sort((a, b) => a.partition - b.partition),
    [detail.offsets, topic],
  );

  /**
   * The partitions this group has a committed offset on for this topic — the
   * list the checkbox means, and the list it sends. A partition the group has
   * never read is deliberately left out: there is nothing of the group's on it
   * to move, and moving it anyway is how a reset does more than it says.
   */
  const groupPartitions = useMemo(
    () => topicOffsets.map((o) => o.partition),
    [topicOffsets],
  );

  // Never `null`. See the scope note at the top of this file.
  const chosenPartitions = useMemo<number[]>(() => {
    if (allPartitions) return groupPartitions;
    const out: number[] = [];
    for (const piece of partitionText.split(/[,\s]+/)) {
      if (piece.length === 0) continue;
      const n = Number.parseInt(piece, 10);
      if (Number.isInteger(n) && !out.includes(n)) out.push(n);
    }
    return out;
  }, [allPartitions, groupPartitions, partitionText]);

  // The preview counts exactly what the spec will carry — one list, so the
  // sentence can never describe a smaller reset than the one that goes out.
  const inScope = useMemo(
    () => topicOffsets.filter((o) => chosenPartitions.includes(o.partition)),
    [topicOffsets, chosenPartitions],
  );

  /** Typed partitions this group has never read. Empty on the checkbox path. */
  const unread = useMemo(
    () => chosenPartitions.filter((p) => !groupPartitions.includes(p)),
    [chosenPartitions, groupPartitions],
  );

  // ── The live preview ───────────────────────────────────────────────────

  const preview = useMemo(() => {
    let reprocess = 0;
    let skip = 0;
    let uncommitted = 0;
    let computable = kind !== "timestamp_ms";

    for (const row of inScope) {
      if (row.committed === null) {
        uncommitted += 1;
        continue;
      }
      const earliest = marks?.earliest.get(row.partition);
      const latest = marks?.latest.get(row.partition) ?? row.end_offset;
      let target: number | null = null;
      switch (kind) {
        case "earliest":
          target = earliest ?? null;
          break;
        case "latest":
          target = latest;
          break;
        case "offset": {
          const typedOffset = Number.parseInt(offsetValue, 10);
          target = Number.isFinite(typedOffset) ? typedOffset : null;
          break;
        }
        case "shift_by": {
          const shift = Number.parseInt(shiftValue, 10);
          target = Number.isFinite(shift) ? row.committed + shift : null;
          break;
        }
        case "timestamp_ms":
          target = null;
          break;
      }
      if (target === null) {
        computable = false;
        continue;
      }
      // The broker clamps to the retained range; so does the estimate, or the
      // sentence promises to re-read messages that no longer exist.
      const low = earliest ?? 0;
      const clamped = Math.max(low, Math.min(latest, target));
      const delta = row.committed - clamped;
      if (delta > 0) reprocess += delta;
      else skip += -delta;
    }

    return {
      computable: computable && !marksFailed && marks !== null,
      reprocess,
      skip,
      uncommitted,
      partitions: inScope.length,
    };
  }, [inScope, kind, offsetValue, shiftValue, marks, marksFailed]);

  const moved = preview.reprocess + preview.skip;
  const isProtected = useIsProtected(profile.environment);
  const needsTyping =
    isProtected || (preview.computable && moved > TYPE_CONFIRM_THRESHOLD);
  const typedOk = !needsTyping || typed === detail.group_id;

  const groupIsLive = detail.state.toLowerCase() !== "empty";

  /**
   * What the §7 error library cannot derive from a broker string on its own:
   * the group's state and member count, and whether this connection's
   * environment is PROTECTED. Passed as the classifier's second argument so
   * `errors.ts` stays pure — and resolved to a boolean here, because the
   * error library must not know that an environment registry exists.
   */
  const errorContext = useMemo(
    () => ({
      groupState: detail.state,
      memberCount: detail.members.length,
      environmentProtected: isProtected,
    }),
    [detail.state, detail.members.length, isProtected],
  );

  // ── Submit ─────────────────────────────────────────────────────────────

  const buildTarget = useCallback((): ResetTarget | string => {
    switch (kind) {
      case "earliest":
        return { kind: "earliest" };
      case "latest":
        return { kind: "latest" };
      case "offset": {
        const n = Number.parseInt(offsetValue, 10);
        if (!Number.isInteger(n) || n < 0)
          return "An offset counts from 0 — e.g. 8412";
        return { kind: "offset", offset: n };
      }
      case "shift_by": {
        const n = Number.parseInt(shiftValue, 10);
        if (!Number.isInteger(n) || n === 0)
          return "Say how far to move, and in which direction — e.g. -100";
        return { kind: "shift_by", shift_by: n };
      }
      case "timestamp_ms": {
        const ms = fromDatetimeLocal(when);
        if (ms === null) return "Pick the date and time to move to.";
        return { kind: "timestamp_ms", timestamp_ms: ms };
      }
    }
  }, [kind, offsetValue, shiftValue, when]);

  const submit = useCallback(async () => {
    const target = buildTarget();
    if (typeof target === "string") {
      setFormError(target);
      return;
    }
    if (chosenPartitions.length === 0) {
      setFormError(
        allPartitions
          ? "This group has no committed offset on that topic, so there is nothing to move."
          : "Name at least one partition, or switch back to the partitions this group reads.",
      );
      return;
    }
    setFormError(null);
    setFailure(null);
    const spec: OffsetResetSpec = {
      group_id: detail.group_id,
      topic,
      // Always explicit: `null` here would mean every partition of the topic.
      partitions: chosenPartitions,
      target,
      force,
    };
    setBusy(true);
    try {
      const after = await offsetsReset(profile.id, spec);
      onDone(after);
    } catch (err) {
      const raw = errorMessage(err);
      setFailure(raw);
      // The classifier owns this judgement now — it is the same §7 row the
      // banner is about to render, and two copies of the rule drift apart.
      if (classifyError(raw, errorContext).cause === "active-group")
        setRefusedAsActive(true);
    } finally {
      setBusy(false);
    }
  }, [
    buildTarget,
    allPartitions,
    chosenPartitions,
    detail.group_id,
    errorContext,
    topic,
    force,
    profile.id,
    onDone,
  ]);

  const classified =
    failure === null ? null : classifyError(failure, errorContext);

  const sentence = (() => {
    if (chosenPartitions.length === 0)
      return "No partitions chosen — name at least one, or switch back to the partitions this group reads.";
    if (preview.partitions === 0)
      return unread.length > 0
        ? `${detail.group_id} has no committed offset on ${
            unread.length === 1
              ? `partition ${unread[0]}`
              : `partitions ${unread.join(", ")}`
          } yet. The reset commits a first offset there; nothing is re-processed.`
        : "This group has no committed offsets on that topic, so there is nothing to move.";
    if (!preview.computable)
      return kind === "timestamp_ms"
        ? `Kavka will ask the broker which offset that time maps to in each of the ${preview.partitions} partitions. How many messages that moves isn't known until the broker answers.`
        : `Kavka couldn't read this topic's earliest offsets, so it can't say how many messages this moves. The reset itself will still work.`;
    if (moved === 0)
      return "Nothing moves — the committed offsets already sit where this would put them.";
    if (preview.reprocess > 0 && preview.skip > 0)
      return `${detail.group_id} will re-process about ${approxCount(
        preview.reprocess,
      )} messages and skip about ${approxCount(
        preview.skip,
      )} across ${preview.partitions} partitions. Anything it does on each message will happen again.`;
    if (preview.reprocess > 0)
      return `${detail.group_id} will re-process about ${approxCount(
        preview.reprocess,
      )} messages. Anything the application does on each message will happen again.`;
    return `${detail.group_id} will skip about ${approxCount(
      preview.skip,
    )} messages. They will never be processed.`;
  })();

  return (
    <Overlay
      surfaceClass="modal modal-wide modal-destructive"
      labelledBy="ro-title"
      initialFocus={cancelRef}
      onClose={onClose}
    >
      <div className="modal-panel">
        <h2 className="modal-title" id="ro-title">
          Reset offsets for {detail.group_id}
        </h2>

        <div className="reset-row">
          <label className="seekbar-field">
            <span className="seekbar-label">Move to</span>
            <select
              value={kind}
              onChange={(e) => {
                setKind(e.target.value as TargetKind);
                setFormError(null);
              }}
            >
              <option value="earliest">The beginning of the topic</option>
              <option value="latest">The newest message</option>
              <option value="offset">A specific offset</option>
              <option value="timestamp_ms">A point in time</option>
              <option value="shift_by">Forward or back by N</option>
            </select>
          </label>

          {kind === "offset" && (
            <label className="seekbar-field">
              <span className="seekbar-label">
                <Term name="offset">Offset</Term>
              </span>
              <input
                type="number"
                min={0}
                step={1}
                className="input-num"
                value={offsetValue}
                onChange={(e) => {
                  setOffsetValue(e.target.value);
                  setFormError(null);
                }}
              />
            </label>
          )}

          {kind === "shift_by" && (
            <label className="seekbar-field">
              <span className="seekbar-label">By</span>
              <input
                type="number"
                step={1}
                className="input-num"
                value={shiftValue}
                onChange={(e) => {
                  setShiftValue(e.target.value);
                  setFormError(null);
                }}
              />
            </label>
          )}

          {kind === "timestamp_ms" && (
            <label className="seekbar-field">
              <span className="seekbar-label">To</span>
              <input
                type="datetime-local"
                step={1}
                className="input-time"
                value={when}
                onChange={(e) => {
                  setWhen(e.target.value);
                  setFormError(null);
                }}
              />
            </label>
          )}

          <label className="seekbar-field">
            <span className="seekbar-label">On topic</span>
            <select value={topic} onChange={(e) => setTopic(e.target.value)}>
              {topics.length === 0 && <option value={topic}>{topic}</option>}
              {topics.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="check-field">
          <input
            id="ro-all"
            type="checkbox"
            checked={allPartitions}
            aria-describedby="ro-all-hint"
            onChange={(e) => {
              setAllPartitions(e.target.checked);
              setFormError(null);
            }}
          />
          <label className="check-label" htmlFor="ro-all">
            Every <Term name="partition">partition</Term> this group reads
          </label>
          {/* The hint names the exact list that will be sent. It is not
              "every partition of the topic": a partition this group has
              never read has no offset of its own to move, and resetting it
              would be a bigger action than the label describes. */}
          <span className="field-hint" id="ro-all-hint">
            {topicOffsets.length === 0 ? (
              <>
                This group has no committed offset on <code>{topic}</code>, so
                there is nothing to move.
              </>
            ) : (
              <>
                {topicOffsets.length} partition
                {topicOffsets.length === 1 ? "" : "s"} of <code>{topic}</code>{" "}
                {topicOffsets.length === 1 ? "has" : "have"} a committed offset
                for this group, and{" "}
                {topicOffsets.length === 1 ? "that is the one" : "those are the ones"}{" "}
                Kavka sends. A partition this group has never read is left alone.
              </>
            )}
          </span>
        </div>

        {!allPartitions && (
          <div className="field">
            <label className="field-label" htmlFor="ro-partitions">
              Which partitions
            </label>
            <input
              id="ro-partitions"
              type="text"
              className="input-mono"
              value={partitionText}
              placeholder="0, 3, 7"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => {
                setPartitionText(e.target.value);
                setFormError(null);
              }}
            />
            {/* Typed partitions are sent verbatim, so the ones this group has
                never read are named out loud: the preview cannot count them
                (there is no committed offset to move) but the reset still
                creates one, and an uncounted effect is the thing this modal
                exists to prevent. */}
            {unread.length > 0 && (
              <span className="field-hint">
                {unread.length === 1
                  ? `Partition ${unread[0]} has`
                  : `Partitions ${unread.join(", ")} have`}{" "}
                no committed offset for this group. Resetting{" "}
                {unread.length === 1 ? "it" : "them"} creates one — the count
                below doesn&apos;t include that.
              </span>
            )}
          </div>
        )}

        {/* THE SENTENCE. It updates live as the mode changes — that is the
            entire feature's UX. */}
        <p className="reset-preview">{sentence}</p>

        {preview.uncommitted > 0 && (
          <p className="dialog-note">
            {preview.uncommitted} of the partitions in scope have never had a
            committed offset from this group, so there is nothing to move for
            them.
          </p>
        )}

        {groupIsLive && (
          <div className="banner banner-warn" role="note">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                This group is active — its state is {detail.state}.
              </p>
              <p className="banner-detail">
                Reset while{" "}
                {detail.members.length === 1
                  ? "its member is"
                  : `its ${detail.members.length} members are`}{" "}
                consuming and Kafka will reject it. Stop the application, wait
                for the group to report Empty, then reset.
              </p>
            </div>
          </div>
        )}

        {needsTyping && (
          <div className="field confirm-type">
            <label className="field-label" htmlFor="ro-type">
              Type <code>{detail.group_id}</code> to confirm
            </label>
            <input
              id="ro-type"
              type="text"
              className="input-mono"
              value={typed}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setTyped(e.target.value)}
            />
            <span className="field-hint">
              {isProtected
                ? `${profile.name} is a production cluster, so Kavka asks every time.`
                : `This moves about ${approxCount(moved)} messages, which is more than Kavka will do on a single click.`}
            </span>
          </div>
        )}

        {formError !== null && (
          <span className="field-error">{formError}</span>
        )}

        {classified !== null && (
          <div className="banner banner-danger" role="alert">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">{classified.title}</p>
              <p className="banner-detail">{classified.detail}</p>
              <details className="banner-details">
                <summary>Show details</summary>
                <pre className="banner-raw">{failure}</pre>
              </details>
            </div>
          </div>
        )}

        {/* The force checkbox exists ONLY after the refusal, and it restates
            the risk in one sentence rather than shrugging. */}
        {refusedAsActive && (
          <div className="check-field">
            <input
              id="ro-force"
              type="checkbox"
              checked={force}
              aria-describedby="ro-force-hint"
              onChange={(e) => setForce(e.target.checked)}
            />
            <label className="check-label" htmlFor="ro-force">
              Send it anyway, while the group is running
            </label>
            <span className="field-hint" id="ro-force-hint">
              This only skips Kavka's own check — Kafka rejects a reset on a
              group with live members too, so the likeliest result is the same
              refusal in the broker's own words. Stopping the application and
              waiting for the group to report Empty is the thing that works.
            </span>
          </div>
        )}

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            ref={cancelRef}
            onClick={onClose}
            disabled={busy}
            title={busy ? "Kavka is resetting the offsets" : undefined}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-danger-confirm btn-swap"
            disabled={busy || !typedOk || chosenPartitions.length === 0}
            aria-busy={busy || undefined}
            title={
              busy
                ? "Kavka is resetting the offsets"
                : chosenPartitions.length === 0
                  ? allPartitions
                    ? "This group has no committed offsets on that topic"
                    : "Name at least one partition to reset"
                  : !typedOk
                    ? `Type ${detail.group_id} exactly to confirm this`
                    : undefined
            }
            onClick={() => void submit()}
          >
            <span className="btn-swap-face">
              Reset offsets
            </span>
            <span className="btn-swap-face btn-swap-busy">
              <span className="spinner" aria-hidden="true" />
              Reset offsets
            </span>
          </button>
        </div>
      </div>
    </Overlay>
  );
}
