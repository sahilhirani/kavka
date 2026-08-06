import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import AclsTab from "./AclsTab";
import AlertsTab from "./AlertsTab";
import { alertToastAction, registerAlertNav } from "./alertNav";
import {
  alertsHistory,
  alertsSubscribe,
  type AlertEvent,
  type ClusterOverview,
  type ConnectionProfile,
} from "./api";
import BrokersTab from "./BrokersTab";
import ConnectTab from "./ConnectTab";
import type { DangerReport } from "./danger";
import GroupsTab from "./GroupsTab";
import HomeSections from "./HomeSections";
import { useEnvironment } from "./environments";
import { ensureMaskRules } from "./masking";
import MaskingTab from "./MaskingTab";
import MonitoringTab from "./MonitoringTab";
import { formatDuration } from "./monitoring";
import Perch from "./Perch";
import StageHead, { CLUSTER_HEADS } from "./StageHead";
import { useStageTop } from "./stage";
import StreamsTab from "./StreamsTab";
import { lsGet, lsSet } from "./storage";
import { useI18n, type MessageKey } from "./i18n";
import { ToastStack, useToasts } from "./Toast";
import TopicsTab, { type TopicActions, type TopicPane } from "./TopicsTab";

/**
 * THE CLUSTER WORKSPACE.
 *
 * One connected cluster, ten screens. Ledger showed the ten as an
 * undifferentiated strip of equal tabs, which is most of why the app read as
 * hard to follow: ten peers in a row tell you nothing about which one answers
 * the question you arrived with.
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
 * THE RAIL ITSELF NO LONGER LIVES HERE. `CLUSTER_RAIL` below is the data; the
 * markup is rendered by the app shell (`App.tsx`), because the shell is now one
 * 254px rail carrying brand, cluster card, Connections, these ten screens and
 * Settings — not a sidebar plus a second rail (DESIGN.md §5.1). So this
 * component is CONTROLLED: `tab` arrives as a prop from the shell, which reads
 * its first value out of `initialTab`.
 * It still owns everything BELOW the tab — which topic, which pane, which
 * broker — and it still owns the persistence of all of it.
 *
 * TabKey VALUES ARE PERSISTED and must not change. They are written into every
 * user's `kavka.cluster.<id>.view` record; renaming one silently moves people
 * off the screen they were last on. Only the presentation moved.
 *
 * NOT A TABLIST. A `role="tablist"` may not contain group headings, and the
 * headings are the entire point — so the rail is a `<nav>` whose current item
 * carries `aria-current="page"`. Arrow-key roving is a tablist affordance and
 * goes with it; Tab walks the rail, as it does in every other sidebar.
 *
 * Where the user was is remembered PER CLUSTER, not globally: switching to a
 * prod cluster must never drop you into the view you had open on dev. The
 * selection also survives the remount App performs for "Refresh topics", which
 * is why it lives in localStorage rather than only in state.
 *
 * BUT A RAIL CLICK ALWAYS OPENS THE SECTION'S ROOT. Restoring a placement is a
 * promise about RECONNECTING — come back tomorrow and you are where you left
 * off. It is not a promise about clicking "Topics", which means "show me the
 * topics", not "show me the message browser I had open on one of them three
 * screens ago". See the `lastNav` effect, and note that it counts PRESSES
 * (`navNonce`) rather than tab changes: pressing the item you are already on
 * is the clearest "take me back to the list" there is.
 */

export type TabKey =
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

export interface TabDef {
  key: TabKey;
  labelKey: MessageKey;
  icon: ReactNode;
}

export interface RailGroup {
  id: string;
  labelKey: MessageKey;
  items: readonly TabDef[];
}

// One stroke weight, one 24-box, no fills — the rail is a list of words with
// a glyph in front of each, never a list of glyphs.
export function railIcon(path: ReactNode): ReactNode {
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

export const CLUSTER_RAIL: readonly RailGroup[] = [
  {
    id: "cluster",
    labelKey: "rail.group.cluster",
    items: [
      {
        key: "overview",
        labelKey: "rail.item.overview",
        icon: railIcon(
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
        icon: railIcon(
          <>
            <path d="M4 6h16M4 12h16M4 18h10" />
          </>,
        ),
      },
      {
        key: "groups",
        labelKey: "rail.item.groups",
        icon: railIcon(
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
        icon: railIcon(
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
        icon: railIcon(<path d="M3 18l5-6 4 3 5-8 4 5" />),
      },
      {
        key: "alerts",
        labelKey: "rail.item.alerts",
        icon: railIcon(
          <>
            <path d="M18 9a6 6 0 1 0-12 0c0 5-2 6-2 6h16s-2-1-2-6" />
            <path d="M10.5 20a2 2 0 0 0 3 0" />
          </>,
        ),
      },
      {
        key: "streams",
        labelKey: "rail.item.streams",
        icon: railIcon(
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
        icon: railIcon(
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
        icon: railIcon(
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
        icon: railIcon(
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
const TABS: ReadonlyArray<{ key: TabKey }> = CLUSTER_RAIL.flatMap((group) =>
  group.items.map((item) => ({ key: item.key })),
);

/** The label for a screen, for the shell's `aria-label` and the Perch. */
export function tabLabelKey(tab: TabKey): MessageKey {
  return (
    CLUSTER_RAIL.flatMap((g) => g.items).find((i) => i.key === tab)?.labelKey ??
    "rail.item.overview"
  );
}

/**
 * Which rail group a screen sits in — the third crumb in the stage head's
 * trail, and the one that teaches. "local · DEV · Safety · ACLs" says where
 * ACLs live in this app's vocabulary every time somebody opens them, which is
 * the whole argument for naming the groups after what they are ABOUT.
 */
export function tabGroupKey(tab: TabKey): MessageKey {
  return (
    CLUSTER_RAIL.find((g) => g.items.some((i) => i.key === tab))?.labelKey ??
    "rail.group.cluster"
  );
}

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
/**
 * Which screen this cluster was last on — read by the SHELL, which owns the
 * rail now and therefore owns the current tab.
 *
 * Exported rather than duplicated because the storage key and the record's
 * shape are this file's, and a second reader that hand-rolled the JSON would
 * be the one nobody remembers to change.
 */
export function initialTab(profileId: string): TabKey {
  return readPlacement(profileId).tab;
}

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
  /**
   * Which screen is on. Controlled by the shell, because the rail is shell —
   * see the header. Its first value comes from `initialTab`, so a reconnect
   * still lands where the user left off.
   */
  tab: TabKey;
  /**
   * BUMPED ON EVERY RAIL CLICK, including a click on the item already on.
   *
   * `tab` alone cannot express "the user asked for Topics again" — pressing
   * Topics while the message browser is open changes nothing about the tab,
   * and the drill-down would survive the one press that most plainly means
   * "take me back to the list". The counter is what makes that press a
   * navigation. It also feeds the stage's scroll-to-top, for the same reason.
   */
  navNonce: number;
  /**
   * How many alert rules are firing right now, reported UP so the shell's rail
   * can badge the Alerts item. The subscription stays here (see below): an
   * alert that only arrives while the Alerts tab is open is not an alert.
   */
  onFiringChange: (count: number) => void;
  /**
   * Open another rail item. The shell owns the rail, so this is the same
   * `goTab` the rail's own buttons press — which is exactly the point: the
   * attention rows on Home, and the alert toast's "View …" button, have to
   * land in the state a rail press produces, not in a private one.
   */
  onTab: (tab: TabKey) => void;
  /**
   * "Refresh" in Cluster home's stage actions (audit item 15, and the mockup).
   *
   * It is the shell's `topicsNonce` and nothing more — App remounts this view,
   * which re-reads everything this view reads live. It cannot re-read the
   * connect-time metadata snapshot (the broker list, the topic and partition
   * counts), and Home's own panel foot says so rather than this button
   * pretending otherwise.
   */
  onRefresh: () => void;
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
  tab,
  navNonce,
  onFiringChange,
  onTab,
  onRefresh,
  onDisconnect,
  onDangerChange,
  onTopicActions,
  onOpenCluster,
}: ClusterViewProps) {
  const { t } = useI18n();
  const [place, setPlace] = useState<Placement>(() => readPlacement(profile.id));

  /**
   * A RAIL CLICK ALWAYS OPENS THE SECTION'S ROOT.
   *
   * The first render adopts whatever the stored placement said, sub-selections
   * and all — that is the reconnect promise, and it is the only promise
   * restoring a placement makes. Every rail press after that means "show me
   * this section", not "show me the message browser I had open on one of its
   * topics three screens ago", so the drill-down is cleared. Clearing here
   * rather than in the shell keeps the shell ignorant of what a placement
   * contains.
   *
   * IT KEYS ON THE PRESS, NOT ON THE TAB. Watching `tab` alone missed the one
   * case users hit most: pressing "Topics" while already on Topics, which is
   * exactly the press that means "back to the list" and which changed no
   * state at all. `navNonce` makes the press itself the event.
   *
   * `connect` GOES WITH THE REST. It is the Connect screen's root selection —
   * which worker cluster — and the section's root is the list of them.
   *
   * ONE EXCEPTION, AND IT IS EXPLICIT. Something can ask for a specific thing
   * INSIDE the section on the way — the alert toast's "View group
   * demo-checkout" is the only caller today. It parks the selection in
   * `pendingGroup` and then presses the rail; this effect adopts it instead of
   * clearing, once, and forgets it. Without that hand-off the navigator would
   * `setPlace` and this effect would wipe it one tick later, which is the
   * failure that is not obvious from either side alone.
   */
  const pendingGroup = useRef<string | null>(null);
  const lastNav = useRef<string>(`${tab}:${navNonce}`);
  useEffect(() => {
    const token = `${tab}:${navNonce}`;
    if (lastNav.current === token) return;
    lastNav.current = token;
    // THE HAND-OFF IS READ HERE, NOT INSIDE THE UPDATER, AND THAT IS THE WHOLE
    // OF IT. React does not promise to call a `setState` updater at the moment
    // you hand it over — it is free to defer it to the render it schedules, and
    // it does exactly that whenever this component already has an update in
    // flight. The toast's button is precisely that case: it runs the navigation
    // and dismisses itself in one click, and the dismiss is a `setToasts` on
    // THIS component. So an updater that read `pendingGroup.current` would read
    // it one render later — after the line below had already cleared it — and
    // the group would arrive as null on the one path this hand-off exists for.
    // A ref that is consumed and cleared in the same breath has to be read in
    // the effect body, where the order is ours.
    const adopted = pendingGroup.current;
    pendingGroup.current = null;
    setPlace((prev) => ({
      ...prev,
      topic: null,
      pane: "detail",
      group: adopted,
      broker: null,
      connect: null,
      connector: null,
      streamsGroup: null,
    }));
  }, [tab, navNonce]);

  /**
   * THE STAGE GOES BACK TO THE TOP WHENEVER THE PLACEMENT MOVES.
   *
   * The shell already does this for the screen, the rail item and the cluster;
   * this half covers everything BELOW the rail — opening a topic, changing its
   * pane, selecting a group, a broker or a connector. It is the same mechanism
   * (see stage.ts), reached from the component that owns these values rather
   * than from ten screens each remembering to do it.
   */
  useStageTop(
    `${tab}:${navNonce}:${place.topic ?? ""}:${place.pane}:${place.group ?? ""}:${
      place.broker ?? ""
    }:${place.connect ?? ""}:${place.connector ?? ""}:${place.streamsGroup ?? ""}`,
  );

  useEffect(() => {
    // The tab is the shell's now, but the RECORD is still this file's: one
    // writer, one shape, one migration path.
    lsSet(placementKey(profile.id), JSON.stringify({ ...place, tab }));
  }, [profile.id, place, tab]);

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
          // The second button, beside Dismiss: it opens the thing the rule is
          // watching. `alertToastAction` returns undefined while no navigator
          // is registered, so a toast never carries a button that cannot go
          // anywhere. ONLY on the firing branch — a resolve is news, not a
          // thing to go and look at.
          action: alertToastAction(profile.id, event, t),
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
    // `t` is memoized per locale (see i18n/index.ts), so listing it here
    // re-binds the subscription when the language changes and never otherwise.
    [pushAlert, profile.id, t],
  );

  useEffect(
    () => alertsSubscribe(profile.id, onAlert),
    [profile.id, onAlert],
  );

  // Seed `firing` from the log at connect. The subscription only carries
  // TRANSITIONS, so a rule that started firing before this session — hours
  // ago, under a different window — would show a firing panel on the Alerts
  // screen while the badge, Home's attention list and the overview Perch all
  // said quiet. One fact, one source: the newest event per rule decides.
  // Union rather than replace: a fire that arrives while the read is in
  // flight must not be dropped; the next resolve event corrects the set.
  useEffect(() => {
    let stale = false;
    alertsHistory(profile.id, 100)
      .then((events) => {
        if (stale) return;
        const newest = new Map<string, boolean>();
        for (const e of events) {
          if (!newest.has(e.rule_id)) newest.set(e.rule_id, e.resolved_ms === null);
        }
        const seeded = [...newest].filter(([, f]) => f).map(([id]) => id);
        if (seeded.length === 0) return;
        setFiring((prev) => new Set([...prev, ...seeded]));
      })
      // A log that cannot be read is AlertsTab's fact to report, with its
      // unread sentence; the badge stays quiet rather than guessing.
      .catch(() => {});
    return () => {
      stale = true;
    };
  }, [profile.id]);

  /**
   * HOW AN ALERT GETS YOU TO WHAT IT IS ABOUT.
   *
   * The registry is a module (alertNav.ts) because the toast can be read from
   * any of the ten screens while the destination is owned half by the shell
   * and half by the placement record here. This effect is the one place both
   * halves are in scope: `onTab` is the shell's rail press, `pendingGroup` is
   * this file's "…and select this on the way".
   */
  useEffect(
    () =>
      registerAlertNav(profile.id, (target) => {
        if (target.screen === "groups") pendingGroup.current = target.group;
        onTab(target.screen);
      }),
    [profile.id, onTab],
  );

  // The rail's Alerts badge belongs to the shell now, so the count is reported
  // up rather than read down. Cleared on unmount: a badge that survives a
  // disconnect is a badge counting a cluster nobody is watching.
  useEffect(() => {
    onFiringChange(firing.size);
  }, [firing, onFiringChange]);
  useEffect(() => () => onFiringChange(0), [onFiringChange]);

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
    tab === "topics" &&
    place.topic !== null &&
    (place.pane === "messages" ||
      place.pane === "search" ||
      place.pane === "sql");

  const currentLabel = t(tabLabelKey(tab));

  /**
   * THE STAGE HEAD — where you are, what this is, what you can do.
   *
   * Drawn here, from the registry in StageHead.tsx, so no screen can ship
   * headless: `CLUSTER_HEADS` is a `Record<TabKey, …>`, so a new rail item
   * without a title and a sentence is a compile error. A screen that has grown
   * its own head — with the chips and actions only it can bind — sets
   * `ownHead` in that table and takes over; until then this is the default and
   * it is a real one, not a placeholder.
   *
   * NOT ON THE THREE FULL-HEIGHT PANES. The message browser, search and SQL
   * own the whole stage: their scrollport, their status line, their docked
   * inspector. A head above them would take that height from the table and
   * duplicate the `← orders` breadcrumb those panes already draw. They are the
   * first screens that should own their heads outright — the trail
   * "local · DEV · Topics · orders · Messages" is exactly what repairs the
   * rail-says-Topics-while-you-read-messages confusion — and that is a change
   * inside those components, not a default the shell can guess.
   */
  const head = CLUSTER_HEADS[tab];
  const env = useEnvironment(profile.environment);
  // The trail mirrors the RAIL — cluster, then the group, then the item as the
  // rail spells it — so it reads as a route back to where you pressed rather
  // than as a restatement of the title under it. That is also why the last
  // crumb is the rail's word and not the head's: "Cluster · Home" over
  // "Cluster home" says something; "Cluster · Cluster home" says it twice.
  const trail = [
    profile.name,
    // Uppercased, never translated — the same string as the chip beside it and
    // the window title (§6, §10).
    env.name.toUpperCase(),
    t(tabGroupKey(tab)),
    currentLabel,
  ];

  return (
    <div className={`cluster-view${full ? " cluster-view-full" : ""}`}>
      {!full && head.ownHead !== true && (
        <StageHead
          trail={trail}
          title={t(head.titleKey)}
          sub={t(head.subKey)}
          actions={
            // Disconnect had exactly one home — the trailing action on this
            // cluster's row in the switcher menu — which made it discoverable
            // only by opening a menu about a different cluster. The mockup
            // puts it in Cluster home's actions, beside Refresh, and so does
            // this. Every other screen's actions arrive with the screen.
            //
            // REFRESH IS FIRST AND IT IS NOT PRIMARY. It re-reads what this
            // screen reads live; the connect-time snapshot behind the tiles
            // and the broker list is not among them, and Home's own panel
            // feet say which is which. Its title says it too, so the promise
            // is on the control rather than only under the table.
            tab === "overview" ? (
              <>
                <button
                  type="button"
                  className="btn"
                  onClick={onRefresh}
                  title={t("stage.overview.refresh.title")}
                >
                  {t("stage.overview.refresh")}
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => onDisconnect(profile.id)}
                >
                  {t("switcher.disconnect")}
                </button>
              </>
            ) : undefined
          }
        />
      )}

      <div className="tabpanel" role="region" aria-label={currentLabel}>
        {tab === "overview" && (
          <OverviewTab
            profile={profile}
            overview={overview}
            firing={firing.size}
            onDanger={reportDanger}
            onOpenScreen={onTab}
          />
        )}

        {tab === "topics" && (
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

        {tab === "groups" && (
          <GroupsTab
            profile={profile}
            group={place.group}
            onSelectGroup={selectGroup}
            onDanger={reportDanger}
          />
        )}

        {tab === "acls" && (
          <AclsTab profile={profile} onDanger={reportDanger} />
        )}

        {tab === "brokers" && (
          <BrokersTab
            profile={profile}
            brokers={overview.brokers}
            brokerId={place.broker}
            onSelectBroker={selectBroker}
            onDanger={reportDanger}
          />
        )}

        {tab === "connect" && (
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

        {tab === "monitoring" && (
          <MonitoringTab
            profile={profile}
            onDanger={reportDanger}
            onEditConnection={editConnection}
          />
        )}

        {tab === "alerts" && (
          <AlertsTab
            profile={profile}
            onDanger={reportDanger}
            eventNonce={alertNonce}
          />
        )}

        {tab === "masking" && (
          <MaskingTab profile={profile} onDanger={reportDanger} />
        )}

        {tab === "streams" && (
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

/**
 * CLUSTER HOME.
 *
 * This function is now the Perch and the read-only note; everything below the
 * verdict is `HomeSections`. That is the audit's item 15: Home used to be a
 * floating row of four numbers, a broker table and a quorum panel — it
 * REPORTED and never TRIAGED — where the mockup has tiles that say what their
 * number is, a list of the specific things that are wrong with a deep link
 * beside each one, and the two tables side by side. The old markup is not
 * kept anywhere: two Homes is how the drift the audit found happened.
 */
function OverviewTab({
  profile,
  overview,
  firing,
  onDanger,
  onOpenScreen,
}: {
  profile: ConnectionProfile;
  overview: ClusterOverview;
  /** How many alert rules are firing right now — live, from the subscription
      in the parent. The Perch says so, and the triage list re-reads on it. */
  firing: number;
  /** The quorum panel can raise a banner, and any danger has to reach the
      app root or a prod cluster paints a coral rule behind it (§5.8). */
  onDanger: DangerReport;
  /** The rail press behind each attention row's button. */
  onOpenScreen: (tab: TabKey) => void;
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

      <HomeSections
        profile={profile}
        overview={overview}
        firing={firing}
        onDanger={onDanger}
        onOpenScreen={onOpenScreen}
      />
    </>
  );
}
