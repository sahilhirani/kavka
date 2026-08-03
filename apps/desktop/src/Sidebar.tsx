import { openUrl } from "@tauri-apps/plugin-opener";
import type { ConnectionProfile, ConnState, ConnStatus, Environment } from "./api";
import { SUPPORT_URL } from "./AboutDialog";

// Sentence-case everywhere except env chips and table column headers. The
// chip renders uppercase via CSS, so the word stays readable in the DOM.
const ENV_LABEL: Record<Environment, string> = {
  dev: "dev",
  staging: "staging",
  prod: "prod",
};

/**
 * Law 2: every dot has a word. "disconnected" used to map to the empty
 * string, so the one state the dot renders as a hollow ring — the state a
 * deuteranope is least able to read — was also the one state with no word
 * anywhere in the row. Every status now produces text, so `.profile-meta`
 * always reads "address · state" and the dot is pure decoration.
 */
const STATUS_WORD: Record<ConnStatus, string> = {
  disconnected: "not connected",
  connecting: "connecting…",
  connected: "connected",
};

export function EnvChip({ env }: { env: Environment }) {
  return <span className={`env-chip env-${env}`}>{ENV_LABEL[env]}</span>;
}

interface SidebarProps {
  profiles: ConnectionProfile[] | null;
  selectedId: string | null;
  connections: Record<string, ConnState>;
  creating: boolean;
  onSelect: (id: string) => void;
  onNew: () => void;
  onAbout: () => void;
}

export default function Sidebar({
  profiles,
  selectedId,
  connections,
  creating,
  onSelect,
  onNew,
  onAbout,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        {/* Section eyebrows keep uppercase micro-caps; labels do not.
            Sections become collapsible with persisted state in Phase 3 —
            the sidebar outgrows the viewport, and ⌘K is the real navigation. */}
        <span className="sidebar-title">Clusters</span>
      </div>

      <nav className="profile-list">
        {profiles === null ? (
          <div className="sidebar-note">Reading your connections…</div>
        ) : profiles.length === 0 && !creating ? (
          <div className="sidebar-note">
            Nothing here yet. Add your first connection below.
          </div>
        ) : (
          profiles.map((profile) => {
            const status = connections[profile.id]?.status ?? "disconnected";
            const address = profile.bootstrap_servers.join(", ");
            const statusWord = STATUS_WORD[status];
            const classes = [
              "profile-row",
              profile.environment === "prod" ? "profile-row-prod" : "",
              profile.id === selectedId ? "profile-row-selected" : "",
            ]
              .filter(Boolean)
              .join(" ");
            return (
              <button
                key={profile.id}
                type="button"
                className={classes}
                onClick={() => onSelect(profile.id)}
                title={address}
              >
                <span className="profile-row-line">
                  <span
                    className={`status-dot status-${status}`}
                    aria-hidden="true"
                  />
                  <span className="profile-name">{profile.name}</span>
                  <EnvChip env={profile.environment} />
                </span>
                {/* Prod guardrail layer 3: the bootstrap address is always on
                    screen. And law 2: the status dot always has its word —
                    this line reads "address · state" in every state, never
                    just "address". */}
                <span className="profile-meta">
                  {address} · {statusWord}
                </span>
              </button>
            );
          })
        )}
        {creating && (
          <div className="profile-row profile-row-selected">
            <span className="profile-row-line">
              <span className="status-dot status-disconnected" aria-hidden="true" />
              <span className="profile-name profile-name-draft">
                New connection
              </span>
            </span>
            <span className="profile-meta">not saved yet</span>
          </div>
        )}
      </nav>

      <button type="button" className="new-connection-btn" onClick={onNew}>
        Add connection
      </button>

      <footer className="sidebar-footer">
        <span className="app-name">Kavka</span>
        <div className="sidebar-footer-links">
          {/* Same look as the support link, so the footer reads as one line
              of quiet text rather than a button next to a link. */}
          <button type="button" className="support-link" onClick={onAbout}>
            About
          </button>
          <span className="footer-sep" aria-hidden="true">
            ·
          </span>
          <a
            className="support-link"
            href={SUPPORT_URL}
            onClick={(e) => {
              e.preventDefault();
              openUrl(SUPPORT_URL).catch(console.error);
            }}
          >
            Support Kavka ☕
          </a>
        </div>
      </footer>
    </aside>
  );
}
