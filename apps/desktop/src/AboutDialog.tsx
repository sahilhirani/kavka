import { useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import Overlay from "./Overlay";

/**
 * The two links Kavka is allowed to open. They live here — the one component
 * that shows both — and the sidebar and the palette import them, so the
 * allowlist in `src-tauri/capabilities/default.json` has exactly two literals
 * to match and neither can drift by being retyped somewhere else.
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
  const closeRef = useRef<HTMLButtonElement>(null);
  // A link that does nothing is a dead end, and `openUrl` can genuinely fail
  // (no browser registered, or the capability allowlist doesn't cover the
  // URL). When it does, the address is put on screen to copy instead.
  const [unopened, setUnopened] = useState<string | null>(null);

  const open = (url: string) => {
    setUnopened(null);
    openUrl(url).catch(() => setUnopened(url));
  };

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
      surfaceClass="modal"
      labelledBy="about-title"
      initialFocus={closeRef}
      onClose={onClose}
    >
      <h2 className="modal-title" id="about-title">
        About Kavka
      </h2>

      <p className="modal-body">
        A desktop client for Apache Kafka. Kavka runs entirely on this machine:
        passwords go to your operating system's keychain, and nothing about your
        clusters leaves this computer.
      </p>

      <dl className="about-facts">
        <div className="about-fact">
          <dt className="about-fact-label">Core version</dt>
          <dd className="about-fact-value">
            {version ? <code>{version}</code> : "Reading it now…"}
          </dd>
        </div>
        <div className="about-fact">
          <dt className="about-fact-label">Licence</dt>
          <dd className="about-fact-value">
            Free and open source under AGPL-3.0
          </dd>
        </div>
      </dl>

      <div className="about-links">
        {link(REPO_URL, "github.com/sahilhirani/kavka")}
        {link(SUPPORT_URL, "Support Kavka ☕")}
      </div>

      {unopened && (
        <p className="dialog-note" role="status">
          Kavka couldn't hand that link to your browser. The address is{" "}
          <code>{unopened}</code> — copy it from here.
        </p>
      )}

      <div className="modal-actions">
        <button ref={closeRef} type="button" className="btn" onClick={onClose}>
          Close
        </button>
      </div>
    </Overlay>
  );
}
