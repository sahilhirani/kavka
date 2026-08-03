import { useCallback, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  profilesDelete,
  profilesSave,
  secretDelete,
  secretSet,
  type AuthConfig,
  type ConnectionProfile,
  type ConnStatus,
  type Environment,
  type ScramMechanism,
} from "./api";
import { classifyError } from "./errors";
import { Term } from "./Glossary";

/**
 * The three-layer error banner from DESIGN.md §5.8 / §7: plain title, then
 * the next click, then the raw broker string verbatim behind `Show details`.
 * Lives here rather than in App so ProfileEditor can use it without a cycle
 * (App already imports this module; this module imports nothing of App's).
 */
export function ErrorBanner({
  raw,
  onDismiss,
}: {
  raw: string;
  onDismiss?: () => void;
}) {
  const { title, detail } = classifyError(raw);
  return (
    <div className="banner banner-danger" role="alert">
      <span className="banner-glyph" aria-hidden="true">
        !
      </span>
      <div className="banner-body">
        <p className="banner-title">{title}</p>
        <p className="banner-detail">{detail}</p>
        {/* Experts get the truth one click away; novices never have to read
            it. Unconditional: even an unrecognized error's title is only its
            first line, so the full text must stay reachable. */}
        <details className="banner-details">
          <summary>Show details</summary>
          <pre className="banner-raw">{raw}</pre>
        </details>
      </div>
      {onDismiss && (
        <div className="banner-actions">
          <button type="button" className="btn btn-ghost" onClick={onDismiss}>
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}

/** Auth kinds fully implemented in this slice. */
type EditableAuthKind = "plaintext" | "sasl_plain" | "sasl_scram";

/** Every control validation can point at. */
type FieldKey = "name" | "bootstrap" | "username" | "password";

interface FieldError {
  field: FieldKey;
  message: string;
}

/** Why each unimplemented sign-in method is disabled. Never a dead option. */
const NOT_YET =
  "Kavka can't set this up yet. A connection that already uses it keeps working and is preserved exactly as it is when you save.";

interface FormState {
  name: string;
  environment: Environment;
  /** Raw text; comma or newline separated. */
  bootstrap: string;
  authKind: EditableAuthKind;
  username: string;
  /** Raw password input. NEVER stored in the profile; sent to secret_set only. */
  password: string;
  mechanism: ScramMechanism;
  tls: boolean;
  readOnly: boolean;
}

function initialForm(profile: ConnectionProfile | null): FormState {
  const base: FormState = {
    name: "",
    environment: "dev",
    bootstrap: "",
    authKind: "plaintext",
    username: "",
    password: "",
    mechanism: "SCRAM-SHA-256",
    tls: false,
    readOnly: false,
  };
  if (!profile) return base;
  base.name = profile.name;
  base.environment = profile.environment;
  base.bootstrap = profile.bootstrap_servers.join("\n");
  base.readOnly = profile.read_only;
  const auth = profile.auth;
  if (auth.kind === "sasl_plain") {
    base.authKind = "sasl_plain";
    base.username = auth.username;
    base.tls = auth.tls;
  } else if (auth.kind === "sasl_scram") {
    base.authKind = "sasl_scram";
    base.username = auth.username;
    base.mechanism = auth.mechanism;
    base.tls = auth.tls;
  }
  // Other kinds (tls / aws_msk_iam / oauth_bearer / kerberos) are not editable
  // yet; they fall back to plaintext in the form.
  return base;
}

function parseBootstrap(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

interface ProfileEditorProps {
  /** null = creating a new profile. */
  profile: ConnectionProfile | null;
  connStatus: ConnStatus;
  /** The last connect failure for this profile, raw. Cleared on retry. */
  connError?: string;
  onSaved: (profile: ConnectionProfile) => void;
  onConnect: (profile: ConnectionProfile) => void;
  onDeleted: (profileId: string) => void;
  onCancelNew: () => void;
  onError: (msg: string) => void;
}

export default function ProfileEditor({
  profile,
  connStatus,
  connError,
  onSaved,
  onConnect,
  onDeleted,
  onCancelNew,
  onError,
}: ProfileEditorProps) {
  const isNew = profile === null;
  const [form, setForm] = useState<FormState>(() => initialForm(profile));
  const [busy, setBusy] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  // Validation belongs next to the control it is about, not in a banner at
  // the top of the workspace where the user has to hunt for the field.
  const [fieldError, setFieldError] = useState<FieldError | null>(null);
  // A new profile keeps one id across save retries so a failed attempt can't
  // orphan keychain entries under abandoned ids.
  const draftIdRef = useRef<string | null>(null);
  // Focus targets for a failed submit. A message the user cannot see because
  // the field is scrolled off screen is not a message.
  const controls = useRef<Partial<Record<FieldKey, HTMLElement | null>>>({});
  // Stable per-field ref callbacks: a fresh closure each render would detach
  // and reattach every control on every keystroke.
  const bind = useMemo(() => {
    const cache: Partial<Record<FieldKey, (el: HTMLElement | null) => void>> = {};
    return (field: FieldKey) =>
      (cache[field] ??= (el: HTMLElement | null) => {
        controls.current[field] = el;
      });
  }, []);

  // Auth kinds the form can't edit yet (tls / aws_msk_iam / oauth_bearer /
  // kerberos): shown read-only and preserved verbatim on save — never silently
  // downgraded to plaintext.
  const unsupportedAuth =
    profile !== null &&
    profile.auth.kind !== "plaintext" &&
    profile.auth.kind !== "sasl_plain" &&
    profile.auth.kind !== "sasl_scram"
      ? profile.auth
      : null;

  const patch = useCallback((partial: Partial<FormState>) => {
    setForm((prev) => ({ ...prev, ...partial }));
  }, []);

  /** Editing a field clears its own error. Never adds one — see §5.3. */
  const edit = useCallback(
    (field: FieldKey, partial: Partial<FormState>) => {
      setForm((prev) => ({ ...prev, ...partial }));
      setFieldError((prev) => (prev?.field === field ? null : prev));
    },
    [],
  );

  // The profile already has a stored password secret we can leave untouched.
  const hasStoredPassword =
    profile !== null &&
    (profile.auth.kind === "sasl_plain" || profile.auth.kind === "sasl_scram");

  const needsPassword =
    (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") &&
    !hasStoredPassword;

  /**
   * Returns the FIRST problem and the control it belongs to, or null. Naming
   * the field is the whole point: "Invalid input" at the top of the page is
   * the failure mode this replaces.
   */
  const validate = useCallback((): FieldError | null => {
    if (form.name.trim().length === 0)
      return {
        field: "name",
        message:
          "Give this connection a name so you can find it in the sidebar.",
      };
    if (parseBootstrap(form.bootstrap).length === 0)
      return {
        field: "bootstrap",
        message: "Add at least one broker, as host:port — e.g. broker-1:9092",
      };
    if (
      !unsupportedAuth &&
      (form.authKind === "sasl_plain" || form.authKind === "sasl_scram")
    ) {
      if (form.username.trim().length === 0)
        return {
          field: "username",
          message: "This sign-in method needs the username the broker knows you by.",
        };
      if (needsPassword && form.password.length === 0)
        return {
          field: "password",
          message: "This sign-in method needs a password.",
        };
    }
    return null;
  }, [form, needsPassword, unsupportedAuth]);

  /**
   * Persist the profile. If a password was entered it is written to the secret
   * store first; the profile itself only ever carries a SecretRef.
   * Returns the saved profile, or null on validation/IPC failure.
   */
  const save = useCallback(async (): Promise<ConnectionProfile | null> => {
    const problem = validate();
    if (problem) {
      setFieldError(problem);
      controls.current[problem.field]?.focus();
      return null;
    }
    setFieldError(null);
    const id = profile?.id ?? (draftIdRef.current ??= crypto.randomUUID());
    const secretEntry = `${id}/password`;

    let auth: AuthConfig;
    if (unsupportedAuth) {
      auth = unsupportedAuth;
    } else {
      switch (form.authKind) {
        case "plaintext":
          auth = { kind: "plaintext" };
          break;
        case "sasl_plain":
          auth = {
            kind: "sasl_plain",
            username: form.username.trim(),
            password: { entry: secretEntry },
            tls: form.tls,
          };
          break;
        case "sasl_scram":
          auth = {
            kind: "sasl_scram",
            mechanism: form.mechanism,
            username: form.username.trim(),
            password: { entry: secretEntry },
            tls: form.tls,
          };
          break;
      }
    }

    const next: ConnectionProfile = {
      id,
      name: form.name.trim(),
      environment: form.environment,
      bootstrap_servers: parseBootstrap(form.bootstrap),
      auth,
      read_only: form.readOnly,
    };

    const writingSecret =
      !unsupportedAuth &&
      (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") &&
      form.password.length > 0;
    // The stored password is obsolete if auth moves off SASL.
    const droppingSasl = hasStoredPassword && form.authKind === "plaintext";

    setBusy(true);
    try {
      if (writingSecret) {
        await secretSet(secretEntry, form.password);
      }
      try {
        await profilesSave(next);
      } catch (err) {
        // Don't leave a fresh secret behind for a profile that never existed.
        if (writingSecret && isNew) void secretDelete(secretEntry);
        throw err;
      }
      if (droppingSasl) {
        // Best-effort cleanup; an orphaned entry is harmless.
        void secretDelete(secretEntry);
      }
      setForm((prev) => ({ ...prev, password: "" }));
      return next;
    } catch (err) {
      onError(errorMessage(err));
      return null;
    } finally {
      setBusy(false);
    }
  }, [
    form,
    profile,
    isNew,
    validate,
    onError,
    unsupportedAuth,
    hasStoredPassword,
  ]);

  const handleSave = useCallback(async () => {
    const saved = await save();
    if (saved) onSaved(saved);
  }, [save, onSaved]);

  const handleConnect = useCallback(async () => {
    const saved = await save();
    if (saved) {
      onSaved(saved);
      onConnect(saved);
    }
  }, [save, onSaved, onConnect]);

  const handleDelete = useCallback(async () => {
    if (!profile) return;
    setBusy(true);
    try {
      await profilesDelete(profile.id);
      onDeleted(profile.id);
    } catch (err) {
      onError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [profile, onDeleted, onError]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLFormElement>) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (isNew) {
          onCancelNew();
        } else {
          setForm(initialForm(profile));
          setFieldError(null);
          setConfirmingDelete(false);
        }
      }
    },
    [isNew, profile, onCancelNew],
  );

  const connecting = connStatus === "connecting";
  const showSasl =
    form.authKind === "sasl_plain" || form.authKind === "sasl_scram";
  const waitReason = connecting
    ? "Wait for the connection attempt to finish"
    : busy
      ? "Kavka is saving this connection"
      : undefined;
  const savingReason = busy ? "Kavka is saving this connection" : undefined;

  /** aria-describedby that keeps the hint AND adds the error when there is one. */
  const describe = (field: FieldKey, hintId?: string) =>
    [hintId, fieldError?.field === field ? `pe-${field}-error` : null]
      .filter(Boolean)
      .join(" ") || undefined;
  const invalid = (field: FieldKey) =>
    fieldError?.field === field ? true : undefined;
  const cls = (field: FieldKey, base = "") =>
    `${base}${fieldError?.field === field ? " input-invalid" : ""}`.trim() ||
    undefined;

  /** The message under the control it belongs to. A plain render helper, not
      a nested component — a component declared here would remount its span
      on every keystroke. */
  const fieldMessage = (field: FieldKey) =>
    fieldError?.field === field ? (
      <span className="field-error" id={`pe-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;

  return (
    // Picking prod swaps this form's substrate live — the rule, the tints and
    // the segmented control all turn coral. That is the single best moment in
    // the product to teach the guardrail.
    <form
      className="editor"
      data-env={form.environment}
      onKeyDown={handleKeyDown}
      onSubmit={(e) => {
        e.preventDefault();
        void handleConnect();
      }}
    >
      <div className="view-header">
        <h1 className="view-title">
          {isNew ? "Add a connection" : profile.name}
        </h1>
        <span className="view-subtitle">
          {isNew
            ? "One broker is enough to start — Kavka discovers the rest of the cluster from there."
            : "Not connected. Check the details below, then connect."}
        </span>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="pe-name">
          Connection name
        </label>
        <input
          id="pe-name"
          ref={bind("name")}
          type="text"
          className={cls("name")}
          value={form.name}
          placeholder="orders — local"
          autoFocus={isNew}
          aria-invalid={invalid("name")}
          aria-describedby={describe("name", "pe-name-hint")}
          onChange={(e) => edit("name", { name: e.target.value })}
        />
        {fieldMessage("name")}
        <span className="field-hint" id="pe-name-hint">
          Whatever you'll recognise in the sidebar. Only Kavka sees it.
        </span>
      </div>

      <div className="field">
        <span className="field-label">Environment</span>
        <div className="env-picker" role="radiogroup" aria-label="Environment">
          {(["dev", "staging", "prod"] as const).map((env) => (
            <button
              key={env}
              type="button"
              role="radio"
              aria-checked={form.environment === env}
              className={`env-option ${
                form.environment === env ? "env-option-active" : ""
              }`}
              onClick={() => patch({ environment: env })}
            >
              {env}
            </button>
          ))}
        </div>
        <span className="field-hint">
          {form.environment === "prod"
            ? "Prod turns the ledger rule coral in every table, marks this cluster in the sidebar and puts a warning bar across the top of the window. Turn on read-only below unless you actually need to write."
            : "Kavka colours every view by environment, so you can't mistake one cluster for another."}
        </span>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="pe-bootstrap">
          <Term name="bootstrap-server">Bootstrap servers</Term>
        </label>
        <textarea
          id="pe-bootstrap"
          ref={bind("bootstrap")}
          rows={3}
          className={cls("bootstrap")}
          value={form.bootstrap}
          placeholder={"broker-1:9092\nbroker-2:9092"}
          spellCheck={false}
          aria-invalid={invalid("bootstrap")}
          aria-describedby={describe("bootstrap", "pe-bootstrap-hint")}
          onChange={(e) => edit("bootstrap", { bootstrap: e.target.value })}
        />
        {fieldMessage("bootstrap")}
        <span className="field-hint" id="pe-bootstrap-hint">
          Any single broker in your cluster — Kavka finds the rest from there.
          One per line, or comma separated. Running this repo's dev cluster?
          Use <code>localhost:9092</code>.
        </span>
      </div>

      <fieldset className="fieldset">
        <legend className="eyebrow">Sign-in</legend>

        {unsupportedAuth && (
          <span className="field-hint">
            This connection signs in with <code>{unsupportedAuth.kind}</code>,
            which Kavka can't edit yet. Saving keeps it exactly as it is; every
            other field here still works.
          </span>
        )}

        {!unsupportedAuth && (
          <div className="field">
            <label className="field-label" htmlFor="pe-auth-kind">
              How does this cluster check who you are?
            </label>
            <select
              id="pe-auth-kind"
              value={form.authKind}
              onChange={(e) =>
                patch({ authKind: e.target.value as EditableAuthKind })
              }
            >
              <option value="plaintext">
                It doesn't — anyone can connect (PLAINTEXT)
              </option>
              <option value="sasl_plain">
                Username and password — SASL/PLAIN
              </option>
              <option value="sasl_scram">
                Username and password — SASL/SCRAM
              </option>
              {/* Every disabled control says why. No dead ends. */}
              <option value="tls" disabled title={NOT_YET}>
                Client certificate (mTLS) — not yet
              </option>
              <option value="aws_msk_iam" disabled title={NOT_YET}>
                AWS MSK IAM — not yet
              </option>
              <option value="oauth_bearer" disabled title={NOT_YET}>
                OAuth / OIDC — not yet
              </option>
              <option value="kerberos" disabled title={NOT_YET}>
                Kerberos — not yet
              </option>
            </select>
            <span className="field-hint">
              Managed Kafka usually wants SASL/SCRAM with TLS on. A local
              broker usually wants nothing at all. The four greyed-out methods
              arrive in a later release.
            </span>
          </div>
        )}

        {!unsupportedAuth && form.authKind === "sasl_scram" && (
          <div className="field">
            <label className="field-label" htmlFor="pe-mechanism">
              SCRAM mechanism
            </label>
            <select
              id="pe-mechanism"
              value={form.mechanism}
              onChange={(e) =>
                patch({ mechanism: e.target.value as ScramMechanism })
              }
            >
              <option value="SCRAM-SHA-256">SCRAM-SHA-256</option>
              <option value="SCRAM-SHA-512">SCRAM-SHA-512</option>
            </select>
            <span className="field-hint">
              If the broker rejects one, it will tell you which it wants.
            </span>
          </div>
        )}

        {!unsupportedAuth && showSasl && (
          <>
            <div className="field">
              <label className="field-label" htmlFor="pe-username">
                Username
              </label>
              <input
                id="pe-username"
                ref={bind("username")}
                type="text"
                className={cls("username", "input-mono")}
                value={form.username}
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("username")}
                aria-describedby={describe("username")}
                onChange={(e) => edit("username", { username: e.target.value })}
              />
              {fieldMessage("username")}
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-password">
                Password
              </label>
              <input
                id="pe-password"
                ref={bind("password")}
                type="password"
                className={cls("password")}
                value={form.password}
                autoComplete="new-password"
                placeholder={
                  hasStoredPassword ? "••••••••  (unchanged)" : "Password"
                }
                aria-invalid={invalid("password")}
                aria-describedby={describe("password", "pe-password-hint")}
                onChange={(e) => edit("password", { password: e.target.value })}
              />
              {fieldMessage("password")}
              <span className="field-hint" id="pe-password-hint">
                Goes to your operating system's keychain — never into the
                connection file, and never off this machine.
              </span>
            </div>

            {/* 18px box in an 18px/1fr grid with the hint in row 2 /
                column 2. The hint is a sibling, not part of the label, so it
                describes the control instead of renaming it. */}
            <div className="check-field">
              <input
                id="pe-tls"
                type="checkbox"
                checked={form.tls}
                aria-describedby="pe-tls-hint"
                onChange={(e) => patch({ tls: e.target.checked })}
              />
              <label className="check-label" htmlFor="pe-tls">
                Encrypt the connection (TLS)
              </label>
              <span className="field-hint" id="pe-tls-hint">
                Managed Kafka almost always needs this on. If the broker
                answers but the handshake fails, this is the first thing to
                try.
              </span>
            </div>
          </>
        )}
      </fieldset>

      <div className="check-field">
        <input
          id="pe-readonly"
          type="checkbox"
          checked={form.readOnly}
          aria-describedby="pe-readonly-hint"
          onChange={(e) => patch({ readOnly: e.target.checked })}
        />
        <label className="check-label" htmlFor="pe-readonly">
          Read-only connection
        </label>
        <span className="field-hint" id="pe-readonly-hint">
          Kavka will still browse everything, but it won't produce messages,
          change topics or commit offsets over this connection.
        </span>
      </div>

      {/* The last connect failure, in full, right above the button that
          caused it — and still on screen while the user fixes the field it
          points at. It clears when the next attempt starts, not on a timer
          and not on a click somewhere else. */}
      {connError && !connecting && <ErrorBanner raw={connError} />}

      <div className="editor-actions">
        <div className="editor-actions-left">
          <button
            type="button"
            className="btn"
            disabled={busy || connecting}
            title={waitReason}
            onClick={() => void handleSave()}
          >
            Save
          </button>
          {/* The busy slot is a fixed 16px and always present, so the button
              never resizes mid-click and shifts everything after it. */}
          <button
            type="submit"
            className="btn btn-primary"
            disabled={busy || connecting}
            aria-busy={connecting}
            title={waitReason}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {connecting ? <span className="spinner" /> : null}
            </span>
            Connect
          </button>
          {isNew && (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              title={savingReason}
              onClick={onCancelNew}
            >
              Cancel
            </button>
          )}
        </div>
        {!isNew && (
          <div className="editor-actions-right">
            {confirmingDelete ? (
              <span className="confirm-inline">
                {/* Destructive copy states the blast radius before the button,
                    and the confirm restates the verb — never "OK". */}
                <span className="confirm-text">
                  Remove {profile.name} from this machine? The cluster itself
                  isn't touched.
                </span>
                <button
                  type="button"
                  className="btn"
                  disabled={busy}
                  title={savingReason}
                  onClick={() => setConfirmingDelete(false)}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className="btn btn-danger-confirm"
                  disabled={busy || connecting}
                  title={waitReason}
                  onClick={() => void handleDelete()}
                >
                  Delete connection
                </button>
              </span>
            ) : (
              <button
                type="button"
                className="btn btn-danger"
                disabled={busy || connecting}
                title={waitReason}
                onClick={() => setConfirmingDelete(true)}
              >
                Delete connection
              </button>
            )}
          </div>
        )}
      </div>

      <span className="editor-kbd-hint">
        <span className="kbd">Enter</span> connect
        <span aria-hidden="true">·</span>
        <span className="kbd">Esc</span> {isNew ? "cancel" : "undo edits"}
      </span>
    </form>
  );
}
