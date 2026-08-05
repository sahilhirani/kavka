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
  envAttrs,
  envWireLabel,
  loadEnvironments,
  useEnvironment,
} from "./environments";
import { maskingChipLabel, maskingChipTitle, useMasking } from "./masking";
import Sidebar from "./Sidebar";
import ProfileEditor, { ErrorBanner } from "./ProfileEditor";
import ClusterView, { stageTopic } from "./ClusterView";
import Palette, {
  paletteKeyLabel,
  type PaletteAction,
  type PaletteCommands,
} from "./Palette";
import AboutDialog from "./AboutDialog";
import ImportExportDialog, { type TransferTab } from "./ImportExportDialog";
import Playground from "./Playground";
import SettingsView from "./SettingsView";
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
  // Settings is a VIEW, not an overlay, and deliberately not a dialog: it has
  // to be reachable with nothing connected, it is where somebody goes to make
  // the app readable before they can read anything, and a modal over an empty
  // workspace is a modal over nothing. It does not clear the selection —
  // closing it puts you back on the cluster you were already on.
  const [settingsOpen, setSettingsOpen] = useState(false);
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

  const select = useCallback((id: string) => {
    setCreating(false);
    setSelectedId(id);
    lsSet(SELECTED_KEY, id);
  }, []);

  const startCreating = useCallback(() => {
    setCreating(true);
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
      if (!already) void connect(target);
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

  let main: React.ReactNode;
  if (settingsOpen) {
    // First branch on purpose: Settings answers "I cannot read this app", and
    // that has to work in every other state the workspace can be in —
    // including the one where reading the connection file failed.
    main = <SettingsView onOpenAbout={openAbout} onUpdateFound={setUpdate} />;
  } else if (profiles === null) {
    // Never a full-screen spinner. A sentence says what we are waiting for.
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <p className="empty-hint">{t("common.readingConnections")}</p>
        </div>
      </div>
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
      />
    );
  } else if (selected && conn.status === "connected" && conn.overview) {
    main = (
      <ClusterView
        // The nonce is "Refresh topics" from the palette — see the state above.
        key={`${selected.id}:${topicsNonce}`}
        profile={selected}
        overview={conn.overview}
        onDisconnect={disconnect}
        onDangerChange={setViewDanger}
        onTopicActions={setTopicActions}
        onOpenCluster={openCluster}
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
        <Sidebar
          profiles={profiles}
          selectedId={creating ? null : selectedId}
          connections={connections}
          creating={creating}
          settingsOpen={settingsOpen}
          // Picking a cluster or starting a new one is a request to look at
          // that, so both close Settings on the way. Toggling Settings does
          // NOT clear the selection — closing it returns you where you were.
          onSelect={(id) => {
            setSettingsOpen(false);
            select(id);
          }}
          onNew={() => {
            setSettingsOpen(false);
            startCreating();
          }}
          onAbout={openAbout}
          onSettings={() => setSettingsOpen((open) => !open)}
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

          <div className="workspace-body">{main}</div>

          <footer className="statusbar">
            <div className="statusbar-left">
              {selected ? (
                <>
                  <span
                    className={`status-dot status-${conn.status}`}
                    aria-hidden="true"
                  />
                  {/* Law 2: the dot never carries the meaning on its own. */}
                  <span className="statusbar-item">
                    {t(STATUS_KEY[conn.status])}
                  </span>
                  <span className="statusbar-sep" aria-hidden="true">
                    ·
                  </span>
                  <span className="statusbar-item">{selected.name}</span>
                  <span className="statusbar-sep" aria-hidden="true">
                    ·
                  </span>
                  {/* Prod guardrail layer 3: the address is always on screen.
                      Most prod accidents are right-action-wrong-cluster. */}
                  <span
                    className="statusbar-item statusbar-mono"
                    title={bootstrap}
                  >
                    {bootstrap}
                  </span>
                  {selected.read_only && (
                    <span
                      className="readonly-chip"
                      title={t("app.readonlyTitle")}
                    >
                      {t("app.readonlyChip")}
                    </span>
                  )}
                  {/* Masking, said out loud wherever data is. A payload that
                      has been rewritten on its way here must never look like
                      what the producer sent, and the number is part of the
                      claim — "on" with nothing to say how much is not a
                      statement anyone can act on. */}
                  {masking.enabled > 0 && (
                    <span
                      className="mask-chip"
                      title={maskingChipTitle(masking)}
                    >
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
              {/* The one shortcut worth advertising, and clickable for
                  whoever finds it here before they find the key. */}
              <button
                type="button"
                className="statusbar-hint"
                title={t("palette.searchLabel")}
                onClick={() => setPaletteOpen(true)}
              >
                <span className="kbd">{paletteKeyLabel()}</span>
                {t("app.statusbar.commands")}
              </button>
              <span className="statusbar-sep" aria-hidden="true">
                ·
              </span>
              <span className="statusbar-item statusbar-mono">
                {t("app.statusbar.coreVersion", { version: version || "…" })}
              </span>
            </div>
          </footer>
        </main>
      </div>

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
