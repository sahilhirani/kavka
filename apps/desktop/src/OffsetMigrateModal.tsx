import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  clusterConnect,
  groupDetail,
  offsetsMigrateApply,
  offsetsMigratePlan,
  profilesList,
  type ConnectionProfile,
  type GroupDetail,
  type GroupOffset,
  type OffsetMigrationRow,
} from "./api";
import { EnvChip, envAttrs, envWireLabel, useEnvironment } from "./environments";
import { classifyError } from "./errors";
import { formatStamp, groupDigits } from "./format";
import { Term } from "./Glossary";
import Overlay from "./Overlay";

/**
 * OFFSET MIGRATION — moving "where an application had got to" between two
 * clusters that have never agreed about a single offset number.
 *
 * THE ONE THING THIS SCREEN EXISTS TO TEACH: an offset means nothing on
 * another cluster. Partition 3 offset 8 412 on staging and partition 3 offset
 * 8 412 on prod are unrelated positions in unrelated logs, and copying the
 * number across is the mistake this feature is here to prevent. So the plan
 * carries a METHOD per partition and the table says it in words rather than as
 * a code:
 *
 *   timestamp — the source's committed record had a timestamp, and the
 *               destination's own broker answered with the offset that time
 *               maps to THERE. The only method that survives two clusters.
 *   earliest  — no timestamp was available, so the destination starts at the
 *               beginning of its partition. Honest, and usually a lot of
 *               re-processing, which the row says out loud.
 *   latest    — the group was already caught up here, so it starts at the END
 *               of the destination partition and waits for new records. No
 *               re-processing at all — the opposite consequence to `earliest`,
 *               which is why the two can never share one "fell back" bucket in
 *               the summary above the table.
 *   none      — the source group never committed here; there is nothing to
 *               move and nothing is written.
 *
 * TWO GUARDS, in this order and both before the button:
 *  - The destination group must be Empty. Same vocabulary as offsets_reset,
 *    and stated BEFORE the apply rather than relayed as a broker refusal
 *    afterwards — a live group is a thing you go and stop, not a thing you
 *    retry.
 *  - Type-to-confirm when the destination is PROTECTED (§6 layer 4) or when the
 *    destination group already has committed offsets, because then this is an
 *    overwrite of a position something else chose.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

type DestGroup =
  | { state: "idle" }
  | { state: "checking" }
  /** The group exists and is Empty — the only state Kafka accepts a write in. */
  | { state: "empty"; committed: number }
  | { state: "live"; kafkaState: string; members: number }
  /** No such group on the destination. Nothing to overwrite, nothing to stop. */
  | { state: "absent" };

interface OffsetMigrateModalProps {
  /** The SOURCE connection — the cluster this workspace is showing. */
  profile: ConnectionProfile;
  /** The source group, already read. */
  detail: GroupDetail;
  /** Which of the group's topics to start on. */
  initialTopic: string;
  onClose: () => void;
}

export default function OffsetMigrateModal({
  profile,
  detail,
  initialTopic,
  onClose,
}: OffsetMigrateModalProps) {
  const cancelRef = useRef<HTMLButtonElement | null>(null);

  const topics = useMemo(
    () => Array.from(new Set(detail.offsets.map((o) => o.topic))).sort(),
    [detail.offsets],
  );

  const [profiles, setProfiles] = useState<ConnectionProfile[] | null>(null);
  const [topic, setTopic] = useState(
    () => (topics.includes(initialTopic) ? initialTopic : (topics[0] ?? initialTopic)),
  );
  const [destId, setDestId] = useState(profile.id);
  const [destGroup, setDestGroup] = useState(detail.group_id);
  const [destTopic, setDestTopic] = useState(topic);

  const [plan, setPlan] = useState<OffsetMigrationRow[] | null>(null);
  const [planning, setPlanning] = useState(false);
  const [check, setCheck] = useState<DestGroup>({ state: "idle" });
  const [applied, setApplied] = useState<GroupOffset[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [typed, setTyped] = useState("");

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
  const destDef = useEnvironment(destEnv);
  const destIsProtected = destDef.protected;
  const destReadOnly = dest?.read_only ?? false;

  // Any change to either end invalidates a plan and the group's state: both
  // are answers about a pair that no longer exists.
  useEffect(() => {
    setPlan(null);
    setApplied(null);
    setCheck({ state: "idle" });
  }, [destId, destGroup, destTopic, topic]);

  /**
   * What the destination group is doing right now.
   *
   * A group that does not exist at all is NOT an error and NOT the same as a
   * live one: it has nothing to overwrite, and the migration creates its first
   * offsets. Kafka answers "no such group" and "I can't describe that" the same
   * way, so the sentence says both.
   */
  const checkDestGroup = useCallback(async () => {
    setCheck({ state: "checking" });
    const read = async () => await groupDetail(destId, destGroup);
    let got: GroupDetail | null = null;
    try {
      got = await read();
    } catch {
      try {
        await clusterConnect(destId);
        got = await read();
      } catch {
        // Either the group isn't there or it can't be described. Both leave
        // nothing to stop, and the apply is what finds out for certain.
        setCheck({ state: "absent" });
        return;
      }
    }
    const committed = got.offsets.filter((o) => o.committed !== null).length;
    if (got.state.toLowerCase() === "empty") {
      setCheck({ state: "empty", committed });
      return;
    }
    setCheck({
      state: "live",
      kafkaState: got.state,
      members: got.members.length,
    });
  }, [destId, destGroup]);

  const buildPlan = useCallback(async () => {
    if (destGroup.trim().length === 0 || destTopic.trim().length === 0) return;
    setPlanning(true);
    setFailure(null);
    setApplied(null);
    try {
      const rows = await offsetsMigratePlan(
        profile.id,
        detail.group_id,
        topic,
        destId,
        destGroup.trim(),
        destTopic.trim(),
      );
      setPlan(rows);
      // The Empty requirement is stated before the button, so it is read at
      // the same time as the plan rather than after a refusal.
      void checkDestGroup();
    } catch (err) {
      setPlan(null);
      setFailure(errorMessage(err));
    } finally {
      setPlanning(false);
    }
  }, [
    profile.id,
    detail.group_id,
    topic,
    destId,
    destGroup,
    destTopic,
    checkDestGroup,
  ]);

  /** Only the rows that have somewhere to go. `none` rows write nothing. */
  const actionable = useMemo(
    () => (plan ?? []).filter((r) => r.dest_offset !== null && r.method !== "none"),
    [plan],
  );
  const byTimestamp = actionable.filter((r) => r.method === "timestamp").length;
  const byEarliest = actionable.filter((r) => r.method === "earliest").length;
  const byLatest = actionable.filter((r) => r.method === "latest").length;
  const nothing = (plan?.length ?? 0) - actionable.length;
  /**
   * The summary's arithmetic, as clauses rather than as a nested ternary.
   *
   * EVERY ACTIONABLE ROW IS IN EXACTLY ONE CLAUSE, and the total of the clauses
   * is `actionable.length` — including rows whose method the core learns after
   * this build, which land in the last one rather than vanishing out of a
   * sentence that opens by counting them. A summary that says "4 of 6
   * partitions" and then accounts for three is worse than no summary.
   */
  const methodClauses = useMemo(() => {
    const other = actionable.length - byTimestamp - byEarliest - byLatest;
    const clauses: string[] = [];
    if (byTimestamp > 0) clauses.push(`${byTimestamp} matched by timestamp`);
    if (byLatest > 0)
      clauses.push(
        `${byLatest} already caught up, starting at the end of the destination`,
      );
    if (byEarliest > 0)
      clauses.push(
        `${byEarliest} falling back to the beginning of the partition, which means re-processing everything there`,
      );
    if (other > 0) clauses.push(`${other} by another method the core reported`);
    return clauses;
  }, [actionable.length, byTimestamp, byEarliest, byLatest]);

  const needsTyping =
    destIsProtected || (check.state === "empty" && check.committed > 0);
  const typedOk = !needsTyping || typed === destGroup.trim();
  const blockedByLiveGroup = check.state === "live";

  const apply = useCallback(async () => {
    if (actionable.length === 0) return;
    setBusy(true);
    setFailure(null);
    try {
      const after = await offsetsMigrateApply(
        destId,
        destGroup.trim(),
        destTopic.trim(),
        actionable,
      );
      setApplied(after);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [actionable, destId, destGroup, destTopic]);

  const classified =
    failure === null
      ? null
      : classifyError(failure, {
          environmentProtected: destIsProtected,
          groupState: check.state === "live" ? check.kafkaState : undefined,
          memberCount: check.state === "live" ? check.members : undefined,
        });

  const applyWhy = destReadOnly
    ? READ_ONLY_WHY
    : blockedByLiveGroup
      ? `${destGroup} is ${check.kafkaState} on ${dest?.name ?? "the destination"} — stop it first, or Kafka will refuse this`
      : actionable.length === 0
        ? "There is nothing to migrate — every partition came back with nothing to move"
        : !typedOk
          ? `Type ${destGroup} exactly to confirm this`
          : undefined;

  return (
    <Overlay
      surfaceClass={`modal modal-wide${destIsProtected ? " modal-destructive" : ""}`}
      labelledBy="om-title"
      initialFocus={cancelRef}
      onClose={onClose}
    >
      {/* The DESTINATION's environment colours the rule inside this modal —
          the same guardrail the copy wizard wears, for the same reason: the
          workspace behind it is showing the safe half. */}
      <div
        className="modal-panel"
        {...envAttrs(destDef)}
        data-env-label={envWireLabel(destDef)}
      >
        <h2 className="modal-title" id="om-title">
          Migrate {detail.group_id}'s offsets
        </h2>

        <p className="dialog-note">
          An offset only means something on the cluster it came from. Kavka
          works out where <code>{detail.group_id}</code> had got to here, then
          asks the destination which of ITS offsets matches that moment in time
          — and says, per partition, how it got the answer.
        </p>

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
                Committing offsets for a group there decides what a production
                application reads — or re-reads — the moment it starts.
              </p>
            </div>
          </div>
        )}

        {destReadOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        <div className="reset-row">
          <label className="seekbar-field">
            <span className="seekbar-label">From topic</span>
            <select value={topic} onChange={(e) => setTopic(e.target.value)}>
              {topics.length === 0 && <option value={topic}>{topic}</option>}
              {topics.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
          <label className="seekbar-field">
            <span className="seekbar-label">To connection</span>
            <select
              value={destId}
              onChange={(e) => setDestId(e.target.value)}
            >
              {(profiles ?? [profile]).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} — {p.environment}
                  {p.read_only ? " · read-only" : ""}
                </option>
              ))}
            </select>
          </label>
          {dest !== null && <EnvChip env={dest.environment} />}
        </div>

        <div className="reset-row">
          <label className="seekbar-field seekbar-field-wide">
            <span className="seekbar-label">
              Destination <Term name="consumer-group">group</Term>
            </span>
            <input
              type="text"
              className="input-mono"
              value={destGroup}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setDestGroup(e.target.value)}
            />
          </label>
          <label className="seekbar-field seekbar-field-wide">
            <span className="seekbar-label">Destination topic</span>
            <input
              type="text"
              className="input-mono"
              value={destTopic}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setDestTopic(e.target.value)}
            />
          </label>
          <div className="seekbar-field seekbar-actions">
            <span className="seekbar-label" aria-hidden="true">
              &nbsp;
            </span>
            <button
              type="button"
              className="btn btn-swap"
              disabled={
                planning ||
                destGroup.trim().length === 0 ||
                destTopic.trim().length === 0
              }
              aria-busy={planning || undefined}
              title={
                destGroup.trim().length === 0 || destTopic.trim().length === 0
                  ? "Name the group and topic on the destination"
                  : "Work out where each partition would land. Nothing is written."
              }
              onClick={() => void buildPlan()}
            >
              <span className="btn-swap-face">
                Plan
              </span>
              <span className="btn-swap-face btn-swap-busy">
                <span className="spinner" aria-hidden="true" />
                Plan
              </span>
            </button>
          </div>
        </div>

        {/* THE EMPTY REQUIREMENT, before the button. */}
        {plan !== null && <DestGroupNote check={check} group={destGroup} name={dest?.name ?? ""} />}

        {plan !== null && (
          <>
            <p className="reset-preview">
              {actionable.length === 0
                ? `Nothing to migrate. ${detail.group_id} has no committed offset on ${topic} that Kavka can place on the destination.`
                : `${actionable.length} of ${plan.length} partition${
                    plan.length === 1 ? "" : "s"
                  } would be committed for ${destGroup} on ${
                    dest?.name ?? "the destination"
                  }${
                    methodClauses.length === 1
                      ? `, ${
                          byTimestamp === actionable.length
                            ? "all matched by timestamp"
                            : methodClauses[0]
                        }.`
                      : ` — ${methodClauses.slice(0, -1).join(", ")} and ${
                          methodClauses[methodClauses.length - 1]
                        }.`
                  }`}
            </p>

            <div className="table-wrap">
              <table className="data-table">
                <caption className="sr-only">
                  Where each partition's offset would land on the destination
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="ledger-gutter">
                      Part.
                    </th>
                    <th scope="col" className="col-num">
                      Committed here
                    </th>
                    <th scope="col">At</th>
                    <th scope="col" className="col-num">
                      Destination offset
                    </th>
                    <th scope="col">How</th>
                  </tr>
                </thead>
                <tbody>
                  {plan.length === 0 ? (
                    <tr>
                      <td colSpan={5} className="cell-empty">
                        The plan came back empty. This group has no committed
                        offsets on <code>{topic}</code>.
                      </td>
                    </tr>
                  ) : (
                    plan.map((row) => (
                      <PlanRow key={row.partition} row={row} />
                    ))
                  )}
                </tbody>
              </table>
            </div>

            {nothing > 0 && (
              <p className="dialog-note">
                {nothing} partition{nothing === 1 ? "" : "s"} had nothing to
                migrate and {nothing === 1 ? "is" : "are"} left alone — Kavka
                sends only the rows with somewhere to go, so the destination
                group is not given a position it never had.
              </p>
            )}
          </>
        )}

        {needsTyping && plan !== null && actionable.length > 0 && applied === null && (
          <div className="field confirm-type">
            <label className="field-label" htmlFor="om-type">
              Type <code>{destGroup}</code> to confirm
            </label>
            <input
              id="om-type"
              type="text"
              className="input-mono"
              value={typed}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setTyped(e.target.value)}
            />
            <span className="field-hint">
              {destIsProtected
                ? `${dest?.name} is a production cluster, so Kavka asks every time.`
                : `${destGroup} already has committed offsets on the destination, so this overwrites a position something else chose.`}
            </span>
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

        {applied !== null && (
          <>
            <div className="banner banner-info" role="status">
              <span className="banner-glyph" aria-hidden="true">
                ✓
              </span>
              <div className="banner-body">
                <p className="banner-title">
                  {destGroup} now starts from these offsets on{" "}
                  {dest?.name ?? "the destination"}.
                </p>
                <p className="banner-detail">
                  Nothing on this cluster changed — {detail.group_id} is exactly
                  where it was.
                </p>
              </div>
            </div>
            <div className="table-wrap">
              <table className="data-table">
                <caption className="sr-only">
                  The destination group's offsets after the migration
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="ledger-gutter">
                      Part.
                    </th>
                    <th scope="col">Topic</th>
                    <th scope="col" className="col-num">
                      Committed
                    </th>
                    <th scope="col" className="col-num">
                      End
                    </th>
                    <th scope="col" className="col-num">
                      <Term name="lag">Lag</Term>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {applied.map((o) => (
                    <tr key={`${o.topic}:${o.partition}`}>
                      <td className="ledger-gutter">{o.partition}</td>
                      <td className="cell-mono">{o.topic}</td>
                      <td className="col-num cell-mono cell-mono-num">
                        {o.committed === null ? (
                          <span className="absent">∅</span>
                        ) : (
                          groupDigits(o.committed)
                        )}
                      </td>
                      <td className="col-num cell-mono cell-mono-num">
                        {groupDigits(o.end_offset)}
                      </td>
                      <td className="col-num cell-num">
                        {o.lag === null ? (
                          <span className="absent">∅</span>
                        ) : (
                          groupDigits(o.lag)
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            ref={cancelRef}
            onClick={onClose}
            disabled={busy}
            title={busy ? "Kavka is committing the offsets" : undefined}
          >
            {applied === null ? "Cancel" : "Done"}
          </button>
          {applied === null && (
            <button
              type="button"
              className={`btn ${destIsProtected ? "btn-danger-confirm" : "btn-primary"} btn-swap`}
              disabled={
                busy ||
                destReadOnly ||
                plan === null ||
                actionable.length === 0 ||
                !typedOk ||
                blockedByLiveGroup
              }
              aria-busy={busy || undefined}
              title={applyWhy}
              onClick={() => void apply()}
            >
              <span className="btn-swap-face">
                Commit offsets
              </span>
              <span className="btn-swap-face btn-swap-busy">
                <span className="spinner" aria-hidden="true" />
                Commit offsets
              </span>
            </button>
          )}
        </div>
      </div>
    </Overlay>
  );
}

/** The Empty-group requirement, in the state it is actually in. */
function DestGroupNote({
  check,
  group,
  name,
}: {
  check: DestGroup;
  group: string;
  name: string;
}) {
  if (check.state === "checking")
    return <p className="dialog-note">Asking {name} about {group}…</p>;
  if (check.state === "absent")
    return (
      <p className="dialog-note">
        Kavka found no group called <code>{group}</code> on {name}. That is the
        easy case: there is nothing to overwrite and nothing to stop, and the
        migration creates its first committed offsets.
      </p>
    );
  if (check.state === "empty")
    return (
      <p className="dialog-note">
        <code>{group}</code> is Empty on {name}
        {check.committed > 0
          ? ` and already has ${check.committed} committed offset${
              check.committed === 1 ? "" : "s"
            }, which this replaces.`
          : " — no members connected, nothing committed yet."}{" "}
        Empty is the only state Kafka accepts a commit in.
      </p>
    );
  if (check.state === "live")
    return (
      <div className="banner banner-warn" role="note">
        <span className="banner-glyph" aria-hidden="true">
          !
        </span>
        <div className="banner-body">
          <p className="banner-title">
            <code>{group}</code> is running on {name} — its state is{" "}
            {check.kafkaState}.
          </p>
          <p className="banner-detail">
            Committing offsets for a group with{" "}
            {check.members === 1
              ? "a live member"
              : `${check.members} live members`}{" "}
            is rejected by Kafka, and Kavka refuses it here for the same reason
            offset reset does. Stop the application, wait for the group to report
            Empty, then plan again.
          </p>
        </div>
      </div>
    );
  return null;
}

/** One partition of the plan. The method is a sentence, never a code. */
function PlanRow({ row }: { row: OffsetMigrationRow }) {
  const word =
    row.method === "timestamp"
      ? "matched by timestamp"
      : row.method === "earliest"
        ? "fell back to earliest"
        : row.method === "latest"
          ? "already caught up"
          : row.method === "none"
            ? "nothing to migrate"
            : row.method;
  const why =
    row.method === "timestamp"
      ? "The committed record here carried a timestamp, and the destination's broker answered with the offset that moment maps to in its own log."
      : row.method === "earliest"
        ? "No usable timestamp, so the destination starts at the beginning of its partition — the application re-processes everything currently retained there."
        : row.method === "latest"
          ? "The group was already caught up — it starts at the end of the destination partition and waits for new records, so nothing is re-processed."
          : row.method === "none"
            ? "This group has never committed an offset for this partition, so there is nothing to place on the destination and nothing is written."
            : `The core reported the method as “${row.method}”.`;
  // `latest` reads as ok, not warn: being caught up is the good outcome, and
  // the destination's end is exactly where a caught-up group belongs.
  const tone =
    row.method === "timestamp" || row.method === "latest"
      ? "ok"
      : row.method === "earliest"
        ? "warn"
        : "quiet";
  return (
    <tr>
      <td className="ledger-gutter">{row.partition}</td>
      <td className="col-num cell-mono cell-mono-num">
        {row.source_committed === null ? (
          <span className="absent" title="Never committed here.">
            ∅
          </span>
        ) : (
          groupDigits(row.source_committed)
        )}
      </td>
      <td className="cell-mono">
        {row.source_ts_ms === null ? (
          <span
            className="absent"
            title="The record at that offset carried no timestamp, which is why the method below is what it is."
          >
            ∅
          </span>
        ) : (
          formatStamp(row.source_ts_ms)
        )}
      </td>
      <td className="col-num cell-mono cell-mono-num">
        {row.dest_offset === null ? (
          <span className="absent" title="Nothing will be written for this partition.">
            ∅
          </span>
        ) : (
          groupDigits(row.dest_offset)
        )}
      </td>
      <td>
        {/* Law 2: dot plus word plus a sentence on hover and focus. */}
        <span className={`health health-${tone}`} title={why} tabIndex={0}>
          <i className="dot" aria-hidden="true" />
          {word}
        </span>
      </td>
    </tr>
  );
}
