import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  clusterConnect,
  clusterDisconnect,
  coreVersion,
  errorMessage,
  profilesList,
  type ConnectionProfile,
  type ConnState,
} from "./api";
import Sidebar from "./Sidebar";
import ProfileEditor, { ErrorBanner } from "./ProfileEditor";
import ClusterView from "./ClusterView";
import Palette, { paletteKeyLabel, type PaletteCommands } from "./Palette";
import AboutDialog from "./AboutDialog";
import ImportExportDialog, { type TransferTab } from "./ImportExportDialog";

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

const STATUS_WORD = {
  disconnected: "Not connected",
  connecting: "Connecting…",
  connected: "Connected",
} as const;

export default function App() {
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
  // Profile ids with a cluster_connect in flight (double-click guard).
  const connectsInFlight = useRef(new Set<string>());
  // Mirror of `profiles` for use after awaits without stale closures.
  const profilesRef = useRef<ConnectionProfile[] | null>(null);
  profilesRef.current = profiles;

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

  // Initial load: profiles + core version.
  useEffect(() => {
    void reloadProfiles();
    coreVersion()
      .then(setVersion)
      .catch(() => setVersion("unknown"));
  }, [reloadProfiles]);

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

  let main: React.ReactNode;
  if (profiles === null) {
    // Never a full-screen spinner. A sentence says what we are waiting for.
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <p className="empty-hint">Reading your saved connections…</p>
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
      />
    );
  } else if (loadFailed && profiles.length === 0) {
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <h1 className="empty-title">Kavka couldn't read its connection file</h1>
          <p className="empty-hint">
            Your connections are still on disk — nothing was lost. Kavka stores
            them in its config directory, alongside this app's settings.
          </p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void reloadProfiles()}
            >
              Try again
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
          <h1 className="empty-title">Point Kavka at a broker</h1>
          <p className="empty-hint">
            A connection is a saved address for one Kafka cluster — a name, one
            broker to start from, and how to sign in. Kavka finds the rest of
            the cluster from there.
          </p>
          <p className="empty-hint">
            A bootstrap server usually looks like{" "}
            <code>kafka-1.internal:9092</code>. Running this repo's dev cluster?
            Use <code>localhost:9092</code>.
          </p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={startCreating}
            >
              Add connection
            </button>
          </div>
          <p className="empty-footnote">
            Passwords go to your operating system's keychain. Nothing about your
            clusters leaves this machine.
          </p>
        </div>
      </div>
    );
  } else {
    main = (
      <div className="empty-state">
        <div className="empty-block">
          <h1 className="empty-title">Pick a connection</h1>
          <p className="empty-hint">
            Choose a cluster on the left to see its brokers and topics, or add
            another connection.
          </p>
          <div className="empty-actions">
            <button type="button" className="btn" onClick={startCreating}>
              Add connection
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Prod guardrail layer 1 + 5: the environment of the selected cluster
  // colours the ledger rule — and the substrate — everywhere below here.
  const env = selected?.environment ?? "dev";
  const bootstrap = selected?.bootstrap_servers.join(", ") ?? "";

  return (
    <div
      className="app"
      data-env={env}
      // Prod de-collision (§5.8): the env rule dampens while ANY danger is on
      // screen — the global banner, the inline connect failure in the editor,
      // and a banner inside a dialog. Miss one and a prod cluster shows a
      // coral rule behind a coral banner, which is the one composition the
      // guardrail must not produce.
      data-alert={
        error !== null || conn.error || transferDanger || viewDanger
          ? "danger"
          : undefined
      }
    >
      {/* Prod guardrail layer 2: a 2px wire under the native title bar.
          Transparent outside prod. Do not remove it because the substrate
          "already says prod" — the substrate is the bonus, this is load-bearing. */}
      <div
        className="app-wire"
        aria-hidden="true"
        data-env-label={env === "prod" ? "PROD" : undefined}
      />

      <div className="app-shell">
        <Sidebar
          profiles={profiles}
          selectedId={creating ? null : selectedId}
          connections={connections}
          creating={creating}
          onSelect={select}
          onNew={startCreating}
          onAbout={openAbout}
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
                    {STATUS_WORD[conn.status]}
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
                      title="This connection is read-only. Turn that off in the connection's settings to produce or edit."
                    >
                      read-only
                    </span>
                  )}
                </>
              ) : (
                <span className="statusbar-item">
                  {creating ? "New connection — not saved yet" : "No connection selected"}
                </span>
              )}
            </div>
            <div className="statusbar-right">
              {/* The one shortcut worth advertising, and clickable for
                  whoever finds it here before they find the key. */}
              <button
                type="button"
                className="statusbar-hint"
                title="Search commands and clusters"
                onClick={() => setPaletteOpen(true)}
              >
                <span className="kbd">{paletteKeyLabel()}</span>
                commands
              </button>
              <span className="statusbar-sep" aria-hidden="true">
                ·
              </span>
              <span className="statusbar-item statusbar-mono">
                core v{version || "…"}
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
          onClose={closePalette}
        />
      )}

      {aboutOpen && <AboutDialog version={version} onClose={closeAbout} />}

      {transfer !== null && (
        <ImportExportDialog
          initialTab={transfer}
          onImported={() => void reloadProfiles()}
          onDangerChange={setTransferDanger}
          onClose={closeTransfer}
        />
      )}
    </div>
  );
}
