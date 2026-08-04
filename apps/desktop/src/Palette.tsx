import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ConnectionProfile, ConnState, Environment } from "./api";
import { EnvChip } from "./Sidebar";
import { SUPPORT_URL } from "./AboutDialog";
import Overlay from "./Overlay";
import { useI18n } from "./i18n";

/**
 * The command palette — DESIGN.md §5.9.
 *
 * ⌘K is the real navigation; the sidebar is not. Rows are
 * `[glyph] [label] [context] ······ [kbd]` and the active row is styled
 * IDENTICALLY to a selected table row, so the pattern is learned once.
 *
 * Matching is bilingual (§5.9): every action carries the Kafka vocabulary in
 * `keywords`, so `bootstrap`, `broker` or `metadata` find the plain-language
 * label. Relabelling a control for a novice must never hide the term from an
 * expert's search.
 *
 * ARIA follows the combobox-with-listbox pattern: the input owns the keyboard
 * and points at the active row with `aria-activedescendant`, so focus never
 * leaves the one control the user is typing into.
 */

/** `⌘K` on macOS, `Ctrl K` everywhere else. Used by the status bar hint too. */
export function paletteKeyLabel(): string {
  return /mac/i.test(navigator.userAgent) ? "⌘K" : "Ctrl K";
}

export interface PaletteCommands {
  /** Select the profile and start connecting. */
  connect: (profile: ConnectionProfile) => void;
  /** Select a profile that is already connected (or connecting). */
  goTo: (profileId: string) => void;
  addConnection: () => void;
  disconnect: (profileId: string) => void;
  refreshTopics: () => void;
  exportConnections: () => void;
  importConnections: () => void;
  about: () => void;
}

/**
 * One row. Exported because the app root contributes CONTEXTUAL rows — "Search
 * in orders.v2", "Produce to orders.v2" — which only the view that owns the
 * topic can build. They arrive as data with their handlers already bound, so
 * the palette still knows nothing about topics, panes or produce panels.
 */
export interface PaletteAction {
  id: string;
  /** One mono glyph. Decoration — the label always carries the meaning. */
  glyph: string;
  label: string;
  /** Tertiary line after the label: the address, the cluster, the reason. */
  context?: string;
  /** Kafka vocabulary that must match even though it isn't in the label. */
  keywords?: string;
  /** Renders the environment chip, so prod is legible before you press ⏎. */
  env?: Environment;
  /** Prod styling: the context line goes coral (§5.9). */
  danger?: boolean;
  /** Set = the row is greyed and says why. Never a dead control. */
  disabledReason?: string;
  /** The action keeps the palette open and closes it itself. */
  keepOpen?: boolean;
  run: () => void;
}

/**
 * Subsequence match with word-start and run bonuses. Returns null for "no
 * match at all" so the caller can drop the row, and a bigger number for a
 * better match. Deliberately small: this ranks at most a few dozen rows.
 */
function score(query: string, hay: string): number | null {
  if (query === "") return 0;
  let qi = 0;
  let total = 0;
  let run = 0;
  let prev = -2;
  for (let i = 0; i < hay.length && qi < query.length; i++) {
    if (hay[i] !== query[qi]) continue;
    const wordStart = i === 0 || /[\s\-_./:@]/.test(hay[i - 1]);
    const consecutive = prev === i - 1;
    run = consecutive ? run + 1 : 0;
    total += 1 + (wordStart ? 4 : 0) + (consecutive ? 3 + run : 0);
    prev = i;
    qi++;
  }
  if (qi < query.length) return null;
  // A literal hit beats a scattered subsequence: typing "topics" should put
  // "Refresh topics" above a row that merely contains those letters in order.
  return hay.includes(query) ? total + 10 : total;
}

interface PaletteProps {
  profiles: ConnectionProfile[];
  connections: Record<string, ConnState>;
  selectedId: string | null;
  commands: PaletteCommands;
  /** Rows for whatever is on screen right now. First, because they are. */
  contextual?: PaletteAction[];
  onClose: () => void;
}

export default function Palette({
  profiles,
  connections,
  selectedId,
  commands,
  contextual,
  onClose,
}: PaletteProps) {
  const { t, tx } = useI18n();
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  // Same failure as the About dialog: if the browser can't be handed the URL,
  // the address goes on screen rather than nothing happening.
  const [unopened, setUnopened] = useState<string | null>(null);

  const actions = useMemo<PaletteAction[]>(() => {
    // What is on screen goes first: with no query the palette shows
    // "Suggested", and the topic you are looking at is the most suggestible
    // thing there is.
    const list: PaletteAction[] = contextual ? [...contextual] : [];

    for (const p of profiles) {
      const status = connections[p.id]?.status ?? "disconnected";
      const address = p.bootstrap_servers.join(", ");
      // Built as parts and joined, rather than as one sentence with three
      // optional fragments: a translator gets whole words to translate and
      // the " · " separator stays punctuation the catalog never sees.
      const bits = [address];
      if (status === "connected") bits.push(t("palette.state.connected"));
      else if (status === "connecting") bits.push(t("palette.state.connecting"));
      if (p.environment === "prod") bits.push(t("palette.prodCluster"));
      list.push({
        id: `profile:${p.id}`,
        glyph: "→",
        // The verb survives the whole flow (§7 rule 2): a cluster that is
        // already up is somewhere you GO, not something you connect.
        label:
          status === "connected"
            ? t("palette.goTo", { name: p.name })
            : t("palette.connectTo", { name: p.name }),
        // Prod guardrail layer 3: the address is on screen before you commit.
        context: bits.join(" · "),
        // §5.9's bilingual matching, and the reason every `*.kw` catalog entry
        // KEEPS the English words and appends the local ones rather than
        // replacing them: `bootstrap` and `broker` are Kafka's vocabulary in
        // every language, and an operator who learned Kafka in English must
        // still be able to type them.
        keywords: `${t("palette.profile.kw")} ${p.environment} ${address}`,
        env: p.environment,
        danger: p.environment === "prod",
        run: () =>
          status === "disconnected" ? commands.connect(p) : commands.goTo(p.id),
      });
    }

    list.push({
      id: "add",
      glyph: "+",
      label: t("common.addConnection"),
      context: t("palette.add.context"),
      keywords: t("palette.add.kw"),
      run: commands.addConnection,
    });

    // Disconnect targets the cluster you are looking at. If you are looking at
    // something else and exactly one cluster is up, it targets that one and
    // says so — anything more ambiguous is greyed with the reason, never a
    // guess about which cluster you meant.
    const selected = profiles.find((p) => p.id === selectedId) ?? null;
    const connected = profiles.filter(
      (p) => connections[p.id]?.status === "connected",
    );
    const target =
      selected && connections[selected.id]?.status === "connected"
        ? selected
        : connected.length === 1
          ? connected[0]
          : null;
    list.push({
      id: "disconnect",
      glyph: "×",
      label: t("palette.disconnect"),
      context: target ? target.name : undefined,
      keywords: t("palette.disconnect.kw"),
      env: target?.environment,
      danger: target?.environment === "prod",
      disabledReason:
        target !== null
          ? undefined
          : connected.length === 0
            ? t("palette.disconnect.none")
            : t("palette.disconnect.ambiguous"),
      run: () => {
        if (target) commands.disconnect(target.id);
      },
    });

    // Only where it means something: the topic list belongs to the cluster on
    // screen, so without one there is nothing to refresh.
    if (selected && connections[selected.id]?.status === "connected") {
      list.push({
        id: "refresh",
        glyph: "↻",
        label: t("palette.refresh"),
        context: selected.name,
        keywords: t("palette.refresh.kw"),
        run: commands.refreshTopics,
      });
    }

    list.push(
      {
        id: "export",
        glyph: "↑",
        label: t("palette.export"),
        context: t("palette.export.context"),
        keywords: t("palette.export.kw"),
        run: commands.exportConnections,
      },
      {
        id: "import",
        glyph: "↓",
        label: t("palette.import"),
        context: t("palette.import.context"),
        keywords: t("palette.import.kw"),
        run: commands.importConnections,
      },
      {
        id: "about",
        glyph: "?",
        label: t("about.title"),
        context: t("palette.about.context"),
        keywords: t("palette.about.kw"),
        run: commands.about,
      },
      {
        id: "support",
        glyph: "☕",
        label: t("common.support"),
        context: t("palette.support.context"),
        keywords: t("palette.support.kw"),
        keepOpen: true,
        run: () => {
          openUrl(SUPPORT_URL)
            .then(onClose)
            .catch(() => setUnopened(SUPPORT_URL));
        },
      },
    );

    return list;
    // `t` is in here on purpose: it is memoized on the locale, so this list
    // rebuilds the moment the language changes and never on any other render.
  }, [profiles, connections, selectedId, commands, contextual, onClose, t]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return actions;
    return actions
      .map((a, i) => ({
        a,
        i,
        s: score(
          q,
          `${a.label} ${a.context ?? ""} ${a.keywords ?? ""}`.toLowerCase(),
        ),
      }))
      .filter((r): r is { a: PaletteAction; i: number; s: number } => r.s !== null)
      .sort((x, y) => y.s - x.s || x.i - y.i)
      .map((r) => r.a);
  }, [actions, query]);

  const activeIdx = Math.min(active, Math.max(0, shown.length - 1));

  // Keep the active row in view without stealing focus from the input.
  useEffect(() => {
    const el = listRef.current?.children[activeIdx];
    if (el instanceof HTMLElement) el.scrollIntoView({ block: "nearest" });
  }, [activeIdx, shown.length]);

  const run = useCallback(
    (a: PaletteAction) => {
      if (a.disabledReason) return;
      if (!a.keepOpen) onClose();
      a.run();
    },
    [onClose],
  );

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (shown.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((activeIdx + 1) % shown.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((activeIdx - 1 + shown.length) % shown.length);
    } else if (e.key === "Home") {
      e.preventDefault();
      setActive(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActive(shown.length - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(shown[activeIdx]);
    }
  };

  return (
    <Overlay
      surfaceClass="palette"
      label={t("palette.label")}
      initialFocus={inputRef}
      onClose={onClose}
    >
      <div className="palette-inputwrap">
        <input
          ref={inputRef}
          type="text"
          className="palette-input"
          role="combobox"
          // Both track the list itself: with no matches there is no listbox
          // in the DOM, and aria-controls must not point at nothing.
          aria-expanded={shown.length > 0}
          aria-controls={shown.length > 0 ? "palette-list" : undefined}
          aria-autocomplete="list"
          aria-activedescendant={
            shown.length > 0 ? `palette-opt-${activeIdx}` : undefined
          }
          aria-label={t("palette.searchLabel")}
          placeholder={t("palette.searchPlaceholder")}
          spellCheck={false}
          autoComplete="off"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        <span className="palette-inputchip" aria-hidden="true">
          {paletteKeyLabel()}
        </span>
      </div>

      {shown.length === 0 ? (
        <p className="palette-empty" role="status">
          {t("palette.empty", { query: query.trim() })}
        </p>
      ) : (
        <ul
          ref={listRef}
          className="palette-list"
          id="palette-list"
          role="listbox"
          aria-label={t("palette.label")}
        >
          {shown.map((a, idx) => (
            <li
              key={a.id}
              id={`palette-opt-${idx}`}
              role="option"
              aria-selected={idx === activeIdx}
              aria-disabled={a.disabledReason ? true : undefined}
              // Every disabled control says why — on hover and in the row.
              title={a.disabledReason}
              className={[
                "palette-row",
                idx === activeIdx ? "palette-row-active" : "",
                a.danger ? "palette-row-danger" : "",
                a.disabledReason ? "palette-row-disabled" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              // Keep focus in the input so aria-activedescendant stays honest.
              onMouseDown={(e) => e.preventDefault()}
              onMouseMove={() => setActive(idx)}
              onClick={() => run(a)}
            >
              <span className="palette-glyph" aria-hidden="true">
                {a.glyph}
              </span>
              <span className="palette-label">{a.label}</span>
              {a.env && <EnvChip env={a.env} />}
              {(a.disabledReason ?? a.context) && (
                <span className="palette-context">
                  {a.disabledReason ?? a.context}
                </span>
              )}
              <span className="palette-kbd">
                {idx === activeIdx && !a.disabledReason ? (
                  <span className="kbd">⏎</span>
                ) : null}
              </span>
            </li>
          ))}
        </ul>
      )}

      {unopened && (
        <p className="dialog-note" role="status">
          {tx("common.linkFailed", { url: <code>{unopened}</code> })}
        </p>
      )}

      <div className="palette-foot" aria-hidden="true">
        <span className="kbd">↑</span>
        <span className="kbd">↓</span> {t("palette.foot.move")}
        <span className="palette-foot-sep">·</span>
        <span className="kbd">⏎</span> {t("palette.foot.run")}
        <span className="palette-foot-sep">·</span>
        <span className="kbd">Esc</span> {t("palette.foot.close")}
      </div>
    </Overlay>
  );
}
