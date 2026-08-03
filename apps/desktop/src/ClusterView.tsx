import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  topicsList,
  type ClusterOverview,
  type ConnectionProfile,
  type TopicInfo,
} from "./api";
import { EnvChip } from "./Sidebar";
import { Term } from "./Glossary";

function isInternal(topic: TopicInfo): boolean {
  return topic.internal || topic.name.startsWith("__");
}

interface ClusterViewProps {
  profile: ConnectionProfile;
  overview: ClusterOverview;
  onDisconnect: (profileId: string) => void;
  onError: (msg: string) => void;
}

export default function ClusterView({
  profile,
  overview,
  onDisconnect,
  onError,
}: ClusterViewProps) {
  const [topics, setTopics] = useState<TopicInfo[] | null>(null);
  const [loadingTopics, setLoadingTopics] = useState(false);
  const [showInternal, setShowInternal] = useState(false);
  // Monotonic id so a slow response can never overwrite a newer one (the
  // component is also remounted per profile via key= in App, but Refresh can
  // still race itself).
  const fetchSeq = useRef(0);

  const fetchTopics = useCallback(async () => {
    const seq = ++fetchSeq.current;
    setLoadingTopics(true);
    try {
      const list = await topicsList(profile.id);
      if (fetchSeq.current === seq) setTopics(list);
    } catch (err) {
      if (fetchSeq.current === seq) onError(errorMessage(err));
    } finally {
      if (fetchSeq.current === seq) setLoadingTopics(false);
    }
  }, [profile.id, onError]);

  useEffect(() => {
    void fetchTopics();
    return () => {
      // Invalidate in-flight responses on unmount/profile change.
      fetchSeq.current++;
    };
  }, [fetchTopics]);

  const internalCount = topics?.filter(isInternal).length ?? 0;
  const visibleTopics =
    topics?.filter((t) => showInternal || !isInternal(t)) ?? [];

  return (
    <div className="cluster-view">
      <header className="view-header">
        <div className="view-title-row">
          <div className="view-title-id">
            <h1 className="view-title">{profile.name}</h1>
            <EnvChip env={profile.environment} />
            {profile.read_only && (
              <span
                className="readonly-chip"
                title="This connection is read-only. Turn that off in the connection's settings to produce or edit."
              >
                read-only
              </span>
            )}
          </div>
          <button
            type="button"
            className="btn"
            onClick={() => onDisconnect(profile.id)}
          >
            Disconnect
          </button>
        </div>
        {/* Prod guardrail layer 3: the address stays on screen next to the
            name, not only in the sidebar. */}
        <span className="view-address">
          {profile.bootstrap_servers.join(", ")}
        </span>
        {profile.read_only && (
          <span className="readonly-note">
            This connection is read-only. Turn that off in the connection's
            settings to produce or edit.
          </span>
        )}
      </header>

      <section className="panel">
        {/* Stat blocks: no border, no background, no radius. Quantities Kavka
            computed are sans + tabular; the cluster id is a literal from
            Kafka, so it is mono. */}
        <div className="stat-grid">
          <div className="stat">
            <span className="stat-label">Cluster ID</span>
            <span className="stat-value stat-value-mono">
              {overview.cluster_id ?? <span className="absent">∅</span>}
            </span>
          </div>
          <div className="stat">
            <span className="stat-label">Brokers</span>
            <span className="stat-value">{overview.brokers.length}</span>
          </div>
          <div className="stat">
            <span className="stat-label">Topics</span>
            <span className="stat-value">{overview.topic_count}</span>
          </div>
          <div className="stat">
            <span className="stat-label">Partitions</span>
            <span className="stat-value">{overview.partition_count}</span>
          </div>
        </div>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <Term name="broker">Brokers</Term>
            <span className="panel-count">{overview.brokers.length}</span>
          </h2>
        </div>

        {/* The ledger gutter carries the broker id — the row's address in
            Kafka's own vocabulary — then the rule, then the payload.

            NO role="grid" and NO aria-rowcount/aria-rowindex. This is a
            static, fully-rendered table, so the implicit <table> semantics
            are already complete and correct. role="grid" promises an
            interactive widget with arrow-key cell navigation that this does
            not implement, and it makes the rows announce as grid cells the
            user is expected to drive. aria-rowcount earns its keep the day
            the virtualizer lands and the rendered count stops matching the
            real one — not before. */}
        <div className="table-wrap">
          <table className="data-table">
            <caption className="sr-only">Brokers in this cluster</caption>
            <thead>
              <tr>
                <th scope="col" className="ledger-gutter">ID</th>
                <th scope="col">Host</th>
                <th scope="col" className="col-num">Port</th>
              </tr>
            </thead>
            <tbody>
              {overview.brokers.length === 0 ? (
                <tr>
                  <td colSpan={3} className="cell-empty">
                    This cluster reported no brokers. That normally means the
                    connection is up but metadata came back empty — try
                    reconnecting.
                  </td>
                </tr>
              ) : (
                overview.brokers.map((b) => (
                  <tr key={b.id}>
                    <td className="ledger-gutter">{b.id}</td>
                    <td className="cell-mono">{b.host}</td>
                    <td className="col-num cell-num">{b.port}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Topics
            {topics !== null && (
              <span className="panel-count">
                {visibleTopics.length}
                {!showInternal && internalCount > 0
                  ? ` shown · ${internalCount} internal hidden`
                  : ""}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <label className="check-row">
              <input
                type="checkbox"
                checked={showInternal}
                onChange={(e) => setShowInternal(e.target.checked)}
              />
              <span>
                Show <Term name="internal-topic">internal</Term>
              </span>
            </label>
            {/* No spinner on the button — the label stays put and the table
                grows a 2px accent bar under its header instead. */}
            <button
              type="button"
              className="btn"
              disabled={loadingTopics}
              aria-busy={loadingTopics}
              title={
                loadingTopics
                  ? "Kavka is already asking the cluster for topics"
                  : undefined
              }
              onClick={() => void fetchTopics()}
            >
              Refresh
            </button>
          </div>
        </div>

        {/* Say what this screen is and is not. Without it a user who has just
            connected reads a list of topic names, clicks one, nothing happens,
            and concludes the app is broken — the dead end is worse than the
            missing feature. */}
        <p className="table-note">
          Message browsing arrives in an upcoming release — this is the live
          topic inventory for now.
        </p>

        {topics === null && loadingTopics ? (
          <>
            <div className="table-note">Asking the cluster for topics…</div>
            {/* Skeleton of the table that is about to appear. No shimmer. */}
            <div className="skeleton-table" aria-hidden="true">
              {[42, 30, 51, 36, 45, 33].map((w, i) => (
                <div className="skeleton-row" key={i}>
                  <div className="skeleton-cell" style={{ width: `${w}%` }} />
                  <div className="skeleton-cell" style={{ width: "36px" }} />
                </div>
              ))}
            </div>
          </>
        ) : topics === null ? (
          <div className="table-note">
            Kavka couldn't list this cluster's topics. If the connection is up,
            the account may not have <code>Describe</code> on the cluster — ask
            whoever issued the credentials for that permission.
          </div>
        ) : (
          <div className="table-wrap">
            {loadingTopics && <div className="table-loading" role="presentation" />}
            {/* Topics have no address of their own, so the gutter is zero and
                the rule sits flush at the table's left edge. It is still the
                app's constant, and it is still coloured by environment. */}
            {/* Implicit table semantics — see the Brokers table above for why
                role="grid" and aria-rowcount are deliberately absent. */}
            <table className="data-table data-table-flush">
              <caption className="sr-only">
                Topics on this cluster
                {showInternal ? ", including Kafka's internal topics" : ""}
              </caption>
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col" className="col-num">
                    <Term name="partition">Partitions</Term>
                  </th>
                  <th scope="col" className="col-num">
                    <Term name="replication-factor">Replication</Term>
                  </th>
                </tr>
              </thead>
              <tbody>
                {visibleTopics.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="cell-empty">
                      {topics.length === 0
                        ? "This cluster has no topics. They appear here as soon as something creates one."
                        : "This cluster only has Kafka's own internal topics. Turn on “Show internal” to see them."}
                    </td>
                  </tr>
                ) : (
                  visibleTopics.map((t) => (
                    <tr
                      key={t.name}
                      className={isInternal(t) ? "row-internal" : ""}
                      // Dimmed, never hidden — and now the dimming explains
                      // itself instead of leaving the user to guess why some
                      // rows are quieter than others.
                      title={
                        isInternal(t)
                          ? `${t.name} is one of Kafka's own internal topics — Kafka uses it for bookkeeping, not for your messages. It is shown greyed rather than hidden so you can see it exists.`
                          : undefined
                      }
                    >
                      <td className="cell-mono">
                        {t.name}
                        {isInternal(t) && (
                          <span className="cell-tag"> internal</span>
                        )}
                      </td>
                      <td className="col-num cell-num">{t.partitions}</td>
                      <td className="col-num cell-num">
                        {t.replication_factor}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
