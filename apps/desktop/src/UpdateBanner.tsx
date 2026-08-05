import { useCallback, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, updatesInstall, updatesRestart } from "./api";
import { useI18n } from "./i18n";
import { updateFailure, type UpdateFailure, type UpdateOffer } from "./updates";

/**
 * THE UPDATE NOTICE — a banner, never a toast.
 *
 * DESIGN.md §5's three-surface rule decides this on its own: a toast is
 * something you did that finished, a banner is a condition you are in, and a
 * modal is something irreversible asking for consent. "There is a newer Kavka"
 * is a condition. It waits, it does not slide, it does not time out, and it
 * does not sit on top of the work — it takes a strip at the top of the
 * workspace and stays there until the user answers it.
 *
 * IT IS DISMISSIBLE, which the Perch deliberately is not, and the difference
 * is worth stating: a Perch is this screen's VERDICT and an app that lets you
 * switch its verdict off stops being accountable for it. This is NEWS. The
 * user is entitled to have heard it and moved on, so "Not now" is remembered
 * against the version rather than against the session (see `updates.ts`).
 *
 * NO LIVE REGION ON THE CONTAINER, for exactly the reason Perch.tsx gives for
 * not being a `role="status"`: this arrives five seconds after launch, over
 * whatever the user actually came here to do, and a polite announcement of it
 * would land on top of the screen they are reading. It sits first in the
 * workspace's DOM order instead, so the next Tab out of the sidebar reaches
 * it. The install FAILURE below is `role="alert"` — that one follows a button
 * the user pressed, and a failure nobody is looking at is announced
 * assertively or not at all (A11Y-38).
 *
 * NO PERCENTAGES. The button says "Downloading…" and goes disabled. The
 * updater reports progress in bytes against a length that is sometimes absent,
 * and a bar that jumps to 100% and then sits there for eleven seconds is a
 * worse answer than a word that was true the whole time.
 */

/**
 * Windows cannot replace a running executable, so the shell exits as it hands
 * the download to NSIS and `updatesInstall` never resolves. Everywhere else it
 * resolves and the app relaunches itself. The copy has to say which, before
 * the click — the same `navigator.userAgent` test the palette uses for ⌘K.
 */
const ON_WINDOWS = /windows/i.test(navigator.userAgent);

export default function UpdateBanner({
  offer,
  onDismiss,
}: {
  offer: UpdateOffer;
  /** Records the dismissal against this version and takes the banner down. */
  onDismiss: () => void;
}) {
  const { t, tx } = useI18n();
  const [installing, setInstalling] = useState(false);
  const [failure, setFailure] = useState<UpdateFailure | null>(null);
  // `openUrl` can genuinely fail — no browser registered, or the capability
  // allowlist doesn't cover the address. The About dialog answers that the
  // same way: put the address on screen to copy rather than leave a dead link.
  const [unopened, setUnopened] = useState<string | null>(null);

  const openReleasePage = useCallback(() => {
    setUnopened(null);
    openUrl(offer.url).catch(() => setUnopened(offer.url));
  }, [offer.url]);

  const install = useCallback(async () => {
    setInstalling(true);
    setFailure(null);
    try {
      await updatesInstall(offer.channel);
      // Reached on macOS and Linux only — see ON_WINDOWS above. The install is
      // already on disk by the time this runs, so a relaunch that fails leaves
      // a correct app that simply has not restarted yet.
      if (!ON_WINDOWS) await updatesRestart();
      // Deliberately NOT re-enabling the button on success. Nothing here is
      // worth a second press: either the process is on its way out, or the
      // update is in place and the app is relaunching.
    } catch (err) {
      setFailure(updateFailure(errorMessage(err), t));
      setInstalling(false);
    }
  }, [offer.channel, t]);

  return (
    <section
      className="banner banner-info"
      aria-label={t("updates.banner.label", { version: offer.version })}
    >
      <span className="banner-glyph" aria-hidden="true">
        ↑
      </span>
      <div className="banner-body">
        <p className="banner-title">
          {t("updates.banner.title", { version: offer.version })}
        </p>

        {/* Two sentences that must never be softened. The first says nothing
            has been downloaded; the second says what pressing Install does to
            the window you are looking at. A user who loses an unsaved produce
            draft to a restart they were not warned about is a user who turns
            the whole feature off. */}
        <p className="banner-detail">
          {t(
            offer.channel === "builds"
              ? "updates.banner.bodyBuild"
              : "updates.banner.body",
          )}
        </p>
        <p className="banner-detail">
          {t(
            ON_WINDOWS
              ? "updates.banner.willClose"
              : "updates.banner.willRestart",
          )}
        </p>

        {/* Release notes verbatim and folded, the same shape the error banner
            uses for a broker's raw reply — GitHub release bodies are Markdown
            written for a web page, and rendering them here would mean shipping
            a Markdown parser to make a paragraph look right. Under the fold,
            as written, is honest; a half-rendered one is not. */}
        {offer.notes !== null && offer.notes.trim() !== "" && (
          <details className="banner-details">
            <summary>{t("updates.banner.notes")}</summary>
            <pre className="banner-raw">{offer.notes}</pre>
          </details>
        )}

        {failure !== null && (
          <div role="alert">
            <p className="banner-title">{failure.title}</p>
            <p className="banner-detail">{failure.detail}</p>
            {failure.raw !== "" && (
              <details className="banner-details">
                <summary>{t("common.showDetails")}</summary>
                <pre className="banner-raw">{failure.raw}</pre>
              </details>
            )}
          </div>
        )}

        {unopened !== null && (
          <p className="banner-detail" role="status">
            {tx("common.linkFailed", { url: <code>{unopened}</code> })}
          </p>
        )}
      </div>

      <div className="banner-actions">
        {/* A link, not a button: it goes somewhere. It is first because
            reading what changed is the reasonable thing to do before
            installing, and last would put it after the answer. */}
        <button
          type="button"
          className="support-link"
          onClick={openReleasePage}
        >
          {t("updates.banner.releasePage")}
        </button>
        <button
          type="button"
          className="btn btn-primary"
          disabled={installing}
          onClick={() => void install()}
        >
          {installing
            ? t("updates.banner.installing")
            : t("updates.banner.install")}
        </button>
        <button
          type="button"
          className="btn btn-ghost"
          // Disabled mid-download on purpose: taking the banner down while the
          // installer is being fetched would leave the user with no surface
          // telling them what the app is doing.
          disabled={installing}
          onClick={onDismiss}
        >
          {t("updates.banner.notNow")}
        </button>
      </div>
    </section>
  );
}
