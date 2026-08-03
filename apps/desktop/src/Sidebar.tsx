import { openUrl } from "@tauri-apps/plugin-opener";
import type { ConnectionProfile, ConnState, Environment } from "./api";

const SUPPORT_URL = "https://buymeacoffee.com/sahilhirani";

const ENV_LABEL: Record<Environment, string> = {
  dev: "dev",
  staging: "stg",
  prod: "prod",
};

export function EnvChip({ env }: { env: Environment }) {
  return <span className={`env-chip env-${env}`}>{ENV_LABEL[env]}</span>;
}

interface SidebarProps {
  profiles: ConnectionProfile[] | null;
  selectedId: string | null;
  connections: Record<string, ConnState>;
  creating: boolean;
  version: string;
  onSelect: (id: string) => void;
  onNew: () => void;
}

export default function Sidebar({
  profiles,
  selectedId,
  connections,
  creating,
  version,
  onSelect,
  onNew,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">Clusters</span>
      </div>

      <nav className="profile-list">
        {profiles === null ? (
          <div className="sidebar-note">Loading…</div>
        ) : profiles.length === 0 && !creating ? (
          <div className="sidebar-note">No connections yet.</div>
        ) : (
          profiles.map((profile) => {
            const status =
              connections[profile.id]?.status ?? "disconnected";
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
                title={profile.bootstrap_servers.join(", ")}
              >
                <span
                  className={`status-dot status-${status}`}
                  aria-label={status}
                />
                <span className="profile-name">{profile.name}</span>
                <EnvChip env={profile.environment} />
              </button>
            );
          })
        )}
        {creating && (
          <div className="profile-row profile-row-selected profile-row-draft">
            <span className="status-dot status-disconnected" />
            <span className="profile-name profile-name-draft">
              New connection
            </span>
          </div>
        )}
      </nav>

      <button type="button" className="new-connection-btn" onClick={onNew}>
        + New connection
      </button>

      <footer className="sidebar-footer">
        <div className="app-meta">
          <span className="app-name">Kavka</span>
          <span className="app-version">core v{version || "…"}</span>
        </div>
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
      </footer>
    </aside>
  );
}
