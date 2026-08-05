import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  DEFAULT_MASK_REPLACEMENT,
  errorMessage,
  maskingDelete,
  maskingList,
  maskingSave,
  maskingToggle,
  type ConnectionProfile,
  type MaskRule,
  type MaskScope,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { useI18n } from "./i18n";
import { noteMaskRules, useMasking } from "./masking";
import Overlay from "./Overlay";
import Perch from "./Perch";
import { ErrorBanner } from "./ProfileEditor";
import { ToastStack, useToasts } from "./Toast";

/**
 * MASKING — where the rules live, and the one screen that explains what they
 * actually do.
 *
 * The load-bearing sentence, and it is on screen rather than only in this
 * comment: KAVKA MASKS IN ITS CORE, BEFORE THE RECORDS REACH THIS WINDOW. A
 * rule is not a CSS class over a payload the webview already holds — the
 * matched text is replaced on the decoded record on its way across IPC, so
 * while a rule is on, the real bytes are not in the window at all.
 *
 * Three consequences, and every one of them is stated to the user:
 *
 *  1. EXPORTS AND COPIES CARRY THE MASKED TEXT. There is no "export the real
 *     values" path from a masked session, because there is nothing here to
 *     export. The export toast says so; so does the inspector.
 *  2. THERE IS NO REVEAL BUTTON, and its absence is the feature. A control that
 *     looked like it could show the raw value would be lying about the only
 *     guarantee this feature sells.
 *  3. TURNING A RULE OFF CHANGES WHAT ARRIVES NEXT, not what is on screen. The
 *     rows already fetched stay masked until they are fetched again, and the
 *     panel says that rather than letting someone think it failed.
 *
 * Like alerts, masking is INDEPENDENT OF READ-ONLY. A rule mutates nothing on
 * the cluster; it is a note Kavka keeps about what not to show. Blocking it on
 * a read-only connection would take the feature away from exactly the operator
 * most likely to be reading production over someone's shoulder.
 */

const SCOPE_LABEL: Record<MaskScope, string> = {
  value: "The message value",
  key: "The message key",
  headers: "Header values",
  all: "Value, key and headers",
};

const SCOPES: readonly MaskScope[] = ["value", "key", "headers", "all"];

/** What a rule reads, as one short phrase for the table. */
function scopeWord(scope: string): string {
  return SCOPE_LABEL[scope as MaskScope] ?? scope;
}

interface MaskingTabProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
}

export default function MaskingTab({ profile, onDanger }: MaskingTabProps) {
  const [rules, setRules] = useState<MaskRule[] | null>(null);
  const [editing, setEditing] = useState<MaskRule | "new" | null>(null);
  const [removing, setRemoving] = useState<MaskRule | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [nonce, setNonce] = useState(0);
  const seq = useRef(0);

  const toaster = useToasts();
  const push = toaster.push;
  const session = useMasking(profile.id);

  const { t } = useI18n();
  useDangerSignal(error !== null, onDanger);

  useEffect(() => {
    const mine = ++seq.current;
    maskingList(profile.id)
      .then((list) => {
        if (seq.current !== mine) return;
        setRules(list);
        // The status-bar chip reads the same numbers. Reporting them here is
        // what keeps one fetch serving both, and what makes the chip correct
        // the moment a rule is saved rather than on the next mount.
        noteMaskRules(profile.id, list);
      })
      .catch((err: unknown) => {
        if (seq.current !== mine) return;
        setRules([]);
        setError(errorMessage(err));
      });
    return () => {
      seq.current += 1;
    };
  }, [profile.id, nonce]);

  const saveRule = useCallback(
    async (rule: MaskRule) => {
      setBusy(true);
      try {
        await maskingSave(profile.id, rule);
        setEditing(null);
        setNonce((n) => n + 1);
        push({
          kind: "ok",
          title: `Saved ${rule.name}`,
          detail: rule.enabled
            ? `${scopeWord(rule.applies_to)} — messages fetched from now on come through masked.`
            : "It is switched off, so nothing is masked by it yet.",
        });
      } catch (err) {
        setError(errorMessage(err));
      } finally {
        setBusy(false);
      }
    },
    [profile.id, push],
  );

  const toggleRule = useCallback(
    async (rule: MaskRule, enabled: boolean) => {
      // Optimistic: a switch that waits for a round trip reads as broken. The
      // reload below is what corrects it if the write failed.
      setRules((prev) =>
        prev === null
          ? prev
          : prev.map((r) => (r.id === rule.id ? { ...r, enabled } : r)),
      );
      try {
        await maskingToggle(profile.id, rule.id, enabled);
      } catch (err) {
        setError(errorMessage(err));
      } finally {
        setNonce((n) => n + 1);
      }
    },
    [profile.id],
  );

  const deleteRule = useCallback(async () => {
    if (removing === null) return;
    setBusy(true);
    try {
      await maskingDelete(profile.id, removing.id);
      setRemoving(null);
      setNonce((n) => n + 1);
      push({ kind: "ok", title: `Deleted ${removing.name}` });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [profile.id, removing, push]);

  const enabled = rules?.filter((rule) => rule.enabled).length ?? 0;

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      {/* Masking's verdict is the only one in Kavka that is about KAVKA rather
          than about the cluster: it answers "is what I am looking at what the
          producer sent?". `ok` is the unmasked case on purpose — a rule in
          force is a caveat on every other screen, so it reads as "watch". */}
      <Perch
        screen={t("rail.item.masking")}
        loading={rules === null}
        tone={rules === null ? "unknown" : enabled > 0 ? "watch" : "ok"}
        caveat={
          rules === null
            ? undefined
            : enabled > 0
              ? t("perch.masking.caveat")
              : session.sawMasked
                ? t("perch.masking.caveat.sawMasked")
                : undefined
        }
      >
        {rules === null
          ? t("perch.masking.unread")
          : rules.length === 0
            ? t("perch.masking.none")
            : enabled > 0
              ? t("perch.masking.inForce", { count: enabled })
              : t("perch.masking.off", { count: rules.length })}
      </Perch>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Masking rules
            {rules !== null && <span className="panel-count">{rules.length}</span>}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy}
              title={
                busy
                  ? "Kavka is saving a rule"
                  : "Hide something you don't want on screen"
              }
              onClick={() => setEditing("new")}
            >
              Add a rule
            </button>
          </div>
        </div>

        <p className="table-note">
          A rule is a regular expression Kavka replaces before a message reaches
          this window. The matched text is gone by the time anything here sees
          it — which is what makes the promise worth having, and why there is no
          button to reveal it. Turn the rule off to read the real values again.
        </p>

        {/* The same deliberate divergence alerts carry, and it is stated for
            the same reason: a control that behaves differently on a read-only
            connection reads as a bug unless somebody says why. */}
        {profile.read_only && (
          <p className="readonly-note">
            This connection is read-only, and masking still works. A rule
            changes nothing on the cluster — it is Kavka's own note about what
            not to show you.
          </p>
        )}

        {enabled > 0 && (
          <p className="table-note">
            {enabled} {enabled === 1 ? "rule is" : "rules are"} in force. Records
            already on screen keep whatever treatment they arrived with: a rule
            you switch on now applies to the next fetch, tail batch, search or
            query, not to rows that are already here.
          </p>
        )}

        {rules === null ? (
          <p className="table-note">Reading this connection's masking rules…</p>
        ) : rules.length === 0 ? (
          <p className="table-note">
            No rules yet. The two most people start with are a card number
            (<code>\d{"{"}13,16{"}"}</code>) and an email address — anything you
            would rather not have in a screenshot of production. Rules are kept
            on this machine, per connection, beside the alert rules.
          </p>
        ) : (
          <div className="table-wrap">
            <table className="data-table data-table-flush data-table-tall">
              <caption className="sr-only">
                Masking rules for {profile.name}
              </caption>
              <thead>
                <tr>
                  <th scope="col">Rule</th>
                  <th scope="col">Pattern</th>
                  <th scope="col">Becomes</th>
                  <th scope="col">Reads</th>
                  <th scope="col">State</th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {rules.map((rule) => (
                  <tr key={rule.id}>
                    <td>
                      <span className="rule-name">{rule.name}</span>
                    </td>
                    <td className="cell-mono mask-pattern">{rule.pattern}</td>
                    <td className="cell-mono">{rule.replacement}</td>
                    <td>{scopeWord(rule.applies_to)}</td>
                    <td>
                      {/* Law 2: the switch carries a word, and the word is the
                          state — never a coloured dot on its own. */}
                      <label className="mask-switch">
                        <input
                          type="checkbox"
                          checked={rule.enabled}
                          // The visible word is the STATE, so it cannot also
                          // be the control's name — six rows would all be
                          // called "Off". The name says which rule this is.
                          aria-label={`Mask with ${rule.name}`}
                          onChange={(e) =>
                            void toggleRule(rule, e.target.checked)
                          }
                        />
                        <span className="mask-switch-word">
                          {rule.enabled ? "Masking" : "Off"}
                        </span>
                      </label>
                    </td>
                    <td className="col-affordance">
                      <span className="rule-actions">
                        <button
                          type="button"
                          className="btn btn-row"
                          title={`Change what ${rule.name} hides`}
                          onClick={() => setEditing(rule)}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          className="btn btn-danger btn-row"
                          title={`Forget the rule ${rule.name}`}
                          onClick={() => setRemoving(rule)}
                        >
                          Delete
                        </button>
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">What masking does to the rest of Kavka</h2>
        </div>
        <ul className="mask-consequences">
          <li>
            <strong>Exports and copies carry the masked text.</strong> A CSV
            written from a masked session holds the bullets, not the values, and
            the toast that confirms it says so. There is no path from here to
            the originals — they never left the core.
          </li>
          <li>
            <strong>The inspector says a payload is not verbatim.</strong> A
            masked record is marked in the payload panel, so nobody reads a
            replacement as something the producer actually sent.
          </li>
          <li>
            <strong>Search and SQL still read the real bytes.</strong> Matching
            happens in the core, before masking; only what comes back to this
            window is rewritten. A search for a card number still finds the
            record — it just doesn't show you the number.
          </li>
          <li>
            <strong>Nothing here is sent anywhere.</strong> Rules live beside
            this connection's alert rules, on this machine.
          </li>
        </ul>
        {session.sawMasked && (
          <p className="table-note" role="status">
            Something on screen in this session has been masked.
          </p>
        )}
      </section>

      {editing !== null && (
        <MaskRuleModal
          rule={editing === "new" ? null : editing}
          busy={busy}
          onSave={saveRule}
          onClose={() => setEditing(null)}
        />
      )}

      {removing !== null && (
        // `plain`, not destructive, and no type-to-confirm even on prod:
        // deleting a rule destroys nothing on the cluster. What it does do is
        // stop hiding something, which the body says out loud.
        <ConfirmModal
          title={`Delete ${removing.name}?`}
          body={
            <>
              <p>
                Kavka stops replacing <code>{removing.pattern}</code> in{" "}
                {scopeWord(removing.applies_to).toLowerCase()}.
              </p>
              <p>
                Messages fetched after this show the real values again. If you
                only want to look once, switch the rule off instead — that keeps
                the pattern.
              </p>
            </>
          }
          confirmLabel="Delete rule"
          tone="plain"
          busy={busy}
          busyLabel="Kavka is deleting this rule"
          onConfirm={() => void deleteRule()}
          onCancel={() => setRemoving(null)}
        />
      )}

      <ToastStack {...toaster} />
    </>
  );
}

// ---------------------------------------------------------------------------
// The builder
// ---------------------------------------------------------------------------

type MaskField = "name" | "pattern" | "replacement";

interface MaskFieldError {
  field: MaskField;
  message: string;
}

interface MaskDraft {
  name: string;
  pattern: string;
  replacement: string;
  appliesTo: MaskScope;
  enabled: boolean;
  /** The preview's input. Never saved — it is a scratch pad for the pattern. */
  sample: string;
}

/** A sample long enough to be a realistic payload, short enough to be safe. */
const SAMPLE_MAX = 500;

const STARTER_SAMPLE =
  '{"orderId":1042,"card":"4111 1111 1111 1111","email":"ana@example.com"}';

function draftFrom(rule: MaskRule | null): MaskDraft {
  if (rule === null)
    return {
      name: "",
      pattern: "",
      replacement: DEFAULT_MASK_REPLACEMENT,
      appliesTo: "value",
      enabled: true,
      sample: STARTER_SAMPLE,
    };
  return {
    name: rule.name,
    pattern: rule.pattern,
    replacement: rule.replacement,
    appliesTo: (SCOPES as readonly string[]).includes(rule.applies_to)
      ? rule.applies_to
      : "value",
    enabled: rule.enabled,
    sample: STARTER_SAMPLE,
  };
}

/** What Kavka calls a rule nobody named. Never blank, never "Rule 1". */
function suggestName(draft: MaskDraft): string {
  const pattern = draft.pattern.trim();
  if (pattern.length === 0) return "Masked text";
  return `Hide ${pattern.length > 24 ? `${pattern.slice(0, 24)}…` : pattern}`;
}

/**
 * The live preview, run in THIS window's regular-expression engine.
 *
 * It is an approximation ON PURPOSE and the modal says so: the core compiles
 * the pattern with Rust's `regex`, which has no back-references and no
 * look-around and is linear-time because of it. A pattern using either compiles
 * here and is refused there — so the preview is a way to see what a match looks
 * like, never a promise that the core will accept it.
 */
function preview(
  pattern: string,
  replacement: string,
  sample: string,
): { text: string; hits: number } | { error: string } {
  if (pattern.length === 0) return { text: sample, hits: 0 };
  let re: RegExp;
  try {
    re = new RegExp(pattern, "g");
  } catch (err) {
    return {
      error:
        err instanceof Error
          ? // The engine's own words, which name the position — more use than
            // anything Kavka could paraphrase.
            err.message
          : "That isn't a regular expression this engine can read.",
    };
  }
  let hits = 0;
  const text = sample.replace(re, () => {
    hits += 1;
    return replacement;
  });
  return { text, hits };
}

function MaskRuleModal({
  rule,
  busy,
  onSave,
  onClose,
}: {
  rule: MaskRule | null;
  busy: boolean;
  onSave: (rule: MaskRule) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<MaskDraft>(() => draftFrom(rule));
  const [fieldError, setFieldError] = useState<MaskFieldError | null>(null);
  const patternRef = useRef<HTMLInputElement | null>(null);
  const controls = useRef<Partial<Record<MaskField, HTMLElement | null>>>({});
  const bind = useMemo(() => {
    const cache: Partial<Record<MaskField, (el: HTMLElement | null) => void>> = {};
    return (field: MaskField) =>
      (cache[field] ??= (el: HTMLElement | null) => {
        controls.current[field] = el;
        if (field === "pattern") patternRef.current = el as HTMLInputElement | null;
      });
  }, []);

  // A new rule keeps one id across save retries — a failed save must not leave
  // two rules behind. Same device as the alert builder.
  const draftId = useRef<string | null>(null);
  const id = rule?.id ?? (draftId.current ??= crypto.randomUUID());

  const edit = (field: MaskField | null, partial: Partial<MaskDraft>) => {
    setDraft((prev) => ({ ...prev, ...partial }));
    setFieldError((prev) => (field !== null && prev?.field === field ? null : prev));
  };

  const shown = preview(draft.pattern, draft.replacement, draft.sample);
  const patternProblem = "error" in shown ? shown.error : null;

  const submit = () => {
    if (draft.pattern.trim().length === 0) {
      setFieldError({
        field: "pattern",
        message:
          "Write the pattern to hide — e.g. \\d{13,16} for a card number, or [\\w.]+@[\\w.]+ for an email address.",
      });
      controls.current.pattern?.focus();
      return;
    }
    if (patternProblem !== null) {
      setFieldError({
        field: "pattern",
        message: `This window's engine can't read that pattern: ${patternProblem}`,
      });
      controls.current.pattern?.focus();
      return;
    }
    if (draft.replacement.length === 0) {
      setFieldError({
        field: "replacement",
        message:
          "What should the matched text become? Leave the bullets, or write something that says why it's hidden.",
      });
      controls.current.replacement?.focus();
      return;
    }
    const name = draft.name.trim();
    onSave({
      id,
      name: name.length > 0 ? name : suggestName(draft),
      pattern: draft.pattern,
      replacement: draft.replacement,
      applies_to: draft.appliesTo,
      enabled: draft.enabled,
    });
  };

  const message = (field: MaskField) =>
    fieldError?.field === field ? (
      <span className="field-error" id={`mk-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;
  const describe = (field: MaskField, hintId: string) =>
    fieldError?.field === field ? `${hintId} mk-${field}-error` : hintId;

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="mk-title"
      initialFocus={patternRef}
      onClose={onClose}
    >
      {/* ⏎ submits: saving a rule changes nothing on the cluster, so the key
          the user is already pressing is allowed to finish the job. */}
      <form
        className="modal-panel"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <h2 className="modal-title" id="mk-title">
          {rule === null ? "Add a masking rule" : `Edit ${rule.name}`}
        </h2>

        <div className="field">
          <label className="field-label" htmlFor="mk-pattern">
            What should Kavka hide?
          </label>
          <input
            id="mk-pattern"
            ref={bind("pattern")}
            type="text"
            className={`input-mono${
              fieldError?.field === "pattern" || patternProblem !== null
                ? " input-invalid"
                : ""
            }`}
            value={draft.pattern}
            placeholder="\d{13,16}"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={
              fieldError?.field === "pattern" || patternProblem !== null
                ? true
                : undefined
            }
            aria-describedby={describe("pattern", "mk-pattern-hint")}
            onChange={(e) => edit("pattern", { pattern: e.target.value })}
          />
          {message("pattern")}
          {/* The engine's complaint, live, at the field — not on submit. A
              half-typed pattern is invalid for a second and that is fine, so
              this reads as guidance rather than as a rejection. */}
          {patternProblem !== null && fieldError?.field !== "pattern" && (
            <span className="field-error">{patternProblem}</span>
          )}
          <span className="field-hint" id="mk-pattern-hint">
            A regular expression. Kavka compiles it in its core with Rust's
            engine, which has no back-references and no look-ahead — the preview
            below runs in this window's engine, so it is close but not
            identical, and a pattern using either of those is refused when you
            save.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="mk-replacement">
            What should it become?
          </label>
          <input
            id="mk-replacement"
            ref={bind("replacement")}
            type="text"
            className={`input-mono${
              fieldError?.field === "replacement" ? " input-invalid" : ""
            }`}
            value={draft.replacement}
            placeholder={DEFAULT_MASK_REPLACEMENT}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={fieldError?.field === "replacement" ? true : undefined}
            aria-describedby={describe("replacement", "mk-replacement-hint")}
            onChange={(e) => edit("replacement", { replacement: e.target.value })}
          />
          {message("replacement")}
          <span className="field-hint" id="mk-replacement-hint">
            Every match becomes this. The bullets are the default because they
            are obviously not data; something like{" "}
            <code>[card hidden]</code> works too, and says why.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="mk-scope">
            Where should Kavka look?
          </label>
          <select
            id="mk-scope"
            value={draft.appliesTo}
            aria-describedby="mk-scope-hint"
            onChange={(e) =>
              edit(null, { appliesTo: e.target.value as MaskScope })
            }
          >
            {SCOPES.map((scope) => (
              <option key={scope} value={scope}>
                {SCOPE_LABEL[scope]}
              </option>
            ))}
          </select>
          <span className="field-hint" id="mk-scope-hint">
            Header <em>values</em>, never header names — a name that vanished
            would make a record look like it carried different headers from the
            ones it has.
          </span>
        </div>

        {/* THE PREVIEW. The same device the alert builder's sentence is: the
            thing you are configuring, rendered as you type, so the fields and
            what they will do can never quietly disagree. */}
        <div className="field">
          <label className="field-label" htmlFor="mk-sample">
            Try it on some text
          </label>
          <textarea
            id="mk-sample"
            className="sql-input mask-sample"
            rows={2}
            value={draft.sample}
            maxLength={SAMPLE_MAX}
            spellCheck={false}
            aria-describedby="mk-sample-hint"
            onChange={(e) => edit(null, { sample: e.target.value })}
          />
          <span className="field-hint" id="mk-sample-hint">
            Paste a real payload if you have one — it stays in this window and
            is not saved with the rule.
          </span>
        </div>

        <div className="mask-preview">
          <span className="eyebrow">What Kavka would show</span>
          {"error" in shown ? (
            <p className="mask-preview-none">
              Nothing yet — the pattern doesn't compile.
            </p>
          ) : (
            <>
              <pre className="banner-raw mask-preview-out">{shown.text}</pre>
              <p className="mask-preview-count" role="status">
                {shown.hits === 0
                  ? draft.pattern.trim().length === 0
                    ? "No pattern yet, so nothing is replaced."
                    : "No match in this text. That may be right — try a sample the rule should catch."
                  : `${shown.hits} ${shown.hits === 1 ? "match" : "matches"} replaced.`}
              </p>
            </>
          )}
        </div>

        <div className="field">
          <label className="field-label" htmlFor="mk-name">
            What should Kavka call it?
          </label>
          <input
            id="mk-name"
            ref={bind("name")}
            type="text"
            value={draft.name}
            placeholder={suggestName(draft)}
            autoComplete="off"
            aria-describedby="mk-name-hint"
            onChange={(e) => edit("name", { name: e.target.value })}
          />
          <span className="field-hint" id="mk-name-hint">
            This is what the rules list shows. Leave it empty and Kavka names it
            after the pattern.
          </span>
        </div>

        <div className="check-field">
          <input
            id="mk-enabled"
            type="checkbox"
            checked={draft.enabled}
            aria-describedby="mk-enabled-hint"
            onChange={(e) => edit(null, { enabled: e.target.checked })}
          />
          <label className="check-label" htmlFor="mk-enabled">
            Mask with this rule now
          </label>
          <span className="field-hint" id="mk-enabled-hint">
            Off keeps the pattern written down without hiding anything — which
            is how you look at the real values for a minute without losing the
            rule.
          </span>
        </div>

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            disabled={busy}
            title={busy ? "Kavka is saving this rule" : undefined}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={busy}
            aria-busy={busy}
            title={busy ? "Kavka is saving this rule" : undefined}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {busy ? <span className="spinner" /> : null}
            </span>
            {rule === null ? "Add rule" : "Save rule"}
          </button>
        </div>
      </form>
    </Overlay>
  );
}
