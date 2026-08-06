import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  clusterConnect,
  clusterDisconnect,
  coreVersion,
  errorMessage,
  profilesList,
  updatesCheck,
  type ConnectionProfile,
  type ConnState,
  type ConnStatus,
} from "./api";
import {
  LAUNCH_CHECK_DELAY_MS,
  dismissVersion,
  dueForCheck,
  getUpdatePrefs,
  isDismissed,
  noteChecked,
  type UpdateOffer,
} from "./updates";
import UpdateBanner from "./UpdateBanner";
import {
  EnvChip,
  envAttrs,
  envWireLabel,
  loadEnvironments,
  useEnvironment,
} from "./environments";
import { maskingChipLabel, maskingChipTitle, useMasking } from "./masking";
import ClusterSwitcher from "./ClusterSwitcher";
import ProfileEditor, { ErrorBanner } from "./ProfileEditor";
import ClusterView, {
  CLUSTER_RAIL,
  initialTab,
  railIcon,
  stageTopic,
  type TabKey,
} from "./ClusterView";
import Palette, {
  paletteKeyLabel,
  type PaletteAction,
  type PaletteCommands,
} from "./Palette";
import AboutDialog from "./AboutDialog";
import ImportExportDialog, { type TransferTab } from "./ImportExportDialog";
import Playground from "./Playground";
import SettingsView from "./SettingsView";
import { registerStage, useStageTop } from "./stage";
import { useWindowTitle } from "./windowTitle";
import { useI18n, type MessageKey } from "./i18n";
import type { TopicActions } from "./TopicsTab";

const SELECTED_KEY = "kavka.selectedProfileId";

// localStorage can throw (disabled storage, quota); selection persistence is
// best-effort.
function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function lsSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* best-effort */
  }
}
function lsRemove(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {
    /* best-effort */
  }
}

const STATUS_KEY: Record<ConnStatus, MessageKey> = {
  disconnected: "app.status.disconnected",
  connecting: "app.status.connecting",
  connected: "app.status.connected",
};

/**
 * WHICH SURFACE THE STAGE IS SHOWING.
 *
 * Three, and only three. They are the rail's own vocabulary: "Connections" is
 * the Set up group, the ten cluster screens are the cluster groups, and
 * "Settings" is the Application group. Anything the app can put on screen is
 * one of these three plus a state the surface is in — a first run with no
 * connections is `connections` with nothing saved, not a fourth screen.
 */
type Screen = "connections" | "cluster" | "settings";

/**
 * THE BRAND LOCKUP'S BIRD.
 *
 * The same path `Perch` draws, at 26px in brass, as the rail's first child.
 * Duplicated rather than shared because the two carry different `fill` for
 * the eye — the Perch's sits on the warm perch ground, this one on the rail —
 * and a component whose only prop is which background it is standing on is
 * a component that has to be read twice to be understood.
 */
function BrandBird() {
  return (
    <svg
      className="brand-bird"
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <path
        d="M20.5 5.4a4.6 4.6 0 0 0-4.6 4.6c0 1.3-.6 2.5-1.7 3.2L5.5 19h8.7c4.9 0 9-3.8 9.4-8.7l.1-1.2 3.8-1.7-3.6-1.1-.8-2.4-2.6 1.5z"
        fill="currentColor"
      />
      <circle cx="22.6" cy="8.3" r="1" fill="var(--bg-rail)" />
      <path
        d="M17.6 19v3.6M13.9 19v3.6"
        stroke="currentColor"
        strokeWidth="1.5"
        fill="none"
        strokeLinecap="round"
      />
      <path
        d="M4 23.6h24"
        stroke="currentColor"
        strokeWidth="1.5"
        opacity=".45"
        strokeLinecap="round"
      />
    </svg>
  );
}

/** The link glyph the Connections item carries — the same one Connect uses. */
const CONNECTIONS_ICON = railIcon(
  <>
    <path d="M9 15l-3 3a3.5 3.5 0 0 1-5-5l3-3" />
    <path d="M15 9l3-3a3.5 3.5 0 0 1 5 5l-3 3" />
    <path d="M9.5 14.5l5-5" />
  </>,
);

const SETTINGS_ICON = railIcon(
  <>
    <circle cx="12" cy="12" r="3.2" />
    <path d="M12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2L5.6 5.6" />
  </>,
);

/** The 15px shield the rail foot states read-only through. */
function ShieldIcon() {
  return (
    <svg
      className="rf-icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M12 3l7 3v6c0 4.2-2.9 7.7-7 9-4.1-1.3-7-4.8-7-9V6z" />
    </svg>
  );
}

function RailItem({
  icon,
  label,
  current,
  badge,
  badgeTitle,
  badgeWord,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  current: boolean;
  badge?: number;
  badgeTitle?: string;
  badgeWord?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="crail-item"
      // Not aria-selected: this is navigation between screens, not a tab in a
      // tablist — a tablist may not contain the group headings, and the
      // headings are the entire point (DESIGN.md §5.1).
      aria-current={current ? "page" : undefined}
      onClick={onClick}
    >
      {icon}
      {label}
      {/* The alert counter. A number in the badge, the word in its title AND
          in an sr-only span — never a bare coloured dot. */}
      {badge !== undefined && badge > 0 && (
        <span className="crail-badge" title={badgeTitle}>
          {badge}
          <span className="sr-only"> {badgeWord}</span>
        </span>
      )}
    </button>
  );
}

interface RailProps {
  version: string;
  profiles: ConnectionProfile[] | null;
  selectedId: string | null;
  connections: Record<string, ConnState>;
  selected: ConnectionProfile | null;
  conn: ConnState;
  /** True while an unsaved connection is being written. */
  creating: boolean;
  screen: Screen;
  tab: TabKey;
  /** How many alert rules are firing on the connected cluster, or 0. */
  firing: number;
  onScreen: (screen: Screen) => void;
  onTab: (tab: TabKey) => void;
  onSelect: (id: string) => void;
  onConnect: (profile: ConnectionProfile) => void;
  onDisconnect: (profileId: string) => void;
  onNew: () => void;
}

/**
 * THE RAIL — one navigator, 254px, for the whole window.
 *
 * The app used to draw two: a permanent 248px "Clusters" sidebar plus a 224px
 * cluster rail inside the workspace, so a connected user spent 472px on chrome
 * before any content. The Jackdaw mockup draws ONE rail and the fidelity audit
 * called the second one inherited-structure drift. This is the correction, and
 * DESIGN.md §5.1 now describes it rather than the thing it replaced.
 *
 * IT RENDERS IN TWO MODES and the difference is one block:
 *
 *   ALWAYS  brand lockup (bird · Kavka · version)
 *           cluster card (name · env · address · state · Switch cluster)
 *           Set up      → Connections
 *           Application → Settings
 *           rail foot   → the read-only readout, when a cluster is selected
 *
 *   PLUS, WHEN CONNECTED   the four cluster groups from `CLUSTER_RAIL`,
 *           inserted between Set up and Application.
 *
 * Everything in the ALWAYS half has to work with NOTHING connected — that is
 * the whole reason Settings is a rail item rather than a cluster screen. The
 * two preferences people want on first launch are the theme and the font size,
 * and on first launch there is no cluster.
 */
function Rail({
  version,
  profiles,
  selectedId,
  connections,
  selected,
  conn,
  creating,
  screen,
  tab,
  firing,
  onScreen,
  onTab,
  onSelect,
  onConnect,
  onDisconnect,
  onNew,
}: RailProps) {
  const { t, tx } = useI18n();
  const connected = conn.status === "connected";
  const address = selected?.bootstrap_servers.join(", ") ?? "";

  // Law 2 in the cluster card: the dot never carries the meaning alone. Every
  // status produces a sentence, and the connected one carries the broker count
  // because "Connected" with no number is a claim nobody can check.
  let stateLine: string;
  if (selected === null) {
    stateLine = creating ? t("switcher.draftMeta") : t("card.state.none");
  } else if (conn.status === "connected") {
    stateLine = t("card.state.connected", {
      count: conn.overview?.brokers.length ?? 0,
    });
  } else if (conn.status === "connecting") {
    stateLine = t("card.state.connecting");
  } else {
    stateLine = t("card.state.disconnected");
  }

  return (
    <nav className="rail" aria-label={t("rail.navLabel")}>
      {/* The identity, top-left, above everything — and the ONLY place the
          version is rendered outside the About dialog. It used to sit in the
          far corner of the status bar, which is for live operational state; a
          build number is an annotation on the name, so it reads once and
          recedes. */}
      <div className="brand">
        <BrandBird />
        <span className="brand-name">Kavka</span>
        <span
          className="brand-ver"
          title={t("brand.versionTitle", { version: version || "…" })}
        >
          {version || "…"}
        </span>
      </div>

      {/* Prod guardrail layer 3 lives here: the name, the environment chip and
          the bootstrap address are pinned beside every screen rather than above
          one of them. Most prod accidents are right-action-wrong-cluster. */}
      <div className="cluster-card">
        <div className="cc-top">
          <h1 className="cc-name">
            {selected?.name ??
              (creating ? t("switcher.draftName") : t("card.none"))}
          </h1>
          {selected !== null && <EnvChip env={selected.environment} />}
        </div>
        {address !== "" && (
          <div className="cc-addr" title={address}>
            {address}
          </div>
        )}
        <div className={`cc-state cc-state-${conn.status}`}>
          <span
            className={`status-dot status-${conn.status}`}
            aria-hidden="true"
          />
          {stateLine}
        </div>
        <ClusterSwitcher
          profiles={profiles}
          selectedId={selectedId}
          connections={connections}
          onSelect={onSelect}
          onConnect={onConnect}
          onDisconnect={onDisconnect}
          onNew={onNew}
        />
      </div>

      <div className="crail-group">
        <h2 className="crail-label">{t("rail.group.setup")}</h2>
        <RailItem
          icon={CONNECTIONS_ICON}
          label={t("rail.item.connections")}
          current={screen === "connections"}
          onClick={() => onScreen("connections")}
        />
      </div>

      {connected &&
        CLUSTER_RAIL.map((group) => (
          <div className="crail-group" key={group.id}>
            <h2 className="crail-label">{t(group.labelKey)}</h2>
            {group.items.map((item) => (
              <RailItem
                key={item.key}
                icon={item.icon}
                label={t(item.labelKey)}
                current={screen === "cluster" && tab === item.key}
                badge={item.key === "alerts" ? firing : undefined}
                badgeTitle={t("rail.firingTitle", { count: firing })}
                badgeWord={t("rail.firing")}
                onClick={() => onTab(item.key)}
              />
            ))}
          </div>
        ))}

      <div className="crail-group">
        <h2 className="crail-label">{t("rail.group.application")}</h2>
        <RailItem
          icon={SETTINGS_ICON}
          label={t("rail.item.settings")}
          current={screen === "settings"}
          onClick={() => onScreen("settings")}
        />
      </div>

      {/* THE RAIL FOOT — the app's only always-visible safety state.
          It states read-only in BOTH directions, with the consequence
          attached. The app used to speak only in the safe direction: a chip
          when read-only was ON and nothing at all when it was off, so a user
          who wanted to confirm that Kavka *cannot* delete here had nothing to
          read. A guardrail that is silent in its dangerous state is not a
          guardrail. Disconnect is not here — it is the trailing action on the
          cluster's own row in the switcher menu. */}
      {selected !== null && (
        <div className="crail-foot">
          <div className="rf-row">
            <ShieldIcon />
            <span>
              {tx("rail.readonly.label", {
                state: (
                  <strong className="rf-state">
                    {t(
                      selected.read_only
                        ? "rail.readonly.on"
                        : "rail.readonly.off",
                    )}
                  </strong>
                ),
              })}
            </span>
          </div>
          <p className="rf-why">
            {t(
              selected.read_only
                ? "rail.readonly.on.why"
                : "rail.readonly.off.why",
            )}
          </p>
        </div>
      )}
    </nav>
  );
}

export default function App() {
  const { t, tx } = useI18n();
  // null = still loading
  const [profiles, setProfiles] = useState<ConnectionProfile[] | null>(null);
  const [version, setVersion] = useState<string>("");
  const [selectedId, setSelectedId] = useState<string | null>(() =>
    lsGet(SELECTED_KEY),
  );
  const [creating, setCreating] = useState(false);
  const [connections, setConnections] = useState<Record<string, ConnState>>({});
  const [error, setError] = useState<string | null>(null);
  // True when the last profiles fetch failed — an empty-looking list must not
  // be trusted (e.g. to prune the persisted selection).
  const [loadFailed, setLoadFailed] = useState(false);
  // Overlays. At most one is up at a time: the palette opens the others and
  // closes itself on the way, and ⌘K is ignored while a dialog is up.
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [transfer, setTransfer] = useState<TransferTab | null>(null);
  /**
   * Which of the rail's three destinations the stage is showing.
   *
   * Settings is one of them rather than an overlay, and deliberately not a
   * dialog: it has to be reachable with nothing connected, it is where somebody
   * goes to make the app readable before they can read anything, and a modal
   * over an empty workspace is a modal over nothing. It does not clear the
   * selection — leaving it puts you back on the cluster you were already on.
   */
  const [screen, setScreen] = useState<Screen>("connections");
  // How many alert rules are firing on the connected cluster. Reported up by
  // ClusterView, because the rail that badges it is the shell's now.
  const [firing, setFiring] = useState(0);
  // A danger banner inside the transfer dialog is still danger on screen, and
  // §5.8's prod damper reads one attribute on the app root — so the dialog
  // reports its banner up here rather than the guardrail missing it.
  const [transferDanger, setTransferDanger] = useState(false);
  // Same reason for the cluster workspace: a fetch failure inside the topic,
  // group or message views raises its own inline banner, and §5.8 says the
  // damper reads ANY danger on screen, not only the global one.
  const [viewDanger, setViewDanger] = useState(false);
  // Bumping this remounts ClusterView, which refetches topics on mount. The
  // honest version of "Refresh topics" from the palette is a handle into that
  // component; until it exposes one, a remount is the whole of the behaviour
  // and none of the coupling. Swap it the day ClusterView owns a ref.
  const [topicsNonce, setTopicsNonce] = useState(0);
  // What the cluster workspace is showing, so ⌘K can act on it. Reported up
  // by ClusterView with its handlers already bound — the palette never
  // reaches down into a view (§5.9).
  const [topicActions, setTopicActions] = useState<TopicActions | null>(null);
  // The release Kavka is offering, or null. App-level rather than per-view
  // because it outlives every screen: it survives switching clusters, opening
  // Settings and disconnecting, and it goes away only when the user answers
  // it. Null while nothing is offered, which is almost always.
  const [update, setUpdate] = useState<UpdateOffer | null>(null);
  // Profile ids with a cluster_connect in flight (double-click guard).
  const connectsInFlight = useRef(new Set<string>());
  // Mirror of `profiles` for use after awaits without stale closures.
  const profilesRef = useRef<ConnectionProfile[] | null>(null);
  profilesRef.current = profiles;
  // Same, for connection state. It exists so `openCluster` can stay identity-
  // stable: it is handed down into the copy wizard, whose session effect must
  // not re-run — a torn-down copy effect stops a running copy and starts a
  // second one.
  const connectionsRef = useRef<Record<string, ConnState>>({});
  connectionsRef.current = connections;
  const selectedIdRef = useRef<string | null>(null);
  selectedIdRef.current = selectedId;

  const showError = useCallback((msg: string) => setError(msg), []);

  const reloadProfiles = useCallback(async (): Promise<
    ConnectionProfile[] | null
  > => {
    try {
      const list = await profilesList();
      setProfiles(list);
      setLoadFailed(false);
      return list;
    } catch (err) {
      setError(errorMessage(err));
      setLoadFailed(true);
      // Keep whatever we had; only fall to [] so the UI leaves "loading".
      setProfiles((prev) => prev ?? []);
      return null;
    }
  }, []);

  // Initial load: profiles + core version + the environment registry.
  //
  // The registry is loaded here rather than at module scope so nothing in the
  // bundle depends on the command being registered, and its failure is silent
  // by design: `environments.ts` keeps the three defaults, which is exactly
  // what the backend writes when `environments.json` is absent. A banner about
  // it would be a banner about a file the user has never heard of.
  useEffect(() => {
    void reloadProfiles();
    void loadEnvironments();
    coreVersion()
      .then(setVersion)
      .catch(() => setVersion("unknown"));
  }, [reloadProfiles]);

  /**
   * THE LAUNCH CHECK — the one request Kavka makes that nobody asked for.
   *
   * Every guard on it is deliberate and each one is a promise made somewhere
   * a user can read it (the README's *What Kavka sends*, the disclosure in
   * Settings → Updates, the About panel's amended no-telemetry paragraph):
   *
   *   · ~5 seconds after boot, not on mount. The first seconds belong to
   *     reading `profiles.json` and connecting to the cluster the user came
   *     back for. A release lookup is the least urgent thing this app does.
   *   · Only when the switch is on, read at fire time rather than at mount,
   *     so turning it off during those five seconds turns it off.
   *   · Only when a day has passed since the last ATTEMPT, which is a stamp
   *     on disk — forty relaunches in an afternoon is still one request.
   *   · Only raised if the user has not already waved this exact version away.
   *
   * A FAILED LAUNCH CHECK SAYS NOTHING. Not a banner, not a toast: the user
   * did not ask this question, so they are not owed an error about it, and a
   * strip across the workspace saying Kavka could not reach github.com is
   * noise on top of whatever they actually opened the app to do. The attempt
   * is recorded, the "Check now" button in Settings reports failures out
   * loud, and nothing anywhere pretends the app is up to date.
   */
  useEffect(() => {
    let alive = true;
    const timer = window.setTimeout(() => {
      const prefs = getUpdatePrefs();
      if (!prefs.auto || !dueForCheck()) return;
      void (async () => {
        try {
          const check = await updatesCheck(prefs.channel);
          noteChecked(check.status !== "error");
          if (!alive) return;
          if (check.status === "update" && !isDismissed(check.version)) {
            setUpdate({ ...check, channel: prefs.channel });
          }
        } catch {
          noteChecked(false);
        }
      })();
    }, LAUNCH_CHECK_DELAY_MS);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, []);

  const dismissUpdate = useCallback(() => {
    if (update === null) return;
    // Against the VERSION, not the session. A notice that comes back every
    // launch is a notice people learn to click through without reading.
    dismissVersion(update.version);
    setUpdate(null);
  }, [update]);

  // Drop a persisted selection that no longer exists — but never based on a
  // failed (spuriously empty) load.
  useEffect(() => {
    if (
      !loadFailed &&
      profiles &&
      selectedId &&
      !profiles.some((p) => p.id === selectedId)
    ) {
      setSelectedId(null);
      lsRemove(SELECTED_KEY);
    }
  }, [profiles, selectedId, loadFailed]);

  /**
   * Pick a cluster.
   *
   * It NEVER connects and it never detonates the workspace: choosing a cold
   * cluster used to swap the whole stage for a connection form, which is the
   * sharpest thing the fidelity audit found. Selection lands you on the cluster
   * if it is already up and on its Connections entry if it is not — and the
   * switcher menu's trailing Connect button is what actually dials.
   */
  const select = useCallback((id: string) => {
    setCreating(false);
    setSelectedId(id);
    lsSet(SELECTED_KEY, id);
    setScreen(
      connectionsRef.current[id]?.status === "connected"
        ? "cluster"
        : "connections",
    );
  }, []);

  const startCreating = useCallback(() => {
    setCreating(true);
    setScreen("connections");
  }, []);

  const stopCreating = useCallback(() => {
    setCreating(false);
  }, []);

  const setConn = useCallback((id: string, state: ConnState) => {
    setConnections((prev) => ({ ...prev, [id]: state }));
  }, []);

  const connect = useCallback(
    async (profile: ConnectionProfile) => {
      if (connectsInFlight.current.has(profile.id)) return;
      connectsInFlight.current.add(profile.id);
      setConn(profile.id, { status: "connecting" });
      try {
        const overview = await clusterConnect(profile.id);
        // The profile may have been deleted while we were connecting; don't
        // resurrect a connection entry for it.
        if (!profilesRef.current?.some((p) => p.id === profile.id)) return;
        setConn(profile.id, { status: "connected", overview });
        // A cluster that just came up is what the user asked to look at — but
        // only if it is the one on screen. Connecting a second cluster from the
        // switcher while reading a first must not yank the stage away.
        if (selectedIdRef.current === profile.id) setScreen("cluster");
      } catch (err) {
        const msg = errorMessage(err);
        if (profilesRef.current?.some((p) => p.id === profile.id)) {
          // Stored on the connection, NOT raised as a global banner: this
          // error belongs beside the form whose fields have to change, and
          // two copies of the same sentence is one copy too many. It clears
          // when the next attempt sets status back to "connecting".
          setConn(profile.id, { status: "disconnected", error: msg });
        }
      } finally {
        connectsInFlight.current.delete(profile.id);
      }
    },
    [setConn],
  );

  const disconnect = useCallback(
    async (profileId: string) => {
      try {
        await clusterDisconnect(profileId);
      } catch (err) {
        setError(errorMessage(err));
      }
      setConn(profileId, { status: "disconnected" });
      // The ten cluster screens leave the rail with the connection, so the
      // stage cannot stay on one of them. Connections is where you land — the
      // screen that can dial it again.
      if (selectedIdRef.current === profileId) setScreen("connections");
    },
    [setConn],
  );

  const handleSaved = useCallback(
    async (profile: ConnectionProfile) => {
      await reloadProfiles();
      setCreating(false);
      setSelectedId(profile.id);
      lsSet(SELECTED_KEY, profile.id);
    },
    [reloadProfiles],
  );

  const handleDeleted = useCallback(
    async (profileId: string) => {
      setConnections((prev) => {
        const next = { ...prev };
        delete next[profileId];
        return next;
      });
      setSelectedId(null);
      lsRemove(SELECTED_KEY);
      setScreen("connections");
      await reloadProfiles();
    },
    [reloadProfiles],
  );

  // ⌘K / Ctrl+K — the app's real navigation (DESIGN.md §5.9). Ignored while a
  // dialog is up: a palette on top of a modal is two focus traps fighting.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      if (e.key !== "k" && e.key !== "K") return;
      e.preventDefault();
      if (aboutOpen || transfer !== null) return;
      setPaletteOpen((open) => !open);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [aboutOpen, transfer]);

  /**
   * "Browse the destination" after a cross-cluster copy.
   *
   * Three things have to happen and the order matters: the target cluster's
   * placement is staged FIRST (a ClusterView reads it on the way up, so
   * writing it afterwards would land the user wherever they were last time),
   * then the selection moves, then the connection is opened if it is not
   * already. Reconnecting a cluster that is already up would flip its status
   * to "connecting" and swap the workspace for the connection form — the user
   * asked to look at a topic, not to be logged out of one.
   *
   * Same cluster is not a no-op: the workspace is already mounted, so nothing
   * would read the staged placement. The nonce remounts it, which is exactly
   * what "Refresh topics" does and for the same reason.
   */
  const openCluster = useCallback(
    (profileId: string, topic: string) => {
      stageTopic(profileId, topic);
      const target = profilesRef.current?.find((p) => p.id === profileId) ?? null;
      if (target === null) {
        setError(t("app.error.unknownProfile"));
        return;
      }
      const already = connectionsRef.current[profileId]?.status === "connected";
      if (selectedIdRef.current === profileId) setTopicsNonce((n) => n + 1);
      setCreating(false);
      setSelectedId(profileId);
      lsSet(SELECTED_KEY, profileId);
      // Straight onto the cluster when it is already up; `connect` moves the
      // stage there itself when it is not, so a failed dial leaves the user on
      // the form that has to change rather than on an empty cluster screen.
      if (already) setScreen("cluster");
      else void connect(target);
    },
    [connect, t],
  );

  /**
   * The playground finished and there is a connection for it.
   *
   * The profile was written by the shell, so the list has to be re-read before
   * anything selects it — `connect` looks the profile up in `profilesRef` and
   * would find nothing. Connecting is left to the same path every other
   * connection takes, with the same status dot: a panel that opened a cluster
   * by itself would be teaching that connections happen unasked.
   */
  const openPlayground = useCallback(
    async (profileId: string) => {
      const list = await reloadProfiles();
      const target =
        (list ?? profilesRef.current)?.find((p) => p.id === profileId) ?? null;
      setCreating(false);
      setSelectedId(profileId);
      lsSet(SELECTED_KEY, profileId);
      if (
        target !== null &&
        connectionsRef.current[profileId]?.status !== "connected"
      ) {
        void connect(target);
      }
    },
    [reloadProfiles, connect],
  );

  const openAbout = useCallback(() => setAboutOpen(true), []);
  const closeAbout = useCallback(() => setAboutOpen(false), []);
  const closeTransfer = useCallback(() => setTransfer(null), []);
  const closePalette = useCallback(() => setPaletteOpen(false), []);

  const paletteCommands = useMemo<PaletteCommands>(
    () => ({
      // Connecting from the palette also selects: the workspace has to show
      // the cluster you just asked for, whichever one was on screen before.
      connect: (profile) => {
        select(profile.id);
        void connect(profile);
      },
      goTo: select,
      addConnection: startCreating,
      disconnect: (profileId) => void disconnect(profileId),
      refreshTopics: () => setTopicsNonce((n) => n + 1),
      exportConnections: () => setTransfer("export"),
      importConnections: () => setTransfer("import"),
      about: openAbout,
    }),
    [select, connect, startCreating, disconnect, openAbout],
  );

  const selected =
    !creating && profiles !== null
      ? (profiles.find((p) => p.id === selectedId) ?? null)
      : null;
  const conn: ConnState = (selected && connections[selected.id]) ?? {
    status: "disconnected",
  };

  // Masking is a per-connection condition, so its chip sits beside the
  // read-only one. The count comes from the same store the Masking tab writes,
  // which is why the status bar is correct the moment a rule is saved rather
  // than on the next connect.
  const masking = useMasking(selected?.id ?? null);

  /**
   * WHICH CLUSTER SCREEN IS ON — the rail's business, so the shell holds it.
   *
   * Keyed to the cluster SESSION (`id:nonce`) rather than to the id alone, so
   * switching clusters and the remount "Refresh topics" performs both re-read
   * the stored placement instead of carrying dev's screen onto prod. Adjusting
   * state during render is the documented React pattern for exactly this, and
   * it is why the tab is right on the first paint rather than one frame later
   * — a frame of the wrong screen is a frame of the wrong cluster's data.
   */
  const sessionKey = selected !== null ? `${selected.id}:${topicsNonce}` : "";
  const [session, setSession] = useState<{
    key: string;
    tab: TabKey;
    /**
     * How many times a rail item has been PRESSED in this session. Not a
     * counter for its own sake: pressing the item you are already on has to
     * count as a navigation — it is the plainest way a user can say "take me
     * back to the top of this section" — and `tab` cannot express it.
     */
    nav: number;
  }>({
    key: "",
    tab: "overview",
    nav: 0,
  });
  if (session.key !== sessionKey) {
    setSession({
      key: sessionKey,
      tab: selected === null ? "overview" : initialTab(selected.id),
      // A new cluster session starts from the stored placement, which is the
      // reconnect promise — so it must NOT look like a press.
      nav: 0,
    });
  }

  /**
   * A rail item always opens its section's ROOT — see ClusterView's `lastNav`
   * effect for the other half. Restoring a placement is a promise about coming
   * back tomorrow, not about pressing "Topics".
   */
  const goTab = useCallback((next: TabKey) => {
    setSession((prev) => ({ ...prev, tab: next, nav: prev.nav + 1 }));
    setScreen("cluster");
  }, []);

  /**
   * THE STAGE SCROLLS BACK TO THE TOP ON EVERY NAVIGATION.
   *
   * Screen, rail item, cluster and the draft state are what "somewhere else"
   * means at the SHELL's level; `nav` is in the token so pressing the current
   * rail item counts too. Everything below the rail — which topic, which pane,
   * which broker — is `ClusterView`'s half of the same mechanism, because that
   * is where those values live. Neither of them touches a scrollport directly:
   * see stage.ts.
   */
  useStageTop(
    `${screen}:${session.tab}:${session.nav}:${selectedId ?? ""}:${creating}`,
  );

  let main: React.ReactNode;
  if (screen === "settings") {
    // First branch on purpose: Settings answers "I cannot read this app", and
    // that has to work in every other state the workspace can be in —
    // including the one where reading the connection file failed.
    // `onProfilesChanged` matters here for one case: deleting an environment
    // from Settings reassigns every profile that used it. Without this the
    // reassignment lands on disk and in the file, and the switcher menu keeps
    // showing the old environment until the next reload.
    main = (
      <SettingsView
        onOpenAbout={openAbout}
        onUpdateFound={setUpdate}
        onProfilesChanged={() => void reloadProfiles()}
      />
    );
  } else if (profiles === null) {
    // Never a full-screen spinner. A sentence says what we are waiting for.
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <p className="empty-hint">{t("common.readingConnections")}</p>
        </div>
      </div>
    );
  } else if (
    screen === "cluster" &&
    selected &&
    conn.status === "connected" &&
    conn.overview
  ) {
    // Guarded on the screen as well as the connection: "Connections" is a
    // destination you can stand on with a cluster up, and it shows that
    // cluster's settings rather than pretending nothing is connected.
    main = (
      <ClusterView
        // The nonce is "Refresh topics" from the palette — see the state above.
        key={sessionKey}
        profile={selected}
        overview={conn.overview}
        tab={session.tab}
        navNonce={session.nav}
        onFiringChange={setFiring}
        // The same handler the rail's own buttons press. Home's attention rows
        // and the alert toast's "View …" button land in the state a rail press
        // produces, because they ARE a rail press.
        onTab={goTab}
        // Cluster home's Refresh (audit item 15). Same nonce as the palette's
        // "Refresh topics": a remount, which is the whole of the behaviour.
        onRefresh={() => setTopicsNonce((n) => n + 1)}
        onDisconnect={disconnect}
        onDangerChange={setViewDanger}
        onTopicActions={setTopicActions}
        onOpenCluster={openCluster}
      />
    );
  } else if (creating) {
    main = (
      <ProfileEditor
        key="new"
        profile={null}
        connStatus="disconnected"
        connError={undefined}
        onSaved={handleSaved}
        onConnect={connect}
        onDeleted={handleDeleted}
        onCancelNew={stopCreating}
        onError={showError}
        onProfilesChanged={() => void reloadProfiles()}
        // The mockup's Connections screen is a list BESIDE an editor, and the
        // list is this component's state. `select` is deliberately the SAME
        // handler the switcher menu presses, so picking a cluster means the
        // same thing in both surfaces.
        profiles={profiles ?? undefined}
        connections={connections}
        selectedId={selectedId}
        onSelect={select}
        onCreate={startCreating}
      />
    );
  } else if (selected) {
    main = (
      <ProfileEditor
        key={selected.id}
        profile={selected}
        connStatus={conn.status}
        connError={conn.error}
        onSaved={handleSaved}
        onConnect={connect}
        onDeleted={handleDeleted}
        onCancelNew={stopCreating}
        onError={showError}
        onProfilesChanged={() => void reloadProfiles()}
        profiles={profiles ?? undefined}
        connections={connections}
        selectedId={selectedId}
        onSelect={select}
        onCreate={startCreating}
      />
    );
  } else if (loadFailed && profiles.length === 0) {
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <h1 className="empty-title">{t("app.profilesFailed.title")}</h1>
          <p className="empty-hint">{t("app.profilesFailed.hint")}</p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void reloadProfiles()}
            >
              {t("common.tryAgain")}
            </button>
          </div>
        </div>
      </div>
    );
  } else if (profiles.length === 0) {
    // First launch. The one screen that has to teach.
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <h1 className="empty-title">{t("app.firstRun.title")}</h1>
          <p className="empty-hint">{t("app.firstRun.what")}</p>
          {/* `tx`, not `t`: the two example addresses are <code> elements
              spliced into the sentence as PARAMS, so a translator gets one
              whole sentence with two slots instead of three fragments whose
              word order they cannot change. */}
          <p className="empty-hint">
            {tx("app.firstRun.example", {
              example: <code>kafka-1.internal:9092</code>,
              local: <code>localhost:9092</code>,
            })}
          </p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={startCreating}
            >
              {t("common.addConnection")}
            </button>
          </div>
          <p className="empty-footnote">{t("app.firstRun.footnote")}</p>

          {/* The second path off screen one, and the one for somebody who has
              no broker to point at yet. It is BELOW the primary action and
              carries no primary button of its own: the empty state is allowed
              exactly one (docs/DESIGN.md §5.5), and "add your cluster" is the
              thing most people came here to do. */}
          <Playground onReady={(id) => void openPlayground(id)} />
        </div>
      </div>
    );
  } else {
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <h1 className="empty-title">{t("app.pick.title")}</h1>
          <p className="empty-hint">{t("app.pick.hint")}</p>
          <div className="empty-actions">
            <button type="button" className="btn" onClick={startCreating}>
              {t("common.addConnection")}
            </button>
          </div>

          {/* Here as well as on screen one, and for a reason that only shows up
              on the second launch: once the Playground connection exists, the
              first-run state never renders again — and the STOP button lives in
              this panel. Without this the only way to shut the container down
              would be the terminal, from an app that started it with a button. */}
          <Playground onReady={(id) => void openPlayground(id)} />
        </div>
      </div>
    );
  }

  // Guardrail layer 1 + 5: the environment of the selected cluster colours the
  // ledger rule — and, when it is protected, the substrate — everywhere below
  // here.
  //
  // The empty string with nothing selected is deliberate: it resolves to the
  // slate stand-in, which is the neutral rule and no substrate — the same thing
  // "dev" used to produce, without privileging a name the user may have
  // renamed or deleted.
  const envDef = useEnvironment(selected?.environment ?? "");
  const bootstrap = selected?.bootstrap_servers.join(", ") ?? "";

  /**
   * The window title carries the cluster and its environment (§6). It is the
   * one piece of chrome that survives the window being minimised, alt-tabbed
   * past or cropped into a bug report, and it used to read "Kavka" on every
   * cluster in the world.
   *
   * `envDef.name` and not `selected.environment`: the registry's spelling is
   * what the chip beside it renders, so a renamed environment renames both or
   * the guardrail says two different words in two places. Nothing selected
   * gives the empty string, which `windowTitleFor` drops — the bar goes back
   * to "Kavka".
   */
  useWindowTitle(selected?.name ?? null, selected === null ? null : envDef.name);

  // The two contextual rows. Bilingual keywords like every other action
  // (§5.9): `cel`, `filter` and `scan` all find "Search in orders.v2".
  const contextualCommands = useMemo<PaletteAction[]>(() => {
    if (topicActions === null || selected === null) return [];
    return [
      {
        id: "topic-search",
        glyph: "⌕",
        label: t("app.cmd.search", { topic: topicActions.topic }),
        context: selected.name,
        keywords: t("app.cmd.search.kw"),
        env: selected.environment,
        run: topicActions.search,
      },
      {
        id: "topic-sql",
        glyph: "∑",
        label: t("app.cmd.sql", { topic: topicActions.topic }),
        context: selected.name,
        keywords: t("app.cmd.sql.kw"),
        env: selected.environment,
        run: topicActions.sql,
      },
      {
        id: "topic-produce",
        glyph: "↑",
        label: t("app.cmd.produce", { topic: topicActions.topic }),
        context: envDef.protected
          ? t("app.cmd.produce.confirmContext", { cluster: selected.name })
          : selected.name,
        keywords: t("app.cmd.produce.kw"),
        env: selected.environment,
        danger: envDef.protected,
        disabledReason: topicActions.produceBlocked,
        run: topicActions.produce,
      },
    ];
    // `t` is memoized on the locale, so this rebuilds when the language
    // changes and on no other render — see the identity rule in i18n/index.ts.
    // `envDef` is listed because protection is a property of the REGISTRY, not
    // of the profile: renaming an environment to protected has to re-mark the
    // palette's produce row without the selection changing.
  }, [topicActions, selected, envDef, t]);

  return (
    <div
      className="app"
      {...envAttrs(envDef)}
      // Protected de-collision (§5.8): the env rule dampens while ANY danger is
      // on screen — the global banner, the inline connect failure in the
      // editor, and a banner inside a dialog. Miss one and a protected cluster
      // shows a coral rule behind a coral banner, which is the one composition
      // the guardrail must not produce.
      data-alert={
        error !== null || conn.error || transferDanger || viewDanger
          ? "danger"
          : undefined
      }
    >
      {/* Guardrail layer 2: a 2px wire under the native title bar. Transparent
          outside a protected environment. Do not remove it because the
          substrate "already says it" — the substrate is the bonus, this is
          load-bearing.

          In forced colors the wire thickens and prints the environment's own
          name, uppercased — `PROD`, `PRODUCTION`, `UAT`, whatever the user
          called it. Untranslated, for the same reason the chip is. */}
      <div
        className="app-wire"
        aria-hidden="true"
        data-env-label={envWireLabel(envDef)}
      />

      <div className="app-shell">
        {/* ONE rail for the whole window (DESIGN.md §5.1). `select` and
            `startCreating` already move the stage to Connections, so nothing
            here has to remember to close Settings. */}
        <Rail
          version={version}
          profiles={profiles}
          selectedId={creating ? null : selectedId}
          connections={connections}
          selected={selected}
          conn={conn}
          creating={creating}
          screen={screen}
          tab={session.tab}
          firing={firing}
          onScreen={setScreen}
          onTab={goTab}
          onSelect={select}
          onConnect={(profile) => {
            select(profile.id);
            void connect(profile);
          }}
          onDisconnect={(id) => void disconnect(id)}
          onNew={startCreating}
        />

        <main className="workspace">
          {/* Banner, not toast: this is a condition you are in, and an error
              the user must act on is never a toast. The raw librdkafka string
              is never the title — errors.ts turns it into "what happened" plus
              "the next click", and keeps the verbatim text under Show details
              for whoever actually wants it. */}
          {error !== null && (
            <ErrorBanner raw={error} onDismiss={() => setError(null)} />
          )}

          {/* BELOW the error banner, always. An error is a thing the user has
              to act on now; a new release has waited days and can wait for the
              sentence above it. Inside the workspace rather than over it, so
              it is still on screen while Settings is open — which is where
              "Check now" raises it from. */}
          {update !== null && (
            <UpdateBanner offer={update} onDismiss={dismissUpdate} />
          )}

          {/* THE STAGE. It is the scrollport, which is why it registers
              itself: every navigation puts it back at the top, and the two
              components that know the user has moved reach it through
              stage.ts rather than through a ref neither of them can hold. */}
          <div className="workspace-body" ref={registerStage}>
            {main}
          </div>
        </main>
      </div>

      {/* THE STATUS BAR — a documented deviation from the mockup, which has
          none (DESIGN.md §11). It stays because it carries LIVE OPERATIONAL
          STATE that a drawing never had to honour: search progress, tail rate,
          the always-visible bootstrap address §6 requires. It now spans the
          whole window rather than stopping at a column edge that no longer
          exists, and the version has left it for the brand lockup — a build
          number is not operational state. */}
      <footer className="statusbar">
        <div className="statusbar-left">
          {selected ? (
            <>
              <span
                className={`status-dot status-${conn.status}`}
                aria-hidden="true"
              />
              {/* Law 2: the dot never carries the meaning on its own. */}
              <span className="statusbar-item">{t(STATUS_KEY[conn.status])}</span>
              <span className="statusbar-sep" aria-hidden="true">
                ·
              </span>
              <span className="statusbar-item">{selected.name}</span>
              <span className="statusbar-sep" aria-hidden="true">
                ·
              </span>
              {/* Prod guardrail layer 3: the address is always on screen.
                  Most prod accidents are right-action-wrong-cluster. */}
              <span className="statusbar-item statusbar-mono" title={bootstrap}>
                {bootstrap}
              </span>
              {selected.read_only && (
                <span className="readonly-chip" title={t("app.readonlyTitle")}>
                  {t("app.readonlyChip")}
                </span>
              )}
              {/* Masking, said out loud wherever data is. A payload that has
                  been rewritten on its way here must never look like what the
                  producer sent, and the number is part of the claim — "on"
                  with nothing to say how much is not a statement anyone can
                  act on. */}
              {masking.enabled > 0 && (
                <span className="mask-chip" title={maskingChipTitle(masking)}>
                  {maskingChipLabel(masking)}
                </span>
              )}
            </>
          ) : (
            <span className="statusbar-item">
              {creating ? t("app.statusbar.draft") : t("app.statusbar.none")}
            </span>
          )}
        </div>
        <div className="statusbar-right">
          {/* The one shortcut worth advertising, and clickable for whoever
              finds it here before they find the key. The version used to sit
              after it; it is in the brand lockup now. */}
          <button
            type="button"
            className="statusbar-hint"
            title={t("palette.searchLabel")}
            onClick={() => setPaletteOpen(true)}
          >
            <span className="kbd">{paletteKeyLabel()}</span>
            {t("app.statusbar.commands")}
          </button>
        </div>
      </footer>

      {paletteOpen && (
        <Palette
          // `?? []` and not a guard on `profiles !== null`: ⌘K during the
          // first read must still open something, or the shortcut looks
          // broken on exactly the launch where a user first tries it.
          profiles={profiles ?? []}
          connections={connections}
          // The cluster actually on screen, not the persisted id: while a new
          // connection is being written there is no cluster in the workspace,
          // so "Refresh topics" must not claim there is one.
          selectedId={selected?.id ?? null}
          commands={paletteCommands}
          contextual={contextualCommands}
          onClose={closePalette}
        />
      )}

      {aboutOpen && <AboutDialog version={version} onClose={closeAbout} />}

      {transfer !== null && (
        <ImportExportDialog
          initialTab={transfer}
          // The envelope carries environment definitions as well as
          // connections (skip-existing by name), so the registry has to be
          // re-read too — otherwise an imported prod connection renders slate
          // until the next launch.
          onImported={() => {
            void reloadProfiles();
            void loadEnvironments();
          }}
          onDangerChange={setTransferDanger}
          onClose={closeTransfer}
        />
      )}
    </div>
  );
}
