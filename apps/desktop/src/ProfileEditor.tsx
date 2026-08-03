import { useCallback, useRef, useState } from "react";
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

/** Auth kinds fully implemented in this slice. */
type EditableAuthKind = "plaintext" | "sasl_plain" | "sasl_scram";

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
  onSaved: (profile: ConnectionProfile) => void;
  onConnect: (profile: ConnectionProfile) => void;
  onDeleted: (profileId: string) => void;
  onCancelNew: () => void;
  onError: (msg: string) => void;
}

export default function ProfileEditor({
  profile,
  connStatus,
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
  // A new profile keeps one id across save retries so a failed attempt can't
  // orphan keychain entries under abandoned ids.
  const draftIdRef = useRef<string | null>(null);

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

  // The profile already has a stored password secret we can leave untouched.
  const hasStoredPassword =
    profile !== null &&
    (profile.auth.kind === "sasl_plain" || profile.auth.kind === "sasl_scram");

  const needsPassword =
    (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") &&
    !hasStoredPassword;

  const validate = useCallback((): string | null => {
    if (form.name.trim().length === 0) return "Connection name is required.";
    if (parseBootstrap(form.bootstrap).length === 0)
      return "At least one bootstrap server is required.";
    if (
      !unsupportedAuth &&
      (form.authKind === "sasl_plain" || form.authKind === "sasl_scram")
    ) {
      if (form.username.trim().length === 0)
        return "Username is required for SASL authentication.";
      if (needsPassword && form.password.length === 0)
        return "Password is required for SASL authentication.";
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
      onError(problem);
      return null;
    }
    const id =
      profile?.id ?? (draftIdRef.current ??= crypto.randomUUID());
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
          setConfirmingDelete(false);
        }
      }
    },
    [isNew, profile, onCancelNew],
  );

  const connecting = connStatus === "connecting";
  const showSasl =
    form.authKind === "sasl_plain" || form.authKind === "sasl_scram";

  return (
    <form
      className="editor"
      onKeyDown={handleKeyDown}
      onSubmit={(e) => {
        e.preventDefault();
        void handleConnect();
      }}
    >
      <div className="editor-header">
        <h1 className="editor-title">
          {isNew ? "New connection" : profile.name}
        </h1>
        {!isNew && (
          <span className="editor-subtitle">
            Disconnected — edit the profile or connect.
          </span>
        )}
      </div>

      <div className="field">
        <label className="field-label" htmlFor="pe-name">
          Name
        </label>
        <input
          id="pe-name"
          type="text"
          value={form.name}
          placeholder="e.g. orders — local"
          autoFocus={isNew}
          onChange={(e) => patch({ name: e.target.value })}
        />
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
              className={`env-option env-option-${env} ${
                form.environment === env ? "env-option-active" : ""
              }`}
              onClick={() => patch({ environment: env })}
            >
              {env}
            </button>
          ))}
        </div>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="pe-bootstrap">
          Bootstrap servers
        </label>
        <textarea
          id="pe-bootstrap"
          rows={3}
          value={form.bootstrap}
          placeholder={"broker-1:9092\nbroker-2:9092"}
          spellCheck={false}
          onChange={(e) => patch({ bootstrap: e.target.value })}
        />
        <span className="field-hint">Comma or newline separated.</span>
      </div>

      <fieldset className="auth-section">
        <legend className="field-label">Authentication</legend>

        {unsupportedAuth && (
          <div className="field">
            <span className="field-hint">
              This profile uses <code>{unsupportedAuth.kind}</code>{" "}
              authentication, which can't be edited yet. Saving keeps it
              unchanged; the other fields stay editable.
            </span>
          </div>
        )}

        {!unsupportedAuth && (
        <div className="field">
          <label className="field-label field-label-sub" htmlFor="pe-auth-kind">
            Method
          </label>
          <select
            id="pe-auth-kind"
            value={form.authKind}
            onChange={(e) =>
              patch({ authKind: e.target.value as EditableAuthKind })
            }
          >
            <option value="plaintext">Plaintext (no authentication)</option>
            <option value="sasl_plain">SASL / PLAIN</option>
            <option value="sasl_scram">SASL / SCRAM</option>
            <option value="tls" disabled>
              TLS client certificate (coming soon)
            </option>
            <option value="aws_msk_iam" disabled>
              AWS MSK IAM (coming soon)
            </option>
            <option value="oauth_bearer" disabled>
              OAuth Bearer (coming soon)
            </option>
            <option value="kerberos" disabled>
              Kerberos (coming soon)
            </option>
          </select>
        </div>
        )}

        {!unsupportedAuth && form.authKind === "sasl_scram" && (
          <div className="field">
            <label
              className="field-label field-label-sub"
              htmlFor="pe-mechanism"
            >
              Mechanism
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
          </div>
        )}

        {!unsupportedAuth && showSasl && (
          <>
            <div className="field">
              <label
                className="field-label field-label-sub"
                htmlFor="pe-username"
              >
                Username
              </label>
              <input
                id="pe-username"
                type="text"
                value={form.username}
                autoComplete="off"
                spellCheck={false}
                onChange={(e) => patch({ username: e.target.value })}
              />
            </div>
            <div className="field">
              <label
                className="field-label field-label-sub"
                htmlFor="pe-password"
              >
                Password
              </label>
              <input
                id="pe-password"
                type="password"
                value={form.password}
                autoComplete="new-password"
                placeholder={
                  hasStoredPassword ? "••••••••  (unchanged)" : "Password"
                }
                onChange={(e) => patch({ password: e.target.value })}
              />
              <span className="field-hint">
                Stored in the OS keychain, never in the profile file.
              </span>
            </div>
            <label className="check-row">
              <input
                type="checkbox"
                checked={form.tls}
                onChange={(e) => patch({ tls: e.target.checked })}
              />
              <span>Use TLS</span>
            </label>
          </>
        )}
      </fieldset>

      <label className="check-row">
        <input
          type="checkbox"
          checked={form.readOnly}
          onChange={(e) => patch({ readOnly: e.target.checked })}
        />
        <span>Read-only connection</span>
      </label>
      <span className="field-hint field-hint-indent">
        Blocks producing, topic changes and offset commits over this
        connection.
      </span>

      <div className="editor-actions">
        <div className="editor-actions-left">
          <button
            type="button"
            className="btn"
            disabled={busy || connecting}
            onClick={() => void handleSave()}
          >
            Save
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={busy || connecting}
          >
            {connecting ? "Connecting…" : "Connect"}
          </button>
          {isNew && (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={onCancelNew}
            >
              Cancel
            </button>
          )}
        </div>
        {!isNew && (
          <div className="editor-actions-right">
            {confirmingDelete ? (
              <span className="confirm-delete">
                <span className="confirm-delete-text">Delete this profile?</span>
                <button
                  type="button"
                  className="btn btn-danger"
                  disabled={busy || connecting}
                  onClick={() => void handleDelete()}
                >
                  Delete
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={busy}
                  onClick={() => setConfirmingDelete(false)}
                >
                  Cancel
                </button>
              </span>
            ) : (
              <button
                type="button"
                className="btn btn-danger-ghost"
                disabled={busy || connecting}
                title={
                  connecting ? "Wait for the connection attempt to finish" : ""
                }
                onClick={() => setConfirmingDelete(true)}
              >
                Delete
              </button>
            )}
          </div>
        )}
      </div>
      <span className="editor-kbd-hint">
        Enter to connect · Esc to {isNew ? "cancel" : "reset"}
      </span>
    </form>
  );
}
