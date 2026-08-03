import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  electLeaders,
  errorMessage,
  topicDelete,
  topicDetail,
  topicsList,
  type BrokerInfo,
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
import {
  PartitionResultsNote,
  ReassignModal,
  ReassignMonitor,
  splitResults,
  type ResultSplit,
} from "./PartitionOps";
import ProducePanel from "./ProducePanel";
import { ErrorBanner } from "./ProfileEditor";
import SchemasPanel from "./SchemasPanel";
import SearchView from "./SearchView";
import { ToastStack, useToasts } from "./Toast";

function isInternal(topic: TopicInfo): boolean {
  return topic.internal || topic.name.startsWith("__");
}

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/**
 * Which of the topic's four screens is open.
 *
 * Phase 2 turned the browser's `browsing` boolean into three states, because
 * search is a peer of the browser rather than a mode inside it: they answer
 * different questions about the same topic and neither belongs inside the
 * other's toolbar. Phase 3a adds `schemas` on the same argument — "what shape
 * are these messages" is a fourth question, and it is asked about a topic
 * rather than about a cluster, which is why it lives here and not in its own
 * tab beside Brokers.
 */
export type TopicPane = "detail" | "messages" | "search" | "schemas";

interface TopicsTabProps {
  profile: ConnectionProfile;
  /**
   * The cluster's brokers, from the overview.
   *
   * It was a count until Phase 3b, which is all "create topic" ever needed.
   * Reassignment needs the ids: a replica list is a list of broker ids, and a
   * picker built from `1..count` would be wrong on every cluster whose broker
   * ids aren't contiguous from one — which is most of them after a rebuild.
   */
  brokers: BrokerInfo[];
  /** null = the topic list; a name = that topic's detail. */
  topic: string | null;
  pane: TopicPane;
  onSelectTopic: (topic: string | null) => void;
  onPane: (pane: TopicPane) => void;
  onDanger: DangerReport;
  /**
   * Lets the palette open "Produce to <topic>" — see ClusterView, which is
   * what threads it up to the app root. Called with the handlers as they
   * change so nothing has to reach back down.
   */
  onActions?: (actions: TopicActions | null) => void;
  /** Opens this connection's settings — the schemas pane offers it when the
      connection has no registry. It disconnects; both call sites say so. */
  onEditConnection: () => void;
}

/** What the command palette can do to the topic currently on screen. */
export interface TopicActions {
  topic: string;
  search: () => void;
  produce: () => void;
  /** Set = producing is unavailable, and this is the sentence saying why. */
  produceBlocked?: string;
}

export default function TopicsTab({
  profile,
  brokers,
  topic,
  pane,
  onSelectTopic,
  onPane,
  onDanger,
  onActions,
  onEditConnection,
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

  // ── Partition ops (Phase 3b) ─────────────────────────────────────────
  // `electing.partitions === null` is the whole topic; an array is the one
  // row that asked. `opResult` is what the last batch did, rendered under the
  // partition table rather than as a toast: a batch where four partitions
  // worked and one didn't is a condition you are in, not a thing you finished.
  const [electing, setElecting] = useState<{ partitions: number[] | null } | null>(
    null,
  );
  const [electBusy, setElectBusy] = useState(false);
  const [opResult, setOpResult] = useState<{
    split: ResultSplit;
    okWord: string;
    benignWord?: string;
  } | null>(null);
  const [reassigning, setReassigning] = useState<PartitionDetail[] | null>(null);
  /** Bumped when the cluster accepts a plan, so the monitor looks immediately. */
  const [moveNonce, setMoveNonce] = useState(0);

  // Produce and its aftermath. `jump` carries the record a produce just wrote
  // so "View it" on the toast lands the browser on that exact offset.
  const [producing, setProducing] = useState(false);
  const [jump, setJump] = useState<{
    partition: number;
    offset: number;
    nonce: number;
  } | null>(null);
  const jumpNonce = useRef(0);
  const toaster = useToasts();
  const push = toaster.push;

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
    // A produce jump belongs to ONE topic. Deleting this topic, or picking
    // another, must not leave the browser aimed at an offset that means
    // something else now — or nothing at all. The same is true of the last
    // batch's results: "partition 3 was refused" says nothing once partition 3
    // belongs to a different topic.
    setJump(null);
    setOpResult(null);
    setElecting(null);
    setReassigning(null);
    // The move nonce belongs to a topic too. It means "a plan was accepted for
    // THIS topic, so poll now and say something if nothing ever shows up" —
    // carried across a topic change it would have the monitor reporting on a
    // submission that was never made for the topic on screen.
    setMoveNonce(0);
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
      onPane("detail");
      onSelectTopic(null);
      await fetchTopics();
    } catch (err) {
      setConfirmingDelete(false);
      setError(errorMessage(err));
    } finally {
      setDeleting(false);
    }
  }, [topic, profile.id, onPane, onSelectTopic, fetchTopics]);

  // ── Preferred-leader election ──────────────────────────────────────────

  const runElection = useCallback(
    async (partitions: number[] | null) => {
      if (topic === null) return;
      // THE SNAPSHOT, taken before the call, because the answer cannot carry
      // this: an election that moved a leader and one that found the leader
      // already in place both come back as `error: null` (the core folds
      // ELECTION_NOT_NEEDED into the benign set — see api.ts). Classifying on
      // the result alone therefore reports every partition of a settled topic
      // as "moved", which is a lie told confidently. Which partitions were off
      // their preferred leader a moment ago is knowable here, and only here.
      const wasUnpreferred = new Set(
        (detail?.partitions ?? [])
          .filter((p) => p.replicas.length > 0 && p.replicas[0] !== p.leader)
          .map((p) => p.partition),
      );
      setElectBusy(true);
      try {
        const results = await electLeaders(profile.id, topic, partitions);
        const split = splitResults(
          results,
          (r) => !wasUnpreferred.has(r.partition),
        );
        setElecting(null);
        setOpResult({
          split,
          okWord: "moved to their preferred leader",
          benignWord: "were already on it",
        });
        if (split.ok.length > 0) {
          push({
            kind: "ok",
            title: `Moved ${split.ok.length} partition${
              split.ok.length === 1 ? "" : "s"
            } to the preferred leader`,
            detail:
              "Clients following this topic reconnect to the new leader on their own, usually within a second.",
          });
        } else if (split.failed.length === 0) {
          push({
            kind: "info",
            title: "Nothing to move",
            detail:
              "Every partition asked about was already led by the first broker in its replica list.",
          });
        }
        // Read again so the table shows the leaders as they are NOW — the
        // summary line above it is about partitions whose rows have just
        // changed, and the two disagreeing is worse than either alone.
        await fetchDetail(topic);
      } catch (err) {
        setElecting(null);
        setError(errorMessage(err));
      } finally {
        setElectBusy(false);
      }
    },
    [topic, detail, profile.id, push, fetchDetail],
  );

  /**
   * Prod asks before every write (§6 layer 4); dev asks only for the
   * whole-topic election, because that is the one whose blast radius isn't
   * written on the button. Same shape as ConnectTab's task restart.
   */
  const askElection = useCallback(
    (partitions: number[] | null) => {
      if (!isProd && partitions !== null) {
        void runElection(partitions);
        return;
      }
      setElecting({ partitions });
    },
    [isProd, runElection],
  );

  /** The monitor says a move finished; the partition table has to catch up. */
  const movesSettled = useCallback(() => {
    if (topic !== null) void fetchDetail(topic);
  }, [topic, fetchDetail]);

  // ── Produce ────────────────────────────────────────────────────────────

  const openProduce = useCallback(() => setProducing(true), []);

  /**
   * Every pane change goes through here so the produce jump dies with it.
   *
   * The browser is remounted at a produced record by keying it on the jump's
   * nonce, which means a stale jump would silently re-seek the NEXT time the
   * browser is opened — the user asks for "Browse messages" and lands on a
   * message they produced ten minutes ago, with no way to tell why.
   */
  const goPane = useCallback(
    (next: TopicPane) => {
      if (next !== "messages") setJump(null);
      onPane(next);
    },
    [onPane],
  );

  /** "View it" on the produce toast: the browser, seeked to that record. */
  const viewRecord = useCallback(
    (partition: number, offset: number) => {
      jumpNonce.current += 1;
      setJump({ partition, offset, nonce: jumpNonce.current });
      onPane("messages");
    },
    [onPane],
  );

  // The palette's contextual actions, reported up rather than reached down
  // for. Cleared on unmount, so a stale "Produce to orders.v2" can never
  // outlive the screen it belongs to.
  useEffect(() => {
    if (onActions === undefined) return;
    onActions(
      topic === null
        ? null
        : {
            topic,
            search: () => goPane("search"),
            produce: openProduce,
            produceBlocked: profile.read_only ? READ_ONLY_WHY : undefined,
          },
    );
    return () => onActions(null);
  }, [onActions, topic, goPane, openProduce, profile.read_only]);

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

  /**
   * The screen itself, as a value rather than a return.
   *
   * Three of the four branches used to `return` straight out, which left
   * nowhere to hang the two things that belong to the WHOLE tab: the produce
   * panel (openable from the topic detail and from the browser) and the toast
   * stack (raised by produce, by export, and by the browser). One stack, one
   * panel, four screens.
   */
  const body = ((): React.ReactNode => {
    // ── Schemas: a page of panels, not a full-height view ──────────────────
    // It is checked BEFORE the two full-height panes because it needs no
    // partition list — a subject can be read (and registered) whether or not
    // the topic's metadata came back.

    if (topic !== null && pane === "schemas") {
      return (
        <SchemasPanel
          key={`schemas:${topic}`}
          profile={profile}
          topic={topic}
          onBack={() => goPane("detail")}
          onDanger={onDanger}
          onEditConnection={onEditConnection}
        />
      );
    }

    // ── The message browser and search own the whole view when open ────────

    if (topic !== null && pane !== "detail") {
      if (detail === null) {
        // Both need the partition list before they can seek anywhere, so they
        // wait — and if the read failed it says so and offers a way out,
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
                    onClick={() => goPane("detail")}
                  >
                    Back to {topic}
                  </button>
                </div>
              </>
            )}
          </section>
        );
      }
      if (pane === "search") {
        return (
          <SearchView
            // Keyed by topic for the same reason the browser is: a query, its
            // results and its saved filters all belong to ONE topic.
            key={`search:${detail.name}`}
            profile={profile}
            topic={detail.name}
            partitions={detail.partitions}
            onBack={() => goPane("detail")}
            onBrowse={() => goPane("messages")}
            onDanger={onDanger}
            push={push}
          />
        );
      }
      return (
        <MessagesView
          // Keyed by topic: the browser's seek bar, buffer and tail all belong
          // to ONE topic, and carrying them across would show the previous
          // topic's rows under the new topic's name. The jump nonce is in the
          // key too, so "View it" on a produce toast lands on that record by
          // remounting the browser at it rather than by reaching into its
          // seek bar from outside.
          key={`browse:${detail.name}:${jump?.nonce ?? 0}`}
          profile={profile}
          topic={detail.name}
          partitions={detail.partitions}
          onBack={() => goPane("detail")}
          onDanger={onDanger}
          push={push}
          onSearch={() => goPane("search")}
          onProduce={openProduce}
          initialSeek={
            jump === null
              ? null
              : { partition: jump.partition, offset: jump.offset }
          }
        />
      );
    }

    // ── Topic detail ─────────────────────────────────────────────────────

    if (topic !== null) {
      const overrides = detail?.configs.filter((c) => !c.is_default) ?? [];
      const shownConfigs =
        detail === null ? [] : showDefaults ? detail.configs : overrides;
      const hiddenDefaults = (detail?.configs.length ?? 0) - overrides.length;
      // How many partitions are led by someone other than the first broker in
      // their replica list. It is the whole case for the election button, so
      // it is also what the button's count and its disabled reason say.
      const unpreferred =
        detail?.partitions.filter(
          (p) => p.replicas.length > 0 && p.replicas[0] !== p.leader,
        ).length ?? 0;

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
                  onClick={() => goPane("messages")}
                  disabled={detail === null}
                  title={
                    detail === null
                      ? "Kavka is still reading this topic's partitions"
                      : undefined
                  }
                >
                  Browse messages
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => goPane("search")}
                  disabled={detail === null}
                  title={
                    detail === null
                      ? "Kavka is still reading this topic's partitions"
                      : "Scan this topic for messages that match"
                  }
                >
                  Search
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => goPane("schemas")}
                  title="See the schemas registered for this topic, and register a new version"
                >
                  Schemas
                </button>
                {/* A write action renders danger-outlined on prod even when it
                    is routine (§6 layer 7). */}
                <button
                  type="button"
                  className={`btn ${isProd ? "btn-danger" : ""}`}
                  onClick={openProduce}
                  disabled={readOnly || detail === null}
                  title={
                    readOnly
                      ? READ_ONLY_WHY
                      : detail === null
                        ? "Kavka is still reading this topic's partitions"
                        : "Send a message to this topic"
                  }
                >
                  Produce
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
                  <span className="panel-count">
                    {detail.partitions.length}
                    {unpreferred > 0
                      ? ` · ${unpreferred} not on the preferred leader`
                      : ""}
                  </span>
                </h2>
                <div className="panel-tools">
                  {/* A write action renders danger-outlined on prod even when
                      it is routine (§6 layer 7). */}
                  <button
                    type="button"
                    className={`btn ${isProd ? "btn-danger" : ""}`}
                    disabled={readOnly || electBusy || unpreferred === 0}
                    aria-busy={electBusy || undefined}
                    title={
                      readOnly
                        ? READ_ONLY_WHY
                        : unpreferred === 0
                          ? "Every partition is already led by the first broker in its replica list, so there is nothing to move."
                          : `Hand leadership of ${unpreferred} partition${
                              unpreferred === 1 ? "" : "s"
                            } back to the first broker in its replica list`
                    }
                    onClick={() => askElection(null)}
                  >
                    <span className="btn-busy-slot" aria-hidden="true">
                      {electBusy ? <span className="spinner" /> : null}
                    </span>
                    Elect preferred leaders
                  </button>
                  <button
                    type="button"
                    className={`btn ${isProd ? "btn-danger" : ""}`}
                    disabled={readOnly || brokers.length === 0}
                    title={
                      readOnly
                        ? READ_ONLY_WHY
                        : brokers.length === 0
                          ? "Kavka has no broker list for this cluster, so it can't offer anywhere to move replicas to."
                          : "Choose which brokers hold each partition's copies"
                    }
                    onClick={() => setReassigning(detail.partitions)}
                  >
                    Move replicas
                  </button>
                </div>
              </div>

              <p className="table-note">
                The <strong>preferred leader</strong> is simply the first broker
                in a partition's replica list. Kafka spreads leadership evenly
                by giving every partition a different one, so after a broker
                restarts, the partitions it used to lead stay where they moved
                to until someone asks for them back — which is what electing
                does. It moves leadership only; no data is copied.
              </p>

              {/* The gutter carries the partition index — the row's address in
                  Kafka's own vocabulary. Static table, so no role="grid". */}
              <div className="table-wrap">
                {loadingDetail && (
                  <div className="table-loading" role="presentation" />
                )}
                <table className="data-table">
                  <caption className="sr-only">
                    Partitions of {detail.name}, with where each one's leader is
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
                      <th scope="col" className="col-affordance">
                        <span className="sr-only">Partition actions</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {detail.partitions.map((p) => (
                      <PartitionRow
                        key={p.partition}
                        partition={p}
                        readOnly={readOnly}
                        isProd={isProd}
                        busy={electBusy}
                        canReassign={brokers.length > 0}
                        onElect={() => askElection([p.partition])}
                        onReassign={() => setReassigning([p])}
                      />
                    ))}
                  </tbody>
                </table>
              </div>

              {opResult !== null && (
                <PartitionResultsNote
                  {...opResult}
                  onDismiss={() => setOpResult(null)}
                />
              )}
            </section>
          )}

          {/* Mounted whether or not this window started the move: a
              reassignment kicked off from a terminal shows up here within two
              seconds, and the panel renders nothing at all while the cluster
              is idle. */}
          {detail !== null && (
            <ReassignMonitor
              profile={profile}
              topic={detail.name}
              nonce={moveNonce}
              onDanger={onDanger}
              onSettled={movesSettled}
              push={push}
            />
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

          {/* Moving leadership is a write, not a delete: the red wire is
              reserved for prod, where §6 layer 7 says every write reads as
              one — so this is the plain tone with the prod gate on top. */}
          {electing !== null && detail !== null && (
            <ConfirmModal
              tone="plain"
              title={
                electing.partitions === null
                  ? isProd
                    ? `Move leadership of ${detail.name} on ${profile.name}?`
                    : `Move leadership for all ${detail.partitions.length} partitions?`
                  : `Move leadership of partition ${electing.partitions[0]}?`
              }
              body={
                <>
                  Kafka hands each partition back to the first broker in its
                  replica list, as long as that broker is in sync. No data is
                  copied and nothing is deleted — only which broker answers for
                  the partition changes.
                  {electing.partitions === null && (
                    <>
                      {" "}
                      {unpreferred} of {detail.partitions.length} partition
                      {detail.partitions.length === 1 ? "" : "s"}{" "}
                      {unpreferred === 1 ? "is" : "are"} somewhere else right
                      now; the rest are already where they should be and Kafka
                      will say so rather than move them.
                    </>
                  )}{" "}
                  Producers and consumers reconnect on their own, usually within
                  a second, and a producer may see a retry or two while that
                  happens.
                  {isProd && (
                    <>
                      {" "}
                      {profile.name} is a production cluster, so those retries
                      are live traffic.
                    </>
                  )}
                </>
              }
              confirmLabel="Move leadership"
              typeToConfirm={isProd ? detail.name : null}
              busy={electBusy}
              busyLabel="Kavka is asking the cluster to move leadership"
              onCancel={() => setElecting(null)}
              onConfirm={() => void runElection(electing.partitions)}
            />
          )}

          {reassigning !== null && detail !== null && (
            <ReassignModal
              profile={profile}
              topic={detail.name}
              partitions={reassigning}
              brokers={brokers}
              onClose={() => setReassigning(null)}
              onSubmitted={(results, specs) => {
                const split = splitResults(results);
                setReassigning(null);
                setOpResult({ split, okWord: "accepted for moving" });
                if (split.ok.length > 0) {
                  // The monitor is the only honest "done" — this call resolves
                  // when the PLAN is accepted, which is minutes to hours before
                  // the data has moved. Bumped only when something was actually
                  // accepted: a batch the cluster refused outright has nothing
                  // to watch, and a monitor announcing that it finished would
                  // be reporting a move that never started.
                  setMoveNonce((n) => n + 1);
                  push({
                    kind: "ok",
                    title: `The cluster accepted ${split.ok.length} of ${specs.length} replica move${
                      specs.length === 1 ? "" : "s"
                    }`,
                    detail:
                      "Copying starts now and runs in the background. Watch it under Replica moves — nothing has actually moved yet.",
                  });
                }
              }}
            />
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
            brokerCount={brokers.length}
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
  })();

  return (
    <>
      {body}

      {producing && topic !== null && (
        <ProducePanel
          profile={profile}
          topic={topic}
          // Without the detail read there is no partition list, so the panel
          // offers "let Kafka choose" and nothing else — which is honest
          // rather than a select full of guesses.
          partitions={detail?.partitions ?? []}
          push={push}
          onViewRecord={(partition, offset) => {
            setProducing(false);
            viewRecord(partition, offset);
          }}
          onClose={() => setProducing(false)}
        />
      )}

      {/* One stack for the whole tab: produce, export and the browser all
          raise into it, and a toast is a thing you did — it outlives the
          screen you did it on. */}
      <ToastStack {...toaster} />
    </>
  );
}

/**
 * One partition. Law 2 in its most load-bearing form: under-replication gets a
 * dot AND a word AND the numbers it was derived from, never a colour alone.
 *
 * Phase 3b gave it two operations, and one derived fact worth as much as
 * either of them: whether the partition is led by the FIRST broker in its
 * replica list. That is what "preferred leader" means, it is computable from
 * data already on screen, and saying it in the row is what stops the election
 * button being a mystery.
 */
function PartitionRow({
  partition,
  readOnly,
  isProd,
  busy,
  canReassign,
  onElect,
  onReassign,
}: {
  partition: PartitionDetail;
  readOnly: boolean;
  isProd: boolean;
  busy: boolean;
  canReassign: boolean;
  onElect: () => void;
  onReassign: () => void;
}) {
  const messages = Math.max(
    0,
    partition.latest_offset - partition.earliest_offset,
  );
  const missing = partition.replicas.length - partition.isr.length;
  const healthy = missing <= 0;
  const preferred = partition.replicas[0];
  const onPreferred = preferred === undefined || preferred === partition.leader;
  // Kafka refuses the election unless the preferred replica is in sync, and
  // saying so before the click is better than relaying the refusal after it.
  const preferredInSync =
    preferred !== undefined && partition.isr.includes(preferred);

  const electWhy = readOnly
    ? READ_ONLY_WHY
    : preferred === undefined
      ? "The cluster reported no replicas for this partition at all, so there is no preferred leader to move it to."
      : onPreferred
        ? `Broker ${partition.leader} is already the first broker in this partition's replica list, so there is nothing to move.`
        : !preferredInSync
          ? `Broker ${preferred} is the preferred leader but isn't in sync right now, and Kafka won't hand a partition to a replica that is behind. It becomes available once that broker catches up.`
          : `Hand this partition back to broker ${preferred}`;

  return (
    <tr>
      <td className="ledger-gutter">{partition.partition}</td>
      <td className="col-num cell-num">
        {partition.leader}
        {!onPreferred && (
          <span
            className="cell-tag"
            title={`This partition's replica list starts with broker ${preferred}, so that is where Kafka would rather it was led from. Leadership moved at some point — usually a broker restart — and stays moved until someone elects it back.`}
          >
            {" "}
            prefers {preferred}
          </span>
        )}
      </td>
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
      <td className="col-affordance partition-actions">
        <button
          type="button"
          className={`btn btn-row ${isProd ? "btn-danger" : ""}`}
          disabled={readOnly || busy || onPreferred || !preferredInSync}
          title={electWhy}
          onClick={onElect}
        >
          Elect
        </button>
        <button
          type="button"
          className={`btn btn-row ${isProd ? "btn-danger" : ""}`}
          disabled={readOnly || !canReassign}
          title={
            readOnly
              ? READ_ONLY_WHY
              : !canReassign
                ? "Kavka has no broker list for this cluster, so it can't offer anywhere to move this partition to."
                : "Choose which brokers hold this partition's copies"
          }
          onClick={onReassign}
        >
          Reassign
        </button>
      </td>
    </tr>
  );
}
