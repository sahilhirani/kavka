import { useCallback, useEffect, useRef, useState } from "react";
import AclsTab from "./AclsTab";
import type { ClusterOverview, ConnectionProfile } from "./api";
import BrokersTab from "./BrokersTab";
import ConnectTab from "./ConnectTab";
import type { DangerReport } from "./danger";
import GroupsTab from "./GroupsTab";
import { Term } from "./Glossary";
import QuorumPanel from "./QuorumPanel";
import { EnvChip } from "./Sidebar";
import { lsGet, lsSet } from "./storage";
import TopicsTab, { type TopicActions, type TopicPane } from "./TopicsTab";

/**
 * THE CLUSTER WORKSPACE.
 *
 * One connected cluster, six views behind DESIGN's 30px tab strip (§5.1):
 * Overview · Topics · Groups · ACLs · Brokers · Connect. The strip sits under
 * the cluster's identity rather than above it, because the identity — name,
 * environment chip and bootstrap address — is prod guardrail layer 3 and must
 * not scroll or switch away with the content.
 *
 * Where the user was is remembered PER CLUSTER, not globally: switching to a
 * prod cluster must never drop you into the view you had open on dev. The
 * selection also survives the remount App performs for "Refresh topics", which
 * is why it lives in localStorage rather than only in state.
 */

type TabKey =
  | "overview"
  | "topics"
  | "groups"
  | "acls"
  | "brokers"
  | "connect";

const TABS: ReadonlyArray<{ key: TabKey; label: string }> = [
  { key: "overview", label: "Overview" },
  { key: "topics", label: "Topics" },
  { key: "groups", label: "Groups" },
  { key: "acls", label: "ACLs" },
  { key: "brokers", label: "Brokers" },
  { key: "connect", label: "Connect" },
];

interface Placement {
  tab: TabKey;
  topic: string | null;
  pane: TopicPane;
  group: string | null;
  /** Which broker's settings are open, if any. */
  broker: number | null;
  /** Which Connect cluster is selected, and which connector inside it. */
  connect: string | null;
  connector: string | null;
}

const EMPTY_PLACEMENT: Placement = {
  tab: "overview",
  topic: null,
  pane: "detail",
  group: null,
  broker: null,
  connect: null,
  connector: null,
};

const PANES: readonly TopicPane[] = ["detail", "messages", "search", "schemas"];

function placementKey(profileId: string): string {
  return `kavka.cluster.${profileId}.view`;
}

/**
 * MIGRATION. Phase 1 persisted `browsing: boolean`; Phase 2 needs three
 * states, so the record now carries `pane`. A stored Phase 1 placement is read
 * through its old field rather than discarded — the whole point of persisting
 * where you were is that an upgrade does not move you.
 */
function readPlacement(profileId: string): Placement {
  const raw = lsGet(placementKey(profileId));
  if (raw === null) return EMPTY_PLACEMENT;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return EMPTY_PLACEMENT;
    const value = parsed as Partial<Record<string, unknown>>;
    const tab = TABS.some((t) => t.key === value.tab)
      ? (value.tab as TabKey)
      : "overview";
    const pane = PANES.includes(value.pane as TopicPane)
      ? (value.pane as TopicPane)
      : value.browsing === true
        ? "messages"
        : "detail";
    return {
      tab,
      topic: typeof value.topic === "string" ? value.topic : null,
      pane,
      group: typeof value.group === "string" ? value.group : null,
      // Phase 3a fields. A placement written by an older build simply has
      // none of them, which is the same as "nothing selected" — the whole
      // point of persisting where you were is that an upgrade does not move
      // you, and a missing key must never cost you the tab you were on.
      broker: typeof value.broker === "number" ? value.broker : null,
      connect: typeof value.connect === "string" ? value.connect : null,
      connector: typeof value.connector === "string" ? value.connector : null,
    };
  } catch {
    return EMPTY_PLACEMENT;
  }
}

interface ClusterViewProps {
  profile: ConnectionProfile;
  overview: ClusterOverview;
  onDisconnect: (profileId: string) => void;
  /**
   * §5.8 prod de-collision: any danger banner inside this view has to reach
   * the app root, or a prod cluster paints a coral rule behind a coral banner.
   */
  onDangerChange: (danger: boolean) => void;
  /**
   * The two contextual palette commands ("Search in x", "Produce to x").
   * They are reported UP, with their handlers, rather than the palette
   * reaching down into a view it knows nothing about — and they are cleared
   * whenever there is no topic on screen, so ⌘K never offers to search
   * something that isn't open.
   */
  onTopicActions?: (actions: TopicActions | null) => void;
}

export default function ClusterView({
  profile,
  overview,
  onDisconnect,
  onDangerChange,
  onTopicActions,
}: ClusterViewProps) {
  const [place, setPlace] = useState<Placement>(() => readPlacement(profile.id));
  const tabRefs = useRef<Partial<Record<TabKey, HTMLButtonElement | null>>>({});

  useEffect(() => {
    lsSet(placementKey(profile.id), JSON.stringify(place));
  }, [profile.id, place]);

  // The danger collector. A count, not a boolean — see danger.ts: child
  // effects run before parent effects, so an outer component reporting "no
  // danger" would otherwise land last and switch the prod damper back off
  // while an inner banner is still up.
  const dangerSources = useRef<Set<string>>(new Set());
  const reportDanger = useCallback<DangerReport>(
    (source, danger) => {
      const set = dangerSources.current;
      if (danger) set.add(source);
      else set.delete(source);
      onDangerChange(set.size > 0);
    },
    [onDangerChange],
  );

  // A view that unmounts with an error on screen must not leave the whole app
  // dampened. Belt and braces: the children clear their own source too.
  useEffect(() => () => onDangerChange(false), [onDangerChange]);

  // Same shape, same reason: a palette that still offers "Produce to
  // orders.v2" after the cluster was disconnected is offering a dead command.
  useEffect(() => () => onTopicActions?.(null), [onTopicActions]);

  const goTab = useCallback((tab: TabKey) => {
    setPlace((prev) => ({ ...prev, tab }));
  }, []);

  const selectTopic = useCallback((topic: string | null) => {
    setPlace((prev) => ({
      ...prev,
      topic,
      // Leaving a topic always leaves its browser and its search too.
      pane: topic === null ? "detail" : prev.pane,
    }));
  }, []);

  const setPane = useCallback((pane: TopicPane) => {
    setPlace((prev) => ({ ...prev, pane }));
  }, []);

  const selectGroup = useCallback((group: string | null) => {
    setPlace((prev) => ({ ...prev, group }));
  }, []);

  const selectBroker = useCallback((broker: number | null) => {
    setPlace((prev) => ({ ...prev, broker }));
  }, []);

  const selectConnectCluster = useCallback((connect: string) => {
    // Leaving a Connect cluster always leaves the connector inside it: a
    // connector name means nothing on a different set of workers.
    setPlace((prev) => ({ ...prev, connect, connector: null }));
  }, []);

  const selectConnector = useCallback((connector: string | null) => {
    setPlace((prev) => ({ ...prev, connector }));
  }, []);

  /**
   * "Open connection settings" from a view that needs a field the profile
   * doesn't have yet (a Connect cluster, a registry address). The editor only
   * exists for a DISCONNECTED profile, so this is a disconnect — said out loud
   * at both call sites rather than performed as a surprise.
   */
  const editConnection = useCallback(() => {
    onDisconnect(profile.id);
  }, [onDisconnect, profile.id]);

  /** Arrow keys walk the strip — a tablist that only responds to clicks is a
      row of buttons wearing a costume. */
  const onTabKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
      let next: number | null = null;
      if (e.key === "ArrowRight") next = (index + 1) % TABS.length;
      else if (e.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = TABS.length - 1;
      if (next === null) return;
      e.preventDefault();
      const target = TABS[next];
      goTab(target.key);
      tabRefs.current[target.key]?.focus();
    },
    [goTab],
  );

  // The message browser and search are the two views that own the full height
  // of the workspace: each has its own scrollport, its own status line and a
  // docked inspector, none of which can live inside a page that scrolls as a
  // whole. The schemas pane is a normal page of panels, so it is NOT in this
  // list — a diff that has to fit the viewport is a diff nobody can read.
  const full =
    place.tab === "topics" &&
    place.topic !== null &&
    (place.pane === "messages" || place.pane === "search");

  return (
    <div className={`cluster-view${full ? " cluster-view-full" : ""}`}>
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
      </header>

      <div className="tabstrip" role="tablist" aria-label="Cluster views">
        {TABS.map((tab, index) => (
          <button
            key={tab.key}
            type="button"
            role="tab"
            id={`clustertab-${tab.key}`}
            aria-selected={place.tab === tab.key}
            aria-controls={`clusterpanel-${tab.key}`}
            tabIndex={place.tab === tab.key ? 0 : -1}
            ref={(el) => {
              tabRefs.current[tab.key] = el;
            }}
            className={`tab${place.tab === tab.key ? " tab-active" : ""}`}
            onClick={() => goTab(tab.key)}
            onKeyDown={(e) => onTabKeyDown(e, index)}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div
        className="tabpanel"
        role="tabpanel"
        id={`clusterpanel-${place.tab}`}
        aria-labelledby={`clustertab-${place.tab}`}
      >
        {place.tab === "overview" && (
          <OverviewTab
            profile={profile}
            overview={overview}
            onDanger={reportDanger}
          />
        )}

        {place.tab === "topics" && (
          <TopicsTab
            profile={profile}
            brokers={overview.brokers}
            topic={place.topic}
            pane={place.pane}
            onSelectTopic={selectTopic}
            onPane={setPane}
            onDanger={reportDanger}
            onActions={onTopicActions}
            onEditConnection={editConnection}
          />
        )}

        {place.tab === "groups" && (
          <GroupsTab
            profile={profile}
            group={place.group}
            onSelectGroup={selectGroup}
            onDanger={reportDanger}
          />
        )}

        {place.tab === "acls" && (
          <AclsTab profile={profile} onDanger={reportDanger} />
        )}

        {place.tab === "brokers" && (
          <BrokersTab
            profile={profile}
            brokers={overview.brokers}
            brokerId={place.broker}
            onSelectBroker={selectBroker}
            onDanger={reportDanger}
          />
        )}

        {place.tab === "connect" && (
          <ConnectTab
            profile={profile}
            cluster={place.connect}
            connector={place.connector}
            onSelectCluster={selectConnectCluster}
            onSelectConnector={selectConnector}
            onDanger={reportDanger}
            onEditConnection={editConnection}
          />
        )}
      </div>
    </div>
  );
}

function OverviewTab({
  profile,
  overview,
  onDanger,
}: {
  profile: ConnectionProfile;
  overview: ClusterOverview;
  /** The quorum panel can raise a banner, and any danger has to reach the
      app root or a prod cluster paints a coral rule behind it (§5.8). */
  onDanger: DangerReport;
}) {
  return (
    <>
      {profile.read_only && (
        <span className="readonly-note">
          This connection is read-only. Turn that off in the connection's
          settings to produce or edit.
        </span>
      )}

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
            are already complete and correct. The message browser's table IS
            virtualized and carries all three — see MessagesView. */}
        <div className="table-wrap">
          <table className="data-table">
            <caption className="sr-only">Brokers in this cluster</caption>
            <thead>
              <tr>
                <th scope="col" className="ledger-gutter">
                  ID
                </th>
                <th scope="col">Host</th>
                <th scope="col" className="col-num">
                  Port
                </th>
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

      {/* The quorum sits UNDER the broker list on purpose: a broker is the
          thing a user came looking for, and the quorum is the thing they need
          once the brokers all look fine and nothing works. It renders its own
          explanation of what a quorum is, and says so plainly on a cluster
          that has none. */}
      <QuorumPanel profile={profile} onDanger={onDanger} />
    </>
  );
}
