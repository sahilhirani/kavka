import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { updatesBuildNumber } from "./api";
import DiagnosticsSection from "./DiagnosticsSection";
import McpSection from "./McpSection";
import Overlay from "./Overlay";
import { LOCALES, useI18n, type Locale } from "./i18n";

/**
 * The two literal links Kavka is allowed to open. They live here — the one
 * component that shows both — and the sidebar and the palette import them, so
 * the allowlist in `src-tauri/capabilities/default.json` has two literals to
 * match and neither can drift by being retyped somewhere else.
 *
 * THE THIRD ENTRY IN THAT ALLOWLIST IS NOT A LITERAL and cannot live here:
 * `UpdateBanner` opens the release page for whatever version GitHub named,
 * which is `…/kavka/releases/*` — a pattern, not a constant. It is the one
 * URL in the app that is not written down in advance, and the scope is
 * narrowed to the releases path for exactly that reason.
 *
 * Anything added here must be added to that capability too, or `openUrl`
 * rejects at runtime and the link silently does nothing.
 */
export const SUPPORT_URL = "https://buymeacoffee.com/sahilhirani";
export const REPO_URL = "https://github.com/sahilhirani/kavka";

interface AboutDialogProps {
  /** The core version App has already fetched. Empty while it is loading. */
  version: string;
  onClose: () => void;
}

export default function AboutDialog({ version, onClose }: AboutDialogProps) {
  const { t, tx, locale, setLocale } = useI18n();
  const closeRef = useRef<HTMLButtonElement>(null);
  // A link that does nothing is a dead end, and `openUrl` can genuinely fail
  // (no browser registered, or the capability allowlist doesn't cover the
  // URL). When it does, the address is put on screen to copy instead.
  const [unopened, setUnopened] = useState<string | null>(null);
  /**
   * The run number this binary was compiled with, or null on a stable or
   * local build.
   *
   * IT IS NOT DERIVABLE FROM THE VERSION, which is the whole reason it is
   * here: `tauri.conf.json` carries the same plain version for every build
   * — MSI/WiX will not take a prerelease string in it — so two installs
   * eleven merges apart both report `0.1.0`, and a bug report that names one
   * of them names nothing. Null is a normal answer and renders nothing.
   */
  const [build, setBuild] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    updatesBuildNumber()
      .then((n) => {
        if (alive) setBuild(n);
      })
      // Silent: an older shell that does not register the command should cost
      // this dialog one absent line, not an error over the licence.
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  const open = useCallback((url: string) => {
    setUnopened(null);
    openUrl(url).catch(() => setUnopened(url));
  }, []);

  // The MCP section points at the example plugin in the repo. It goes through
  // the same helper rather than calling `openUrl` itself, so the capability
  // allowlist still has exactly two literals to match.
  const openRepo = useCallback(() => open(REPO_URL), [open]);

  /** The row in LOCALES for what is on screen — it carries the `machine` flag. */
  const active = LOCALES.find((info) => info.code === locale);

  const link = (url: string, text: string) => (
    <a
      className="support-link"
      href={url}
      onClick={(e) => {
        e.preventDefault();
        open(url);
      }}
    >
      {text}
    </a>
  );

  return (
    <Overlay
      // Wide, and it scrolls: the MCP section carries two config snippets that
      // are literals someone pastes elsewhere, and re-wrapping them to fit a
      // 440px dialog would make them wrong.
      surfaceClass="modal modal-wide"
      labelledBy="about-title"
      initialFocus={closeRef}
      onClose={onClose}
    >
      <h2 className="modal-title" id="about-title">
        {t("about.title")}
      </h2>

      <p className="modal-body">{t("about.body")}</p>

      <dl className="about-facts">
        <div className="about-fact">
          <dt className="about-fact-label">{t("about.coreVersion")}</dt>
          <dd className="about-fact-value">
            {version ? (
              <>
                <code>{version}</code>
                {/* The number is an identifier, not a quantity: passed as a
                    string so `Intl.NumberFormat` doesn't render run 1234 as
                    "1,234" and send someone looking for a release that has no
                    such tag. */}
                {build !== null && ` · ${t("about.build", { number: String(build) })}`}
              </>
            ) : (
              t("about.versionLoading")
            )}
          </dd>
        </div>
        <div className="about-fact">
          <dt className="about-fact-label">{t("about.licence")}</dt>
          <dd className="about-fact-value">{t("about.licenceValue")}</dd>
        </div>

        {/* The language picker lives here rather than in a settings screen the
            app doesn't have, and it is a third about-fact rather than a
            section of its own: "Language: Deutsch" is the same kind of
            statement as "Licence: AGPL-3.0", it just happens to be editable.
            A plain <select> so it inherits every control token — §5.3's
            sunken fill and the --border-control edge that is the only thing
            satisfying SC 1.4.11 on it.

            The honesty line is the point of the whole feature: five of the
            six catalogs came out of a machine, and someone deciding whether
            to trust a translated warning about a prod cluster is entitled to
            know that before they read one. It is stated in the language being
            offered rather than in English, because the person who needs it is
            the person who has just switched away from English. */}
        <div className="about-fact">
          <dt className="about-fact-label">
            <label htmlFor="about-locale">{t("about.language")}</label>
          </dt>
          <dd className="about-fact-value">
            <select
              id="about-locale"
              value={locale}
              aria-describedby="about-locale-hint"
              onChange={(e) => setLocale(e.target.value as Locale)}
            >
              {LOCALES.map((info) => (
                <option key={info.code} value={info.code}>
                  {info.endonym}
                </option>
              ))}
            </select>
            <span className="field-hint" id="about-locale-hint">
              {t("about.language.hint")}
              {active?.machine
                ? ` ${t("about.language.machine", { language: active.endonym })}`
                : ""}
            </span>
          </dd>
        </div>
      </dl>

      <McpSection onOpenRepo={openRepo} />

      {/* After MCP, because the two sections answer the same question from
          opposite directions — "what can leave this machine" — and this is the
          one whose answer is "nothing, and here is the file". */}
      <DiagnosticsSection />

      <div className="about-links">
        {/* The repository address is a literal, not prose: it stays verbatim
            in every locale, like a bootstrap host or a topic name (§4). */}
        {link(REPO_URL, "github.com/sahilhirani/kavka")}
        {link(SUPPORT_URL, t("common.support"))}
      </div>

      {unopened && (
        <p className="dialog-note" role="status">
          {tx("common.linkFailed", { url: <code>{unopened}</code> })}
        </p>
      )}

      <div className="modal-actions">
        <button ref={closeRef} type="button" className="btn" onClick={onClose}>
          {t("common.close")}
        </button>
      </div>
    </Overlay>
  );
}
