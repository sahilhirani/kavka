import { useCallback, useEffect, useRef, useState } from "react";
import type { ClusterOverview, ConnectionProfile } from "./api";
import type { DangerReport } from "./danger";
import GroupsTab from "./GroupsTab";
import { Term } from "./Glossary";
import { EnvChip } from "./Sidebar";
import { lsGet, lsSet } from "./storage";
import TopicsTab from "./TopicsTab";

/**
 * THE CLUSTER WORKSPACE.
 *
 * One connected cluster, three views behind DESIGN's 30px tab strip (§5.1):
 * Overview · Topics · Groups. The strip sits under the cluster's identity
 * rather than above it, because the identity — name, environment chip and
 * bootstrap address — is prod guardrail layer 3 and must not scroll or switch
 * away with the content.
 *
 * Where the user was is remembered PER CLUSTER, not globally: switching to a
 * prod cluster must never drop you into the view you had open on dev. The
 * selection also survives the remount App performs for "Refresh topics", which
 * is why it lives in localStorage rather than only in state.
 */

type TabKey = "overview" | "topics" | "groups";

const TABS: ReadonlyArray<{ key: TabKey; label: string }> = [
  { key: "overview", label: "Overview" },
  { key: "topics", label: "Topics" },
  { key: "groups", label: "Groups" },
];

interface Placement {
  tab: TabKey;
  topic: string | null;
  browsing: boolean;
  group: string | null;
}

const EMPTY_PLACEMENT: Placement = {
  tab: "overview",
  topic: null,
  browsing: false,
  group: null,
};

function placementKey(profileId: string): string {
  return `kavka.cluster.${profileId}.view`;
}

function readPlacement(profileId: string): Placement {
  const raw = lsGet(placementKey(profileId));
  if (raw === null) return EMPTY_PLACEMENT;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return EMPTY_PLACEMENT;
    const value = parsed as Partial<Record<keyof Placement, unknown>>;
    const tab = TABS.some((t) => t.key === value.tab)
      ? (value.tab as TabKey)
      : "overview";
    return {
      tab,
      topic: typeof value.topic === "string" ? value.topic : null,
      browsing: value.browsing === true,
      group: typeof value.group === "string" ? value.group : null,
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
}

export default function ClusterView({
  profile,
  overview,
  onDisconnect,
  onDangerChange,
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

  const goTab = useCallback((tab: TabKey) => {
    setPlace((prev) => ({ ...prev, tab }));
  }, []);

  const selectTopic = useCallback((topic: string | null) => {
    setPlace((prev) => ({
      ...prev,
      topic,
      // Leaving a topic always leaves its message browser too.
      browsing: topic === null ? false : prev.browsing,
    }));
  }, []);

  const setBrowsing = useCallback((browsing: boolean) => {
    setPlace((prev) => ({ ...prev, browsing }));
  }, []);

  const selectGroup = useCallback((group: string | null) => {
    setPlace((prev) => ({ ...prev, group }));
  }, []);

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

  // The message browser is the one view that owns the full height of the
  // workspace: it has its own scrollport, its own status line and a docked
  // inspector, none of which can live inside a page that scrolls as a whole.
  const full = place.tab === "topics" && place.topic !== null && place.browsing;

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
          <OverviewTab profile={profile} overview={overview} />
        )}

        {place.tab === "topics" && (
          <TopicsTab
            profile={profile}
            brokerCount={overview.brokers.length}
            topic={place.topic}
            browsing={place.browsing}
            onSelectTopic={selectTopic}
            onBrowse={setBrowsing}
            onDanger={reportDanger}
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
      </div>
    </div>
  );
}

function OverviewTab({
  profile,
  overview,
}: {
  profile: ConnectionProfile;
  overview: ClusterOverview;
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
    </>
  );
}
