import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import AclsTab from "./AclsTab";
import AlertsTab from "./AlertsTab";
import {
  alertsSubscribe,
  type AlertEvent,
  type ClusterOverview,
  type ConnectionProfile,
} from "./api";
import BrokersTab from "./BrokersTab";
import ConnectTab from "./ConnectTab";
import type { DangerReport } from "./danger";
import GroupsTab from "./GroupsTab";
import { Term } from "./Glossary";
import { ensureMaskRules } from "./masking";
import MaskingTab from "./MaskingTab";
import MonitoringTab from "./MonitoringTab";
import { formatDuration } from "./monitoring";
import Perch from "./Perch";
import QuorumPanel from "./QuorumPanel";
import { EnvChip } from "./Sidebar";
import StreamsTab from "./StreamsTab";
import { lsGet, lsSet } from "./storage";
import { useI18n, type MessageKey } from "./i18n";
import { ToastStack, useToasts } from "./Toast";
import TopicsTab, { type TopicActions, type TopicPane } from "./TopicsTab";

/**
 * THE CLUSTER WORKSPACE.
 *
 * One connected cluster, ten screens behind a GROUPED RAIL. Ledger showed the
 * ten as an undifferentiated strip of equal tabs, which is most of why the app
 * read as hard to follow: ten peers in a row tell you nothing about which one
 * answers the question you arrived with.
 *
 * Jackdaw names the groups after what their screens are ABOUT, so a user who
 * does not yet know what an ACL is can still find it under Safety:
 *
 *   Cluster       Home · Topics · Consumer groups · Brokers
 *   Observe       Monitoring · Alerts · Streams
 *   Safety        ACLs · Masking
 *   Integrations  Connect
 *
 * FULL COVERAGE IS A CONTRACT. Every screen reachable before this redesign is
 * reachable here. The Jackdaw mockup drew a four-item rail and left ACLs,
 * Connect, Masking and Streams with no home at all — that was an execution gap
 * in a static drawing, not the bet the direction is making.
 *
 * TabKey VALUES ARE PERSISTED and must not change. They are written into every
 * user's `kavka.cluster.<id>.view` record; renaming one silently moves people
 * off the screen they were last on. Only the presentation moved.
 *
 * NOT A TABLIST. A `role="tablist"` may not contain group headings, and the
 * headings are the entire point — so this is a `<nav>` whose current item
 * carries `aria-current="page"`. Arrow-key roving is a tablist affordance and
 * goes with it; Tab walks the rail, as it does in every other sidebar.
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
  | "connect"
  | "monitoring"
  | "alerts"
  | "masking"
  | "streams";

interface TabDef {
  key: TabKey;
  labelKey: MessageKey;
  icon: ReactNode;
}

interface RailGroup {
  id: string;
  labelKey: MessageKey;
  items: readonly TabDef[];
}

// One stroke weight, one 24-box, no fills — the rail is a list of words with
// a glyph in front of each, never a list of glyphs.
function icon(path: ReactNode): ReactNode {
  return (
    <svg
      className="crail-icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {path}
    </svg>
  );
}

const RAIL: readonly RailGroup[] = [
  {
    id: "cluster",
    labelKey: "rail.group.cluster",
    items: [
      {
        key: "overview",
        labelKey: "rail.item.overview",
        icon: icon(
          <>
            <path d="M4 11.2 12 4l8 7.2" />
            <path d="M6 10.4V20h12v-9.6" />
            <path d="M10 20v-5h4v5" />
          </>,
        ),
      },
      {
        key: "topics",
        labelKey: "rail.item.topics",
        icon: icon(
          <>
            <path d="M4 6h16M4 12h16M4 18h10" />
          </>,
        ),
      },
      {
        key: "groups",
        labelKey: "rail.item.groups",
        icon: icon(
          <>
            <circle cx="9" cy="8.5" r="3" />
            <path d="M3.5 19a5.5 5.5 0 0 1 11 0" />
            <path d="M16 6.2a3 3 0 0 1 0 5.6M17.5 19a5.5 5.5 0 0 0-2.4-4.5" />
          </>,
        ),
      },
      {
        key: "brokers",
        labelKey: "rail.item.brokers",
        icon: icon(
          <>
            <rect x="3" y="4" width="18" height="6" rx="2" />
            <rect x="3" y="14" width="18" height="6" rx="2" />
            <path d="M7 7h.01M7 17h.01" />
          </>,
        ),
      },
    ],
  },
  {
    id: "observe",
    labelKey: "rail.group.observe",
    items: [
      {
        key: "monitoring",
        labelKey: "rail.item.monitoring",
        icon: icon(<path d="M3 18l5-6 4 3 5-8 4 5" />),
      },
      {
        key: "alerts",
        labelKey: "rail.item.alerts",
        icon: icon(
          <>
            <path d="M18 9a6 6 0 1 0-12 0c0 5-2 6-2 6h16s-2-1-2-6" />
            <path d="M10.5 20a2 2 0 0 0 3 0" />
          </>,
        ),
      },
      {
        key: "streams",
        labelKey: "rail.item.streams",
        icon: icon(
          <>
            <path d="M3 7.5c3-2 6 2 9 0s6-2 9 0" />
            <path d="M3 12.5c3-2 6 2 9 0s6-2 9 0" />
            <path d="M3 17.5c3-2 6 2 9 0s6-2 9 0" />
          </>,
        ),
      },
    ],
  },
  {
    id: "safety",
    labelKey: "rail.group.safety",
    items: [
      {
        key: "acls",
        labelKey: "rail.item.acls",
        icon: icon(
          <>
            <path d="M12 3l7 3v6c0 4.2-2.9 7.7-7 9-4.1-1.3-7-4.8-7-9V6z" />
            <path d="M9.5 12l1.8 1.8L15 10" />
          </>,
        ),
      },
      // Beside ACLs on purpose: both answer "who can see what". Masking is
      // Kavka's own note about this connection and never touches the cluster,
      // which the screen itself says out loud.
      {
        key: "masking",
        labelKey: "rail.item.masking",
        icon: icon(
          <>
            <path d="M2.5 12S6 6 12 6s9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6z" />
            <circle cx="12" cy="12" r="2.6" />
            <path d="M4 20L20 4" />
          </>,
        ),
      },
    ],
  },
  {
    id: "integrations",
    labelKey: "rail.group.integrations",
    items: [
      {
        key: "connect",
        labelKey: "rail.item.connect",
        icon: icon(
          <>
            <path d="M9 15l-3 3a3.5 3.5 0 0 1-5-5l3-3" />
            <path d="M15 9l3-3a3.5 3.5 0 0 1 5 5l-3 3" />
            <path d="M9.5 14.5l5-5" />
          </>,
        ),
      },
    ],
  },
];

/** Every key the rail renders — the guard that keeps coverage a contract. */
const TABS: ReadonlyArray<{ key: TabKey }> = RAIL.flatMap((group) =>
  group.items.map((item) => ({ key: item.key })),
);

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
  /**
   * Which group the Streams tab is drawing. Separate from `group`, which is the
   * consumer-groups tab's selection: the two views ask different questions
   * about a group id, and coming back to Groups on the app the Streams tab
   * happened to be showing would move you somewhere you never went.
   */
  streamsGroup: string | null;
}

const EMPTY_PLACEMENT: Placement = {
  tab: "overview",
  topic: null,
  pane: "detail",
  group: null,
  broker: null,
  connect: null,
  connector: null,
  streamsGroup: null,
};

const PANES: readonly TopicPane[] = [
  "detail",
  "messages",
  "search",
  "schemas",
  "sql",
];

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
      // Phase 4, same rule again: a placement written by an older build has no
      // such key, which is the same as "nothing selected".
      streamsGroup:
        typeof value.streamsGroup === "string" ? value.streamsGroup : null,
    };
  } catch {
    return EMPTY_PLACEMENT;
  }
}

/**
 * Point another cluster's workspace at a topic BEFORE it is mounted.
 *
 * The copy wizard finishes on cluster A and offers to open the destination
 * topic on cluster B, and the shell answers that by selecting B — which mounts
 * a fresh ClusterView that reads its placement from storage on the way up.
 * So the jump is: write the placement, then select. The alternative was a prop
 * threaded from the app root into a component that may not exist yet, for a
 * value it would have to ignore on every other render.
 *
 * The placement format is this file's, which is why the writer is too: a caller
 * that hand-rolled the JSON would be the second place that has to change when
 * the shape does, and it is the one nobody would remember.
 */
export function stageTopic(profileId: string, topic: string): void {
  const current = readPlacement(profileId);
  lsSet(
    placementKey(profileId),
    JSON.stringify({ ...current, tab: "topics", topic, pane: "detail" }),
  );
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
  /**
   * Switch the whole workspace to another connection, landing on a topic —
   * the copy wizard's "browse the destination". Handled by the app root,
   * because selecting a cluster is the shell's job.
   */
  onOpenCluster?: (profileId: string, topic: string) => void;
}

export default function ClusterView({
  profile,
  overview,
  onDisconnect,
  onDangerChange,
  onTopicActions,
  onOpenCluster,
}: ClusterViewProps) {
  const { t } = useI18n();
  const [place, setPlace] = useState<Placement>(() => readPlacement(profile.id));
  const address = profile.bootstrap_servers.join(", ");

  useEffect(() => {
    lsSet(placementKey(profile.id), JSON.stringify(place));
  }, [profile.id, place]);

  // ── Alerts ──────────────────────────────────────────────────────────────
  //
  // THE SUBSCRIPTION LIVES HERE, NOT IN THE ALERTS TAB. An alert that only
  // arrives while you happen to have the Alerts tab open is not an alert, it is
  // a page. This component is mounted for exactly as long as the cluster is
  // connected, which is exactly as long as the core is watching — so the two
  // start and stop together, and the toast reaches whichever view is on screen.
  const alertToaster = useToasts();
  const pushAlert = alertToaster.push;
  const [firing, setFiring] = useState<Set<string>>(new Set());
  // Bumped on every fire and resolve, so the Alerts tab's history reloads
  // without polling and without this component knowing what it renders.
  const [alertNonce, setAlertNonce] = useState(0);

  const onAlert = useCallback(
    (event: AlertEvent) => {
      setAlertNonce((n) => n + 1);
      setFiring((prev) => {
        const next = new Set(prev);
        if (event.resolved_ms === null) next.add(event.rule_id);
        else next.delete(event.rule_id);
        return next;
      });
      if (event.resolved_ms === null) {
        // §5.8: an error the user must act on is never a toast — but a firing
        // is not an error, it is something that just happened, and it also has
        // a permanent home in the alert log. It gets `danger` so it does not
        // auto-dismiss: a condition that appeared and vanished while nobody was
        // looking is the failure mode alerting exists to prevent.
        pushAlert({
          kind: "danger",
          title: event.rule_name,
          detail: event.detail,
        });
      } else {
        pushAlert({
          kind: "ok",
          title: `Resolved — ${event.rule_name}`,
          detail: `It lasted ${formatDuration(
            event.resolved_ms - event.fired_ms,
          )}.`,
        });
      }
    },
    [pushAlert],
  );

  useEffect(
    () => alertsSubscribe(profile.id, onAlert),
    [profile.id, onAlert],
  );

  // ── Masking ─────────────────────────────────────────────────────────────
  //
  // Read once here, for the same reason the alert subscription lives here: the
  // status bar has to be able to say "Masking on — 3 rules" from the moment a
  // cluster is on screen, not from the first time somebody opens the Masking
  // tab. `ensureMaskRules` is deduplicated per connection, so a remount (the
  // palette's "Refresh topics") costs nothing.
  useEffect(() => {
    ensureMaskRules(profile.id);
  }, [profile.id]);

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

  const selectStreamsGroup = useCallback((streamsGroup: string | null) => {
    setPlace((prev) => ({ ...prev, streamsGroup }));
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

  // The message browser and search are the two views that own the full height
  // of the workspace: each has its own scrollport, its own status line and a
  // docked inspector, none of which can live inside a page that scrolls as a
  // whole. The schemas pane is a normal page of panels, so it is NOT in this
  // list — a diff that has to fit the viewport is a diff nobody can read.
  const full =
    place.tab === "topics" &&
    place.topic !== null &&
    (place.pane === "messages" ||
      place.pane === "search" ||
      place.pane === "sql");

  const currentLabel = t(
    RAIL.flatMap((g) => g.items).find((i) => i.key === place.tab)?.labelKey ??
      "rail.item.overview",
  );

  return (
    <div className={`cluster-view${full ? " cluster-view-full" : ""}`}>
      <nav className="crail" aria-label={t("rail.label")}>
        {/* Prod guardrail layer 3 lives here now: the name, the environment
            chip and the bootstrap address are pinned beside every screen
            rather than above one of them. Most prod accidents are
            right-action-wrong-cluster. */}
        <div className="crail-id">
          <div className="crail-id-line">
            <h1 className="crail-name">{profile.name}</h1>
            <EnvChip env={profile.environment} />
          </div>
          <span className="crail-address" title={address}>
            {address}
          </span>
          {profile.read_only && (
            <span className="readonly-chip" title={t("app.readonlyTitle")}>
              {t("app.readonlyChip")}
            </span>
          )}
        </div>

        {RAIL.map((group) => (
          <div className="crail-group" key={group.id}>
            <h2 className="crail-label">{t(group.labelKey)}</h2>
            {group.items.map((item) => (
              <button
                key={item.key}
                type="button"
                className="crail-item"
                // Not aria-selected: this is navigation between screens, not
                // a tab in a tablist — see the header comment.
                aria-current={place.tab === item.key ? "page" : undefined}
                onClick={() => goTab(item.key)}
              >
                {item.icon}
                {t(item.labelKey)}
                {/* The alert counter. A number in the badge, the word in its
                    title AND in an sr-only span — never a bare coloured dot.
                    It is on the rail rather than the status bar because the
                    status bar belongs to the app shell, not to one cluster. */}
                {item.key === "alerts" && firing.size > 0 && (
                  <span
                    className="crail-badge"
                    title={t("rail.firingTitle", { count: firing.size })}
                  >
                    {firing.size}
                    <span className="sr-only"> {t("rail.firing")}</span>
                  </span>
                )}
              </button>
            ))}
          </div>
        ))}

        <div className="crail-foot">
          <button
            type="button"
            className="btn"
            onClick={() => onDisconnect(profile.id)}
          >
            {t("rail.disconnect")}
          </button>
        </div>
      </nav>

      <div className="tabpanel" role="region" aria-label={currentLabel}>
        {place.tab === "overview" && (
          <OverviewTab
            profile={profile}
            overview={overview}
            firing={firing.size}
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
            onOpenCluster={onOpenCluster}
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

        {place.tab === "monitoring" && (
          <MonitoringTab
            profile={profile}
            onDanger={reportDanger}
            onEditConnection={editConnection}
          />
        )}

        {place.tab === "alerts" && (
          <AlertsTab
            profile={profile}
            onDanger={reportDanger}
            eventNonce={alertNonce}
          />
        )}

        {place.tab === "masking" && (
          <MaskingTab profile={profile} onDanger={reportDanger} />
        )}

        {place.tab === "streams" && (
          <StreamsTab
            profile={profile}
            group={place.streamsGroup}
            onSelectGroup={selectStreamsGroup}
            onDanger={reportDanger}
          />
        )}
      </div>

      {/* Alert toasts belong to the whole workspace, not to the Alerts tab —
          see the subscription above. */}
      <ToastStack {...alertToaster} />
    </div>
  );
}

/**
 * THE REFERENCE PERCH.
 *
 * Every other screen's verdict is written against this one, so it is worth
 * reading as a specimen rather than as a paragraph of markup. Three things
 * make it honest:
 *
 * · It is derived from LIVE state — the broker list the cluster actually
 *   answered with, and the set of alert rules firing right now. No constant,
 *   no "looks good" that is true by construction.
 * · Its worst case is a real case. A cluster that connects and reports zero
 *   brokers is a real failure mode of a load balancer in front of Kafka, and
 *   the verdict says so instead of rendering a cheerful "0 brokers".
 * · It carries a caveat it would be easy to omit: these counts came back at
 *   the moment of connection and do NOT track the cluster. A banner that let
 *   a user believe otherwise would be exactly the "cheerful verdict computed
 *   from stale data" §3 forbids.
 */
function OverviewPerch({
  overview,
  firing,
  screen,
}: {
  overview: ClusterOverview;
  firing: number;
  screen: string;
}) {
  const { t } = useI18n();
  const brokers = overview.brokers.length;

  if (brokers === 0) {
    return (
      <Perch screen={screen} tone="problem" caveat={t("perch.overview.noBrokers.next")}>
        {t("perch.overview.noBrokers")}
      </Perch>
    );
  }

  const counts = t("perch.overview.counts", {
    brokers,
    topics: overview.topic_count,
    partitions: overview.partition_count,
  });

  if (firing > 0) {
    return (
      <Perch screen={screen} tone="watch" caveat={t("perch.overview.snapshot")}>
        {t("perch.overview.firing", { count: firing, counts })}
      </Perch>
    );
  }

  return (
    <Perch screen={screen} tone="ok" caveat={t("perch.overview.snapshot")}>
      {counts}
    </Perch>
  );
}

function OverviewTab({
  profile,
  overview,
  firing,
  onDanger,
}: {
  profile: ConnectionProfile;
  overview: ClusterOverview;
  /** How many alert rules are firing right now — live, from the subscription
      in the parent. The Perch says so. */
  firing: number;
  /** The quorum panel can raise a banner, and any danger has to reach the
      app root or a prod cluster paints a coral rule behind it (§5.8). */
  onDanger: DangerReport;
}) {
  const { t } = useI18n();
  return (
    <>
      <OverviewPerch
        overview={overview}
        firing={firing}
        screen={t("rail.item.overview")}
      />

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
