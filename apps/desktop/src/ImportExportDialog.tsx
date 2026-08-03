import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  profilesExport,
  profilesImport,
  type ImportReport,
  type ImportStrategy,
} from "./api";
import { classifyError } from "./errors";
import Overlay from "./Overlay";

/**
 * Export and import the connection list — DESIGN.md §5.8 (modal) and §7.
 *
 * The whole point of the export is that it is safe to send to a colleague:
 * profiles carry keychain REFERENCES, never secrets. That promise is stated on
 * screen, next to the text it is about, because nobody reads it anywhere else.
 */

export type TransferTab = "export" | "import";

const ON_MAC = /mac/i.test(navigator.userAgent);
const COPY_KEY = ON_MAC ? "⌘C" : "Ctrl C";
/** Modifier for "confirm from inside a textarea", where ⏎ is a newline. */
const CONFIRM_KEY = ON_MAC ? "⌘" : "Ctrl";

/**
 * The three-layer error banner, with a fallback sentence that fits a dialog.
 *
 * `classifyError` is the error library (§7) and still owns the title, but its
 * unrecognised-case line 2 talks about "the broker's full reply" — correct for
 * a connect failure, nonsense for "that isn't valid JSON". So the known cases
 * are used verbatim and only the unknown branch gets copy that belongs here.
 */
function TransferError({ raw, what }: { raw: string; what: string }) {
  const { title, detail, known } = classifyError(raw);
  return (
    <div className="banner banner-danger" role="alert">
      <span className="banner-glyph" aria-hidden="true">
        !
      </span>
      <div className="banner-body">
        <p className="banner-title">{title}</p>
        <p className="banner-detail">{known ? detail : what}</p>
        <details className="banner-details">
          <summary>Show details</summary>
          <pre className="banner-raw">{raw}</pre>
        </details>
      </div>
    </div>
  );
}

/** Counts as a sentence. Exact numbers — §7 rule 5: tables and reports don't round. */
function reportLines(r: ImportReport): { title: string; detail: string } {
  const changed = r.imported + r.replaced;
  if (changed === 0 && r.skipped === 0) {
    return {
      title: "That JSON had no connections in it",
      detail:
        "Check you pasted the whole export, including the outer braces — Kavka read it fine, there was just nothing to add.",
    };
  }
  const bits: string[] = [];
  if (r.imported) bits.push(`${r.imported} added`);
  if (r.replaced) bits.push(`${r.replaced} replaced`);
  if (r.skipped) bits.push(`${r.skipped} skipped — already on this machine`);
  return {
    title:
      changed === 0
        ? `Nothing changed — ${r.skipped} ${r.skipped === 1 ? "connection was" : "connections were"} already here`
        : `Imported ${changed} ${changed === 1 ? "connection" : "connections"}`,
    detail:
      changed === 0
        ? `${bits.join(" · ")}. Choose “Replace it with the one in the JSON” above if you meant to overwrite them.`
        : `${bits.join(" · ")}. Passwords aren't in an export — open each new connection and enter its password before connecting.`,
  };
}

interface ImportExportDialogProps {
  initialTab: TransferTab;
  /** Called after a successful import so App can reload the profile list. */
  onImported: () => void;
  /**
   * Whether this dialog is currently showing a danger banner. §5.8: the prod
   * damper keys off ANY danger on screen, and the app root is the only place
   * that attribute can live — so the dialog has to say.
   */
  onDangerChange: (danger: boolean) => void;
  onClose: () => void;
}

export default function ImportExportDialog({
  initialTab,
  onImported,
  onDangerChange,
  onClose,
}: ImportExportDialogProps) {
  const [tab, setTab] = useState<TransferTab>(initialTab);
  // Focus lands on the tab we open on, so focus and aria-selected agree. Bind
  // it to "export" and opening on Import puts the caret on the wrong tab —
  // announced as the Export tab, one arrow key away from switching the panel
  // out from under the user.
  const initialTabRef = useRef<HTMLButtonElement>(null);
  const exportRef = useRef<HTMLTextAreaElement>(null);

  // Export side.
  const [json, setJson] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [copied, setCopied] = useState<"clipboard" | "manual" | null>(null);

  // Import side.
  const [pasted, setPasted] = useState("");
  const [strategy, setStrategy] = useState<ImportStrategy>("skip");
  const [importing, setImporting] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [importError, setImportError] = useState<string | null>(null);

  const loadExport = useCallback(async () => {
    setExportError(null);
    try {
      setJson(await profilesExport());
    } catch (err) {
      setExportError(errorMessage(err));
    }
  }, []);

  useEffect(() => {
    if (tab === "export" && json === null && exportError === null) {
      void loadExport();
    }
  }, [tab, json, exportError, loadExport]);

  const copy = async () => {
    if (json === null) return;
    setCopied(null);
    try {
      if (!navigator.clipboard?.writeText) throw new Error("no clipboard");
      await navigator.clipboard.writeText(json);
      setCopied("clipboard");
    } catch {
      // No dead end: select the text and say which keys finish the job.
      exportRef.current?.focus();
      exportRef.current?.select();
      setCopied("manual");
    }
  };

  const runImport = async () => {
    const text = pasted.trim();
    if (text === "" || importing) return;
    setImporting(true);
    setImportError(null);
    setReport(null);
    try {
      const result = await profilesImport(text, strategy);
      setReport(result);
      onImported();
    } catch (err) {
      setImportError(errorMessage(err));
    } finally {
      setImporting(false);
    }
  };

  const tabKeys = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const next: TransferTab = tab === "export" ? "import" : "export";
    setTab(next);
    // Follow the focus, so the arrow keys read as one movement.
    const el = document.getElementById(`io-tab-${next}`);
    if (el instanceof HTMLElement) el.focus();
  };

  const tabButton = (id: TransferTab, label: string) => (
    <button
      type="button"
      id={`io-tab-${id}`}
      ref={id === initialTab ? initialTabRef : undefined}
      role="tab"
      aria-selected={tab === id}
      aria-controls={`io-panel-${id}`}
      className={`tab ${tab === id ? "tab-active" : ""}`}
      onClick={() => setTab(id)}
      onKeyDown={tabKeys}
    >
      {label}
    </button>
  );

  // Only the panel on screen counts: an export failure the Import tab is
  // covering up is not a danger the user can see.
  const showingDanger =
    tab === "export" ? exportError !== null : importError !== null;

  useEffect(() => {
    onDangerChange(showingDanger);
    // And it is gone the moment this dialog is, or the damper outlives the
    // banner it was for and prod's rule stays muted with nothing on screen.
    return () => onDangerChange(false);
  }, [showingDanger, onDangerChange]);

  const importReason =
    pasted.trim() === ""
      ? "Paste the JSON from an export first"
      : importing
        ? "Kavka is importing those connections now"
        : undefined;

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="io-title"
      initialFocus={initialTabRef}
      onClose={onClose}
    >
      <h2 className="modal-title" id="io-title">
        Connections
      </h2>

      <div className="modal-tabs" role="tablist" aria-label="Export or import">
        {tabButton("export", "Export")}
        {tabButton("import", "Import")}
      </div>

      {tab === "export" ? (
        <div
          className="modal-panel"
          id="io-panel-export"
          role="tabpanel"
          aria-labelledby="io-tab-export"
        >
          <p className="modal-body">
            Every connection on this machine, as JSON. Paste it into another
            copy of Kavka to set the same clusters up there.
          </p>
          <p className="dialog-note">
            Passwords and keys never leave this machine — exports carry
            references, not secrets.
          </p>

          {exportError ? (
            <>
              <TransferError
                raw={exportError}
                what="Kavka couldn't read its connection file. Your connections are still on disk — nothing was lost."
              />
              <div className="dialog-inline-action">
                <button
                  type="button"
                  className="btn"
                  onClick={() => void loadExport()}
                >
                  Try again
                </button>
              </div>
            </>
          ) : json === null ? (
            <p className="dialog-note">Reading your saved connections…</p>
          ) : (
            <>
              <label className="sr-only" htmlFor="io-export">
                Your connections, as JSON
              </label>
              <textarea
                id="io-export"
                ref={exportRef}
                className="io-json"
                rows={9}
                readOnly
                spellCheck={false}
                value={json}
                onFocus={(e) => e.currentTarget.select()}
              />
              {copied && (
                <p className="dialog-note" role="status">
                  {copied === "clipboard"
                    ? "Copied to the clipboard."
                    : `Kavka couldn't reach the clipboard. The text is selected — press ${COPY_KEY} to copy it.`}
                </p>
              )}
            </>
          )}
        </div>
      ) : (
        <div
          className="modal-panel"
          id="io-panel-import"
          role="tabpanel"
          aria-labelledby="io-tab-import"
        >
          <p className="modal-body">
            Paste an export from another copy of Kavka. Passwords aren't in it —
            each imported connection asks for its own the first time you
            connect.
          </p>

          <label className="field-label" htmlFor="io-import">
            Exported JSON
          </label>
          <textarea
            id="io-import"
            className="io-json"
            rows={7}
            spellCheck={false}
            placeholder={'{"version": 1, "profiles": [ … ]}'}
            value={pasted}
            onChange={(e) => setPasted(e.target.value)}
            onKeyDown={(e) => {
              // Enter belongs to the textarea; ⌘/Ctrl+⏎ confirms, as in every
              // other multi-line box people type JSON into.
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void runImport();
              }
            }}
          />

          <span className="dialog-kbd-hint">
            <span className="kbd">{CONFIRM_KEY}</span>
            <span className="kbd">⏎</span> import
            <span aria-hidden="true">·</span>
            <span className="kbd">Esc</span> close
          </span>

          <fieldset className="fieldset io-strategy">
            <legend className="eyebrow">If a connection is already here</legend>
            <div className="check-field">
              <input
                type="radio"
                id="io-skip"
                name="io-strategy"
                checked={strategy === "skip"}
                onChange={() => setStrategy("skip")}
              />
              <label className="check-label" htmlFor="io-skip">
                Keep the one on this machine
              </label>
              <span className="field-hint">
                Connections already saved here are left exactly as they are.
                Everything new in the JSON is still added.
              </span>
            </div>
            <div className="check-field">
              <input
                type="radio"
                id="io-replace"
                name="io-strategy"
                checked={strategy === "replace"}
                onChange={() => setStrategy("replace")}
              />
              <label className="check-label" htmlFor="io-replace">
                Replace it with the one in the JSON
              </label>
              <span className="field-hint">
                The pasted version wins — name, address, environment and
                sign-in method. Passwords already in your keychain stay where
                they are.
              </span>
            </div>
          </fieldset>

          {importError && (
            <TransferError
              raw={importError}
              what="Kavka couldn't read that as an export. Check you pasted the whole file, including the outer braces — the text Kavka got is below."
            />
          )}

          {report && !importError && (
            <div className="banner banner-info" role="status">
              <span className="banner-glyph" aria-hidden="true">
                ✓
              </span>
              <div className="banner-body">
                <p className="banner-title">{reportLines(report).title}</p>
                <p className="banner-detail">{reportLines(report).detail}</p>
              </div>
            </div>
          )}
        </div>
      )}

      <div className="modal-actions">
        <button type="button" className="btn" onClick={onClose}>
          Close
        </button>
        {tab === "export" ? (
          <button
            type="button"
            className="btn btn-primary"
            disabled={json === null}
            title={
              exportError !== null
                ? "There's nothing to copy — Kavka couldn't read its connection file"
                : json === null
                  ? "Kavka is still reading your connections"
                  : undefined
            }
            onClick={() => void copy()}
          >
            Copy to clipboard
          </button>
        ) : (
          <button
            type="button"
            className="btn btn-primary"
            disabled={pasted.trim() === "" || importing}
            aria-busy={importing}
            title={importReason}
            onClick={() => void runImport()}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {importing ? <span className="spinner" /> : null}
            </span>
            {importing ? "Importing…" : "Import connections"}
          </button>
        )}
      </div>
    </Overlay>
  );
}
