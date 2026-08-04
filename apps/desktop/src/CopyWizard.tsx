import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  COPY_PROVENANCE_HEADERS,
  clusterConnect,
  copyDryRun,
  copyStart,
  copyStop,
  copySubscribe,
  errorMessage,
  profilesList,
  topicDetail,
  type ConnectionProfile,
  type CopyEstimate,
  type CopyProgress,
  type CopySpec,
  type PartitionDetail,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { envAttrs, envWireLabel, useEnvironment } from "./environments";
import { classifyError } from "./errors";
import { approxCount, groupDigits } from "./format";
import Overlay from "./Overlay";
import { ErrorBanner } from "./ProfileEditor";
import SeekBar, {
  buildSeek,
  initialSeekState,
  type SeekError,
  type SeekField,
  type SeekState,
} from "./SeekBar";
import { EnvChip } from "./Sidebar";
import type { ToastSpec } from "./Toast";

/**
 * CROSS-CLUSTER COPY — the first screen in Kavka where the cluster you are
 * looking at is not the cluster you are about to write to.
 *
 * Every guardrail in docs/DESIGN.md §6 is phrased about "the cluster on
 * screen", and here that is the SOURCE, which is the safe half. So this
 * component reads the DESTINATION profile for all four load-bearing layers:
 *
 *  - Layer 1 + 5: the wizard's body carries `data-env` of the DESTINATION, so
 *    picking a PROTECTED destination takes its colour and warms the substrate
 *    inside the modal the instant it is chosen — the same moment the
 *    environment picker teaches the guardrail in the connection form (§5.3).
 *  - Layer 4: type-to-confirm is gated on the destination's environment, not
 *    the source's. Copying into a protected environment always asks; copying
 *    out of one into an unprotected one never does.
 *  - Layer 7: the start control renders danger-outlined for a protected
 *    destination, and the panel carries an undismissable warning.
 *  - Layer 8: a read-only DESTINATION disables the start with the same
 *    sentence the rest of the app uses. The core refuses it anyway —
 *    `copy_start` runs `ensure_writable` against `dest_profile_id` — this is
 *    only so nobody clicks into a refusal.
 *
 * THE DRY RUN IS NOT OPTIONAL AND NOT AFTER THE FACT. §7 rule 8: destructive
 * copy states the blast radius before the button. So the wizard asks the
 * cluster what the range holds and shows that number before the confirm — and
 * when a filter is set it says, in the same sentence, that the number is an
 * upper bound rather than a count, because nothing can know how many records
 * match without reading them, and reading them is the copy.
 *
 * A COPY OUTLIVES THIS PANEL BY EXACTLY ONE PAYLOAD, for the same reason a
 * bulk produce does — see `reportFinalCounts` in ProducePanel, which this
 * mirrors. Closing mid-copy stops it, and the counts describing what actually
 * landed arrive after the component is gone.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/** How long a detached reporter waits for the final counts. */
const FINAL_COUNT_WAIT_MS = 30_000;

/** `[scheme://]host[:port]` → `host:port`, lower-cased, port defaulted. */
function normalizeBroker(server: string): string | null {
  const trimmed = server.trim();
  const scheme = trimmed.indexOf("://");
  const address = (scheme === -1 ? trimmed : trimmed.slice(scheme + 3)).trim();
  if (address.length === 0) return null;
  const colon = address.lastIndexOf(":");
  const port = colon === -1 ? "" : address.slice(colon + 1);
  return colon !== -1 && port.length > 0 && /^\d+$/.test(port)
    ? `${address.slice(0, colon).toLowerCase()}:${port}`
    : `${address.toLowerCase()}:9092`;
}

/**
 * Do these two connections point at the same cluster, as far as their
 * ADDRESSES can say?
 *
 * A PRE-FLIGHT HINT, not the guard. The guard is
 * `kavka_core::xcluster::ensure_not_self_copy`, which asks the brokers for
 * their cluster id and only falls back to this comparison — so the copy is
 * refused in core whatever this answers (D5). What this buys is the sentence
 * arriving while the user is still choosing, instead of after they press the
 * button on a copy that cannot run.
 *
 * Any address in common is conclusive, because a broker address belongs to
 * exactly one cluster. Two profiles listing DIFFERENT brokers of one cluster
 * come back false here — the core's cluster-id check is what catches those,
 * which is the whole reason the guard could not stay in this file.
 */
export function sameBootstrap(
  a: readonly string[],
  b: readonly string[],
): boolean {
  const left = new Set(
    a.map(normalizeBroker).filter((s): s is string => s !== null),
  );
  return b
    .map(normalizeBroker)
    .some((server) => server !== null && left.has(server));
}

type Step = "where" | "what" | "check";

type DestCheck =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "exists"; partitions: number }
  | { state: "missing"; message: string }
  | { state: "unreachable"; message: string };

/**
 * Keep a listener alive just long enough to hear how much of the copy actually
 * happened, then say so and unsubscribe. Bounded twice — by the `done` payload
 * and by a timeout — so it can neither leak nor go quiet.
 */
function reportFinalCounts(
  copyId: string,
  destTopic: string,
  lastSeen: CopyProgress | null,
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

  unsubscribe = copySubscribe(copyId, (payload) => {
    latest = payload;
    if (!payload.done || settled) return;
    close();
    if (payload.copied === 0 && payload.failed === 0 && payload.error === null)
      return;
    if (payload.error !== null) {
      const { title, detail } = classifyError(payload.error);
      push({ kind: "danger", title, detail });
      return;
    }
    const quiet = payload.assumed_complete.length;
    push({
      kind: payload.failed > 0 || quiet > 0 ? "warn" : "ok",
      title: `Copied ${groupDigits(payload.copied)} ${
        payload.copied === 1 ? "message" : "messages"
      } to ${destTopic}`,
      detail:
        payload.failed > 0
          ? `${groupDigits(payload.failed)} were refused by the destination.`
          : quiet > 0
            ? // Never folded into the count above it: "copied 900" with no
              // caveat is a claim about the whole topic.
              `${quiet} partition${quiet === 1 ? "" : "s"} went quiet before ${
                quiet === 1 ? "its end offset" : "their end offsets"
              }, so this may not be everything.`
            : "The window was closed while it ran, so this is the final count from the core.",
    });
  });
  if (settled) unsubscribe?.();

  timer = window.setTimeout(() => {
    if (settled) return;
    close();
    if (latest === null || (latest.copied === 0 && latest.failed === 0)) return;
    push({
      kind: "warn",
      title: `Stopped copying to ${destTopic} — at least ${groupDigits(
        latest.copied,
      )} messages were written`,
      detail:
        "The window was closed before the destination finished acknowledging them, so this is a floor rather than a total.",
    });
  }, FINAL_COUNT_WAIT_MS);

  // Idempotent, and the reason there is anything to report.
  void copyStop(copyId);
}

interface CopyWizardProps {
  /** The SOURCE connection — the cluster this workspace is showing. */
  profile: ConnectionProfile;
  topic: string;
  partitions: PartitionDetail[];
  push: (spec: ToastSpec) => void;
  /** Land the user on the destination cluster once the copy is done. */
  onOpenCluster?: (profileId: string, topic: string) => void;
  onClose: () => void;
}

export default function CopyWizard({
  profile,
  topic,
  partitions,
  push,
  onOpenCluster,
  onClose,
}: CopyWizardProps) {
  const firstRef = useRef<HTMLSelectElement | null>(null);

  const [step, setStep] = useState<Step>("where");
  const [profiles, setProfiles] = useState<ConnectionProfile[] | null>(null);
  const [destId, setDestId] = useState(profile.id);
  const [destTopic, setDestTopic] = useState(topic);
  const [check, setCheck] = useState<DestCheck>({ state: "idle" });

  // ── Scope ──────────────────────────────────────────────────────────────
  const [seek, setSeek] = useState<SeekState>(() =>
    initialSeekState("earliest", "1000"),
  );
  const [seekError, setSeekError] = useState<SeekError | null>(null);
  const [cel, setCel] = useState(false);
  const [text, setText] = useState("");
  const [celText, setCelText] = useState("");
  const [maxMessages, setMaxMessages] = useState("");
  const [rate, setRate] = useState("");

  // ── Options ────────────────────────────────────────────────────────────
  const [preservePartition, setPreservePartition] = useState(false);
  const [provenance, setProvenance] = useState(true);

  // ── Dry run ────────────────────────────────────────────────────────────
  const [dry, setDry] = useState<CopyEstimate | null>(null);
  const [dryBusy, setDryBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  // ── The run ────────────────────────────────────────────────────────────
  const [runNonce, setRunNonce] = useState<number | null>(null);
  const [progress, setProgress] = useState<CopyProgress | null>(null);
  const [stopping, setStopping] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const runSpec = useRef<CopySpec | null>(null);
  const copyId = useRef<string | null>(null);
  const finalized = useRef(false);
  const latest = useRef<CopyProgress | null>(null);
  /**
   * The jump handler, read through a ref rather than closed over.
   *
   * The copy session's effect must NOT re-run on anything but the run
   * generation: its cleanup stops the copy, so an identity change in a prop
   * would tear down a running copy and start a second one against the same
   * destination. A callback the app root recreates on any state change is
   * exactly that hazard, and a ref costs one line to remove it entirely.
   */
  const openClusterRef = useRef(onOpenCluster);
  openClusterRef.current = onOpenCluster;

  useEffect(() => {
    let live = true;
    void profilesList()
      .then((list) => {
        if (live) setProfiles(list);
      })
      .catch((err: unknown) => {
        if (live) setFailure(errorMessage(err));
      });
    return () => {
      live = false;
    };
  }, []);

  const dest = useMemo(
    () => profiles?.find((p) => p.id === destId) ?? null,
    [profiles, destId],
  );
  const destEnv = dest?.environment ?? profile.environment;
  // The DESTINATION's definition, not the workspace's: everything below
  // keys on where the records are going.
  const destDef = useEnvironment(destEnv);
  const destIsProtected = destDef.protected;
  const destReadOnly = dest?.read_only ?? false;
  /**
   * The same cluster, by ADDRESS rather than by profile id — two saved
   * connections to one set of brokers have two ids and one log, and a copy into
   * the topic it is reading is a topic that feeds itself.
   *
   * Falls back to the id while the profile list is still loading, which is the
   * only moment there is nothing to compare.
   */
  const sameCluster =
    dest === null
      ? destId === profile.id
      : sameBootstrap(profile.bootstrap_servers, dest.bootstrap_servers);

  const known = useMemo(
    () => new Set(partitions.map((p) => p.partition)),
    [partitions],
  );

  const filtered = cel ? celText.trim().length > 0 : text.trim().length > 0;

  // ── The destination check ──────────────────────────────────────────────

  /**
   * Does the destination topic exist?
   *
   * Two failures with two different answers, so they are not collapsed: a
   * connection Kavka has never opened in this window cannot answer at all
   * (which is not the topic's fault), and a topic read that fails on an open
   * connection is either "no such topic" or "no Describe on it" — Kafka does
   * not distinguish, so neither does the sentence.
   */
  const runCheck = useCallback(async () => {
    const name = destTopic.trim();
    if (name.length === 0) {
      setCheck({ state: "idle" });
      return;
    }
    setCheck({ state: "checking" });
    try {
      const detail = await topicDetail(destId, name);
      setCheck({ state: "exists", partitions: detail.partitions.length });
      return;
    } catch {
      /* fall through — the connection may simply not be open here */
    }
    try {
      await clusterConnect(destId);
    } catch (err) {
      setCheck({ state: "unreachable", message: errorMessage(err) });
      return;
    }
    try {
      const detail = await topicDetail(destId, name);
      setCheck({ state: "exists", partitions: detail.partitions.length });
    } catch (err) {
      setCheck({ state: "missing", message: errorMessage(err) });
    }
  }, [destId, destTopic]);

  // Changing either half of the destination invalidates what we knew about it.
  useEffect(() => {
    setCheck({ state: "idle" });
    setDry(null);
  }, [destId, destTopic]);

  // ── Building the spec ──────────────────────────────────────────────────

  const buildSpec = useCallback((): CopySpec | { message: string } => {
    const name = destTopic.trim();
    if (name.length === 0)
      return { message: "Name the topic to copy into on the destination." };
    // The early half of the guard. The core refuses this too, with the brokers'
    // own cluster ids rather than their addresses, so a destination this cannot
    // recognise as the same cluster is still stopped — just one click later.
    if (sameCluster && name === topic)
      return {
        message:
          "That is the topic you are copying from, on the same cluster. Every record the copy wrote would land back in the log it is reading, so the topic would grow for as long as the copy ran. Pick another topic, or a connection to another cluster.",
      };
    const built = buildSeek(seek, known, { maxCount: 1_000_000 });
    if ("field" in built) {
      setSeekError(built);
      return { message: built.message };
    }
    setSeekError(null);

    let max: number | null = null;
    if (maxMessages.trim().length > 0) {
      const n = Number.parseInt(maxMessages, 10);
      if (!Number.isFinite(n) || n < 1)
        return { message: "Copy at least one message, or leave the ceiling empty." };
      max = n;
    }
    let perSec: number | null = null;
    if (rate.trim().length > 0) {
      const n = Number.parseInt(rate, 10);
      if (!Number.isFinite(n) || n < 1)
        return {
          message:
            "A rate limit is messages per second — leave it empty for as fast as the destination will take them.",
        };
      perSec = n;
    }

    const substring = cel ? null : text.trim().length === 0 ? null : text;
    const program = cel && celText.trim().length > 0 ? celText.trim() : null;

    return {
      source_topic: topic,
      dest_profile_id: destId,
      dest_topic: name,
      seek: built.seek,
      partitions: built.partitions,
      filter:
        substring === null && program === null
          ? null
          : { substring, cel: program },
      max_messages: max,
      rate_per_sec: perSec,
      preserve_partition: preservePartition,
      provenance_headers: provenance,
    };
  }, [
    destTopic,
    sameCluster,
    topic,
    seek,
    known,
    maxMessages,
    rate,
    cel,
    text,
    celText,
    destId,
    preservePartition,
    provenance,
  ]);

  const runDry = useCallback(async () => {
    const spec = buildSpec();
    if ("message" in spec) {
      setFormError(spec.message);
      return;
    }
    setFormError(null);
    setFailure(null);
    setDryBusy(true);
    try {
      setDry(await copyDryRun(profile.id, spec));
    } catch (err) {
      setDry(null);
      setFailure(errorMessage(err));
    } finally {
      setDryBusy(false);
    }
  }, [buildSpec, profile.id]);

  // The dry run runs on the way INTO the confirm step, never after it: the
  // number has to be on screen before the button that spends it.
  useEffect(() => {
    if (step !== "check" || dry !== null || dryBusy) return;
    void runDry();
    // `runDry` changes identity with every field; re-running on that would ask
    // the cluster again on each keystroke of a field the user can't reach here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  // ── The copy session ───────────────────────────────────────────────────

  const startCopy = useCallback(() => {
    const spec = buildSpec();
    if ("message" in spec) {
      setFormError(spec.message);
      return;
    }
    setFormError(null);
    setFailure(null);
    setProgress(null);
    setStopping(false);
    finalized.current = false;
    latest.current = null;
    runSpec.current = spec;
    setRunNonce((prev) => (prev ?? 0) + 1);
  }, [buildSpec]);

  const stopCopy = useCallback(() => {
    const id = copyId.current;
    if (id === null) {
      setRunNonce(null);
      return;
    }
    setStopping(true);
    void copyStop(id);
  }, []);

  useEffect(() => {
    if (runNonce === null) return;
    const spec = runSpec.current;
    if (spec === null) return;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    let started: string | null = null;
    const destName = spec.dest_topic;
    const destProfile = spec.dest_profile_id;

    void (async () => {
      try {
        const id = await copyStart(profile.id, spec);
        if (cancelled) {
          // The panel went away while `copy_start` was in flight, so this
          // component will never see the events — and records may already be
          // on their way. Stop it, and stay around for the counts.
          reportFinalCounts(id, destName, null, push);
          return;
        }
        started = id;
        copyId.current = id;
        unsubscribe = copySubscribe(
          id,
          (payload) => {
            latest.current = payload;
            setProgress(payload);
            if (!payload.done) return;
            finalized.current = true;
            setStopping(false);
            setRunNonce(null);
            if (payload.error !== null) {
              setFailure(payload.error);
              return;
            }
            const quiet = payload.assumed_complete.length;
            push({
              kind: payload.failed > 0 || quiet > 0 ? "warn" : "ok",
              title: `Copied ${groupDigits(payload.copied)} ${
                payload.copied === 1 ? "message" : "messages"
              } to ${destName}`,
              detail:
                payload.failed > 0
                  ? `${groupDigits(
                      payload.failed,
                    )} were refused by the destination — they are not there.`
                  : quiet > 0
                    ? `Read ${groupDigits(payload.scanned)} from ${topic} — but ${quiet} partition${
                        quiet === 1 ? "" : "s"
                      } went quiet before ${
                        quiet === 1 ? "its end offset" : "their end offsets"
                      }, so this may not be everything.`
                    : `Read ${groupDigits(payload.scanned)} from ${topic}.`,
              action:
                openClusterRef.current === undefined
                  ? undefined
                  : {
                      label: "Browse the destination",
                      run: () => openClusterRef.current?.(destProfile, destName),
                    },
            });
          },
          (message) => {
            setFailure(message);
            finalized.current = true;
            setStopping(false);
            setRunNonce(null);
          },
        );
      } catch (err) {
        if (cancelled) return;
        const raw = errorMessage(err);
        const { cause, title, detail } = classifyError(raw, {
          environmentProtected: destIsProtected,
        });
        if (cause === "read-only") push({ kind: "danger", title, detail });
        else setFailure(raw);
        finalized.current = true;
        setStopping(false);
        setRunNonce(null);
      }
    })();

    return () => {
      cancelled = true;
      unsubscribe?.();
      if (started !== null) {
        if (finalized.current) void copyStop(started);
        else reportFinalCounts(started, destName, latest.current, push);
      }
      copyId.current = null;
    };
  }, [runNonce, profile.id, topic, destEnv, push]);

  const running = runNonce !== null;
  const done = progress?.done === true;

  // ── The numbers the confirm step spends ────────────────────────────────

  const estimate = dry?.would_copy_estimate ?? 0;
  /**
   * Is that number a scan size rather than a count of what will be written?
   *
   * The core says so when it can — it also reads high on a topic written by a
   * transactional producer, where commit markers occupy offsets no consumer
   * ever receives, which the UI has no way to know. The filter is the fallback
   * inference for a build that answers with the contract's two fields only.
   */
  const estimateOnly = dry?.estimate_only ?? filtered;
  const ceiling =
    maxMessages.trim().length > 0
      ? Math.min(estimate, Number.parseInt(maxMessages, 10) || estimate)
      : estimate;

  const destPartitionCount =
    check.state === "exists" ? check.partitions : null;
  const partitionMismatch =
    preservePartition &&
    destPartitionCount !== null &&
    destPartitionCount < partitions.length;

  const startBlocked = destReadOnly
    ? READ_ONLY_WHY
    : running
      ? "Kavka is already copying"
      : undefined;

  // ── The prod confirmation, which REPLACES the panel ────────────────────
  // Two overlays at once are two focus traps; only the tree changes, so
  // Cancel comes back to exactly what the user had set up (see ProducePanel).
  if (confirming) {
    return (
      <ConfirmModal
        title={`Copy ${topic} into ${destTopic} on ${dest?.name ?? "the destination"}?`}
        body={
          <>
            This writes {estimateOnly ? "up to " : "about "}
            {approxCount(ceiling)} real messages into <code>{destTopic}</code> on{" "}
            <code>{dest?.name ?? destId}</code>, which is a production cluster.
            Every consumer group reading that topic sees them immediately, and a
            message can't be unsent.
            {estimateOnly && (
              <>
                {" "}
                {filtered
                  ? "The filter is applied while reading, so the real number is whatever matches"
                  : "Kavka counted offsets rather than records, so the real number can be lower"}{" "}
                — that estimate is a ceiling, not a count.
              </>
            )}
          </>
        }
        confirmLabel="Copy messages"
        typeToConfirm={destTopic}
        typePrompt={
          <>
            Type <code>{destTopic}</code> to confirm the destination topic
          </>
        }
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          startCopy();
        }}
      />
    );
  }

  return (
    <Overlay
      surfaceClass={`modal modal-wide copy-modal${
        destIsProtected ? " modal-destructive" : ""
      }`}
      labelledBy="copy-title"
      initialFocus={firstRef}
      onClose={onClose}
    >
      {/* The DESTINATION's environment, not the workspace's: the ledger rule
          inside this modal takes the destination's colour the moment one is
          picked, and its warm substrate the moment that destination is
          PROTECTED (§6 layer 1) — the guardrail this screen most needs. */}
      <div
        className="copy-body"
        {...envAttrs(destDef)}
        data-env-label={envWireLabel(destDef)}
      >
        <h2 className="modal-title" id="copy-title">
          Copy messages from {topic}
        </h2>

        {/* Where you are in the wizard. Whitespace and one hairline, no card. */}
        <ol className="wizard-steps">
          {(
            [
              ["where", "Destination"],
              ["what", "What to copy"],
              ["check", "Check and copy"],
            ] as const
          ).map(([key, label], i) => (
            <li
              key={key}
              className={`wizard-step${step === key ? " wizard-step-active" : ""}`}
              aria-current={step === key ? "step" : undefined}
            >
              <span className="wizard-step-n" aria-hidden="true">
                {i + 1}
              </span>
              {label}
            </li>
          ))}
        </ol>

        {/* §6 layer 7, aimed at the destination. Undismissable, on purpose. */}
        {destIsProtected && (
          <div className="banner banner-danger" role="alert">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                {dest?.name ?? "The destination"} is a production cluster.
              </p>
              <p className="banner-detail">
                A copy is a produce, repeated. Whatever is consuming{" "}
                <code>{destTopic || "the destination topic"}</code> right now
                will see every record this writes, and none of it can be unsent.
              </p>
            </div>
          </div>
        )}

        {destReadOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {failure !== null && (
          <ErrorBanner raw={failure} onDismiss={() => setFailure(null)} />
        )}

        {/* ── Step 1: where ─────────────────────────────────────────────── */}
        {step === "where" && (
          <div className="modal-panel">
            <div className="field">
              <label className="field-label" htmlFor="cw-dest">
                Destination connection
              </label>
              <select
                id="cw-dest"
                ref={firstRef}
                value={destId}
                onChange={(e) => setDestId(e.target.value)}
              >
                {(profiles ?? [profile]).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} — {p.environment}
                    {p.read_only ? " · read-only" : ""}
                    {p.id === profile.id ? " · this connection" : ""}
                  </option>
                ))}
              </select>
              <span className="field-hint">
                {profiles === null
                  ? "Reading your connections…"
                  : sameCluster
                    ? "The same cluster — this is a replay into another topic, which is the same operation."
                    : `${dest?.bootstrap_servers.join(", ") ?? ""} — the address the records will be written to.`}
              </span>
              {/* The env chip beside the picker: a protected destination is the
                  only FILLED chip, so it reads as a badge from across the room. */}
              {dest !== null && (
                <span className="copy-dest-id">
                  <EnvChip env={dest.environment} />
                  <span className="copy-dest-address">
                    {dest.bootstrap_servers.join(", ")}
                  </span>
                </span>
              )}
            </div>

            <div className="field">
              <label className="field-label" htmlFor="cw-topic">
                Destination topic
              </label>
              <div className="copy-topic-row">
                <input
                  id="cw-topic"
                  type="text"
                  className="input-mono"
                  value={destTopic}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(e) => setDestTopic(e.target.value)}
                />
                <button
                  type="button"
                  className="btn"
                  disabled={check.state === "checking" || destTopic.trim().length === 0}
                  aria-busy={check.state === "checking" || undefined}
                  title={
                    destTopic.trim().length === 0
                      ? "Name the topic first"
                      : "Ask the destination whether this topic exists"
                  }
                  onClick={() => void runCheck()}
                >
                  <span className="btn-busy-slot" aria-hidden="true">
                    {check.state === "checking" ? (
                      <span className="spinner" />
                    ) : null}
                  </span>
                  Check
                </button>
              </div>
              <DestCheckNote
                check={check}
                destTopic={destTopic.trim()}
                destName={dest?.name ?? ""}
                sourcePartitions={partitions.length}
              />
            </div>
          </div>
        )}

        {/* ── Step 2: what ──────────────────────────────────────────────── */}
        {step === "what" && (
          <div className="modal-panel">
            <SeekBar
              idPrefix="cw"
              label="What to copy"
              state={seek}
              onChange={(patch) => {
                setSeek((prev) => ({ ...prev, ...patch }));
                if (patch.mode !== undefined) setSeekError(null);
                setDry(null);
              }}
              partitions={partitions}
              error={seekError}
              onClearError={(field: SeekField) =>
                setSeekError((prev) => (prev?.field === field ? null : prev))
              }
              showCount={seek.mode === "latest"}
              countLabel="Newest per partition"
            />

            {/* The same bar search uses, so a filter written there can be
                pasted here and mean the same thing. */}
            <div className="field">
              <span className="field-label">Only copy messages that match</span>
              <div className="searchbar">
                <span className="searchbar-glyph" aria-hidden="true">
                  ⌕
                </span>
                <input
                  type="text"
                  className={`searchbar-input${cel ? " input-mono" : ""}`}
                  value={cel ? celText : text}
                  placeholder={
                    cel
                      ? 'value.status == "failed"'
                      : "Everything in the range — or type to filter"
                  }
                  autoComplete="off"
                  spellCheck={false}
                  aria-label={cel ? "CEL filter" : "Find in key, value or headers"}
                  onChange={(e) => {
                    if (cel) setCelText(e.target.value);
                    else setText(e.target.value);
                    setDry(null);
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
                  onClick={() => setCel((prev) => !prev)}
                >
                  ƒx
                </button>
              </div>
              <span className="field-hint">
                Leave it empty to copy everything in the range. A filter is
                applied while reading, so a filtered copy can only ever be
                estimated before it runs — the check step says so.
              </span>
            </div>

            <div className="reset-row">
              <label className="seekbar-field">
                <span className="seekbar-label">Stop after</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  className="input-num"
                  value={maxMessages}
                  placeholder="no limit"
                  onChange={(e) => {
                    setMaxMessages(e.target.value);
                    setDry(null);
                  }}
                />
              </label>
              <label className="seekbar-field">
                <span className="seekbar-label">Messages per second</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  className="input-num"
                  value={rate}
                  placeholder="as fast as it takes them"
                  onChange={(e) => setRate(e.target.value)}
                />
              </label>
            </div>
            <span className="field-hint">
              A rate limit is how you copy into a live cluster without becoming
              the incident. It costs nothing but time.
            </span>

            <div className="check-field">
              <input
                id="cw-preserve"
                type="checkbox"
                checked={preservePartition}
                aria-describedby="cw-preserve-hint"
                onChange={(e) => setPreservePartition(e.target.checked)}
              />
              <label className="check-label" htmlFor="cw-preserve">
                Keep each message on the partition it came from
              </label>
              <span className="field-hint" id="cw-preserve-hint">
                Off, the destination's own partitioner decides — by key, so
                ordering per key survives. On, partition 3 goes to partition 3,
                which only works when the destination has at least as many
                partitions as the source.
                {partitionMismatch && (
                  <>
                    {" "}
                    <strong>
                      {destTopic} has {destPartitionCount} partition
                      {destPartitionCount === 1 ? "" : "s"} and {topic} has{" "}
                      {partitions.length}
                    </strong>
                    , so records above partition {destPartitionCount! - 1} have
                    nowhere to go and the destination will refuse them.
                  </>
                )}
              </span>
            </div>

            <div className="check-field">
              <input
                id="cw-prov"
                type="checkbox"
                checked={provenance}
                aria-describedby="cw-prov-hint"
                onChange={(e) => setProvenance(e.target.checked)}
              />
              <label className="check-label" htmlFor="cw-prov">
                Add headers saying where each message came from
              </label>
              <span className="field-hint" id="cw-prov-hint">
                On by default. Every copied record carries{" "}
                {COPY_PROVENANCE_HEADERS.map((h, i) => (
                  <span key={h}>
                    {i > 0 ? ", " : ""}
                    <code>{h}</code>
                  </span>
                ))}
                , so six months from now the answer to “where did this record
                come from” is in the record. The timestamp one is only added
                when the record has a timestamp — an invented one would be
                worse than none. Turn the lot off only when the consumer
                validates headers strictly.
              </span>
            </div>
          </div>
        )}

        {/* ── Step 3: check and copy ────────────────────────────────────── */}
        {step === "check" && (
          <div className="modal-panel">
            {running || progress !== null ? (
              <CopyProgressPanel
                progress={progress}
                sourceTopic={topic}
                destTopic={destTopic}
                running={running}
                stopping={stopping}
              />
            ) : dryBusy ? (
              <p className="dialog-note">
                Asking {dest?.name ?? "the cluster"} what that range holds…
              </p>
            ) : dry === null ? (
              <p className="dialog-note">
                Kavka couldn't work out what this would copy. The message above
                says why; the copy itself would hit the same problem.
              </p>
            ) : (
              <>
                <p className="reset-preview">
                  {estimateOnly
                    ? `Kavka will read about ${approxCount(
                        estimate,
                      )} messages from ${topic} and copy into ${destTopic} on ${
                        dest?.name ?? "the destination"
                      } the ones that count. That number is what gets READ — ${
                        filtered
                          ? "a filter is set, so how many get written is whatever matches"
                          : "the core reports it as an upper bound, which is what watermark arithmetic can promise"
                      }, and nothing can know the real figure without reading them.`
                    : `About ${approxCount(
                        ceiling,
                      )} messages will be copied from ${topic} into ${destTopic} on ${
                        dest?.name ?? "the destination"
                      }.${
                        maxMessages.trim().length > 0
                          ? ` The ceiling you set is ${groupDigits(
                              Number.parseInt(maxMessages, 10) || 0,
                            )}.`
                          : ""
                      }`}
                </p>

                {dry.per_partition.length > 0 && (
                  <div className="table-wrap">
                    <table className="data-table">
                      <caption className="sr-only">
                        What each partition would contribute to the copy
                      </caption>
                      <thead>
                        <tr>
                          <th scope="col" className="ledger-gutter">
                            Part.
                          </th>
                          <th scope="col" className="col-num">
                            From
                          </th>
                          <th scope="col" className="col-num">
                            To
                          </th>
                          <th scope="col" className="col-num">
                            Messages
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {dry.per_partition.map((p) => (
                          <tr key={p.partition}>
                            <td className="ledger-gutter">{p.partition}</td>
                            <td className="col-num cell-mono cell-mono-num">
                              {groupDigits(p.current_offset)}
                            </td>
                            <td className="col-num cell-mono cell-mono-num">
                              {groupDigits(p.end_offset)}
                            </td>
                            <td className="col-num cell-num">
                              {groupDigits(
                                Math.max(0, p.end_offset - p.current_offset),
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}

                <p className="dialog-note">
                  Nothing has been written yet. This came from the destination's
                  watermarks, and both clusters keep moving — a copy started in a
                  minute reads whatever is there then.{" "}
                  <button
                    type="button"
                    className="btn btn-ghost inspector-inline-btn"
                    onClick={() => void runDry()}
                  >
                    Check again
                  </button>
                </p>
              </>
            )}
          </div>
        )}

        {formError !== null && <span className="field-error">{formError}</span>}

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            onClick={onClose}
            title={
              running
                ? "Stops the copy — Kavka reports what actually reached the destination in a message"
                : undefined
            }
          >
            {running ? "Close" : done ? "Done" : "Cancel"}
          </button>

          {step !== "where" && !running && (
            <button
              type="button"
              className="btn"
              onClick={() => setStep(step === "check" ? "what" : "where")}
            >
              Back
            </button>
          )}

          {step === "where" && (
            <button
              type="button"
              className="btn btn-primary"
              disabled={destTopic.trim().length === 0}
              title={
                destTopic.trim().length === 0
                  ? "Name the topic to copy into"
                  : undefined
              }
              onClick={() => setStep("what")}
            >
              Next
            </button>
          )}

          {step === "what" && (
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                const spec = buildSpec();
                if ("message" in spec) {
                  setFormError(spec.message);
                  return;
                }
                setFormError(null);
                setDry(null);
                setStep("check");
              }}
            >
              Next
            </button>
          )}

          {step === "check" &&
            (running ? (
              <button
                type="button"
                className="btn btn-latched"
                onClick={stopCopy}
                disabled={stopping}
                aria-busy={stopping || undefined}
                title={
                  stopping
                    ? "Kavka is stopping the copy and counting what the destination took"
                    : "Stop copying — what has already been written stays written"
                }
              >
                <span className="btn-busy-slot" aria-hidden="true">
                  {stopping ? <span className="spinner" /> : null}
                </span>
                Stop
              </button>
            ) : (
              // §6 layer 7: a write control reads as danger on a prod
              // destination even when routine.
              <button
                type="button"
                className={`btn ${destIsProtected ? "btn-danger" : "btn-primary"}`}
                disabled={destReadOnly || dry === null || dryBusy}
                title={
                  startBlocked ??
                  (dry === null
                    ? "Kavka hasn't been able to check what this would copy yet"
                    : undefined)
                }
                onClick={() => {
                  if (destIsProtected) setConfirming(true);
                  else startCopy();
                }}
              >
                {done ? "Copy again" : "Copy messages"}
              </button>
            ))}
        </div>
      </div>
    </Overlay>
  );
}

/** What Kavka found when it asked the destination about the topic. */
function DestCheckNote({
  check,
  destTopic,
  destName,
  sourcePartitions,
}: {
  check: DestCheck;
  destTopic: string;
  destName: string;
  sourcePartitions: number;
}) {
  if (check.state === "idle")
    return (
      <span className="field-hint">
        Kavka hasn't looked yet. Check first — a copy into a topic that isn't
        there fails on the first record, or silently auto-creates one with the
        destination's default partition count.
      </span>
    );
  if (check.state === "checking")
    return <span className="field-hint">Asking {destName} about {destTopic}…</span>;
  if (check.state === "exists")
    return (
      <span className="field-hint">
        <code>{destTopic}</code> exists on {destName} with {check.partitions}{" "}
        partition{check.partitions === 1 ? "" : "s"}
        {check.partitions < sourcePartitions
          ? ` — fewer than the ${sourcePartitions} here, which matters only if you keep the source partitions.`
          : "."}
      </span>
    );
  if (check.state === "unreachable")
    return (
      <span className="field-error">
        Kavka couldn't reach {destName} to check. {classifyError(check.message).title} — the copy
        would hit the same thing, so fix the connection first.
      </span>
    );
  return (
    <span className="field-hint">
      Kavka couldn't read <code>{destTopic}</code> on {destName}. Either it
      doesn't exist there yet, or this account can't describe it — Kafka answers
      the same way for both. Create it on {destName} first (Topics → Create
      topic) so it gets the partition count and settings you meant, rather than
      whatever auto-creation would give it.
    </span>
  );
}

/** The three numbers, while it runs and after it stops. */
function CopyProgressPanel({
  progress,
  sourceTopic,
  destTopic,
  running,
  stopping,
}: {
  progress: CopyProgress | null;
  sourceTopic: string;
  destTopic: string;
  running: boolean;
  stopping: boolean;
}) {
  const copied = progress?.copied ?? 0;
  const scanned = progress?.scanned ?? 0;
  const failed = progress?.failed ?? 0;
  const done = progress?.done === true;
  const assumed = progress?.assumed_complete ?? [];
  return (
    <>
      <p className="reset-preview" role="status">
        {done
          ? `Copied ${groupDigits(copied)} of ${groupDigits(
              scanned,
            )} messages read from ${sourceTopic} into ${destTopic}.`
          : `Copying ${sourceTopic} into ${destTopic} — ${groupDigits(
              copied,
            )} written, ${groupDigits(scanned)} read.`}
        {stopping ? " Stopping — waiting for the destination's final count." : ""}
      </p>
      {/* A COPY THAT FINISHED BECAUSE THE SOURCE WENT QUIET IS NOT THE SAME
          CLAIM AS ONE THAT REACHED THE WATERMARKS — the same rule the search
          progress block states, and it matters more here, because this one
          wrote to another cluster and the number it reports is what somebody
          will reconcile against. Only on a finished copy: mid-run it would fire
          and clear as partitions catch up. */}
      {done && assumed.length > 0 && (
        <div className="banner banner-warn" role="status">
          <span className="banner-glyph" aria-hidden="true">
            !
          </span>
          <div className="banner-body">
            <p className="banner-title">
              {assumed.length === 1 ? "Partition " : "Partitions "}
              {assumed.map((p, i) => (
                <span key={p}>
                  {i > 0 ? ", " : ""}
                  <code>P{p}</code>
                </span>
              ))}{" "}
              of {sourceTopic} went quiet before{" "}
              {assumed.length === 1 ? "its end offset" : "their end offsets"}.
            </p>
            <p className="banner-detail">
              Everything Kavka could read there was copied, but this is not the
              same as reaching the end. Transactional markers usually explain
              it: they take offsets a consumer never receives, so the last one
              can never be read. A broker that stopped answering looks identical
              from here — so check <code>{destTopic}</code> against the source
              before you treat this as a complete replay.
            </p>
          </div>
        </div>
      )}
      {failed > 0 && (
        <div className="banner banner-warn" role="status">
          <span className="banner-glyph" aria-hidden="true">
            !
          </span>
          <div className="banner-body">
            <p className="banner-title">
              {groupDigits(failed)} {failed === 1 ? "message was" : "messages were"}{" "}
              refused by the destination.
            </p>
            <p className="banner-detail">
              They are not in <code>{destTopic}</code>. The usual causes are a
              record larger than the destination's <code>max.message.bytes</code>{" "}
              and a partition the destination doesn't have.
            </p>
          </div>
        </div>
      )}
      {running && !done && (
        <p className="dialog-note">
          Closing this window stops the copy. What has already been written
          stays written — Kavka reports the final count either way.
        </p>
      )}
    </>
  );
}
