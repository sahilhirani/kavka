import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type {
  ConnectionProfile,
  ConnState,
  ConnStatus,
  EnvironmentDef,
} from "./api";
import {
  EnvChip,
  PadLock,
  resolveEnvironment,
  useEnvironments,
} from "./environments";
import { useI18n, type MessageKey } from "./i18n";

/**
 * THE CLUSTER SWITCHER — how you change cluster now that there is no sidebar.
 *
 * The Jackdaw mockup asserts this control and never draws it open: a full-width
 * `.cc-switch` trigger pinned to the bottom of the rail's cluster card, reading
 * "Switch cluster" with a down-caret. A down-caret on a full-width trigger
 * reads as a MENU, not a drawer, and that is the whole argument for deleting
 * the 248px permanent Clusters panel: the list of your connections is a thing
 * you consult for two seconds every few hours, not a column you pay for on
 * every screen. The canonical, browsable enumeration still exists — it is the
 * Connections screen, and ⌘K reaches every cluster by name.
 *
 * WHAT THE SIDEBAR DID, AND WHERE EACH DUTY WENT (DESIGN.md §5.1):
 *
 *   the profile list          → this menu's rows, and the Connections screen
 *   connect / disconnect      → the trailing action button ON each row
 *   "Add connection"          → this menu's foot, ⌘K, and the empty states
 *   the env chip + status     → the cluster card above the trigger, and rows
 *   Settings                  → a rail item in the Application group
 *   About · Support Kavka     → Settings → About
 *
 * SELECTING A COLD CLUSTER NO LONGER DETONATES THE WORKSPACE. That was the
 * sharpest complaint in the fidelity audit: clicking a disconnected row swapped
 * the whole workspace for a connection form. Here the row and the action are
 * two controls. Clicking the row selects the cluster and shows it; clicking the
 * trailing button connects or disconnects it, in place, with the row's status
 * word updating underneath. Neither one is a surprise.
 *
 * NOT A DIALOG. It borrows `Overlay`'s two promises — Esc closes and focus goes
 * back where it came from — and none of its furniture: no scrim, no
 * `aria-modal`, no focus trap. A menu that dims the app behind it to let you
 * pick a cluster is a menu that thinks it is more important than the cluster.
 *
 * KEYS. `role="menu"` with roving focus over a FLAT list of menu items in DOM
 * order: row, its action, the next row, its action, …, "Add connection". Up and
 * Down alone reach every control, which is the property that matters for a menu
 * whose rows have two jobs; Home/End jump the ends; Tab closes, because a menu
 * that you can Tab out of while it is still painted is a menu that lies about
 * where focus is.
 *
 * AND NOTHING IN THAT LIST MAY EVER BE `disabled`. Roving focus walks the list
 * by INDEX — it reads `document.activeElement` to learn where it is and calls
 * `focus()` to move — and a disabled <button> is not focusable, so focusing one
 * is a silent no-op that leaves the index exactly where it was. The next press
 * recomputes the same index and lands on the same dead control: arrow keys stuck
 * at whichever cluster is connecting, which is precisely when this menu gets
 * opened. The connecting row's action therefore carries `aria-disabled` plus a
 * guard in its handler, and any future item in here owes the same.
 */

const STATUS_KEY: Record<ConnStatus, MessageKey> = {
  disconnected: "switcher.status.disconnected",
  connecting: "switcher.status.connecting",
  connected: "switcher.status.connected",
};

/** Rows grouped under the environment they belong to, registry order first. */
interface Group {
  /** The environment's own name — user data, never translated. */
  name: string;
  def: EnvironmentDef;
  profiles: ConnectionProfile[];
}

/**
 * Group the profiles by environment, in REGISTRY order.
 *
 * Registry order rather than alphabetical because the registry is the order the
 * user put their environments in (dev, staging, production is the shape almost
 * everyone writes), and because it keeps the protected group in the same place
 * every time you open the menu — a destructive row that moves around is a
 * destructive row you click by accident. Profiles whose environment is not in
 * the registry are gathered at the end under their own literal name, so a
 * connection pointing at a deleted environment is visible rather than missing.
 */
function groupByEnvironment(
  profiles: readonly ConnectionProfile[],
  defs: EnvironmentDef[],
  unknownLabel: string,
): Group[] {
  const groups: Group[] = [];
  const index = new Map<string, Group>();
  const keyOf = (name: string) => name.trim().toLowerCase();

  for (const def of defs) {
    const group: Group = { name: def.name, def, profiles: [] };
    groups.push(group);
    index.set(keyOf(def.name), group);
  }

  for (const profile of profiles) {
    const key = keyOf(profile.environment);
    let group = index.get(key);
    if (group === undefined) {
      const def = resolveEnvironment(profile.environment, defs);
      group = {
        // An empty environment string has no name to show, so the group says
        // so in words rather than rendering a blank heading.
        name: profile.environment.trim() === "" ? unknownLabel : def.name,
        def,
        profiles: [],
      };
      groups.push(group);
      index.set(key, group);
    }
    group.profiles.push(profile);
  }

  return groups.filter((group) => group.profiles.length > 0);
}

export interface ClusterSwitcherProps {
  profiles: ConnectionProfile[] | null;
  selectedId: string | null;
  connections: Record<string, ConnState>;
  onSelect: (id: string) => void;
  onConnect: (profile: ConnectionProfile) => void;
  onDisconnect: (profileId: string) => void;
  onNew: () => void;
}

export default function ClusterSwitcher({
  profiles,
  selectedId,
  connections,
  onSelect,
  onConnect,
  onDisconnect,
  onNew,
}: ClusterSwitcherProps) {
  const { t } = useI18n();
  // Read once for the whole menu rather than per row: resolving every row
  // against one snapshot guarantees that a single paint agrees with itself
  // about what is protected.
  const envDefs = useEnvironments();
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  /**
   * The menu is `position: fixed` and measured, not absolutely positioned
   * inside the rail. The rail is a scrollport with `overflow-y: auto`, and an
   * absolutely positioned popover inside a scrollport is a popover with its
   * bottom half cut off on a short window. Fixed positioning escapes the
   * clip without leaving the `.app` subtree, so the protected substrate and
   * the danger damper still cascade into it (same reasoning as `Overlay`).
   */
  const [anchor, setAnchor] = useState<{
    top: number;
    left: number;
    width: number;
    maxHeight: number;
  } | null>(null);

  const close = useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus();
  }, []);

  useLayoutEffect(() => {
    if (!open) {
      setAnchor(null);
      return;
    }
    const measure = () => {
      const el = triggerRef.current;
      if (el === null) return;
      const r = el.getBoundingClientRect();
      const top = r.bottom + 6;
      setAnchor({
        top,
        left: r.left,
        // Wider than the trigger on purpose: a row carries a name, a chip and
        // a mono address, and the rail is only 254px. 288 is the floor at
        // which the address stops truncating on a localhost-shaped cluster.
        width: Math.max(r.width, 288),
        // Never taller than what is left of the window. The menu scrolls; it
        // does not run off the bottom edge where a protected row could hide.
        maxHeight: Math.max(160, window.innerHeight - top - 12),
      });
    };
    measure();
    window.addEventListener("resize", measure);
    // Capture phase: the rail is the scroller, not the window.
    window.addEventListener("scroll", measure, true);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [open]);

  // Click-outside. `pointerdown` rather than `click` so the menu is gone before
  // whatever was underneath receives the press, and so a drag that starts
  // inside the menu and ends outside it does not close.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (target === null) return;
      if (menuRef.current?.contains(target)) return;
      if (triggerRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, [open]);

  const items = useCallback(
    () =>
      Array.from(
        menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ??
          [],
      ),
    [],
  );

  // Opening lands on the cluster you are already on, not on the first row: the
  // menu's job is "move from here to there", and the answer to "where am I"
  // should be under the caret when it appears.
  useEffect(() => {
    if (!open) return;
    const all = items();
    const current =
      all.find((el) => el.getAttribute("aria-current") === "true") ?? all[0];
    current?.focus();
  }, [open, items]);

  const onMenuKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      // Stop here: the profile editor underneath treats Esc as "undo edits",
      // and one key press must never do two things.
      e.stopPropagation();
      close(true);
      return;
    }
    if (e.key === "Tab") {
      // A menu you can Tab out of while it is still on screen is a menu that
      // lies about where focus is. Close, and let Tab do its normal job from
      // the trigger.
      close(true);
      return;
    }
    const step =
      e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    const all = items();
    if (all.length === 0) return;
    let next: HTMLElement | undefined;
    if (step !== 0) {
      const from = all.indexOf(document.activeElement as HTMLElement);
      next = all[(from + step + all.length) % all.length];
    } else if (e.key === "Home") next = all[0];
    else if (e.key === "End") next = all[all.length - 1];
    else return;
    e.preventDefault();
    next?.focus();
  };

  const groups = groupByEnvironment(
    profiles ?? [],
    envDefs,
    t("switcher.noEnvironment"),
  );

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="cc-switch"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => (open ? close(true) : setOpen(true))}
        onKeyDown={(e) => {
          // Down-arrow opens onto the first item, which is what the caret in
          // the trigger promises.
          if (!open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span>{t("switcher.trigger")}</span>
        <svg
          className="cc-caret"
          viewBox="0 0 16 16"
          width="13"
          height="13"
          aria-hidden="true"
          focusable="false"
        >
          <path
            d="M4 6l4 4 4-4"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
          />
        </svg>
      </button>

      {open && (
        <div
          ref={menuRef}
          className="cc-menu"
          role="menu"
          aria-label={t("switcher.menuLabel")}
          onKeyDown={onMenuKeyDown}
          style={
            anchor === null
              ? // First layout pass, before the trigger has been measured.
                // Hidden rather than parked at 0,0: a menu that flashes in the
                // top-left corner of the window is a menu that looks broken.
                { visibility: "hidden" }
              : {
                  top: anchor.top,
                  left: anchor.left,
                  width: anchor.width,
                  maxHeight: anchor.maxHeight,
                }
          }
        >
          {/* `role="none"` on the three generic wrappers between the menu and
              its items — this one, `.cc-menu-item` below and `.cc-menu-foot`.
              ARIA 1.2 requires a `menu` to OWN its `menuitem`s (or a `group`
              of them); an unroled div in between breaks that relationship, and
              some assistive tech responds by mis-counting the menu or dropping
              position-in-set entirely. Presentational wrappers are exactly what
              `none` is for. Purely additive: no visual or behavioural change. */}
          <div className="cc-menu-scroll" role="none">
            {profiles === null ? (
              <p className="cc-menu-note">{t("common.readingConnections")}</p>
            ) : groups.length === 0 ? (
              <p className="cc-menu-note">{t("switcher.empty")}</p>
            ) : (
              groups.map((group) => (
                <div
                  className="cc-menu-group"
                  key={group.name}
                  role="group"
                  // The visible label is hidden from assistive tech below, so
                  // the group name is announced once, on entry, rather than
                  // twice — once as a heading and once as the group.
                  aria-label={group.name}
                >
                  <div className="cc-menu-label" aria-hidden="true">
                    {group.name}
                  </div>
                  {group.profiles.map((profile) => {
                    const status =
                      connections[profile.id]?.status ?? "disconnected";
                    const address = profile.bootstrap_servers.join(", ");
                    const statusWord = t(STATUS_KEY[status]);
                    const isCurrent = profile.id === selectedId;
                    const connected = status === "connected";
                    const busy = status === "connecting";
                    // Guardrail layer 6, carried over from `.profile-row`
                    // verbatim: the row stays tinted whether it is selected or
                    // not, for every PROTECTED environment — not for the one
                    // that happens to be spelled "prod".
                    const classes = [
                      "cc-menu-item",
                      group.def.protected ? "cc-menu-item-protected" : "",
                      isCurrent ? "cc-menu-item-selected" : "",
                    ]
                      .filter(Boolean)
                      .join(" ");
                    return (
                      <div className={classes} key={profile.id} role="none">
                        <button
                          type="button"
                          role="menuitem"
                          tabIndex={-1}
                          className="cc-menu-row"
                          // Which cluster is open is carried by a tint and a
                          // 2px left border — nothing a screen reader can see.
                          // `aria-current` is the one channel that says "this
                          // is the one you are on" without inventing a control
                          // state this row does not have.
                          aria-current={isCurrent ? "true" : undefined}
                          title={address}
                          onClick={() => {
                            onSelect(profile.id);
                            close(true);
                          }}
                        >
                          <span className="cc-row-line">
                            <span
                              className={`status-dot status-${status}`}
                              aria-hidden="true"
                            />
                            <span className="cc-row-name">{profile.name}</span>
                            <EnvChip env={profile.environment} />
                            {/* THE THIRD CHANNEL. §6 and audit C8 both say a
                                protected environment is spelled three ways;
                                this row carried two, and both of them were
                                colour — the warm ground and the 2px --danger
                                spine. The chip's text names the ENVIRONMENT,
                                not its protection, so an org whose protected
                                environment is called "UAT" read nothing at all.
                                The glyph is the visible third channel and the
                                sr-only word is the announced one; the visible
                                text of this row is already the cluster's name
                                and address, so printing "Protected" a fourth
                                time in the line would crowd out the address
                                the guardrail depends on. */}
                            {group.def.protected && (
                              <>
                                <PadLock />
                                <span className="sr-only">
                                  {t("switcher.protected")}
                                </span>
                              </>
                            )}
                          </span>
                          {/* Prod guardrail layer 3: the bootstrap address is
                              always on screen. And law 2: the status dot always
                              has its word — this line reads "address · state"
                              in every state, never just "address". */}
                          <span className="cc-row-meta">
                            {t("switcher.rowMeta", {
                              address,
                              status: statusWord,
                            })}
                          </span>
                        </button>
                        {/* The whole point of the two-control row: choosing a
                            cold cluster no longer swaps the workspace for a
                            form. The row selects; this connects. */}
                        <button
                          type="button"
                          role="menuitem"
                          tabIndex={-1}
                          className="btn btn-sm cc-menu-act"
                          /* `aria-disabled`, NEVER `disabled`. A disabled
                             <button> is not focusable, and the roving focus
                             above walks a FLAT list of menu items by index: it
                             calls `focus()` on the next one and reads
                             `document.activeElement` to find the current one.
                             Focusing an unfocusable button is a silent no-op,
                             so activeElement never moves, so the next Arrow
                             press computes the same index and lands on the same
                             dead control — Arrow navigation stuck at whichever
                             cluster is connecting. That window is not small: a
                             connect against an unreachable broker holds
                             `connecting` for the whole timeout, which is
                             exactly when you open this menu to go elsewhere.
                             aria-disabled keeps the item focusable, counted and
                             announced; the guard in `onClick` is what actually
                             makes the press do nothing. */
                          aria-disabled={busy ? true : undefined}
                          title={t(
                            connected
                              ? "switcher.disconnectTitle"
                              : "switcher.connectTitle",
                            { name: profile.name },
                          )}
                          onClick={() => {
                            // The other half of aria-disabled. Without this a
                            // second press would queue a second connect.
                            if (busy) return;
                            if (connected) onDisconnect(profile.id);
                            else onConnect(profile);
                            close(true);
                          }}
                        >
                          {t(
                            busy
                              ? "switcher.connecting"
                              : connected
                                ? "switcher.disconnect"
                                : "switcher.connect",
                          )}
                        </button>
                      </div>
                    );
                  })}
                </div>
              ))
            )}
          </div>

          {/* One of three homes for this action, and the least surprising: the
              other two are ⌘K and the empty states. It is a foot strip rather
              than a row in the list because it is not a cluster. */}
          <div className="cc-menu-foot" role="none">
            <button
              type="button"
              role="menuitem"
              tabIndex={-1}
              className="cc-menu-add"
              onClick={() => {
                onNew();
                close(true);
              }}
            >
              {t("common.addConnection")}
            </button>
          </div>
        </div>
      )}
    </>
  );
}
