import { useCallback, useEffect, useRef, useState } from "react";
import {
  DIAGNOSTICS_MAX_BYTES,
  diagnosticsClear,
  diagnosticsOpenLogs,
  diagnosticsSetEnabled,
  diagnosticsStatus,
  errorMessage,
  type DiagnosticsStatus,
} from "./api";
import { noteToLog } from "./diagnostics";

/**
 * DIAGNOSTICS, as the About dialog explains it.
 *
 * THE ONE CLAIM THIS SECTION MAKES, and the reason it can: **nothing leaves
 * this machine.** Not "we anonymise it", not "only with your consent" — there
 * is no telemetry endpoint anywhere in Kavka, so there is no transmission to
 * consent to. The feature writes a text file. You open the folder, you read
 * it, you decide whether to paste it into a GitHub issue. That is the whole
 * product.
 *
 * Which is exactly why the copy here is specific rather than reassuring. "We
 * respect your privacy" is what a page says when it is collecting something.
 * This one lists what a line contains, lists what one never contains, gives the
 * path, gives the ceiling in megabytes, and puts a delete button next to it.
 * Every one of those is checkable by the person reading it, and a claim you can
 * check is worth more than a promise you cannot.
 *
 * DEFAULT OFF, and it costs the first crash. That is the right trade for this
 * audience: a log line can carry a topic name, a bootstrap address or a
 * profile name, and a lot of Kafka operators work inside networks where a
 * screenshot of those is a reportable event. So Kavka writes nothing until
 * asked, and says exactly what it will write before it is.
 *
 * WHY IT LIVES IN ABOUT rather than a Settings dialog: there is no Settings
 * dialog, and inventing one for a single toggle would be a screen whose only
 * job is to be looked for. About is already where the app explains itself —
 * the version, the licence, the MCP server's write gates — and this is the same
 * kind of statement.
 */

/** `12 KB`, `1.4 MB`. Prose rounds; the ceiling is exact (docs/DESIGN.md §7). */
function bytesLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function DiagnosticsSection() {
  const [status, setStatus] = useState<DiagnosticsStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    diagnosticsStatus()
      .then((next) => {
        if (mounted.current) setStatus(next);
      })
      .catch((err: unknown) => {
        if (mounted.current) setError(errorMessage(err));
      });
  }, []);

  const toggle = useCallback(
    async (enabled: boolean) => {
      setBusy(true);
      setError(null);
      try {
        const next = await diagnosticsSetEnabled(enabled);
        if (mounted.current) setStatus(next);
        // The first thing in the file is the fact that somebody asked for it,
        // so a log that later records a crash also records how it came to
        // exist. Written after the toggle, from the webview, so the entry
        // proves the whole path works rather than only the Rust half.
        if (enabled) noteToLog("diagnostics turned on from the About panel");
      } catch (err) {
        if (mounted.current) setError(errorMessage(err));
      } finally {
        if (mounted.current) setBusy(false);
      }
    },
    [],
  );

  const openFolder = useCallback(async () => {
    setError(null);
    try {
      await diagnosticsOpenLogs();
    } catch (err) {
      setError(errorMessage(err));
    }
  }, []);

  const clear = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await diagnosticsClear();
      if (mounted.current) setStatus(next);
    } catch (err) {
      if (mounted.current) setError(errorMessage(err));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }, []);

  return (
    <section className="mcp-section" aria-labelledby="about-diagnostics-title">
      <h3 className="eyebrow" id="about-diagnostics-title">
        Diagnostics
      </h3>

      <p className="dialog-note">
        Kavka has <strong>no telemetry</strong>. There is no endpoint, no
        analytics, no crash reporter phoning anywhere — not a disabled one, not
        one behind a flag. This switch does exactly one thing: it lets Kavka
        write a text file on this computer when something goes wrong, so you
        have something to attach to a GitHub issue.
      </p>

      {/* 18px box in a 18px/1fr grid with the hint in row 2 — 14px fails
          SC 2.5.8, and the hint is a sibling of the label so it describes the
          control instead of renaming it (docs/DESIGN.md §5.3). */}
      <div className="check-field">
        <input
          type="checkbox"
          id="diagnostics-enabled"
          checked={status?.enabled ?? false}
          disabled={status === null || busy}
          aria-describedby="diagnostics-enabled-hint"
          onChange={(e) => void toggle(e.target.checked)}
          title={
            status === null
              ? "Kavka is reading the current setting"
              : busy
                ? "Kavka is saving this setting"
                : undefined
          }
        />
        <label className="check-label" htmlFor="diagnostics-enabled">
          Write a diagnostics log on this machine
        </label>
        <span className="field-hint" id="diagnostics-enabled-hint">
          Off by default. Turning it on now means the <em>next</em> problem is
          recorded — it can't recover one that already happened.
        </span>
      </div>

      {/* THE ONLY FOLD IN THIS SECTION, and the asymmetry is the point.
          "What goes in the file" is a specification — four record kinds a
          reader checks once and then never again — so it earns a disclosure.
          "What never goes in it" is the honest LIMIT of the no-telemetry
          claim, and a limit behind a fold is a limit that gets quoted
          without it (docs/DESIGN.md §7), so it stays open below.

          The summary carries the heading text verbatim: this section is
          English in every locale by the i18n wave's own scope decision, and
          folding it must not be the thing that introduces a half-translated
          file. */}
      <details className="fold">
        <summary className="fold-summary">
          <span className="fold-caret" aria-hidden="true">
            ▸
          </span>
          <span className="fold-title">What goes in the file</span>
        </summary>
        <ul className="mcp-tools fold-body">
          <li className="mcp-tool">
            <code>panic</code>
            <span className="mcp-tool-what">
              A Rust crash: the message and the source file, line and column it
              came from.
            </span>
          </li>
          <li className="mcp-tool">
            <code>error</code>
            <span className="mcp-tool-what">
              An uncaught error in the window: its message and JavaScript stack.
            </span>
          </li>
          <li className="mcp-tool">
            <code>rejection</code>
            <span className="mcp-tool-what">
              A promise nobody caught — usually a command that failed while
              nothing was watching.
            </span>
          </li>
          <li className="mcp-tool">
            <code>session</code>
            <span className="mcp-tool-what">
              One line per launch: Kavka's version, the operating system and the
              processor architecture.
            </span>
          </li>
        </ul>
      </details>

      <h4 className="mcp-subhead">What never goes in it</h4>
      <p className="dialog-note">
        Kavka does not log message payloads, keys, headers, passwords, tokens or
        anything read out of your keychain, and it does not write a line per
        action — there is no record of what you browsed, searched or produced.
        An error message can quote a broker's own reply, so an address or a
        topic name can appear in one when that is what failed. That is the
        honest limit of the claim: <strong>read the file before you attach it</strong>,
        which is the button below and the reason the file is plain text.
      </p>

      {status !== null && (
        <>
          <div className="mcp-field">
            <span className="mcp-label">Folder</span>
            <code className="mcp-path" title={status.dir}>
              {status.dir}
            </code>
            <button
              type="button"
              className="btn btn-ghost"
              title="Open the logs folder in your file manager"
              onClick={() => void openFolder()}
            >
              Open logs folder
            </button>
          </div>

          <p className="dialog-note">
            {/* role="status" on the COUNT ALONE, not on the paragraph
                (docs/A11Y-AUDIT.md A11Y-39). Pressing "Delete them" removes
                the button the user was standing on, and without this nothing
                said why: now the region they just emptied reports "No log
                files yet." The ceiling sentence beside it never changes, so
                keeping it outside the region is the difference between one
                announced clause and three. */}
            <span role="status">
              {status.files === 0
                ? "No log files yet."
                : `${status.files} file${status.files === 1 ? "" : "s"}, ${bytesLabel(status.bytes)}.`}
            </span>{" "}
            Kavka keeps the five most recent and rotates at 512 KB apiece, so
            this can never exceed {bytesLabel(DIAGNOSTICS_MAX_BYTES)}.
            {status.files > 0 && (
              <>
                {" "}
                <button
                  type="button"
                  className="support-link"
                  disabled={busy}
                  // "Delete them" is a pronoun, and a screen-reader user who
                  // tabs straight onto it has no antecedent (SC 2.4.6,
                  // A11Y-40). The visible words LEAD the accessible name
                  // rather than being replaced by it, which is what SC 2.5.3
                  // requires of a voice-control user saying "delete them".
                  aria-label="Delete them — every diagnostics log file on this machine"
                  title={
                    busy
                      ? "Kavka is working"
                      : "Delete every log file on this machine"
                  }
                  onClick={() => void clear()}
                >
                  Delete them
                </button>
              </>
            )}
          </p>
        </>
      )}

      {/* role="alert", not "status" (docs/A11Y-AUDIT.md A11Y-38). Every one of
          these arrives from a command that failed while focus stayed on the
          control that started it — the same shape as A11Y-32 through A11Y-34,
          and the same answer: a failure nobody is looking at is announced
          assertively or not at all. */}
      {error !== null && (
        <p className="dialog-note" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
