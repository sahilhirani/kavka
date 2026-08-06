import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ENV_COLORS,
  environmentsDelete,
  environmentsSave,
  errorMessage,
  profilesList,
  profilesSave,
  type ConnectionProfile,
  type EnvColor,
  type EnvironmentDef,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import {
  envAttrs,
  loadEnvironments,
  PadLock,
  sameEnvironmentName,
  useEnvironments,
} from "./environments";
import { useI18n } from "./i18n";
import Overlay from "./Overlay";

/**
 * The environment manager — where dev/staging/prod stops being Kavka's opinion.
 *
 * THREE THINGS IT HAS TO GET RIGHT, and they are the reasons it is a dialog of
 * its own rather than three controls bolted onto the connection form:
 *
 *  1. **The semantics split is visible.** Colour and name are identity; the
 *     protected toggle is the guardrail. The toggle therefore states, in one
 *     sentence, exactly what flipping it changes — because it changes the
 *     behaviour of the CLI and of the MCP server, neither of which is on
 *     screen. A checkbox whose consequence lives in another process needs its
 *     consequence written next to it.
 *
 *  2. **Deleting is refused, not cascaded.** An environment a connection still
 *     names cannot be removed, because removing it would silently disarm that
 *     connection's guardrail. The flow offers REASSIGNMENT instead: pick where
 *     those connections go, and Kavka rewrites them one `profiles_save` at a
 *     time before removing the definition. Same reasoning as the store's own
 *     refusal — this dialog is the affordance that makes the refusal
 *     actionable rather than a dead end.
 *
 *  3. **Removing protection is destructive.** It is the one edit here that
 *     turns a guardrail off, so it takes the same type-to-confirm every other
 *     destructive action in the app takes (§6 layer 4) — environment-gated,
 *     which is exactly what this dialog defines.
 */

interface EnvironmentsManagerProps {
  onClose: () => void;
  /** The reassign flow rewrites profiles; the sidebar has to hear about it. */
  onProfilesChanged: () => void;
  /**
   * An environment moved: renamed, or deleted with its connections reassigned.
   *
   * The connection form underneath is holding an environment NAME in local
   * state, and rewriting the stored profiles does not reach it — so renaming
   * `prod` to `production` while editing a prod connection would leave the
   * form pointing at a name nothing defines and showing the unknown hint about
   * a change the user just made themselves. The manager reports the move and
   * the form follows it.
   */
  onEnvironmentMoved: (from: string, to: string) => void;
}

/** The editor pane's state. `null` = the list is showing. */
type Draft = {
  /** The name this draft replaces, or null when it is a new environment. */
  original: string | null;
  name: string;
  color: EnvColor;
  protected: boolean;
};

/** The delete flow's state. */
interface Pending {
  def: EnvironmentDef;
  /** Connections that still name it. Empty = a plain delete. */
  users: ConnectionProfile[];
  /** Where the users go. Empty until the user picks. */
  reassignTo: string;
}

export default function EnvironmentsManager({
  onClose,
  onProfilesChanged,
  onEnvironmentMoved,
}: EnvironmentsManagerProps) {
  const { t } = useI18n();
  const defs = useEnvironments();
  const [profiles, setProfiles] = useState<ConnectionProfile[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const nameRef = useRef<HTMLInputElement | null>(null);

  // Who uses what. Read once when the dialog opens: this is a decision aid on
  // a list the user is about to edit, not live data, and re-reading it on
  // every keystroke would put an IPC call behind a text field.
  useEffect(() => {
    let live = true;
    profilesList()
      .then((list) => {
        if (live) setProfiles(list);
      })
      // Silent: the counts degrade to zero, and the STORE still refuses to
      // delete a referenced environment. The refusal is the guarantee; this
      // list is only how the user finds out before pressing the button.
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  const usersOf = useCallback(
    (name: string) =>
      profiles.filter((p) => sameEnvironmentName(p.environment, name)),
    [profiles],
  );

  const startAdd = useCallback(() => {
    setFailure(null);
    setDraft({ original: null, name: "", color: "blue", protected: false });
  }, []);

  const startEdit = useCallback((def: EnvironmentDef) => {
    setFailure(null);
    setDraft({
      original: def.name,
      name: def.name,
      color: def.color,
      protected: def.protected,
    });
  }, []);

  // Focus the name field when the editor pane opens — it is the one control
  // the user came here to use, and the pane replaces the dialog's whole body,
  // so `Overlay`'s mount-time focus has long since run. Keyed on the boolean
  // rather than on `draft` so a keystroke in the name field does not yank the
  // caret back to its own start.
  const editing = draft !== null;
  useEffect(() => {
    if (editing) nameRef.current?.focus();
  }, [editing]);

  /**
   * Whether this draft's name collides with a DIFFERENT environment.
   *
   * Case-insensitive, matching the store: `QA` and `qa` are one environment,
   * and letting the form create the second would produce a registry the
   * backend then refuses to write — a validation message here is the same
   * answer, three seconds earlier and next to the field.
   */
  const nameTaken = useMemo(() => {
    if (draft === null) return false;
    const typed = draft.name.trim();
    if (typed === "") return false;
    return defs.some(
      (d) =>
        sameEnvironmentName(d.name, typed) &&
        (draft.original === null ||
          !sameEnvironmentName(d.name, draft.original)),
    );
  }, [draft, defs]);

  const draftValid =
    draft !== null && draft.name.trim().length > 0 && !nameTaken;

  /**
   * True when saving this draft would REMOVE protection from an environment
   * that has it. The one edit in this dialog that disarms a guardrail, so the
   * one that asks the user to type the name (§6 layer 4).
   */
  const unprotecting =
    draft !== null &&
    draft.original !== null &&
    !draft.protected &&
    defs.some(
      (d) => sameEnvironmentName(d.name, draft.original ?? "") && d.protected,
    );
  const [unprotectTyped, setUnprotectTyped] = useState("");
  const unprotectOk =
    !unprotecting || unprotectTyped === (draft?.original ?? "");

  const saveDraft = useCallback(async () => {
    if (draft === null || !draftValid || !unprotectOk) return;
    setBusy(true);
    setFailure(null);
    try {
      const name = draft.name.trim();
      // The name this save moves AWAY from, or null when nothing moves: a new
      // environment, or an edit that only touched the colour, the flag or the
      // casing (`qa` → `QA` is one environment, so it is a plain overwrite).
      const renamedFrom =
        draft.original !== null && !sameEnvironmentName(draft.original, name)
          ? draft.original
          : null;

      // DEFINITIONS FIRST — the same rule kavka-core's `apply_import` follows,
      // and for the same reason. A profile must never be readable tagged with
      // a name nothing on this machine defines, because a concurrent read (the
      // other window's sidebar, an `environments_list` this dialog already has
      // in flight) would resolve it neutral and UNPROTECTED. So the new
      // definition lands before anything points at it, and the old one goes
      // only once nothing does.
      //
      // Every intermediate state therefore has BOTH names defined, and the
      // worst a failure can leave behind is one unused definition — harmless,
      // and sitting in this dialog's own list where the user can remove it —
      // rather than N connections whose guardrail silently went away.
      //
      // Within the rename the order matters again, and the other way round:
      // the profiles move BEFORE the delete, because the store refuses to
      // remove an environment a connection still names.
      await environmentsSave({
        name,
        color: draft.color,
        protected: draft.protected,
      });
      if (renamedFrom !== null) {
        for (const p of usersOf(renamedFrom)) {
          await profilesSave({ ...p, environment: name });
        }
        await environmentsDelete(renamedFrom);
        onProfilesChanged();
        onEnvironmentMoved(renamedFrom, name);
      }
      await loadEnvironments();
      setProfiles(await profilesList().catch(() => profiles));
      setDraft(null);
      setUnprotectTyped("");
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [
    draft,
    draftValid,
    unprotectOk,
    usersOf,
    onProfilesChanged,
    onEnvironmentMoved,
    profiles,
  ]);

  const startDelete = useCallback(
    (def: EnvironmentDef) => {
      setFailure(null);
      const users = usersOf(def.name);
      setPending({
        def,
        users,
        // Default the reassignment to the first environment that is NOT the one
        // being removed. Never blank, so the flow has a valid answer the moment
        // it opens — but never guessed silently either: the select is on screen
        // and the sentence above it names the count.
        reassignTo:
          users.length === 0
            ? ""
            : (defs.find((d) => !sameEnvironmentName(d.name, def.name))?.name ??
              ""),
      });
    },
    [usersOf, defs],
  );

  const confirmDelete = useCallback(async () => {
    if (pending === null) return;
    setBusy(true);
    setFailure(null);
    try {
      if (pending.users.length > 0) {
        if (pending.reassignTo === "") {
          setFailure(t("env.mgr.delete.needTarget"));
          return;
        }
        for (const p of pending.users) {
          await profilesSave({ ...p, environment: pending.reassignTo });
        }
        onProfilesChanged();
        onEnvironmentMoved(pending.def.name, pending.reassignTo);
      }
      await environmentsDelete(pending.def.name);
      await loadEnvironments();
      setProfiles(await profilesList().catch(() => profiles));
      setPending(null);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [pending, onProfilesChanged, onEnvironmentMoved, profiles, t]);

  // The manager cannot leave the user with nothing to pick. Deleting the last
  // environment is refused here rather than by the backend, because the fix is
  // "add another one first" and this dialog is where that happens.
  const lastOne = defs.length <= 1;

  const banner =
    failure === null ? null : (
      <div className="banner banner-danger" role="alert">
        <span className="banner-glyph" aria-hidden="true">
          !
        </span>
        <div className="banner-body">
          <p className="banner-title">{t("env.mgr.failed")}</p>
          <p className="banner-detail">{failure}</p>
        </div>
      </div>
    );

  // ── The delete flow, as its own modal on top ──────────────────────────────
  if (pending !== null) {
    const { def, users } = pending;
    return (
      <ConfirmModal
        title={t("env.mgr.delete.title", { name: def.name })}
        body={
          <>
            <p>
              {users.length === 0
                ? t("env.mgr.delete.unused", { name: def.name })
                : t("env.mgr.delete.used", {
                    name: def.name,
                    count: users.length,
                  })}
            </p>
            {users.length > 0 && (
              <div className="field">
                <label className="field-label" htmlFor="env-reassign">
                  {t("env.mgr.delete.moveTo")}
                </label>
                <select
                  id="env-reassign"
                  value={pending.reassignTo}
                  disabled={busy}
                  onChange={(e) =>
                    setPending((prev) =>
                      prev === null
                        ? prev
                        : { ...prev, reassignTo: e.target.value },
                    )
                  }
                >
                  {defs
                    .filter((d) => !sameEnvironmentName(d.name, def.name))
                    .map((d) => (
                      <option key={d.name} value={d.name}>
                        {d.name}
                      </option>
                    ))}
                </select>
                <span className="field-hint">
                  {t("env.mgr.delete.moveHint", {
                    names: users.map((p) => p.name).join(", "),
                  })}
                </span>
              </div>
            )}
            {banner}
          </>
        }
        confirmLabel={t("env.mgr.delete.confirm")}
        // Environment-gated, exactly like every other destructive action:
        // removing a PROTECTED environment is the case where a mis-click
        // disarms a guardrail on every connection that named it.
        typeToConfirm={def.protected ? def.name : null}
        busy={busy}
        busyLabel={t("env.mgr.working")}
        onCancel={() => {
          setPending(null);
          setFailure(null);
        }}
        onConfirm={() => void confirmDelete()}
      />
    );
  }

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="env-mgr-title"
      initialFocus={draft === null ? closeRef : nameRef}
      onClose={onClose}
    >
      <h2 className="modal-title" id="env-mgr-title">
        {t("env.mgr.title")}
      </h2>

      {draft === null ? (
        <>
          <p className="modal-body">{t("env.mgr.intro")}</p>
          {banner}

          {/* WHAT PROTECTED ACTUALLY DOES, before the switches that set it.
              This dialog is where a user decides what protection means for
              their organisation, and until now it asked for that decision
              without stating the consequences — two of which happen in other
              processes (the CLI and the MCP server) and are therefore
              invisible from here no matter how carefully you read the list.
              A guardrail nobody can see the shape of is a guardrail people
              turn off. */}
          <div className="banner banner-info">
            <span className="banner-glyph" aria-hidden="true">
              i
            </span>
            <div className="banner-body">
              <p className="banner-title">{t("env.mgr.hint.title")}</p>
              <p className="banner-detail">{t("env.mgr.hint.detail")}</p>
            </div>
          </div>

          <ul className="env-list">
            {defs.map((def) => {
              const users = usersOf(def.name);
              return (
                // The ATTRIBUTES ARE ON THE ROW, not only on the chip. This
                // list is the one screen whose subject is which environments
                // are protected, and a chip alone left the answer to a 26px
                // pill — legible in the dark theme's warm ground, close to
                // invisible against warm paper. Carrying the flag on the <li>
                // gives the row the protected substrate that every other
                // protected surface in the app has, in BOTH themes, and the
                // spine and warm ground come from tokens rather than from a
                // literal that would only be right in one of them.
                //
                // Identity still comes from the chip, protection from the
                // ground, and the WORD in `.env-list-meta` still says which
                // is which — Law 2 is what makes the ground safe to add.
                <li key={def.name} className="env-list-row" {...envAttrs(def)}>
                  <span className="env-chip" {...envAttrs(def)}>
                    {def.name}
                    {/* THE THIRD SIGNAL. §6 and the mockup both insist a
                        protected environment is spelled three ways — the warm
                        ground under the row, the padlock, and the WORD in the
                        meta line beside it. The app carried two. The glyph is
                        `aria-hidden` precisely because the word is already
                        there and doubling it would make a screen reader say
                        "protected" twice on one row. */}
                    {def.protected && <PadLock />}
                  </span>
                  {/* Law 2: the colour never carries the meaning alone. The
                      row says in words whether it is protected and how many
                      connections it governs. */}
                  <span className="env-list-meta">
                    {def.protected
                      ? t("env.mgr.row.protected")
                      : t("env.mgr.row.unprotected")}
                    {" · "}
                    {t("env.mgr.row.used", { count: users.length })}
                  </span>
                  <span className="env-list-actions">
                    <button
                      type="button"
                      className="btn btn-row"
                      onClick={() => startEdit(def)}
                    >
                      {t("env.mgr.edit")}
                    </button>
                    <button
                      type="button"
                      className="btn btn-row"
                      disabled={lastOne}
                      title={lastOne ? t("env.mgr.delete.last") : undefined}
                      onClick={() => startDelete(def)}
                    >
                      {t("common.remove")}
                    </button>
                  </span>
                </li>
              );
            })}
          </ul>

          {/* The list is not a menu of three. Kavka ships dev/staging/prod
              because most people start there, and a user whose organisation
              has five is being told, here, that the app is not going to argue
              about it. */}
          <p className="dialog-note">{t("env.mgr.namesAreYours")}</p>

          <div className="modal-actions">
            <button type="button" className="btn" onClick={startAdd}>
              {t("env.mgr.add")}
            </button>
            <button
              type="button"
              className="btn btn-primary"
              ref={closeRef}
              onClick={onClose}
            >
              {t("common.close")}
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="field">
            <label className="field-label" htmlFor="env-name">
              {t("env.mgr.name.label")}
            </label>
            <input
              id="env-name"
              ref={nameRef}
              type="text"
              className="input-mono"
              value={draft.name}
              spellCheck={false}
              autoComplete="off"
              placeholder="UAT"
              aria-invalid={nameTaken || undefined}
              aria-describedby="env-name-hint"
              onChange={(e) =>
                setDraft((prev) =>
                  prev === null ? prev : { ...prev, name: e.target.value },
                )
              }
            />
            {nameTaken && (
              <span className="field-error">{t("env.mgr.name.taken")}</span>
            )}
            <span className="field-hint" id="env-name-hint">
              {t("env.mgr.name.hint")}
            </span>
          </div>

          <div className="field">
            <span className="field-label">{t("env.mgr.color.label")}</span>
            {/* A radio group, so it is one tab stop walked with the arrows —
                the same promise `ClusterView`'s strip keeps (§9 gate 7.1). */}
            <div
              className="env-swatches"
              role="radiogroup"
              aria-label={t("env.mgr.color.label")}
            >
              {ENV_COLORS.map((color, index) => (
                <button
                  key={color}
                  type="button"
                  role="radio"
                  aria-checked={draft.color === color}
                  aria-label={t(COLOR_KEY[color])}
                  tabIndex={draft.color === color ? 0 : -1}
                  className={`env-swatch ${
                    draft.color === color ? "env-swatch-active" : ""
                  }`}
                  data-env-color={color}
                  data-env-protected={draft.protected ? "true" : undefined}
                  onClick={() =>
                    setDraft((prev) => (prev === null ? prev : { ...prev, color }))
                  }
                  onKeyDown={(e) => {
                    let next: number | null = null;
                    if (e.key === "ArrowRight" || e.key === "ArrowDown")
                      next = (index + 1) % ENV_COLORS.length;
                    else if (e.key === "ArrowLeft" || e.key === "ArrowUp")
                      next = (index - 1 + ENV_COLORS.length) % ENV_COLORS.length;
                    else if (e.key === "Home") next = 0;
                    else if (e.key === "End") next = ENV_COLORS.length - 1;
                    if (next === null) return;
                    e.preventDefault();
                    const target = ENV_COLORS[next];
                    setDraft((prev) =>
                      prev === null ? prev : { ...prev, color: target },
                    );
                    (
                      e.currentTarget.parentElement?.children[
                        next
                      ] as HTMLElement | null
                    )?.focus();
                  }}
                >
                  {/* Never colour alone: the swatch carries its own name.
                      This grid is the one place in the app whose SUBJECT is
                      colour, so it is the one place the word matters most. */}
                  {t(COLOR_KEY[color])}
                </button>
              ))}
            </div>
            <span className="field-hint">{t("env.mgr.color.hint")}</span>
          </div>

          <div className="check-field">
            <input
              id="env-protected"
              type="checkbox"
              checked={draft.protected}
              disabled={busy}
              aria-describedby="env-protected-hint"
              onChange={(e) =>
                setDraft((prev) =>
                  prev === null
                    ? prev
                    : { ...prev, protected: e.target.checked },
                )
              }
            />
            <label className="check-label" htmlFor="env-protected">
              {t("env.mgr.protected.label")}
            </label>
            {/* The one sentence that says what the toggle actually does — and
                it has to be one sentence, because two of the four things it
                changes are in other processes. */}
            <span className="field-hint" id="env-protected-hint">
              {t("env.mgr.protected.hint")}
            </span>
          </div>

          {unprotecting && (
            <div className="field confirm-type">
              <label className="field-label" htmlFor="env-unprotect">
                {t("env.mgr.unprotect.prompt", { name: draft.original ?? "" })}
              </label>
              <input
                id="env-unprotect"
                type="text"
                className="input-mono"
                value={unprotectTyped}
                autoComplete="off"
                spellCheck={false}
                onChange={(e) => setUnprotectTyped(e.target.value)}
              />
              <span className="field-hint">
                {t("env.mgr.unprotect.hint", { name: draft.original ?? "" })}
              </span>
            </div>
          )}

          {banner}

          <div className="modal-actions">
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => {
                setDraft(null);
                setUnprotectTyped("");
                setFailure(null);
              }}
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className={`btn ${unprotecting ? "btn-danger-confirm" : "btn-primary"} btn-swap`}
              disabled={!draftValid || !unprotectOk || busy}
              aria-busy={busy || undefined}
              title={
                busy
                  ? t("env.mgr.working")
                  : !draftValid
                    ? t("env.mgr.name.required")
                    : !unprotectOk
                      ? t("env.mgr.unprotect.prompt", {
                          name: draft.original ?? "",
                        })
                      : undefined
              }
              onClick={() => void saveDraft()}
            >
              <span className="btn-swap-face">
                {t("common.save")}
              </span>
              <span className="btn-swap-face btn-swap-busy">
                <span className="spinner" aria-hidden="true" />
                {t("common.save")}
              </span>
            </button>
          </div>
        </>
      )}
    </Overlay>
  );
}

/**
 * The colour tokens' words. Law 2 again: a swatch grid identified by hue alone
 * is unusable to a deuteranope, and these are the labels the screen reader
 * reads too.
 */
const COLOR_KEY = {
  green: "env.color.green",
  amber: "env.color.amber",
  red: "env.color.red",
  blue: "env.color.blue",
  violet: "env.color.violet",
  cyan: "env.color.cyan",
  slate: "env.color.slate",
} as const;
