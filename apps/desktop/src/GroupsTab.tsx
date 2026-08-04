import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  groupDetail,
  groupsList,
  type ConnectionProfile,
  type GroupDetail,
  type GroupInfo,
  type GroupOffset,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { groupDigits } from "./format";
import { Term } from "./Glossary";
import OffsetMigrateModal from "./OffsetMigrateModal";
import { ErrorBanner } from "./ProfileEditor";
import ResetOffsetsModal from "./ResetOffsetsModal";
import ShareGroupsPanel from "./ShareGroupsPanel";

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/**
 * Every state gets a word AND a sentence. A state chip whose only content is
 * `PreparingRebalance` tells a novice nothing, and colouring it amber tells
 * them even less — law 2 is about meaning, not only about contrast.
 */
const STATE_GLOSS: Record<string, string> = {
  empty:
    "No members are connected right now. Kafka still keeps the committed offsets, so an application can pick up where it left off.",
  stable:
    "Members are connected and every partition is assigned to one of them. This is the normal running state.",
  preparingrebalance:
    "A member joined or left, so Kafka is about to redistribute the partitions. Consumption pauses while it does.",
  completingrebalance:
    "Kafka has worked out the new assignment and is handing it to the members. Consumption resumes as soon as they take it.",
  dead: "This group has no members and nothing committed. Kafka has effectively forgotten it.",
  unknown: "Kafka didn't report a state for this group.",
};

function stateGloss(state: string): string {
  return (
    STATE_GLOSS[state.toLowerCase().replace(/[^a-z]/g, "")] ??
    `Kafka reports this group as ${state}.`
  );
}

function stateHealth(state: string): "ok" | "warn" | "danger" | "quiet" {
  switch (state.toLowerCase().replace(/[^a-z]/g, "")) {
    case "stable":
      return "ok";
    case "preparingrebalance":
    case "completingrebalance":
      return "warn";
    case "dead":
      return "danger";
    default:
      return "quiet";
  }
}

/**
 * Lag's third channel. DESIGN asks for "a bar length AND a number AND a trend
 * arrow"; Kavka has no lag history until Phase 4, so an arrow here would be
 * invented. A word is the honest third channel, and it survives greyscale.
 */
function lagWord(lag: number): { word: string; tone: "ok" | "warn" | "danger" } {
  if (lag <= 0) return { word: "caught up", tone: "ok" };
  if (lag <= 1_000) return { word: "keeping up", tone: "ok" };
  if (lag <= 100_000) return { word: "behind", tone: "warn" };
  return { word: "far behind", tone: "danger" };
}

interface GroupsTabProps {
  profile: ConnectionProfile;
  group: string | null;
  onSelectGroup: (group: string | null) => void;
  onDanger: DangerReport;
}

export default function GroupsTab({
  profile,
  group,
  onSelectGroup,
  onDanger,
}: GroupsTabProps) {
  const [groups, setGroups] = useState<GroupInfo[] | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const [listFailed, setListFailed] = useState(false);

  const [detail, setDetail] = useState<GroupDetail | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [resetting, setResetting] = useState(false);
  const [resetDone, setResetDone] = useState<string | null>(null);
  /**
   * Phase 5a: moving this group's position onto ANOTHER cluster.
   *
   * A sibling of reset rather than a mode inside it — reset moves a group
   * within one cluster, where an offset is a number that means something;
   * migration crosses two clusters, where it is not, and the whole modal is
   * about that difference.
   */
  const [migrating, setMigrating] = useState(false);

  const listSeq = useRef(0);
  const detailSeq = useRef(0);

  useDangerSignal(error !== null, onDanger);

  const readOnly = profile.read_only;

  const fetchGroups = useCallback(async () => {
    const seq = ++listSeq.current;
    setLoadingList(true);
    try {
      const list = await groupsList(profile.id);
      if (listSeq.current !== seq) return;
      setGroups(list);
      setListFailed(false);
    } catch (err) {
      if (listSeq.current !== seq) return;
      setListFailed(true);
      setError(errorMessage(err));
    } finally {
      if (listSeq.current === seq) setLoadingList(false);
    }
  }, [profile.id]);

  useEffect(() => {
    void fetchGroups();
    return () => {
      listSeq.current += 1;
    };
  }, [fetchGroups]);

  const fetchDetail = useCallback(
    async (groupId: string) => {
      const seq = ++detailSeq.current;
      setLoadingDetail(true);
      try {
        const next = await groupDetail(profile.id, groupId);
        if (detailSeq.current === seq) setDetail(next);
      } catch (err) {
        if (detailSeq.current === seq) setError(errorMessage(err));
      } finally {
        if (detailSeq.current === seq) setLoadingDetail(false);
      }
    },
    [profile.id],
  );

  useEffect(() => {
    if (group === null) {
      setDetail(null);
      setResetDone(null);
      return;
    }
    setDetail(null);
    void fetchDetail(group);
    return () => {
      detailSeq.current += 1;
    };
  }, [group, fetchDetail]);

  /** The post-reset offsets the command hands back, merged in place. */
  const applyReset = useCallback((after: GroupOffset[]) => {
    setResetting(false);
    setDetail((prev) => {
      if (prev === null) return prev;
      const byKey = new Map(
        after.map((o) => [`${o.topic}:${o.partition}`, o] as const),
      );
      const merged = prev.offsets.map(
        (o) => byKey.get(`${o.topic}:${o.partition}`) ?? o,
      );
      // Anything the reset created that the group had never committed before.
      for (const [key, value] of byKey) {
        if (!merged.some((o) => `${o.topic}:${o.partition}` === key))
          merged.push(value);
      }
      return { ...prev, offsets: merged };
    });
    setResetDone(
      `Offsets reset. The table below is where ${
        after.length === 1 ? "the partition" : `all ${after.length} partitions`
      } now stand.`,
    );
  }, []);

  const banner =
    error === null ? null : (
      <ErrorBanner raw={error} onDismiss={() => setError(null)} />
    );

  // ── Group detail ───────────────────────────────────────────────────────

  if (group !== null) {
    const offsets = detail?.offsets ?? [];
    const totalLag = offsets.reduce((sum, o) => sum + (o.lag ?? 0), 0);
    const maxLag = offsets.reduce((max, o) => Math.max(max, o.lag ?? 0), 0);
    const topicsRead = new Set(offsets.map((o) => o.topic));

    return (
      <>
        {banner}

        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              <button
                type="button"
                className="btn btn-ghost crumb-btn"
                onClick={() => onSelectGroup(null)}
              >
                ← Groups
              </button>
              <span className="topic-name">{group}</span>
              {detail !== null && <StateChip state={detail.state} />}
            </h2>
            <div className="panel-tools">
              {/* Neutral here, not danger-outlined: the write lands on ANOTHER
                  connection, and the modal wears that connection's
                  environment — including its read-only flag, which this one's
                  cannot speak for. */}
              <button
                type="button"
                className="btn"
                disabled={detail === null || detail.offsets.length === 0}
                title={
                  detail === null
                    ? "Kavka is still reading this group"
                    : detail.offsets.length === 0
                      ? "This group has never committed an offset, so there is no position to migrate."
                      : "Give another cluster's group the same position, matched by time"
                }
                onClick={() => setMigrating(true)}
              >
                Migrate offsets…
              </button>
              <button
                type="button"
                className="btn btn-danger"
                disabled={readOnly || detail === null}
                title={
                  readOnly
                    ? READ_ONLY_WHY
                    : detail === null
                      ? "Kavka is still reading this group"
                      : "Move where this group reads from"
                }
                onClick={() => setResetting(true)}
              >
                Reset offsets
              </button>
            </div>
          </div>

          {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

          {detail === null ? (
            <p className="table-note">
              {loadingDetail
                ? "Asking the cluster about this group…"
                : "Kavka couldn't read this group. It may have been removed, or the account may not have Describe on it."}
            </p>
          ) : (
            <>
              {resetDone !== null && (
                <div className="banner banner-info" role="status">
                  <span className="banner-glyph" aria-hidden="true">
                    ✓
                  </span>
                  <div className="banner-body">
                    <p className="banner-title">{resetDone}</p>
                  </div>
                  <div className="banner-actions">
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => setResetDone(null)}
                    >
                      Dismiss
                    </button>
                  </div>
                </div>
              )}

              <div className="stat-grid">
                <div className="stat">
                  <span className="stat-label">Members</span>
                  <span className="stat-value">{detail.members.length}</span>
                </div>
                <div className="stat">
                  <span className="stat-label">Topics read</span>
                  <span className="stat-value">{topicsRead.size}</span>
                </div>
                <div className="stat">
                  <span className="stat-label">Total lag</span>
                  <span className="stat-value">{groupDigits(totalLag)}</span>
                </div>
              </div>
            </>
          )}
        </section>

        {detail !== null && (
          <section className="panel">
            <div className="panel-head">
              <h2 className="panel-title">
                Members
                <span className="panel-count">{detail.members.length}</span>
              </h2>
            </div>

            {detail.members.length === 0 ? (
              <p className="table-note">
                Nothing is connected to this group right now. Its committed
                offsets are still here, so an application that starts up will
                carry on from them.
              </p>
            ) : (
              <div className="table-wrap">
                <table className="data-table data-table-flush">
                  <caption className="sr-only">
                    Members of {detail.group_id}
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">Client</th>
                      <th scope="col">Host</th>
                      <th scope="col">Member id</th>
                      <th scope="col" className="col-num">
                        <Term name="partition">Partitions</Term>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {detail.members.map((m) => (
                      <tr key={m.member_id}>
                        <td className="cell-mono">{m.client_id}</td>
                        <td className="cell-mono">{m.client_host}</td>
                        <td className="cell-mono member-id" title={m.member_id}>
                          {m.member_id}
                        </td>
                        <td
                          className="col-num cell-num"
                          title={
                            m.assignments.length === 0
                              ? "This member holds no partitions right now."
                              : m.assignments
                                  .map((a) => `${a.topic} ${a.partition}`)
                                  .join(", ")
                          }
                        >
                          {m.assignments.length}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        )}

        {detail !== null && (
          <section className="panel">
            <div className="panel-head">
              <h2 className="panel-title">
                <Term name="lag">Lag</Term>
                <span className="panel-count">
                  {offsets.length} partition{offsets.length === 1 ? "" : "s"}
                </span>
              </h2>
            </div>

            <div className="table-wrap">
              <table className="data-table">
                <caption className="sr-only">
                  Committed offsets and lag for {detail.group_id}
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
                    <th scope="col" className="col-lag">
                      <Term name="lag">Lag</Term>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {offsets.length === 0 ? (
                    <tr>
                      <td colSpan={5} className="cell-empty">
                        This group has never committed an offset. It may only
                        ever have produced, or it may have been created and
                        never read anything.
                      </td>
                    </tr>
                  ) : (
                    offsets.map((o) => (
                      <LagRow
                        key={`${o.topic}:${o.partition}`}
                        offset={o}
                        maxLag={maxLag}
                      />
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </section>
        )}

        {resetting && detail !== null && (
          <ResetOffsetsModal
            profile={profile}
            detail={detail}
            initialTopic={detail.offsets[0]?.topic ?? ""}
            onDone={applyReset}
            onClose={() => setResetting(false)}
          />
        )}

        {migrating && detail !== null && (
          <OffsetMigrateModal
            profile={profile}
            detail={detail}
            initialTopic={detail.offsets[0]?.topic ?? ""}
            onClose={() => setMigrating(false)}
          />
        )}
      </>
    );
  }

  // ── The list ───────────────────────────────────────────────────────────

  return (
    <>
      {banner}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <Term name="consumer-group">Consumer groups</Term>
            {groups !== null && (
              <span className="panel-count">{groups.length}</span>
            )}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={loadingList}
              aria-busy={loadingList || undefined}
              title={
                loadingList
                  ? "Kavka is already asking the cluster for groups"
                  : undefined
              }
              onClick={() => void fetchGroups()}
            >
              Refresh
            </button>
          </div>
        </div>

        {groups === null && loadingList ? (
          <>
            <div className="table-note">
              Asking the cluster for consumer groups…
            </div>
            <div className="skeleton-table" aria-hidden="true">
              {[38, 46, 29, 52].map((w, i) => (
                <div className="skeleton-row" key={i}>
                  <div className="skeleton-cell" style={{ width: `${w}%` }} />
                  <div className="skeleton-cell" style={{ width: "48px" }} />
                </div>
              ))}
            </div>
          </>
        ) : groups === null || listFailed ? (
          <div className="table-note">
            Kavka couldn't list this cluster's consumer groups. If the
            connection is up, the account may not have <code>Describe</code> on
            the cluster.
          </div>
        ) : groups.length === 0 ? (
          <div className="table-note">
            No consumer groups yet. Groups appear here as soon as an application
            starts reading from this cluster. A group that has only ever
            produced won't show up.
          </div>
        ) : (
          <div className="table-wrap">
            {loadingList && (
              <div className="table-loading" role="presentation" />
            )}
            <table className="data-table data-table-flush">
              <caption className="sr-only">
                Consumer groups on this cluster
              </caption>
              <thead>
                <tr>
                  <th scope="col">Group</th>
                  <th scope="col">State</th>
                  <th scope="col">Protocol</th>
                  <th scope="col" className="col-num">
                    Members
                  </th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Open</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {groups.map((g) => (
                  <tr
                    key={g.group_id}
                    className="row-click"
                    onClick={() => onSelectGroup(g.group_id)}
                  >
                    <td className="cell-mono">{g.group_id}</td>
                    <td>
                      <StateChip state={g.state} />
                    </td>
                    <td className="cell-mono">
                      {g.protocol_type.length === 0 ? (
                        <span className="absent">∅</span>
                      ) : (
                        g.protocol_type
                      )}
                    </td>
                    <td className="col-num cell-num">{g.member_count}</td>
                    {/* The row's keyboard path, and the only element here
                        with a role that says it opens something (SC 4.1.2). */}
                    <td className="col-affordance">
                      <button
                        type="button"
                        className="row-affordance"
                        aria-label={`View lag for ${g.group_id}`}
                        // The row is still clickable for the pointer;
                        // without this the handler runs twice per click.
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectGroup(g.group_id);
                        }}
                      >
                        View lag →
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {/* Share groups sit UNDER the consumer groups and not beside them, because
          they are a different model rather than a different flavour: one hands
          out partitions, the other hands out records. Most clusters can't answer
          this call at all, and the panel says why in one sentence instead of
          surfacing a protocol version number. */}
      <ShareGroupsPanel profile={profile} onDanger={onDanger} />
    </>
  );
}

/** Dot plus word plus a sentence on hover and focus. Never colour alone. */
function StateChip({ state }: { state: string }) {
  const tone = stateHealth(state);
  return (
    <span
      className={`health health-${tone === "quiet" ? "quiet" : tone}`}
      title={stateGloss(state)}
      tabIndex={0}
    >
      <i className="dot" aria-hidden="true" />
      {state}
    </span>
  );
}

/**
 * Lag: the number, then a 40×3 track filled proportionally, then the word.
 * Three channels, and the fill clears 3:1 against the track. `--accent` never
 * appears in an adjacent column — the status-adjacency law (§2).
 */
function LagRow({
  offset,
  maxLag,
}: {
  offset: GroupOffset;
  maxLag: number;
}) {
  const lag = offset.lag;
  const { word, tone } = lagWord(lag ?? 0);
  const pct = maxLag > 0 && lag !== null ? Math.min(100, (lag / maxLag) * 100) : 0;
  return (
    <tr>
      <td className="ledger-gutter">{offset.partition}</td>
      <td className="cell-mono">{offset.topic}</td>
      <td className="col-num cell-mono cell-mono-num">
        {offset.committed === null ? (
          <span
            className="absent"
            title="This group has never committed an offset for this partition."
          >
            ∅
          </span>
        ) : (
          groupDigits(offset.committed)
        )}
      </td>
      <td className="col-num cell-mono cell-mono-num">
        {groupDigits(offset.end_offset)}
      </td>
      <td className="col-lag">
        {lag === null ? (
          <span className="absent" title="Nothing committed, so there is no lag to compute.">
            ∅
          </span>
        ) : (
          <span className={`lag lag-${tone}`}>
            <span className="lag-number">{groupDigits(lag)}</span>
            <span className="lag-track" aria-hidden="true">
              <span className="lag-fill" style={{ width: `${pct}%` }} />
            </span>
            <span className="lag-word">{word}</span>
          </span>
        )}
      </td>
    </tr>
  );
}
