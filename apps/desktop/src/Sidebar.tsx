import { openUrl } from "@tauri-apps/plugin-opener";
import type { ConnectionProfile, ConnState, ConnStatus, Environment } from "./api";
import { SUPPORT_URL } from "./AboutDialog";
import { useI18n, type MessageKey } from "./i18n";

/**
 * Sentence-case everywhere except env chips and table column headers. The
 * chip renders uppercase via CSS, so the word stays readable in the DOM.
 *
 * DELIBERATELY NOT TRANSLATED. `dev` / `staging` / `prod` are the same three
 * tokens as `data-env`, the window title and the forced-colors `PROD` label
 * on the wire (§6, §10). Prod is a guardrail that has to read identically in
 * every locale, in every one of its nine layers; a chip that says one thing
 * and a wire that says another is two signals where the design specifies one.
 */
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
const STATUS_KEY: Record<ConnStatus, MessageKey> = {
  disconnected: "sidebar.status.disconnected",
  connecting: "sidebar.status.connecting",
  connected: "sidebar.status.connected",
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
  const { t } = useI18n();
  return (
    // Named landmarks: "complementary" and "navigation" with no accessible
    // name are two unlabelled entries in a screen reader's landmark list, on
    // the one region a user jumps to first.
    <aside className="sidebar" aria-label={t("sidebar.title")}>
      <div className="sidebar-header">
        {/* Section eyebrows keep uppercase micro-caps; labels do not.
            Sections become collapsible with persisted state in Phase 3 —
            the sidebar outgrows the viewport, and ⌘K is the real navigation. */}
        <span className="sidebar-title">{t("sidebar.title")}</span>
      </div>

      <nav className="profile-list" aria-label={t("sidebar.navLabel")}>
        {profiles === null ? (
          <div className="sidebar-note">{t("sidebar.loading")}</div>
        ) : profiles.length === 0 && !creating ? (
          <div className="sidebar-note">{t("sidebar.empty")}</div>
        ) : (
          profiles.map((profile) => {
            const status = connections[profile.id]?.status ?? "disconnected";
            const address = profile.bootstrap_servers.join(", ");
            const statusWord = t(STATUS_KEY[status]);
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
                // Which cluster is open is carried by a tint and a 2px left
                // border — nothing a screen reader can see. `aria-current`
                // is the one channel that says "this is the one you are on"
                // without inventing a control state this row does not have
                // (it is not pressed, and it is not selected in a listbox).
                aria-current={profile.id === selectedId ? "true" : undefined}
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
                  {t("sidebar.profileMeta", { address, status: statusWord })}
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
                {t("sidebar.draftName")}
              </span>
            </span>
            <span className="profile-meta">{t("sidebar.draftMeta")}</span>
          </div>
        )}
      </nav>

      <button type="button" className="new-connection-btn" onClick={onNew}>
        {t("common.addConnection")}
      </button>

      <footer className="sidebar-footer">
        <span className="app-name">Kavka</span>
        <div className="sidebar-footer-links">
          {/* Same look as the support link, so the footer reads as one line
              of quiet text rather than a button next to a link. */}
          <button type="button" className="support-link" onClick={onAbout}>
            {t("sidebar.about")}
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
            {t("common.support")}
          </a>
        </div>
      </footer>
    </aside>
  );
}
