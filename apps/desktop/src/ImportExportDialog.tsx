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
import { useI18n, type TFunction } from "./i18n";

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
  const { t } = useI18n();
  // classifyError is `errors.ts`, which is outside this wave's extraction
  // boundary: the error library is still English in every locale. Stated in
  // docs/I18N.md rather than papered over — a half-translated banner where
  // only the fallback sentence moved would be worse than an honest one.
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
          <summary>{t("common.showDetails")}</summary>
          <pre className="banner-raw">{raw}</pre>
        </details>
      </div>
    </div>
  );
}

/**
 * Counts as a sentence. Exact numbers — §7 rule 5: tables and reports don't
 * round.
 *
 * `t` is a parameter rather than a hook because this is a plain function the
 * render calls twice; the caller already holds the translator.
 */
function reportLines(
  t: TFunction,
  r: ImportReport,
): { title: string; detail: string } {
  const changed = r.imported + r.replaced;
  // Absent, not zero, on an envelope written before exports carried
  // environments — so `?? 0` here is "the file had none to merge", which is
  // not the same statement as "it had some and none were new". Both read the
  // same in the sentence below because both mean "nothing to report".
  const envAdded = r.environments_imported ?? 0;
  const envSkipped = r.environments_skipped ?? 0;
  if (changed === 0 && r.skipped === 0 && envAdded === 0 && envSkipped === 0) {
    return {
      title: t("transfer.report.empty.title"),
      detail: t("transfer.report.empty.detail"),
    };
  }
  const bits: string[] = [];
  if (r.imported) bits.push(t("transfer.report.added", { count: r.imported }));
  if (r.replaced)
    bits.push(t("transfer.report.replaced", { count: r.replaced }));
  if (r.skipped) bits.push(t("transfer.report.skipped", { count: r.skipped }));
  // The environments the envelope carried, reported separately because they
  // are a different kind of thing and merge under a different rule:
  // skip-existing by case-insensitive name, so a colleague's file can never
  // re-colour — or unprotect — an environment this machine already relies on.
  if (envAdded) bits.push(t("transfer.report.envAdded", { count: envAdded }));
  if (envSkipped)
    bits.push(t("transfer.report.envSkipped", { count: envSkipped }));
  const joined = bits.join(" · ");
  // An envelope can carry environments and no new connections — the normal
  // shape when a colleague re-sends a file you have already imported. Saying
  // "Nothing changed — 0 connections were already here" would be both wrong
  // and confusing, so that case gets its own line.
  if (changed === 0 && r.skipped === 0) {
    return {
      title: t("transfer.report.envOnly.title"),
      detail: joined,
    };
  }
  return {
    title:
      changed === 0
        ? t("transfer.report.unchanged.title", { count: r.skipped })
        : t("transfer.report.imported.title", { count: changed }),
    detail:
      changed === 0
        ? t("transfer.report.unchanged.detail", { bits: joined })
        : t("transfer.report.imported.detail", { bits: joined }),
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
  const { t } = useI18n();
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
      // Only the selected panel is mounted, so only the selected tab has an
      // id to point at — an unresolvable IDREF is worse than none.
      aria-controls={tab === id ? `io-panel-${id}` : undefined}
      // A tab strip is ONE tab stop walked with the arrows (`tabKeys`). Both
      // buttons being tabbable made Tab and ArrowRight mean different things
      // inside one widget, and put a control the user is not on into the
      // overlay's focus-trap list.
      tabIndex={tab === id ? 0 : -1}
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
      ? t("transfer.import.needsJson")
      : importing
        ? t("transfer.import.busy")
        : undefined;

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="io-title"
      initialFocus={initialTabRef}
      onClose={onClose}
    >
      <h2 className="modal-title" id="io-title">
        {t("transfer.title")}
      </h2>

      <div
        className="modal-tabs"
        role="tablist"
        aria-label={t("transfer.tablist")}
      >
        {tabButton("export", t("transfer.tab.export"))}
        {tabButton("import", t("transfer.tab.import"))}
      </div>

      {tab === "export" ? (
        <div
          className="modal-panel"
          id="io-panel-export"
          role="tabpanel"
          aria-labelledby="io-tab-export"
        >
          <p className="modal-body">{t("transfer.export.body")}</p>
          <p className="dialog-note">{t("transfer.export.promise")}</p>

          {exportError ? (
            <>
              <TransferError
                raw={exportError}
                what={t("transfer.export.failed")}
              />
              <div className="dialog-inline-action">
                <button
                  type="button"
                  className="btn"
                  onClick={() => void loadExport()}
                >
                  {t("common.tryAgain")}
                </button>
              </div>
            </>
          ) : json === null ? (
            <p className="dialog-note">{t("common.readingConnections")}</p>
          ) : (
            <>
              <label className="sr-only" htmlFor="io-export">
                {t("transfer.export.label")}
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
                    ? t("transfer.export.copied")
                    : t("transfer.export.copyManual", { key: COPY_KEY })}
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
          <p className="modal-body">{t("transfer.import.body")}</p>

          <label className="field-label" htmlFor="io-import">
            {t("transfer.import.label")}
          </label>
          <textarea
            id="io-import"
            className="io-json"
            rows={7}
            spellCheck={false}
            // A JSON skeleton, not prose: a literal stays verbatim in every
            // locale for the same reason a bootstrap host does (§4).
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
            <span className="kbd">⏎</span> {t("transfer.import.kbd")}
            <span aria-hidden="true">·</span>
            <span className="kbd">Esc</span> {t("transfer.import.kbdClose")}
          </span>

          <fieldset className="fieldset io-strategy">
            <legend className="eyebrow">{t("transfer.import.legend")}</legend>
            {/* §5.3: the hint is a SIBLING of the label wired with
                `aria-describedby`, so it describes the control instead of
                renaming it — and so it reaches a screen reader at all. These
                two were the only check-fields in the app rendering the hint
                with nothing pointing at it. */}
            <div className="check-field">
              <input
                type="radio"
                id="io-skip"
                name="io-strategy"
                checked={strategy === "skip"}
                aria-describedby="io-skip-hint"
                onChange={() => setStrategy("skip")}
              />
              <label className="check-label" htmlFor="io-skip">
                {t("transfer.import.skip")}
              </label>
              <span className="field-hint" id="io-skip-hint">
                {t("transfer.import.skipHint")}
              </span>
            </div>
            <div className="check-field">
              <input
                type="radio"
                id="io-replace"
                name="io-strategy"
                checked={strategy === "replace"}
                aria-describedby="io-replace-hint"
                onChange={() => setStrategy("replace")}
              />
              <label className="check-label" htmlFor="io-replace">
                {t("transfer.import.replace")}
              </label>
              <span className="field-hint" id="io-replace-hint">
                {t("transfer.import.replaceHint")}
              </span>
            </div>
          </fieldset>

          {importError && (
            <TransferError
              raw={importError}
              what={t("transfer.import.failed")}
            />
          )}

          {report && !importError && (
            <div className="banner banner-info" role="status">
              <span className="banner-glyph" aria-hidden="true">
                ✓
              </span>
              <div className="banner-body">
                <p className="banner-title">{reportLines(t, report).title}</p>
                <p className="banner-detail">
                  {reportLines(t, report).detail}
                </p>
              </div>
            </div>
          )}
        </div>
      )}

      <div className="modal-actions">
        <button type="button" className="btn" onClick={onClose}>
          {t("common.close")}
        </button>
        {tab === "export" ? (
          <button
            type="button"
            className="btn btn-primary"
            disabled={json === null}
            title={
              exportError !== null
                ? t("transfer.export.nothingToCopy")
                : json === null
                  ? t("transfer.export.stillReading")
                  : undefined
            }
            onClick={() => void copy()}
          >
            {t("transfer.export.copy")}
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
            {importing
              ? t("transfer.import.running")
              : t("transfer.import.run")}
          </button>
        )}
      </div>
    </Overlay>
  );
}
