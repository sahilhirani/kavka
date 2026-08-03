import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  topicDelete,
  topicDetail,
  topicsList,
  type ConnectionProfile,
  type PartitionDetail,
  type TopicDetail,
  type TopicInfo,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import CreateTopicModal from "./CreateTopicModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { approxCount, groupDigits } from "./format";
import { Term } from "./Glossary";
import MessagesView from "./MessagesView";
import { ErrorBanner } from "./ProfileEditor";

function isInternal(topic: TopicInfo): boolean {
  return topic.internal || topic.name.startsWith("__");
}

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

interface TopicsTabProps {
  profile: ConnectionProfile;
  brokerCount: number;
  /** null = the topic list; a name = that topic's detail. */
  topic: string | null;
  /** True when the message browser is open for `topic`. */
  browsing: boolean;
  onSelectTopic: (topic: string | null) => void;
  onBrowse: (browsing: boolean) => void;
  onDanger: DangerReport;
}

export default function TopicsTab({
  profile,
  brokerCount,
  topic,
  browsing,
  onSelectTopic,
  onBrowse,
  onDanger,
}: TopicsTabProps) {
  const [topics, setTopics] = useState<TopicInfo[] | null>(null);
  const [loadingTopics, setLoadingTopics] = useState(false);
  const [listFailed, setListFailed] = useState(false);
  const [showInternal, setShowInternal] = useState(false);

  const [detail, setDetail] = useState<TopicDetail | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [showDefaults, setShowDefaults] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const listSeq = useRef(0);
  const detailSeq = useRef(0);

  useDangerSignal(error !== null, onDanger);

  const isProd = profile.environment === "prod";
  const readOnly = profile.read_only;

  // ── The list ───────────────────────────────────────────────────────────

  const fetchTopics = useCallback(async () => {
    const seq = ++listSeq.current;
    setLoadingTopics(true);
    try {
      const list = await topicsList(profile.id);
      if (listSeq.current !== seq) return;
      setTopics(list);
      setListFailed(false);
    } catch (err) {
      if (listSeq.current !== seq) return;
      setListFailed(true);
      setError(errorMessage(err));
    } finally {
      if (listSeq.current === seq) setLoadingTopics(false);
    }
  }, [profile.id]);

  useEffect(() => {
    void fetchTopics();
    return () => {
      listSeq.current += 1;
    };
  }, [fetchTopics]);

  // ── The detail ─────────────────────────────────────────────────────────

  const fetchDetail = useCallback(
    async (name: string) => {
      const seq = ++detailSeq.current;
      setLoadingDetail(true);
      try {
        const next = await topicDetail(profile.id, name);
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
    if (topic === null) {
      setDetail(null);
      return;
    }
    setDetail(null);
    void fetchDetail(topic);
    return () => {
      detailSeq.current += 1;
    };
  }, [topic, fetchDetail]);

  // ── Delete ─────────────────────────────────────────────────────────────

  const handleDelete = useCallback(async () => {
    if (topic === null) return;
    setDeleting(true);
    try {
      await topicDelete(profile.id, topic);
      setConfirmingDelete(false);
      onBrowse(false);
      onSelectTopic(null);
      await fetchTopics();
    } catch (err) {
      setConfirmingDelete(false);
      setError(errorMessage(err));
    } finally {
      setDeleting(false);
    }
  }, [topic, profile.id, onBrowse, onSelectTopic, fetchTopics]);

  const internalCount = topics?.filter(isInternal).length ?? 0;
  const visibleTopics = useMemo(
    () => topics?.filter((t) => showInternal || !isInternal(t)) ?? [],
    [topics, showInternal],
  );

  const approxRecords = useMemo(() => {
    if (detail === null) return 0;
    return detail.partitions.reduce(
      (sum, p) => sum + Math.max(0, p.latest_offset - p.earliest_offset),
      0,
    );
  }, [detail]);

  const banner =
    error === null ? null : (
      <ErrorBanner raw={error} onDismiss={() => setError(null)} />
    );

  // ── The message browser owns the whole view when it is open ────────────

  if (topic !== null && browsing) {
    if (detail === null) {
      // The browser needs the partition list before it can seek anywhere, so
      // it waits — and if the read failed it says so and offers a way out,
      // rather than spinning on a sentence that stopped being true.
      return (
        <section className="panel">
          {banner}
          {loadingDetail ? (
            <p className="table-note">
              Asking the cluster about <code>{topic}</code>…
            </p>
          ) : (
            <>
              <p className="table-note">
                Kavka couldn't read <code>{topic}</code>'s partitions, so it
                can't say where to start reading. The topic may have been
                deleted, or the account may not have <code>Describe</code> on
                it.
              </p>
              <div className="empty-actions">
                <button
                  type="button"
                  className="btn"
                  onClick={() => void fetchDetail(topic)}
                >
                  Try again
                </button>
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => onBrowse(false)}
                >
                  Back to {topic}
                </button>
              </div>
            </>
          )}
        </section>
      );
    }
    return (
      <MessagesView
        // Keyed by topic: the browser's seek bar, buffer and tail all belong
        // to ONE topic, and carrying them across would show the previous
        // topic's rows under the new topic's name.
        key={detail.name}
        profile={profile}
        topic={detail.name}
        partitions={detail.partitions}
        onBack={() => onBrowse(false)}
        onDanger={onDanger}
      />
    );
  }

  // ── Topic detail ───────────────────────────────────────────────────────

  if (topic !== null) {
    const overrides = detail?.configs.filter((c) => !c.is_default) ?? [];
    const shownConfigs =
      detail === null ? [] : showDefaults ? detail.configs : overrides;
    const hiddenDefaults = (detail?.configs.length ?? 0) - overrides.length;

    return (
      <>
        {banner}

        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              <button
                type="button"
                className="btn btn-ghost crumb-btn"
                onClick={() => onSelectTopic(null)}
              >
                ← Topics
              </button>
              <span className="topic-name">{topic}</span>
              {detail?.internal && (
                <span
                  className="cell-tag"
                  title="Kafka uses this topic for its own bookkeeping, not for your messages."
                >
                  internal
                </span>
              )}
            </h2>
            <div className="panel-tools">
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => onBrowse(true)}
                disabled={detail === null}
                title={
                  detail === null
                    ? "Kavka is still reading this topic's partitions"
                    : undefined
                }
              >
                Browse messages
              </button>
              {/* Danger OUTLINE: it only ever opens the confirmation. */}
              <button
                type="button"
                className="btn btn-danger"
                disabled={readOnly || detail === null}
                title={
                  readOnly
                    ? READ_ONLY_WHY
                    : detail === null
                      ? "Kavka is still reading this topic"
                      : undefined
                }
                onClick={() => setConfirmingDelete(true)}
              >
                Delete topic
              </button>
            </div>
          </div>

          {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

          {detail === null ? (
            <p className="table-note">
              {loadingDetail
                ? "Asking the cluster about this topic…"
                : "Kavka couldn't read this topic. It may have been deleted, or the account may not have Describe on it."}
            </p>
          ) : (
            <div className="stat-grid">
              <div className="stat">
                <span className="stat-label">Partitions</span>
                <span className="stat-value">{detail.partitions.length}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Messages</span>
                <span className="stat-value">{groupDigits(approxRecords)}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Settings set here</span>
                <span className="stat-value">{overrides.length}</span>
              </div>
            </div>
          )}
        </section>

        {detail !== null && (
          <section className="panel">
            <div className="panel-head">
              <h2 className="panel-title">
                <Term name="partition">Partitions</Term>
                <span className="panel-count">{detail.partitions.length}</span>
              </h2>
            </div>

            {/* The gutter carries the partition index — the row's address in
                Kafka's own vocabulary. Static table, so no role="grid". */}
            <div className="table-wrap">
              <table className="data-table">
                <caption className="sr-only">
                  Partitions of {detail.name}
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="ledger-gutter">
                      Part.
                    </th>
                    <th scope="col" className="col-num">
                      Leader
                    </th>
                    <th scope="col">Replicas</th>
                    <th scope="col">
                      <Term name="isr">In sync</Term>
                    </th>
                    <th scope="col" className="col-num">
                      Earliest
                    </th>
                    <th scope="col" className="col-num">
                      Latest
                    </th>
                    <th scope="col" className="col-num">
                      Messages
                    </th>
                    <th scope="col">Health</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.partitions.map((p) => (
                    <PartitionRow key={p.partition} partition={p} />
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        )}

        {detail !== null && (
          <section className="panel">
            <div className="panel-head">
              <h2 className="panel-title">
                Configuration
                <span className="panel-count">
                  {overrides.length} set here
                  {!showDefaults && hiddenDefaults > 0
                    ? ` · ${hiddenDefaults} broker defaults hidden`
                    : ""}
                </span>
              </h2>
              <div className="panel-tools">
                <label className="check-row">
                  <input
                    type="checkbox"
                    checked={showDefaults}
                    onChange={(e) => setShowDefaults(e.target.checked)}
                  />
                  <span>Show broker defaults</span>
                </label>
              </div>
            </div>

            <p className="table-note">
              A <code>+</code> in the gutter means this topic overrides the
              broker — everything else is whatever the cluster's own default
              happens to be right now.
            </p>

            <div className="table-wrap">
              <table className="data-table data-table-sign">
                <caption className="sr-only">
                  Configuration of {detail.name}, overrides marked
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="ledger-gutter">
                      <span className="sr-only">Overridden</span>
                    </th>
                    <th scope="col">Setting</th>
                    <th scope="col">Value</th>
                    <th scope="col">Source</th>
                  </tr>
                </thead>
                <tbody>
                  {shownConfigs.length === 0 ? (
                    <tr>
                      <td colSpan={4} className="cell-empty">
                        This topic changes nothing from the broker's defaults.
                        Turn on “Show broker defaults” to see what it inherits.
                      </td>
                    </tr>
                  ) : (
                    shownConfigs.map((entry) => (
                      <tr
                        key={entry.name}
                        className={entry.is_default ? "" : "diff-add"}
                      >
                        <td className="ledger-gutter" aria-hidden="true">
                          {entry.is_default ? "" : "+"}
                        </td>
                        <td className="cell-mono">
                          {entry.name}
                          {entry.is_read_only && (
                            <span
                              className="cell-tag"
                              title="The broker won't accept a change to this one from a client."
                            >
                              {" "}
                              read-only
                            </span>
                          )}
                        </td>
                        <td className="cell-mono">
                          {entry.is_sensitive ? (
                            <span
                              className="absent"
                              title="Kafka withholds this value — it is marked sensitive, so no client ever reads it back."
                            >
                              —
                              <span className="sr-only">
                                withheld by the broker
                              </span>
                            </span>
                          ) : entry.value === null ? (
                            <span className="absent">∅</span>
                          ) : (
                            entry.value
                          )}
                          {entry.is_default && (
                            <span className="cell-tag"> broker default</span>
                          )}
                        </td>
                        <td className="cell-tag config-source">
                          {entry.source}
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </section>
        )}

        {confirmingDelete && detail !== null && (
          <ConfirmModal
            title={
              isProd
                ? `Delete ${detail.name} on ${profile.name}?`
                : `Delete ${detail.name}?`
            }
            body={
              <>
                This removes the topic and every message in it — about{" "}
                {approxCount(approxRecords)} records. It can't be undone, and
                any consumer group reading it will start failing.
              </>
            }
            confirmLabel="Delete topic"
            // Environment-gated, not action-gated: prod always asks, dev never
            // does (§6 layer 4).
            typeToConfirm={isProd ? detail.name : null}
            busy={deleting}
            busyLabel="Kavka is deleting the topic"
            onCancel={() => setConfirmingDelete(false)}
            onConfirm={() => void handleDelete()}
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
            <button
              type="button"
              className="btn"
              disabled={loadingTopics}
              aria-busy={loadingTopics || undefined}
              title={
                loadingTopics
                  ? "Kavka is already asking the cluster for topics"
                  : undefined
              }
              onClick={() => void fetchTopics()}
            >
              Refresh
            </button>
            <button
              type="button"
              className={`btn ${isProd ? "btn-danger" : ""}`}
              disabled={readOnly}
              title={readOnly ? READ_ONLY_WHY : undefined}
              onClick={() => setCreating(true)}
            >
              Create topic
            </button>
          </div>
        </div>

        {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {topics === null && loadingTopics ? (
          <>
            <div className="table-note">Asking the cluster for topics…</div>
            <div className="skeleton-table" aria-hidden="true">
              {[42, 30, 51, 36, 45, 33].map((w, i) => (
                <div className="skeleton-row" key={i}>
                  <div className="skeleton-cell" style={{ width: `${w}%` }} />
                  <div className="skeleton-cell" style={{ width: "36px" }} />
                </div>
              ))}
            </div>
          </>
        ) : topics === null || listFailed ? (
          <div className="table-note">
            Kavka couldn't list this cluster's topics. If the connection is up,
            the account may not have <code>Describe</code> on the cluster — ask
            whoever issued the credentials for that permission.
          </div>
        ) : (
          <div className="table-wrap">
            {loadingTopics && (
              <div className="table-loading" role="presentation" />
            )}
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
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Open</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {visibleTopics.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="cell-empty">
                      {topics.length === 0 ? (
                        <>
                          This cluster has no topics. They appear here as soon
                          as something creates one.
                        </>
                      ) : (
                        "This cluster only has Kafka's own internal topics. Turn on “Show internal” to see them."
                      )}
                    </td>
                  </tr>
                ) : (
                  visibleTopics.map((t) => (
                    <tr
                      key={t.name}
                      className={`row-click${isInternal(t) ? " row-internal" : ""}`}
                      tabIndex={0}
                      onClick={() => onSelectTopic(t.name)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          onSelectTopic(t.name);
                        }
                      }}
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
                      {/* Row affordance for novices: the whole row is
                          clickable, and the last column says so on hover. */}
                      <td className="col-affordance">
                        <span className="row-affordance">View messages →</span>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {creating && (
        <CreateTopicModal
          profileId={profile.id}
          isProd={isProd}
          clusterName={profile.name}
          brokerCount={brokerCount}
          existing={topics?.map((t) => t.name) ?? []}
          onCreated={(name) => {
            setCreating(false);
            void fetchTopics();
            onSelectTopic(name);
          }}
          onClose={() => setCreating(false)}
        />
      )}
    </>
  );
}

/**
 * One partition. Law 2 in its most load-bearing form: under-replication gets a
 * dot AND a word AND the numbers it was derived from, never a colour alone.
 */
function PartitionRow({ partition }: { partition: PartitionDetail }) {
  const messages = Math.max(
    0,
    partition.latest_offset - partition.earliest_offset,
  );
  const missing = partition.replicas.length - partition.isr.length;
  const healthy = missing <= 0;
  return (
    <tr>
      <td className="ledger-gutter">{partition.partition}</td>
      <td className="col-num cell-num">{partition.leader}</td>
      <td className="cell-mono">{partition.replicas.join(", ")}</td>
      <td className="cell-mono">{partition.isr.join(", ")}</td>
      {/* An offset is a literal you could paste into a seek command, so it is
          mono. The message count is a quantity Kavka computed, so it is not. */}
      <td className="col-num cell-mono cell-mono-num">
        {groupDigits(partition.earliest_offset)}
      </td>
      <td className="col-num cell-mono cell-mono-num">
        {groupDigits(partition.latest_offset)}
      </td>
      <td className="col-num cell-num">{groupDigits(messages)}</td>
      <td>
        <span className={`health ${healthy ? "health-ok" : "health-warn"}`}>
          <i className="dot" aria-hidden="true" />
          {healthy ? (
            "In sync"
          ) : (
            <>
              <Term name="under-replicated">Under-replicated</Term>
              <span className="cell-tag"> {missing} missing</span>
            </>
          )}
        </span>
      </td>
    </tr>
  );
}
