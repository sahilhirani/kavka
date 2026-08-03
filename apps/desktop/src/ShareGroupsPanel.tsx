import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  shareGroupDetail,
  shareGroupsList,
  type ConnectionProfile,
  type ShareGroupDetail,
  type ShareGroupInfo,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { groupDigits } from "./format";
import { ErrorBanner } from "./ProfileEditor";

/**
 * SHARE GROUPS (KIP-932) — Kafka's queue-shaped consumer, and the one view in
 * Kavka most people will open on a cluster that cannot answer it.
 *
 * A share group is not a consumer group with a different name. A consumer group
 * hands each partition to exactly one member and tracks one committed offset per
 * partition; a share group hands out individual RECORDS, several members can
 * read the same partition at once, and each record is acknowledged on its own.
 * That is why this lives in its own section with its own vocabulary rather than
 * as a column on the groups table — calling a share group's start offset a
 * "committed offset" would teach the wrong model of what it does.
 *
 * THE UNSUPPORTED CASE IS THE COMMON CASE, and it is the reason this component
 * exists rather than a list. Share groups landed in Kafka 4.0 and are off by
 * default even there, so most clusters refuse the call with a protocol version
 * number. That is true and useless. §7's rule applies: the broker told us the
 * answer, so the answer goes in the message — this cluster's brokers don't have
 * the feature turned on, which is a broker setting and not a permission the user
 * is missing.
 */

/**
 * A cluster without share groups refuses the request rather than answering with
 * an empty list, and its word for that is an API version. Same shape as
 * QuorumPanel's ZooKeeper note, and for the same reason.
 */
function unsupportedNote(raw: string): string | null {
  const text = raw.toLowerCase();
  return text.includes("unsupported_version") ||
    text.includes("unsupported version") ||
    text.includes("unknown api") ||
    text.includes("unknown_topic_or_partition") ||
    text.includes("share group") ||
    text.includes("sharegroup") ||
    text.includes("group.coordinator.rebalance.protocols")
    ? "This cluster doesn't offer share groups."
    : null;
}

interface ShareGroupsPanelProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
}

export default function ShareGroupsPanel({
  profile,
  onDanger,
}: ShareGroupsPanelProps) {
  const [groups, setGroups] = useState<ShareGroupInfo[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<ShareGroupDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const listSeq = useRef(0);
  const detailSeq = useRef(0);

  const note = failure === null ? null : unsupportedNote(failure);
  // A cluster that simply doesn't have the feature is a fact about the cluster,
  // not a danger — only an unexplained failure dampens the prod rule.
  useDangerSignal(
    (failure !== null && note === null) || detailError !== null,
    onDanger,
  );

  const fetchList = useCallback(() => {
    const mine = ++listSeq.current;
    setLoading(true);
    shareGroupsList(profile.id)
      .then((list) => {
        if (listSeq.current !== mine) return;
        setGroups(list);
        setFailure(null);
      })
      .catch((err: unknown) => {
        if (listSeq.current !== mine) return;
        setGroups(null);
        setFailure(errorMessage(err));
      })
      .finally(() => {
        if (listSeq.current === mine) setLoading(false);
      });
  }, [profile.id]);

  useEffect(() => {
    fetchList();
    return () => {
      listSeq.current += 1;
    };
  }, [fetchList]);

  useEffect(() => {
    if (selected === null) {
      setDetail(null);
      setDetailError(null);
      return;
    }
    const mine = ++detailSeq.current;
    setDetail(null);
    setDetailError(null);
    setLoadingDetail(true);
    shareGroupDetail(profile.id, selected)
      .then((next) => {
        if (detailSeq.current === mine) setDetail(next);
      })
      .catch((err: unknown) => {
        if (detailSeq.current === mine) setDetailError(errorMessage(err));
      })
      .finally(() => {
        if (detailSeq.current === mine) setLoadingDetail(false);
      });
    return () => {
      detailSeq.current += 1;
    };
  }, [profile.id, selected]);

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">
          Share groups
          {groups !== null && (
            <span className="panel-count">{groups.length}</span>
          )}
        </h2>
        <div className="panel-tools">
          <button
            type="button"
            className="btn"
            disabled={loading}
            aria-busy={loading || undefined}
            title={
              loading
                ? "Kavka is already asking the cluster for share groups"
                : "Ask the cluster for share groups again"
            }
            onClick={fetchList}
          >
            Refresh
          </button>
        </div>
      </div>

      <p className="table-note">
        A share group reads a topic like a queue. Where a consumer group gives
        each partition to exactly one member and remembers one offset per
        partition, a share group hands out <strong>individual records</strong> —
        several members can read the same partition at once, and each record is
        acknowledged on its own. It is what you want when the work per message is
        uneven and ordering doesn't matter.
      </p>

      {failure !== null && note !== null ? (
        <>
          <p className="table-note">
            {note} Share groups arrived in Kafka 4.0 as <code>KIP-932</code> and
            are off even there until a broker lists <code>share</code> in{" "}
            <code>group.coordinator.rebalance.protocols</code>. So this is a
            broker setting, not a permission you're missing, and nothing else in
            Kavka is affected by it.
          </p>
          {/* Three layers, as everywhere else: the plain answer, what to do,
              then the broker's own words for whoever wants them. */}
          <details className="banner-details">
            <summary>Show details</summary>
            <pre className="banner-raw">{failure}</pre>
          </details>
        </>
      ) : failure !== null ? (
        <ErrorBanner raw={failure} onDismiss={() => setFailure(null)} />
      ) : groups === null ? (
        <p className="table-note">Asking the cluster for share groups…</p>
      ) : groups.length === 0 ? (
        <p className="table-note">
          This cluster supports share groups and has none right now. One appears
          here as soon as an application starts reading with a share consumer —
          a group that has only ever produced won't show up.
        </p>
      ) : (
        <div className="table-wrap">
          {loading && <div className="table-loading" role="presentation" />}
          <table className="data-table data-table-flush">
            <caption className="sr-only">
              Share groups on this cluster
            </caption>
            <thead>
              <tr>
                <th scope="col">Group</th>
                <th scope="col">State</th>
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
                  // `row-selected`, not `aria-selected`: this is a static
                  // table, and §5.2 keeps grid semantics off one until the
                  // virtualizer lands. A row that announces itself as
                  // selectable in a table with no cell navigation is a promise
                  // nothing keeps.
                  className={`row-click${
                    selected === g.group_id ? " row-selected" : ""
                  }`}
                  tabIndex={0}
                  onClick={() =>
                    setSelected((prev) =>
                      prev === g.group_id ? null : g.group_id,
                    )
                  }
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setSelected((prev) =>
                        prev === g.group_id ? null : g.group_id,
                      );
                    }
                  }}
                >
                  <td className="cell-mono">{g.group_id}</td>
                  <td>
                    <span
                      className={`health health-${
                        g.state.toLowerCase() === "stable" ? "ok" : "quiet"
                      }`}
                      title={`Kafka reports this share group as ${g.state}.`}
                    >
                      <i className="dot" aria-hidden="true" />
                      {g.state}
                    </span>
                  </td>
                  <td className="col-num cell-num">{g.member_count}</td>
                  <td className="col-affordance">
                    <span className="row-affordance">
                      {selected === g.group_id ? "Hide ↑" : "View members →"}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {selected !== null && (
        <ShareGroupDetailBlock
          groupId={selected}
          detail={detail}
          loading={loadingDetail}
          error={detailError}
          onDismissError={() => setDetailError(null)}
        />
      )}
    </section>
  );
}

function ShareGroupDetailBlock({
  groupId,
  detail,
  loading,
  error,
  onDismissError,
}: {
  groupId: string;
  detail: ShareGroupDetail | null;
  loading: boolean;
  error: string | null;
  onDismissError: () => void;
}) {
  if (error !== null)
    return <ErrorBanner raw={error} onDismiss={onDismissError} />;

  if (detail === null)
    return (
      <p className="table-note">
        {loading
          ? `Asking the cluster about ${groupId}…`
          : `Kavka couldn't read ${groupId}. It may have been removed, or the account may not have Describe on it.`}
      </p>
    );

  return (
    <>
      <div className="stat-grid">
        <div className="stat">
          <span className="stat-label">Members</span>
          <span className="stat-value">{detail.members.length}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Partitions served</span>
          <span className="stat-value">{detail.offsets.length}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Topics</span>
          <span className="stat-value">
            {new Set(detail.offsets.map((o) => o.topic)).size}
          </span>
        </div>
      </div>

      <h3 className="chart-title">
        <span className="chart-title-name">Members</span>
        <span className="chart-title-count">
          each one takes records from any partition it is assigned
        </span>
      </h3>

      {detail.members.length === 0 ? (
        <p className="table-note">
          Nothing is connected to {groupId} right now. Kafka keeps where the
          group had got to, so an application that starts up carries on from
          there.
        </p>
      ) : (
        <div className="table-wrap">
          <table className="data-table data-table-flush">
            <caption className="sr-only">Members of {groupId}</caption>
            <thead>
              <tr>
                <th scope="col">Client</th>
                <th scope="col">Member id</th>
                <th scope="col" className="col-num">
                  Partitions
                </th>
              </tr>
            </thead>
            <tbody>
              {detail.members.map((m) => (
                <tr key={m.member_id}>
                  <td className="cell-mono">{m.client_id}</td>
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

      <h3 className="chart-title">
        <span className="chart-title-name">Where delivery starts</span>
        <span className="chart-title-count">
          not a committed offset — see the note below
        </span>
      </h3>

      <p className="table-note">
        A share group acknowledges records one at a time, so there is no single
        "we have read up to here". What Kafka keeps is the point before which
        everything <em>has</em> been acknowledged; records after it may have been
        delivered, acknowledged, or neither. Reading this as a consumer group's
        committed offset will mislead you about how much work is outstanding.
      </p>

      {detail.offsets.length === 0 ? (
        <p className="table-note">
          The cluster reports no partitions for this group yet.
        </p>
      ) : (
        <div className="table-wrap">
          <table className="data-table">
            <caption className="sr-only">
              Share-group start offsets for {groupId}
            </caption>
            <thead>
              <tr>
                <th scope="col" className="ledger-gutter">
                  Part.
                </th>
                <th scope="col">Topic</th>
                <th scope="col" className="col-num">
                  Start offset
                </th>
              </tr>
            </thead>
            <tbody>
              {detail.offsets.map((o) => (
                <tr key={`${o.topic}:${o.partition}`}>
                  <td className="ledger-gutter">{o.partition}</td>
                  <td className="cell-mono">{o.topic}</td>
                  <td className="col-num cell-mono cell-mono-num">
                    {o.start_offset === null ? (
                      <span
                        className="absent"
                        title="The broker didn't report a start offset for this partition — a gap in the answer, not a zero."
                      >
                        ∅
                      </span>
                    ) : (
                      groupDigits(o.start_offset)
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
