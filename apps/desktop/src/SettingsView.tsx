import { useRef, useState } from "react";
import {
  ACCENTS,
  DENSITIES,
  FONT_SIZES,
  MOTIONS,
  THEMES,
  useAppearance,
  type AccentId,
  type Density,
  type FontSize,
  type MotionPref,
  type ThemePref,
} from "./appearance";
import { LOCALES, useI18n, type Locale, type MessageKey } from "./i18n";
import Perch from "./Perch";

/**
 * SETTINGS — an app-level surface, reachable with NO cluster connected.
 *
 * That is the whole reason it exists as a view rather than a dialog: the two
 * preferences people most want on first launch are the theme and the font
 * size, and on first launch there is nothing to connect to. It is opened from
 * the sidebar footer and closes back to whatever the workspace was showing.
 *
 * NO SAVE BUTTON. Every control applies on change and persists on change —
 * a preference you have to commit is a preference you cannot preview, and
 * appearance is the one category of setting where previewing IS the decision.
 *
 * DIAGNOSTICS AND MCP ARE NOT MOVED HERE. They live in the About dialog, they
 * are linked from here, and both stay where every existing link and every
 * screenshot in the docs already points. Settings gains a door, not a landlord.
 */

type Section = "appearance" | "language" | "about";

const SECTION_KEY: Record<Section, MessageKey> = {
  appearance: "settings.section.appearance",
  language: "settings.section.language",
  about: "settings.section.about",
};

const SECTIONS: readonly Section[] = ["appearance", "language", "about"];

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
  children,
}: {
  title: string;
  help: string;
  children: React.ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <div className="settings-row-title">{title}</div>
        <p className="settings-row-help">{help}</p>
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

export default function SettingsView({
  onOpenAbout,
}: {
  /** Diagnostics and the MCP section live in the About dialog. */
  onOpenAbout: () => void;
}) {
  const { t, locale, setLocale } = useI18n();
  const { appearance, theme, set } = useAppearance();
  const [section, setSection] = useState<Section>("appearance");

  const activeLocale = LOCALES.find((info) => info.code === locale);

  return (
    // The workspace grid with its rail column removed: Settings is app-level,
    // so there is no cluster rail beside it.
    <div className="cluster-view cluster-view-solo">
      {/* Live: the verdict names the theme actually resolved right now, which
          is the one thing "System" leaves ambiguous. */}
      <Perch screen={t("settings.title")} tone="ok">
        {t("settings.perch", {
          theme: t(theme === "dark" ? "settings.theme.dark" : "settings.theme.light"),
        })}
      </Perch>

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
                </div>
              </Row>

              <Row title={t("settings.accent.title")} help={t("settings.accent.help")}>
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
            </section>
          )}

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
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
