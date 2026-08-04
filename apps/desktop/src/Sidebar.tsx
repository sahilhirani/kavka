import { openUrl } from "@tauri-apps/plugin-opener";
import type { ConnectionProfile, ConnState, ConnStatus, Environment } from "./api";
import { SUPPORT_URL } from "./AboutDialog";
import {
  envAttrs,
  resolveEnvironment,
  useEnvironment,
  useEnvironments,
} from "./environments";
import { useI18n, type MessageKey } from "./i18n";

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

/**
 * The environment chip — identity, in one pill.
 *
 * The NAME IS NEVER TRANSLATED and never sentence-cased in the DOM: it is user
 * data now, and it is the same string as the `data-env-color` sibling, the
 * forced-colors wire label, the CLI's refusal and the window title (§6, §10).
 * The chip uppercases in CSS so the word stays readable to anyone reading the
 * DOM or copying it into a bug report.
 *
 * Colour is identity; the FILLED form is the guardrail. An unprotected
 * environment is a tint-on-tag, a protected one a solid badge — the treatment
 * prod had, re-keyed onto the flag rather than onto the name, so a company
 * whose production environment is called `PRD` gets the badge too.
 */
export function EnvChip({ env }: { env: Environment }) {
  const def = useEnvironment(env);
  return (
    <span className="env-chip" {...envAttrs(def)}>
      {def.name}
    </span>
  );
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
  // Read once for the whole list rather than per row: a hook cannot be called
  // inside `profiles.map`, and resolving against one snapshot also guarantees
  // every row in a single paint agrees about what is protected.
  const envDefs = useEnvironments();
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
            const envDef = resolveEnvironment(profile.environment, envDefs);
            const classes = [
              "profile-row",
              // Guardrail layer 6, re-keyed: the row stays tinted whether it is
              // selected or not, for every PROTECTED environment — not for the
              // one that happens to be spelled "prod".
              envDef.protected ? "profile-row-protected" : "",
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
