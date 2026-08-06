import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { SUPPORT_URL } from "./AboutDialog";
import {
  ACCENTS,
  DENSITIES,
  FONT_SIZES,
  MOTIONS,
  PERCH_MODES,
  THEMES,
  useAppearance,
  type AccentId,
  type Density,
  type FontSize,
  type MotionPref,
  type PerchMode,
  type ThemePref,
} from "./appearance";
import { errorMessage, updatesCheck, type UpdateChannel, type UpdateCheck } from "./api";
import { formatStamp } from "./format";
import {
  UPDATE_CHANNELS,
  clearDismissal,
  noteChecked,
  readLastCheck,
  setUpdatePrefs,
  updateFailure,
  useUpdatePrefs,
  type LastCheck,
  type UpdateFailure,
  type UpdateOffer,
} from "./updates";
import { LOCALES, useI18n, type Locale, type MessageKey } from "./i18n";
import { envAttrs, useEnvironments } from "./environments";
import EnvironmentsManager from "./EnvironmentsManager";
import Perch from "./Perch";
import StageHead from "./StageHead";

/**
 * SETTINGS — an app-level surface, reachable with NO cluster connected.
 *
 * That is the whole reason it exists as a view rather than a dialog, and the
 * reason it is a RAIL ITEM in the Application group: the two preferences
 * people most want on first launch are the theme and the font size, and on
 * first launch there is nothing to connect to — so a Settings that lived
 * inside a cluster workspace would be a Settings a new user cannot reach.
 *
 * NO SAVE BUTTON. Every control applies on change and persists on change —
 * a preference you have to commit is a preference you cannot preview, and
 * appearance is the one category of setting where previewing IS the decision.
 *
 * DIAGNOSTICS AND MCP ARE NOT MOVED HERE. They live in the About dialog, they
 * are linked from here, and both stay where every existing link and every
 * screenshot in the docs already points. Settings gains a door, not a landlord.
 * About and Support Kavka DID move here, from the deleted sidebar footer.
 */

type Section = "appearance" | "language" | "updates" | "about";

const SECTION_KEY: Record<Section, MessageKey> = {
  appearance: "settings.section.appearance",
  language: "settings.section.language",
  updates: "settings.section.updates",
  about: "settings.section.about",
};

// Updates sits before About because the README, the website and the About
// panel's own no-telemetry paragraph all point at "Settings → Updates" by
// name — it is the switch those sentences promise, so it is a section of its
// own rather than a row inside a dialog somebody has to find.
const SECTIONS: readonly Section[] = [
  "appearance",
  "language",
  "updates",
  "about",
];

const CHANNEL_KEY: Record<UpdateChannel, MessageKey> = {
  stable: "settings.updates.channel.stable",
  builds: "settings.updates.channel.builds",
};

const THEME_KEY: Record<ThemePref, MessageKey> = {
  system: "settings.theme.system",
  light: "settings.theme.light",
  dark: "settings.theme.dark",
};
const ACCENT_KEY: Record<AccentId, MessageKey> = {
  brass: "settings.accent.brass",
  moss: "settings.accent.moss",
  sky: "settings.accent.sky",
  plum: "settings.accent.plum",
};
const DENSITY_KEY: Record<Density, MessageKey> = {
  comfortable: "settings.density.comfortable",
  compact: "settings.density.compact",
};
const FONT_KEY: Record<FontSize, MessageKey> = {
  s: "settings.font.s",
  m: "settings.font.m",
  l: "settings.font.l",
};
const MOTION_KEY: Record<MotionPref, MessageKey> = {
  system: "settings.motion.system",
  reduce: "settings.motion.reduce",
};
const PERCH_KEY: Record<PerchMode, MessageKey> = {
  full: "settings.perch.full",
  line: "settings.perch.line",
  hidden: "settings.perch.hidden",
};

/**
 * The sentence each section's header says about itself.
 *
 * Not decoration: it is the one honest line about what the section can and
 * cannot change — "None of this changes a cluster" is the answer to the fear
 * that stops people opening Settings on a production connection at all.
 */
const SECTION_SUB: Record<Section, MessageKey> = {
  appearance: "settings.section.appearance.sub",
  language: "settings.section.language.sub",
  updates: "settings.section.updates.sub",
  about: "settings.section.about.sub",
};

/**
 * THE KEYBOARD `role="radio"` PROMISES.
 *
 * A radiogroup is announced as "1 of 3" and the next thing a screen-reader
 * user presses is an arrow key. Tab-per-option answers nothing there, so both
 * halves of the pattern are here: arrows (and Home/End) move the selection and
 * the focus together, and only the checked option is in the tab order, so the
 * group is one Tab stop rather than three.
 *
 * Selection follows focus, which is the correct behaviour for a radiogroup
 * whose every option applies instantly and reverses instantly — see the file
 * header on why nothing in Settings is committed.
 */
function useRovingRadio<T extends string>(
  options: readonly T[],
  value: T,
  onChange: (next: T) => void,
) {
  const refs = useRef<Partial<Record<T, HTMLButtonElement | null>>>({});
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step =
      e.key === "ArrowRight" || e.key === "ArrowDown"
        ? 1
        : e.key === "ArrowLeft" || e.key === "ArrowUp"
          ? -1
          : 0;
    let next: T | undefined;
    if (step !== 0) {
      // `indexOf` is -1 only if the stored value is not an option at all,
      // which the types forbid; wrapping from 0 is the harmless answer.
      const from = options.indexOf(value);
      next = options[(from + step + options.length) % options.length];
    } else if (e.key === "Home") next = options[0];
    else if (e.key === "End") next = options[options.length - 1];
    else return;
    if (next === undefined) return;
    e.preventDefault();
    onChange(next);
    refs.current[next]?.focus();
  };
  return { refs, onKeyDown };
}

/**
 * A segmented radio group. `role="radiogroup"` and not a `<select>` because
 * every option is worth showing at once and there are never more than three;
 * every segment carries its WORD, so no state here is colour-only.
 */
function Segmented<T extends string>({
  label,
  value,
  options,
  labelFor,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly T[];
  labelFor: (option: T) => string;
  onChange: (next: T) => void;
}) {
  const { refs, onKeyDown } = useRovingRadio(options, value, onChange);
  return (
    <div
      className="seg"
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
    >
      {options.map((option) => (
        <button
          key={option}
          ref={(el) => {
            refs.current[option] = el;
          }}
          type="button"
          role="radio"
          aria-checked={value === option}
          tabIndex={value === option ? 0 : -1}
          className="seg-btn"
          onClick={() => onChange(option)}
        >
          {labelFor(option)}
        </button>
      ))}
    </div>
  );
}

/**
 * The accent picker. Same pattern as `Segmented` and deliberately not the same
 * component: these options are colours, so each one carries its NAME in an
 * `aria-label` and a `title` rather than in a visible word.
 */
function Swatches({
  label,
  value,
  options,
  labelFor,
  onChange,
}: {
  label: string;
  value: AccentId;
  options: readonly AccentId[];
  labelFor: (option: AccentId) => string;
  onChange: (next: AccentId) => void;
}) {
  const { refs, onKeyDown } = useRovingRadio(options, value, onChange);
  return (
    <div
      className="swatches"
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
    >
      {options.map((option) => {
        const name = labelFor(option);
        return (
          <button
            key={option}
            ref={(el) => {
              refs.current[option] = el;
            }}
            type="button"
            role="radio"
            aria-checked={value === option}
            tabIndex={value === option ? 0 : -1}
            // The name, not just the colour: a picker whose options are only
            // colours is unusable to the people most likely to open it.
            aria-label={name}
            title={name}
            className={`swatch swatch-${option}`}
            onClick={() => onChange(option)}
          >
            <span className="swatch-dot" />
          </button>
        );
      })}
    </div>
  );
}

function Row({
  title,
  help,
  helpId,
  children,
}: {
  title: string;
  help: string;
  /**
   * Set when the control is a bare checkbox whose own label is too short to
   * carry the explanation — the help line then IS the description, and
   * `aria-describedby` points at it rather than at a duplicate hint nobody
   * can see. A sibling of the label, so it describes the control instead of
   * renaming it (DESIGN.md §5.3).
   */
  helpId?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <div className="settings-row-title">{title}</div>
        <p className="settings-row-help" id={helpId}>
          {help}
        </p>
      </div>
      <div className="settings-row-ctl">{children}</div>
    </div>
  );
}

/** Two miniature windows, each painted in its own theme's literal colours,
    so "System" is a picture rather than a promise. */
function ThemePreview() {
  return (
    <div className="theme-preview" aria-hidden="true">
      {(["dark", "light"] as const).map((which) => (
        <div key={which} className={`tp-card tp-${which}`}>
          <div className="tp-bar" />
          <div className="tp-body">
            <div className="tp-line tp-line-accent" style={{ width: "42%" }} />
            <div className="tp-line" style={{ width: "80%" }} />
            <div className="tp-line" style={{ width: "62%" }} />
          </div>
        </div>
      ))}
    </div>
  );
}

/**
 * "WHAT THIS LOOKS LIKE" — the preference made legible before it is committed.
 *
 * Density and text size are the two settings nobody can predict from their
 * name: "Compact" is a promise about a table the user cannot see from the
 * Settings screen, so the choice is made blind and then undone on the screen
 * where it turned out to be wrong. This panel is three rows of the app's real
 * table idiom — same tokens, same row height, same mono columns — so every
 * control above it changes something visible in the same viewport. It is the
 * same argument as the theme mini-previews the app already shipped; the app
 * agreed with the principle and stopped one row early.
 *
 * THE ROWS ARE LABELLED AS INVENTED. They look exactly like message rows,
 * which is the point and also the risk: a table of plausible offsets and keys
 * that came from nowhere is the one thing this product may not put on screen
 * unmarked. The note above it says so before the table, not after.
 *
 * THE FOOT IS THE COMPONENT SAMPLER. A primary button, a normal one, and the
 * two health chips — the four surfaces whose contrast a theme or accent change
 * can break — plus the line that tells you to Tab through them, because the
 * focus ring is the one thing a screenshot of a theme never shows you.
 */
function SamplePanel() {
  const { t, tx } = useI18n();
  return (
    <section className="panel" aria-label={t("settings.sample.title")}>
      <div className="panel-head">
        <h2 className="panel-title">{t("settings.sample.title")}</h2>
        <span className="panel-head-note">{t("settings.sample.sub")}</span>
      </div>

      {/* Before the table, never under it: a qualification the reader meets
          after the numbers is a qualification that has already done its
          damage. */}
      <p className="table-note">{t("settings.sample.note")}</p>

      <div className="table-wrap">
        <table className="data-table">
          <caption className="sr-only">{t("settings.sample.caption")}</caption>
          <thead>
            <tr>
              <th scope="col" className="ledger-gutter">
                Part
              </th>
              <th scope="col" className="col-num">
                Offset
              </th>
              <th scope="col">Key</th>
              <th scope="col">Value</th>
            </tr>
          </thead>
          <tbody>
            {SAMPLE_ROWS.map((row) => (
              <tr key={row.offset}>
                <td className="ledger-gutter">{row.partition}</td>
                <td className="col-num cell-num">{row.offset}</td>
                <td className="cell-mono">{row.key}</td>
                <td className="cell-mono">{row.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="panel-foot panel-foot-row">
        <button type="button" className="btn btn-primary btn-sm">
          {t("settings.sample.primary")}
        </button>
        <button type="button" className="btn btn-sm">
          {t("settings.sample.normal")}
        </button>
        {/* The app's own health chip — dot AND word, so the sampler shows the
            same Law 2 shape every status in Kavka uses. */}
        <span className="health health-ok">
          <i className="dot" aria-hidden="true" />
          {t("settings.sample.chip.ok")}
        </span>
        <span className="health health-warn">
          <i className="dot" aria-hidden="true" />
          {t("settings.sample.chip.warn")}
        </span>
        <span className="panel-foot-tail">
          {tx("settings.sample.focus", { key: <kbd className="kbd">Tab</kbd> })}
        </span>
      </div>
    </section>
  );
}

/**
 * Three rows of nothing. Deliberately mundane — an invented row that looks
 * like an incident would teach the reader something untrue about their own
 * cluster while they are choosing a font size.
 */
const SAMPLE_ROWS = [
  { partition: 2, offset: "1 407", key: "cust-4471", value: '{"status": "paid"}' },
  { partition: 2, offset: "1 406", key: "cust-5510", value: '{"status": "held"}' },
  { partition: 0, offset: "1 419", key: "cust-9034", value: '{"status": "sent"}' },
] as const;

/**
 * UPDATES — the switch every honesty claim in this product points at.
 *
 * The README's *What Kavka sends*, the website, the feature spec and the
 * About panel's no-telemetry paragraph all say "Settings → Updates turns it
 * off" in those words. This section is what makes that sentence true, which
 * is why the disclosure beside the toggle is the LONGEST piece of copy in
 * Settings and must not be shortened: it names the host, the frequency, what
 * the request does not carry, and the fact that nothing installs on its own.
 * A user has to be able to check every clause of it.
 *
 * THE VERDICT IS NEVER CHEERFUL ABOUT A CHECK THAT FAILED. A failed request
 * renders as a failure (through `updateFailure`, which consults the error
 * library and then says github.com out loud rather than inheriting a sentence
 * about brokers), and the last-checked line says "tried" rather than
 * "checked". `no-stable-release` is neither: today the repository has
 * published only automated builds, so the stable endpoint answers 404 — a
 * fact about the project, said as one. Rust reaches that status only for an
 * observed 404 (`stable_absence` in src-tauri/src/update.rs); a rate limit or
 * an outage arrives here as `error` and renders as one, because "nobody has
 * published a stable release" is a claim and a 503 is not evidence for it.
 */
function UpdatesSection({
  onUpdateFound,
}: {
  /**
   * A manual check that finds a release raises the app's banner — the one
   * surface with the Install button — instead of this panel growing a second
   * one. Settings renders above the workspace body, so the banner is on
   * screen the moment it is raised.
   */
  onUpdateFound: (offer: UpdateOffer) => void;
}) {
  const { t } = useI18n();
  const prefs = useUpdatePrefs();
  const [checking, setChecking] = useState(false);
  // The channel travels with the answer: a user who switches the picker while
  // a request is in flight must not be told "you are on the newest build"
  // about a question that was asked of the stable endpoint.
  const [result, setResult] = useState<{
    check: UpdateCheck;
    channel: UpdateChannel;
  } | null>(null);
  const [failure, setFailure] = useState<UpdateFailure | null>(null);
  const [last, setLast] = useState<LastCheck | null>(() => readLastCheck());
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const checkNow = useCallback(async () => {
    const channel = prefs.channel;
    setChecking(true);
    setFailure(null);
    setResult(null);
    try {
      const check = await updatesCheck(channel);
      // The stamp records the ATTEMPT, whatever came back — "at most once a
      // day" is a promise about requests, not about successes.
      noteChecked(check.status !== "error");
      if (check.status === "error") {
        if (mounted.current) setFailure(updateFailure(check.message, t));
      } else {
        if (check.status === "update") {
          // They asked out loud. An old "Not now" is not the answer to that.
          clearDismissal();
          onUpdateFound({ ...check, channel });
        }
        if (mounted.current) setResult({ check, channel });
      }
    } catch (err) {
      noteChecked(false);
      if (mounted.current) setFailure(updateFailure(errorMessage(err), t));
    } finally {
      if (mounted.current) {
        setLast(readLastCheck());
        setChecking(false);
      }
    }
  }, [prefs.channel, onUpdateFound, t]);

  let verdict = "";
  if (checking) {
    verdict = t("settings.updates.check.checking");
  } else if (result !== null) {
    const { check, channel } = result;
    if (check.status === "update") {
      verdict = t("settings.updates.result.update", { version: check.version });
    } else if (check.status === "current") {
      verdict = t(
        channel === "builds"
          ? "settings.updates.result.currentBuild"
          : "settings.updates.result.currentStable",
      );
    } else if (check.status === "no-stable-release") {
      verdict = t("settings.updates.result.noStable");
    }
  }

  const lastLine =
    last === null
      ? t("settings.updates.never")
      : last.ok
        ? t("settings.updates.lastChecked", { when: formatStamp(last.at) })
        : t("settings.updates.lastCheckedFailed", { when: formatStamp(last.at) });

  return (
    <section className="panel" aria-label={t("settings.section.updates")}>
      <div className="panel-head">
        <h2 className="panel-title">{t("settings.section.updates")}</h2>
      </div>

      <Row
        title={t("settings.updates.auto.title")}
        help={t("settings.updates.auto.hint")}
        helpId="updates-auto-help"
      >
        {/* 18px box in the check-field grid — 14px fails SC 2.5.8 — and the
            row's help line is the description rather than a second hint. */}
        <div className="check-field">
          <input
            type="checkbox"
            id="updates-auto"
            checked={prefs.auto}
            aria-describedby="updates-auto-help"
            onChange={(e) => setUpdatePrefs({ auto: e.target.checked })}
          />
          <label className="check-label" htmlFor="updates-auto">
            {t("settings.updates.auto.label")}
          </label>
        </div>
      </Row>

      <Row
        title={t("settings.updates.channel.title")}
        help={t("settings.updates.channel.help")}
      >
        <div className="settings-stack">
          <Segmented
            label={t("settings.updates.channel.title")}
            value={prefs.channel}
            options={UPDATE_CHANNELS}
            labelFor={(option) => t(CHANNEL_KEY[option])}
            onChange={(next) => {
              // An answer about the other channel is not an answer about this
              // one. Clearing it beats leaving a stale verdict beside the
              // picker that produced it.
              setResult(null);
              setFailure(null);
              setUpdatePrefs({ channel: next });
            }}
          />
          {/* Beside the choice it describes, the same way the language panel
              states that a catalog came out of a machine: a caveat about what
              you are on, shown when you are on it. The row's help line
              carries enough of it to make the choice informed either way. */}
          {prefs.channel === "builds" && (
            <p className="settings-row-help">
              {t("settings.updates.channel.warning")}
            </p>
          )}
        </div>
      </Row>

      <Row
        title={t("settings.updates.check.title")}
        help={t("settings.updates.check.help")}
      >
        <button
          type="button"
          className="btn"
          disabled={checking}
          onClick={() => void checkNow()}
        >
          {t("settings.updates.check.button")}
        </button>
      </Row>

      {/* Rendered unconditionally and empty until there is something to say:
          a live region that appears at the same moment as its text is a live
          region a screen reader never announces. Polite, because it answers a
          button the user just pressed and nothing is waiting on it. */}
      <p className="settings-row-help" role="status">
        {verdict}
      </p>

      {failure !== null && (
        // Assertive: this arrives while focus is still on the button that
        // started it, and a failure nobody is looking at is announced
        // assertively or not at all (docs/A11Y-AUDIT.md A11Y-38).
        <div role="alert">
          <div className="settings-row-title">{failure.title}</div>
          <p className="settings-row-help">{failure.detail}</p>
          {failure.raw !== "" && (
            <details className="banner-details">
              <summary>{t("common.showDetails")}</summary>
              <pre className="banner-raw">{failure.raw}</pre>
            </details>
          )}
        </div>
      )}

      <p className="settings-row-help">{lastLine}</p>
    </section>
  );
}

export default function SettingsView({
  onOpenAbout,
  onUpdateFound,
  onProfilesChanged,
}: {
  /** Diagnostics and the MCP section live in the About dialog. */
  onOpenAbout: () => void;
  /** See `UpdatesSection` — a manual check raises the app-level banner. */
  onUpdateFound: (offer: UpdateOffer) => void;
  /**
   * The environments manager can REWRITE PROFILES — deleting an environment
   * reassigns every connection that named it — and the shell is holding that
   * list. Optional so this screen still works when nothing passes it (Settings
   * has to render with the connection file unreadable), but wire it: without
   * it, the switcher menu keeps showing the old environment until the next
   * reload. See the report's integrator note.
   */
  onProfilesChanged?: () => void;
}) {
  const { t, locale, setLocale } = useI18n();
  const { appearance, theme, set } = useAppearance();
  const [section, setSection] = useState<Section>("appearance");
  /**
   * The environments modal's SECOND opener. It had exactly one in the whole
   * app — a ghost button inside the connection form — which meant the app's
   * identity system was reachable only from a screen you go to for another
   * reason. The mockup opens it from three places; this is the one a user
   * actually looks for.
   */
  const [managingEnvs, setManagingEnvs] = useState(false);
  const envs = useEnvironments();

  const activeLocale = LOCALES.find((info) => info.code === locale);

  return (
    // The stage's content column, same as every cluster screen: there is one
    // rail and it belongs to the shell, so Settings needs no variant.
    <div className="cluster-view">
      {/* The screen introduces itself before the Perch says how it is going —
          the same order every cluster screen keeps. The title is the SECTION,
          because that is what changes under the reader; the trail is the rail's
          own words for how they got here. */}
      <StageHead
        trail={[t("rail.group.application"), t("rail.item.settings")]}
        title={t(SECTION_KEY[section])}
        sub={t(SECTION_SUB[section])}
      />

      {/* Live: the verdict names the theme actually resolved right now, which
          is the one thing "System" leaves ambiguous. */}
      <Perch screen={t("settings.title")} tone="ok">
        {t("settings.perch", {
          theme: t(theme === "dark" ? "settings.theme.dark" : "settings.theme.light"),
        })}
      </Perch>

      {managingEnvs && (
        <EnvironmentsManager
          onClose={() => setManagingEnvs(false)}
          onProfilesChanged={() => onProfilesChanged?.()}
          // Nothing on this screen is holding an environment name in local
          // state, so a rename has nowhere to follow it to. The connection
          // form's copy of this modal is the one that needs the callback.
          onEnvironmentMoved={() => undefined}
        />
      )}

      <div className="settings">
        <nav className="settings-nav" aria-label={t("settings.navLabel")}>
          {SECTIONS.map((id) => (
            <button
              key={id}
              type="button"
              className="settings-nav-item"
              aria-current={section === id ? "true" : undefined}
              onClick={() => setSection(id)}
            >
              {t(SECTION_KEY[id])}
            </button>
          ))}
        </nav>

        <div className="settings-sections">
          {section === "appearance" && (
            <section className="panel" aria-label={t("settings.section.appearance")}>
              <div className="panel-head">
                <h2 className="panel-title">{t("settings.section.appearance")}</h2>
              </div>

              <Row title={t("settings.theme.title")} help={t("settings.theme.help")}>
                <div className="settings-stack">
                  <Segmented
                    label={t("settings.theme.title")}
                    value={appearance.theme}
                    options={THEMES}
                    labelFor={(option) => t(THEME_KEY[option])}
                    onChange={(next) => set({ theme: next })}
                  />
                  <ThemePreview />
                  {/* The floor, stated where the choice is made. Both themes
                      are held to it, which is the reason neither of them
                      dims text to look calmer — and a user who has been
                      handed a "dark mode" that was unreadable somewhere else
                      has no way to know that unless it is written down. */}
                  <p className="settings-row-help">
                    {t("settings.theme.contrast")}
                  </p>
                </div>
              </Row>

              {/* The help line NAMES the accent that is on. A swatch grid
                  whose only feedback is which square has a ring is a control
                  a colour-blind user cannot read back — and the second half
                  of the sentence is the promise that makes changing it safe:
                  the accent never carries status, so no setting here can
                  hide a warning. */}
              <Row
                title={t("settings.accent.title")}
                help={t("settings.accent.note", {
                  name: t(ACCENT_KEY[appearance.accent]),
                })}
              >
                <Swatches
                  label={t("settings.accent.title")}
                  value={appearance.accent}
                  options={ACCENTS}
                  labelFor={(option) => t(ACCENT_KEY[option])}
                  onChange={(next) => set({ accent: next })}
                />
              </Row>

              <Row title={t("settings.density.title")} help={t("settings.density.help")}>
                <Segmented
                  label={t("settings.density.title")}
                  value={appearance.density}
                  options={DENSITIES}
                  labelFor={(option) => t(DENSITY_KEY[option])}
                  onChange={(next) => set({ density: next })}
                />
              </Row>

              <Row title={t("settings.font.title")} help={t("settings.font.help")}>
                <Segmented
                  label={t("settings.font.title")}
                  value={appearance.fontSize}
                  options={FONT_SIZES}
                  labelFor={(option) => t(FONT_KEY[option])}
                  onChange={(next) => set({ fontSize: next })}
                />
              </Row>

              <Row title={t("settings.motion.title")} help={t("settings.motion.help")}>
                <Segmented
                  label={t("settings.motion.title")}
                  value={appearance.motion}
                  options={MOTIONS}
                  labelFor={(option) => t(MOTION_KEY[option])}
                  onChange={(next) => set({ motion: next })}
                />
              </Row>

              {/* THE PERCH PREFERENCE. Three states rather than a switch, and
                  the help line carries the clause that makes it safe to offer
                  at all: neither "One line" nor "Hidden" can suppress a screen
                  that is still loading or one whose read failed. `Perch.tsx`
                  forces the full note back in every mode for both, and takes
                  its own Hide control away while it does. */}
              <Row title={t("settings.perch.title")} help={t("settings.perch.help")}>
                <Segmented
                  label={t("settings.perch.title")}
                  value={appearance.perch}
                  options={PERCH_MODES}
                  labelFor={(option) => t(PERCH_KEY[option])}
                  onChange={(next) => set({ perch: next })}
                />
              </Row>

              {/* ENVIRONMENT COLOURS. The one row here that is not local to
                  this machine — an environment definition travels with an
                  exported profile, because a colour nobody else sees is not
                  how a team recognises production. The chips are the live
                  definitions, so the row is also the answer to "which ones do
                  I have"; the button is the modal's second door. */}
              <Row
                title={t("settings.env.title")}
                help={t("settings.env.help", { count: envs.length })}
              >
                <div className="settings-env-ctl">
                  <span className="settings-env-chips">
                    {/* Law 2 survives the shrink: every chip spells its own
                        name, so the row is readable with no colour at all. */}
                    {envs.map((def) => (
                      <span
                        key={def.name}
                        className="env-chip"
                        {...envAttrs(def)}
                      >
                        {def.name}
                      </span>
                    ))}
                  </span>
                  <button
                    type="button"
                    className="btn"
                    onClick={() => setManagingEnvs(true)}
                  >
                    {t("settings.env.manage")}
                  </button>
                </div>
              </Row>
            </section>
          )}

          {/* Outside the Appearance panel, not inside it: it is not a
              preference, it is the consequence of five of them. */}
          {section === "appearance" && <SamplePanel />}

          {section === "language" && (
            <section className="panel" aria-label={t("settings.section.language")}>
              <div className="panel-head">
                <h2 className="panel-title">{t("settings.section.language")}</h2>
              </div>

              <Row
                title={t("settings.language.title")}
                help={t("settings.language.help")}
              >
                {/* The same picker the About dialog has, reading the same
                    store — two copies of one control, never two settings. */}
                <select
                  aria-label={t("settings.language.title")}
                  value={locale}
                  onChange={(e) => setLocale(e.target.value as Locale)}
                >
                  {LOCALES.map((info) => (
                    <option key={info.code} value={info.code}>
                      {info.endonym}
                    </option>
                  ))}
                </select>
              </Row>

              {/* A user deciding whether to trust a translation deserves to
                  know it came out of a machine. The flag is the contract. */}
              {activeLocale?.machine === true && (
                <p className="settings-row-help">{t("settings.language.machine")}</p>
              )}
            </section>
          )}

          {section === "updates" && (
            <UpdatesSection onUpdateFound={onUpdateFound} />
          )}

          {section === "about" && (
            <section className="panel" aria-label={t("settings.section.about")}>
              <div className="panel-head">
                <h2 className="panel-title">{t("settings.section.about")}</h2>
              </div>

              <Row title={t("settings.about.title")} help={t("settings.about.help")}>
                <button type="button" className="btn" onClick={onOpenAbout}>
                  {t("settings.about.open")}
                </button>
              </Row>

              {/* Support Kavka lived in the sidebar footer, and deleting the
                  sidebar (DESIGN.md §5.1) orphaned it. It comes here rather
                  than into new chrome the mockup does not have: an OSS,
                  donation-funded app owes people a way to find this, and
                  About is where somebody already goes to read the licence.
                  ⌘K still reaches About; this is the browsable home. */}
              <Row
                title={t("settings.support.title")}
                help={t("settings.support.help")}
              >
                <a
                  className="btn"
                  href={SUPPORT_URL}
                  onClick={(e) => {
                    e.preventDefault();
                    openUrl(SUPPORT_URL).catch(console.error);
                  }}
                >
                  {t("common.support")}
                </a>
              </Row>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
