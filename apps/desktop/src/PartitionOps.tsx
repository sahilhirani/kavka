import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  reassignAlter,
  reassignCancel,
  reassignList,
  type BrokerInfo,
  type ConnectionProfile,
  type PartitionDetail,
  type PartitionResult,
  type ReassignmentSpec,
  type ReassignmentState,
  type TopicPartition,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { useIsProtected } from "./environments";
import { classifyError } from "./errors";
import Overlay from "./Overlay";
import { ErrorBanner } from "./ProfileEditor";
import type { ToastSpec } from "./Toast";

/**
 * THE TWO OPERATIONS A TOPIC'S PARTITIONS CAN HAVE DONE TO THEM.
 *
 * Preferred-leader election and replica reassignment are the two things a
 * platform engineer does to a partition, and they are the two things every
 * Kafka GUI gets wrong in the same way: they surface Kafka's own vocabulary
 * (`ELECTION_NOT_NEEDED`, `adding_replicas`) and leave the user to work out
 * whether anything bad just happened.
 *
 * Two rules this module exists to hold:
 *
 *  - A NO-OP IS NOT A FAILURE. Asking Kafka to move leadership to the
 *    preferred replica when it is already there comes back as an error code,
 *    and rendering that as an error teaches people to ignore errors. It reads
 *    "already preferred" here, in the same list as the ones that moved.
 *  - A REASSIGNMENT IS WATCHED, NOT FIRED. `reassign_alter` resolves when the
 *    cluster ACCEPTS the plan, which is minutes to hours before the data has
 *    moved. A UI that reports "done" at that point is lying, so acceptance
 *    raises a monitor instead.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/** How often the monitor asks what is still moving. */
const POLL_MS = 2000;

/**
 * How many empty polls after an accepted plan before the monitor says, in so
 * many words, that it never saw anything move.
 *
 * Three rather than one: a plan the cluster accepted a moment ago can take a
 * poll or two to appear in `reassign_list`, and announcing "nothing is moving"
 * in that window would be as wrong as the opposite claim.
 */
const QUIET_POLLS = 3;

// ───────────────────────────────────────────────────────────────────────────
// Reading a batch of per-partition results
// ───────────────────────────────────────────────────────────────────────────

export interface ResultSplit {
  ok: PartitionResult[];
  /** Successes that changed nothing, per the caller's pre-call snapshot. */
  benign: PartitionResult[];
  failed: PartitionResult[];
}

/**
 * Sorts a batch of per-partition results into what worked, what was already
 * true, and what was refused.
 *
 * THE CONTRACT'S ONE BLIND SPOT, and where it is answered. `PartitionResult`
 * carries `error: string | null` and nothing else, and the core folds Kafka's
 * "the end state you asked for already holds" codes — ELECTION_NOT_NEEDED,
 * NO_REASSIGNMENT_IN_PROGRESS — into `error: null`. That is the right call
 * (see api.ts): they are successes, and rendering them red would teach people
 * to ignore red. But it means `error: null` cannot tell "this changed" from
 * "this was already so", and no amount of reading the string can recover the
 * difference — the string is empty.
 *
 * So the difference is supplied by the CALLER, from the state it saw before it
 * asked: `wasNoOp` is a question about the partition, not about the error text.
 * A caller with no snapshot passes nothing and gets every success in `ok`,
 * which is the honest answer to "I don't know".
 */
export function splitResults(
  results: PartitionResult[],
  wasNoOp: (result: PartitionResult) => boolean = () => false,
): ResultSplit {
  const split: ResultSplit = { ok: [], benign: [], failed: [] };
  for (const result of results) {
    if (result.error !== null) split.failed.push(result);
    else if (wasNoOp(result)) split.benign.push(result);
    else split.ok.push(result);
  }
  return split;
}

/**
 * What a batch did, in one line, with the failures underneath.
 *
 * Rendered where the action was taken rather than as a toast: a toast is
 * something you did and finished (§5.8), and a batch where four partitions
 * worked and one didn't is a condition you are now in.
 */
export function PartitionResultsNote({
  split,
  okWord,
  benignWord,
  onDismiss,
}: {
  split: ResultSplit;
  /** Past tense, for the ones that worked: "moved to their preferred leader". */
  okWord: string;
  /** What a no-op means here: "were already on their preferred leader". */
  benignWord?: string;
  onDismiss: () => void;
}) {
  const parts: string[] = [];
  if (split.ok.length > 0)
    parts.push(
      `${split.ok.length} partition${split.ok.length === 1 ? "" : "s"} ${okWord}`,
    );
  if (split.benign.length > 0 && benignWord !== undefined)
    parts.push(`${split.benign.length} ${benignWord}`);
  if (split.failed.length > 0)
    parts.push(
      `${split.failed.length} ${split.failed.length === 1 ? "was" : "were"} refused`,
    );

  return (
    <div className="ops-result">
      <p className="ops-result-line" role="status">
        {parts.length === 0
          ? "The cluster answered with nothing at all — no partition matched this request."
          : parts.join(" · ")}
      </p>

      {split.failed.length > 0 && (
        <ul className="ops-failures">
          {split.failed.map((result) => {
            const classified = classifyError(result.error ?? "");
            return (
              <li className="ops-failure" key={`${result.topic}/${result.partition}`}>
                <p className="ops-failure-title">
                  Partition {result.partition} — {classified.title}
                </p>
                <p className="ops-failure-detail">{classified.detail}</p>
                <details className="banner-details">
                  <summary>Show details</summary>
                  <pre className="banner-raw">{result.error}</pre>
                </details>
              </li>
            );
          })}
        </ul>
      )}

      <div className="empty-actions">
        <button type="button" className="btn btn-ghost" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}

// ───────────────────────────────────────────────────────────────────────────
// The reassignment editor
// ───────────────────────────────────────────────────────────────────────────

/** Same list, same order. Order matters: `[0]` is the preferred leader. */
function sameReplicas(a: number[], b: number[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

/**
 * THE IMPACT LINE. One sentence that has to survive being the only thing a
 * hurried operator reads, so it says all three things: it moves data, the
 * topic stays up, and the cluster pays for it while the copying runs.
 */
const IMPACT =
  "This moves data between brokers. The topic stays available the whole time — Kafka only hands a partition over once the new broker holds a full copy — but throughput can dip on every broker involved while the copying runs, and on a big partition that is hours, not seconds.";

interface ReassignModalProps {
  profile: ConnectionProfile;
  topic: string;
  /** The partitions this modal may edit — one, or all of the topic's. */
  partitions: PartitionDetail[];
  /** The cluster's brokers, which is the picker. Ids come from the overview. */
  brokers: BrokerInfo[];
  onClose: () => void;
  /** Called with what the cluster said, and the plan it was said about. */
  onSubmitted: (results: PartitionResult[], specs: ReassignmentSpec[]) => void;
}

/**
 * A MODAL THAT CARRIES ITS OWN CONFIRMATION, rather than opening a second one.
 *
 * §5.8's destructive pattern wants a typed confirm on prod, and ConnectTab
 * already found what happens if you reach for ConfirmModal from inside an
 * overlay: two focus traps fighting over the same Tab key. So the wire, the
 * blast radius, the typed gate and the restated verb all live here, in one
 * trap — the pattern, not the component.
 */
export function ReassignModal({
  profile,
  topic,
  partitions,
  brokers,
  onClose,
  onSubmitted,
}: ReassignModalProps) {
  const [targets, setTargets] = useState<Record<number, number[]>>(() => {
    const initial: Record<number, number[]> = {};
    for (const p of partitions) initial[p.partition] = [...p.replicas];
    return initial;
  });
  const [typed, setTyped] = useState("");
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cancelRef = useRef<HTMLButtonElement | null>(null);

  const isProtected = useIsProtected(profile.environment);

  const toggle = useCallback((partition: number, brokerId: number) => {
    setTargets((prev) => {
      const current = prev[partition] ?? [];
      // Clicking appends, so click order IS replica order — which is the only
      // ordering control this editor needs, because the only position that
      // carries meaning is the first one.
      const next = current.includes(brokerId)
        ? current.filter((id) => id !== brokerId)
        : [...current, brokerId];
      return { ...prev, [partition]: next };
    });
  }, []);

  const reset = useCallback((partition: number, replicas: number[]) => {
    setTargets((prev) => ({ ...prev, [partition]: [...replicas] }));
  }, []);

  const changed = useMemo(
    () =>
      partitions.filter(
        (p) => !sameReplicas(targets[p.partition] ?? [], p.replicas),
      ),
    [partitions, targets],
  );

  const empty = useMemo(
    () => changed.filter((p) => (targets[p.partition] ?? []).length === 0),
    [changed, targets],
  );

  const specs = useMemo<ReassignmentSpec[]>(
    () =>
      changed.map((p) => ({
        topic,
        partition: p.partition,
        replicas: targets[p.partition] ?? [],
      })),
    [changed, targets, topic],
  );

  const matches = !isProtected || typed === topic;
  const blocked =
    changed.length === 0
      ? "Nothing has changed yet — pick different brokers for at least one partition."
      : empty.length > 0
        ? `Partition ${empty[0].partition} has no brokers picked. A partition with no replicas has nowhere to live.`
        : !matches
          ? `Type ${topic} exactly to confirm this on ${profile.name}`
          : busy
            ? "Kavka is sending the plan to the cluster"
            : undefined;

  const submit = useCallback(async () => {
    if (blocked !== undefined) return;
    setFailure(null);
    setBusy(true);
    try {
      const results = await reassignAlter(profile.id, specs);
      onSubmitted(results, specs);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [blocked, profile.id, specs, onSubmitted]);

  const classified = failure === null ? null : classifyError(failure);

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="reassign-title"
      initialFocus={cancelRef}
      onClose={onClose}
    >
      <div className="modal-panel">
        <h2 className="modal-title" id="reassign-title">
          {partitions.length === 1
            ? `Move partition ${partitions[0].partition} of ${topic}`
            : `Move replicas for ${topic}`}
          {isProtected ? ` on ${profile.name}` : ""}
        </h2>

        {/* The loudest prose in the modal, the same device the offset reset
            and the ACL editor use: what this is about to cost, before the
            controls that cause it. */}
        <p className="reset-preview">{IMPACT}</p>

        {isProtected && (
          <div className="banner banner-warn" role="note">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                {profile.name} is a production cluster.
              </p>
              <p className="banner-detail">
                The copying starts as soon as the cluster accepts this, and it
                competes with live traffic for disk and network on every broker
                named below.
              </p>
            </div>
          </div>
        )}

        <p className="dialog-note">
          Click a broker to add it to the end of that partition's list; click it
          again to take it off. <strong>Order matters</strong> — the first
          broker is the preferred leader, which is where a later leader election
          moves leadership to.
        </p>

        <div className="reassign-list">
          {partitions.map((p) => {
            const target = targets[p.partition] ?? [];
            const dirty = !sameReplicas(target, p.replicas);
            const rfChanged = target.length !== p.replicas.length;
            return (
              <div className="reassign-part" key={p.partition}>
                <div className="reassign-head">
                  <span className="reassign-part-id">
                    Partition {p.partition}
                  </span>
                  <span className="cell-tag">
                    now on {p.replicas.join(", ")}
                    {p.isr.length < p.replicas.length
                      ? ` · ${p.replicas.length - p.isr.length} not in sync`
                      : ""}
                  </span>
                  <button
                    type="button"
                    className="btn btn-ghost btn-row"
                    disabled={!dirty}
                    title={
                      dirty
                        ? "Put this partition's list back to the brokers it is on now"
                        : "This partition is unchanged"
                    }
                    onClick={() => reset(p.partition, p.replicas)}
                  >
                    Reset
                  </button>
                </div>

                <div
                  className="reassign-picker"
                  role="group"
                  aria-label={`Brokers for partition ${p.partition}`}
                >
                  {brokers.map((b) => {
                    const at = target.indexOf(b.id);
                    const on = at >= 0;
                    return (
                      <button
                        key={b.id}
                        type="button"
                        className={`btn btn-row${on ? " btn-latched" : ""}`}
                        aria-pressed={on}
                        title={
                          on
                            ? `${b.host}:${b.port} — replica ${at + 1} of ${
                                target.length
                              }${at === 0 ? ", and the preferred leader" : ""}. Click to take it off.`
                            : `${b.host}:${b.port} — click to add it to the end of the list.`
                        }
                        onClick={() => toggle(p.partition, b.id)}
                      >
                        {b.id}
                      </button>
                    );
                  })}
                </div>

                <p className="reassign-target">
                  {target.length === 0 ? (
                    <span className="field-error">
                      No brokers picked. A partition needs at least one replica
                      — pick the brokers it should live on, or press Reset.
                    </span>
                  ) : (
                    <>
                      <span className="reassign-target-label">Move to</span>
                      <code>{target.join(", ")}</code>
                      <span className="cell-tag">
                        {" "}
                        broker {target[0]} would lead it
                      </span>
                    </>
                  )}
                </p>

                {rfChanged && target.length > 0 && (
                  <span className="field-hint">
                    This also changes how many copies the partition has, from{" "}
                    {p.replicas.length} to {target.length}.{" "}
                    {target.length < p.replicas.length
                      ? "Fewer copies means it survives fewer broker failures."
                      : "More copies means more disk and more replication traffic, permanently."}
                  </span>
                )}
              </div>
            );
          })}
        </div>

        {isProtected && (
          <div className="field confirm-type">
            <label className="field-label" htmlFor="reassign-type">
              Type <code>{topic}</code> to confirm you are moving this on{" "}
              {profile.name}
            </label>
            <input
              id="reassign-type"
              type="text"
              className="input-mono"
              value={typed}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setTyped(e.target.value)}
            />
          </div>
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

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            ref={cancelRef}
            onClick={onClose}
            disabled={busy}
            title={busy ? "Kavka is sending the plan to the cluster" : undefined}
          >
            Cancel
          </button>
          {/* The verb, restated. Never "OK" — and never "Done" either: the
              cluster has only accepted a plan when this resolves. */}
          <button
            type="button"
            className={`btn ${isProtected ? "btn-danger-confirm" : "btn-primary"}`}
            disabled={blocked !== undefined}
            aria-busy={busy || undefined}
            title={
              blocked ??
              `Ask the cluster to move ${changed.length} partition${
                changed.length === 1 ? "" : "s"
              }`
            }
            onClick={() => void submit()}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {busy ? <span className="spinner" /> : null}
            </span>
            Start moving replicas
          </button>
        </div>
      </div>
    </Overlay>
  );
}

// ───────────────────────────────────────────────────────────────────────────
// The monitor
// ───────────────────────────────────────────────────────────────────────────

/** What one partition's move is doing right now, in words. */
function movePhrase(state: ReassignmentState): string {
  const adding = state.adding.join(", ");
  const removing = state.removing.join(", ");
  if (state.adding.length > 0 && state.removing.length > 0)
    return `Broker ${adding} ${state.adding.length === 1 ? "is" : "are"} copying this partition. Broker ${removing} hand${state.removing.length === 1 ? "s" : ""} it back once that copy is in sync.`;
  if (state.adding.length > 0)
    return `Broker ${adding} ${state.adding.length === 1 ? "is" : "are"} copying this partition. Nothing is being taken off — this move only adds copies.`;
  if (state.removing.length > 0)
    return `Broker ${removing} ${state.removing.length === 1 ? "is" : "are"} being taken off as soon as the rest are in sync. No new copy is being made.`;
  return "The cluster still lists this partition as moving, but it is neither adding nor removing a broker — it is finishing up.";
}

interface ReassignMonitorProps {
  profile: ConnectionProfile;
  topic: string;
  /**
   * Bumped by the caller when a plan has just been accepted. It restarts the
   * poll immediately rather than waiting out the interval, so the monitor
   * appears in the same breath as the confirmation.
   *
   * That is ALL it does. It is not evidence that anything is moving — the
   * cluster accepting a plan and the cluster having work to do are different
   * facts, and only a poll can tell them apart.
   */
  nonce: number;
  onDanger: DangerReport;
  /** Called once, when the last move finishes — the caller refetches. */
  onSettled: () => void;
  push: (spec: ToastSpec) => void;
}

/**
 * WHAT IS STILL MOVING, asked every two seconds.
 *
 * It renders nothing at all when nothing is in flight, which is what makes it
 * safe to mount unconditionally: a reassignment started from a terminal in
 * another window shows up here within two seconds without anyone asking.
 *
 * The one thing it refuses to do is invent progress. Kafka reports which
 * brokers are joining and leaving, and nothing about how far the copy has got,
 * so there is no percentage here and no bar — a bar whose length is a guess is
 * worse than a sentence that is true.
 */
export function ReassignMonitor({
  profile,
  topic,
  nonce,
  onDanger,
  onSettled,
  push,
}: ReassignMonitorProps) {
  const [states, setStates] = useState<ReassignmentState[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [settled, setSettled] = useState(false);
  const [cancelling, setCancelling] = useState<TopicPartition[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [retry, setRetry] = useState(0);
  /**
   * Whether a poll has SEEN something in flight since the last submission.
   *
   * Only a poll sets this, and that is the whole point. It used to be set by
   * the nonce as well — "a plan was accepted, so there must be something to
   * finish" — which made the very first empty poll read as a drain that had
   * been watched to completion, and the panel announce that every move had
   * finished when it had never observed one. Acceptance is not motion.
   */
  const sawActive = useRef(false);
  /** Empty polls since the last submission — see [`QUIET_POLLS`]. */
  const quietRef = useRef(0);
  const [quietPolls, setQuietPolls] = useState(0);
  const [quietDismissed, setQuietDismissed] = useState(false);
  /**
   * True once this submission reached a conclusion (drain observed, or the
   * quiet no-op note fired). Stops the quiet counter so dismissing one note
   * can never surface the other for the same submission.
   */
  const concludedRef = useRef(false);

  const readOnly = profile.read_only;
  const isProtected = useIsProtected(profile.environment);

  useDangerSignal(error !== null, onDanger);

  // Held in a ref so a caller that re-creates the callback every render does
  // not restart the poll — and restarting the poll is exactly how a monitor
  // ends up asking the cluster twenty times a second.
  const settledRef = useRef(onSettled);
  useEffect(() => {
    settledRef.current = onSettled;
  }, [onSettled]);

  useEffect(() => {
    let alive = true;
    let timer: number | null = null;
    const stop = () => {
      if (timer !== null) window.clearInterval(timer);
      timer = null;
    };

    // A new submission (or a retry) starts the quiet count over: it only ever
    // means anything relative to the plan it is counting for.
    quietRef.current = 0;
    setQuietPolls(0);
    setQuietDismissed(false);
    concludedRef.current = false;

    const tick = async () => {
      try {
        const list = await reassignList(profile.id, topic);
        if (!alive) return;
        setStates(list);
        setError(null);
        if (list.length > 0) {
          sawActive.current = true;
          quietRef.current = 0;
          setQuietPolls(0);
          setSettled(false);
          return;
        }
        // A concluded submission counts nothing further: dismissing its note
        // must not let the quiet counter promote the OTHER note afterwards.
        if (concludedRef.current) return;
        quietRef.current += 1;
        setQuietPolls(quietRef.current);
        if (sawActive.current) {
          // Something was moving and now nothing is. This is the only path
          // that may claim a move finished, because it is the only one that
          // watched one.
          sawActive.current = false;
          concludedRef.current = true;
          setSettled(true);
          settledRef.current();
        } else if (nonce > 0 && quietRef.current === QUIET_POLLS) {
          concludedRef.current = true;
          // Never saw it move. The plan may genuinely have been a no-op, or it
          // may have completed between the acceptance and the first poll —
          // either way the partition table is now out of date, so it is read
          // again. What is NOT done here is claim a drain was observed.
          settledRef.current();
        }
      } catch (err) {
        if (!alive) return;
        // Stop on the first failure. A banner that reappears every two seconds
        // is not a monitor, it is a stuck key — the user gets one, with a way
        // to start it again.
        stop();
        setError(errorMessage(err));
      }
    };

    void tick();
    timer = window.setInterval(() => void tick(), POLL_MS);
    return () => {
      alive = false;
      stop();
    };
  }, [profile.id, topic, nonce, retry]);

  const cancelMoves = useCallback(async () => {
    if (cancelling === null) return;
    setBusy(true);
    // WHAT WAS ACTUALLY IN FLIGHT when the confirmation was answered. A cancel
    // aimed at a partition that had already finished comes back as Kafka's
    // NO_REASSIGNMENT_IN_PROGRESS, which the core folds into `error: null` —
    // so counting null errors would report stopping moves that had already
    // stopped themselves, which is precisely the reassurance nobody should be
    // given about a destructive action.
    const wasMoving = new Set(
      (states ?? []).map((s) => `${s.topic}/${s.partition}`),
    );
    const asked = cancelling.length;
    try {
      const results = await reassignCancel(profile.id, cancelling);
      const split = splitResults(
        results,
        (r) => !wasMoving.has(`${r.topic}/${r.partition}`),
      );
      setCancelling(null);
      if (split.failed.length > 0) {
        push({
          kind: "warn",
          title: `Stopped ${split.ok.length} of ${asked} move${
            asked === 1 ? "" : "s"
          }`,
          detail: `The cluster refused the rest: ${
            classifyError(split.failed[0].error ?? "").title
          }`,
        });
      } else if (split.ok.length > 0) {
        push({
          kind: "ok",
          title: `Stopped ${split.ok.length} replica move${
            split.ok.length === 1 ? "" : "s"
          }`,
          detail:
            "Each partition is back on the brokers it had. Whatever the new brokers had copied is gone.",
        });
      } else {
        push({
          kind: "info",
          title:
            asked === 1
              ? "That move had already finished"
              : "Those moves had already finished",
          detail:
            "The cluster reports nothing in flight for them, so there was nothing left to stop and nothing was thrown away.",
        });
      }
      setRetry((n) => n + 1);
    } catch (err) {
      setCancelling(null);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [cancelling, profile.id, push, states]);

  const active = states ?? [];
  /**
   * A plan was accepted here and several polls running have found nothing in
   * flight. Worth saying out loud — the alternative is a toast promising a
   * monitor and then no monitor — but worth saying HONESTLY: what Kafka
   * reports is an empty list, not a move that was watched to completion.
   */
  const quiet =
    nonce > 0 &&
    !settled &&
    !quietDismissed &&
    active.length === 0 &&
    quietPolls >= QUIET_POLLS;

  // Nothing in flight, nothing just finished, nothing to say.
  if (error === null && !settled && !quiet && active.length === 0) return null;

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">
          Replica moves
          {active.length > 0 && (
            <span className="panel-count">
              {active.length} partition{active.length === 1 ? "" : "s"} still
              moving
            </span>
          )}
        </h2>
        {active.length > 0 && (
          <div className="panel-tools">
            <button
              type="button"
              className="btn btn-danger"
              disabled={readOnly || busy}
              title={
                readOnly
                  ? READ_ONLY_WHY
                  : "Stop every move on this topic and put the partitions back"
              }
              onClick={() =>
                setCancelling(
                  active.map((s) => ({
                    topic: s.topic,
                    partition: s.partition,
                  })),
                )
              }
            >
              Stop every move
            </button>
          </div>
        )}
      </div>

      {error !== null && (
        <>
          <ErrorBanner raw={error} onDismiss={() => setError(null)} />
          <p className="table-note">
            Kavka stopped asking after that. Whatever is moving is still moving
            — the cluster does not need this window open to finish.
          </p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn"
              onClick={() => {
                setError(null);
                setRetry((n) => n + 1);
              }}
            >
              Start watching again
            </button>
          </div>
        </>
      )}

      {error === null && active.length === 0 && settled && (
        <>
          <p className="table-note">
            Every replica move on <code>{topic}</code> has finished. The
            partition table above has been read again, so it shows where the
            data actually is now.
          </p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setSettled(false)}
            >
              Dismiss
            </button>
          </div>
        </>
      )}

      {error === null && quiet && (
        <>
          <p className="table-note">
            Kafka reports no replica moves in flight for <code>{topic}</code> —
            a move to the brokers a partition is already on has nothing to copy
            and completes immediately. The partition table above has been read
            again either way, so it shows where the data is now.
          </p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setQuietDismissed(true)}
            >
              Dismiss
            </button>
          </div>
        </>
      )}

      {active.length > 0 && (
        <>
          <p className="table-note">
            Kafka doesn't report how far a copy has got, so neither does Kavka.
            A partition leaves this list the moment its new replicas are in sync
            — that is the finish line, and this asks every two seconds.
          </p>

          {/* The gutter carries the partition index, as it does everywhere
              else a partition has a row. Static table, so no role="grid". */}
          <div className="table-wrap">
            <table className="data-table">
              <caption className="sr-only">
                Partitions of {topic} whose replicas are still moving
              </caption>
              <thead>
                <tr>
                  <th scope="col" className="ledger-gutter">
                    Part.
                  </th>
                  <th scope="col">Brokers involved</th>
                  <th scope="col">Copying in</th>
                  <th scope="col">Handing back</th>
                  <th scope="col">What's happening</th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Stop</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {active.map((state) => (
                  <tr key={`${state.topic}/${state.partition}`}>
                    <td className="ledger-gutter">{state.partition}</td>
                    <td className="cell-mono">{state.replicas.join(", ")}</td>
                    <td className="cell-mono">
                      {state.adding.length === 0 ? (
                        <span className="absent">∅</span>
                      ) : (
                        state.adding.join(", ")
                      )}
                    </td>
                    <td className="cell-mono">
                      {state.removing.length === 0 ? (
                        <span className="absent">∅</span>
                      ) : (
                        state.removing.join(", ")
                      )}
                    </td>
                    <td className="move-phrase">{movePhrase(state)}</td>
                    <td className="col-affordance">
                      {/* Danger OUTLINE: it only ever opens the confirmation. */}
                      <button
                        type="button"
                        className="btn btn-danger btn-row"
                        disabled={readOnly || busy}
                        title={
                          readOnly
                            ? READ_ONLY_WHY
                            : "Stop this partition's move and put it back"
                        }
                        onClick={() =>
                          setCancelling([
                            { topic: state.topic, partition: state.partition },
                          ])
                        }
                      >
                        Stop
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {cancelling !== null && (
        <ConfirmModal
          title={
            cancelling.length === 1
              ? `Stop the move on partition ${cancelling[0].partition}?`
              : `Stop all ${cancelling.length} moves on ${topic}?`
          }
          body={
            <>
              Kafka puts{" "}
              {cancelling.length === 1
                ? "this partition"
                : "each of these partitions"}{" "}
              back on the brokers it had before the move started. Everything the
              new brokers have copied so far is thrown away — starting the same
              move again starts the copying from the beginning, not from where
              it got to.
              {isProtected && (
                <> {profile.name} is a production cluster, so this is live.</>
              )}
            </>
          }
          confirmLabel={
            cancelling.length === 1 ? "Stop this move" : "Stop every move"
          }
          typeToConfirm={isProtected ? topic : null}
          busy={busy}
          busyLabel="Kavka is asking the cluster to stop"
          onCancel={() => setCancelling(null)}
          onConfirm={() => void cancelMoves()}
        />
      )}
    </section>
  );
}
