import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  topicsList,
  type ClusterOverview,
  type ConnectionProfile,
  type TopicInfo,
} from "./api";
import { EnvChip } from "./Sidebar";

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
      <header className="cluster-header">
        <div className="cluster-header-id">
          <h1 className="cluster-title">{profile.name}</h1>
          <EnvChip env={profile.environment} />
          {profile.read_only && (
            <span className="readonly-chip" title="Read-only connection">
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
      </header>

      <section className="overview">
        <div className="stat-grid">
          <div className="stat">
            <span className="stat-label">Cluster ID</span>
            <span className="stat-value stat-mono">
              {overview.cluster_id ?? "—"}
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

        <div className="table-card">
          <div className="table-card-header">
            <h2 className="table-card-title">Brokers</h2>
          </div>
          <table className="data-table">
            <thead>
              <tr>
                <th className="col-num">ID</th>
                <th>Host</th>
                <th className="col-num">Port</th>
              </tr>
            </thead>
            <tbody>
              {overview.brokers.length === 0 ? (
                <tr>
                  <td colSpan={3} className="cell-empty">
                    No brokers reported.
                  </td>
                </tr>
              ) : (
                overview.brokers.map((b) => (
                  <tr key={b.id}>
                    <td className="col-num cell-mono">{b.id}</td>
                    <td className="cell-mono">{b.host}</td>
                    <td className="col-num cell-mono">{b.port}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="table-card">
        <div className="table-card-header">
          <h2 className="table-card-title">
            Topics
            {topics !== null && (
              <span className="table-card-count">
                {visibleTopics.length}
                {!showInternal && internalCount > 0
                  ? ` (${internalCount} internal hidden)`
                  : ""}
              </span>
            )}
          </h2>
          <div className="table-card-tools">
            <label className="check-row check-row-inline">
              <input
                type="checkbox"
                checked={showInternal}
                onChange={(e) => setShowInternal(e.target.checked)}
              />
              <span>Show internal</span>
            </label>
            <button
              type="button"
              className="btn"
              disabled={loadingTopics}
              onClick={() => void fetchTopics()}
            >
              {loadingTopics ? "Refreshing…" : "Refresh"}
            </button>
          </div>
        </div>

        {topics === null && loadingTopics ? (
          <div className="table-note">Loading topics…</div>
        ) : topics === null ? (
          <div className="table-note">Topics could not be loaded.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Name</th>
                <th className="col-num">Partitions</th>
                <th className="col-num">Replication</th>
              </tr>
            </thead>
            <tbody>
              {visibleTopics.length === 0 ? (
                <tr>
                  <td colSpan={3} className="cell-empty">
                    {topics.length === 0
                      ? "No topics in this cluster."
                      : "Only internal topics — enable “Show internal”."}
                  </td>
                </tr>
              ) : (
                visibleTopics.map((t) => (
                  <tr
                    key={t.name}
                    className={isInternal(t) ? "row-internal" : ""}
                  >
                    <td className="cell-mono">{t.name}</td>
                    <td className="col-num cell-mono">{t.partitions}</td>
                    <td className="col-num cell-mono">
                      {t.replication_factor}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
