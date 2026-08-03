import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  profilesDelete,
  profilesSave,
  secretDelete,
  secretExists,
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

/** Auth kinds the form can create and edit. Everything except Kerberos. */
type EditableAuthKind =
  | "plaintext"
  | "sasl_plain"
  | "sasl_scram"
  | "tls"
  | "aws_msk_iam"
  | "oauth_bearer";

/** Every control validation can point at. */
type FieldKey =
  | "name"
  | "bootstrap"
  | "username"
  | "password"
  | "caPath"
  | "clientCert"
  | "clientKey"
  | "region"
  | "awsProfile"
  | "tokenEndpoint"
  | "clientId"
  | "clientSecret"
  | "srUrl"
  | "srUsername"
  | "srPassword";

interface FieldError {
  field: FieldKey;
  message: string;
}

/** Why Kerberos is disabled. Never a dead option. */
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
  /** mTLS: path to the CA .pem on this machine. */
  caPath: string;
  /** mTLS: path to the client certificate .pem on this machine. */
  clientCert: string;
  /**
   * mTLS: the private key's PEM CONTENT, pasted — not a path. NEVER stored in
   * the profile; sent to secret_set exactly like a password.
   */
  clientKey: string;
  /** MSK IAM: the AWS region the cluster runs in. */
  region: string;
  /** MSK IAM: a named profile from ~/.aws, or empty for the default chain. */
  awsProfile: string;
  /** OAuth: the URL the identity provider issues tokens at. */
  tokenEndpoint: string;
  /** OAuth: the client id the identity provider issued. */
  clientId: string;
  /** OAuth: raw client secret. NEVER stored in the profile. */
  clientSecret: string;
  /** Schema Registry base URL. Empty = this cluster has no registry. */
  srUrl: string;
  /** Schema Registry basic-auth user, if the registry wants one. */
  srUsername: string;
  /** Schema Registry password. NEVER stored in the profile; keychain only. */
  srPassword: string;
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
    caPath: "",
    clientCert: "",
    clientKey: "",
    region: "",
    awsProfile: "",
    tokenEndpoint: "",
    clientId: "",
    clientSecret: "",
    srUrl: "",
    srUsername: "",
    srPassword: "",
    readOnly: false,
  };
  if (!profile) return base;
  base.name = profile.name;
  base.environment = profile.environment;
  base.bootstrap = profile.bootstrap_servers.join("\n");
  base.readOnly = profile.read_only;
  // Absent on every profile written before Phase 1 — serde defaults it to
  // None, so `?.` here is the same statement the Rust side makes.
  base.srUrl = profile.schema_registry?.url ?? "";
  base.srUsername = profile.schema_registry?.username ?? "";
  // The registry password lives in the keychain and is never read back into
  // the form: blank means "leave the stored one alone", like every other.
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
  } else if (auth.kind === "tls") {
    base.authKind = "tls";
    base.caPath = auth.ca_pem_path ?? "";
    base.clientCert = auth.client_cert_pem_path ?? "";
    // The key itself lives in the keychain and is never read back into the
    // form: blank means "leave the stored one alone".
  } else if (auth.kind === "aws_msk_iam") {
    base.authKind = "aws_msk_iam";
    base.region = auth.region;
    base.awsProfile = auth.profile ?? "";
  } else if (auth.kind === "oauth_bearer") {
    base.authKind = "oauth_bearer";
    base.tokenEndpoint = auth.token_endpoint;
    base.clientId = auth.client_id;
  }
  // Kerberos is not editable; it falls back to plaintext in the form and is
  // preserved verbatim on save (see `unsupportedAuth`).
  return base;
}

function parseBootstrap(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** Optional text field → the `null` the Rust side expects, never `""`. */
function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/** A token endpoint has to be a URL we can actually fetch. */
function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:";
  } catch {
    return false;
  }
}

/**
 * Which of this profile's secrets the keychain has actually been asked about
 * and confirmed. Never inferred from the sign-in method: a profile that
 * arrived through an import carries a SecretRef pointing at nothing on this
 * machine, and a profile's entry can be removed outside Kavka. Telling that
 * user their password is "unchanged" hides the problem until the handshake
 * fails, in the one form that exists to fix it.
 */
interface StoredSecrets {
  password: boolean;
  clientKey: boolean;
  clientSecret: boolean;
  srPassword: boolean;
}

/** Until the keychain answers, nothing is stored — so the form asks for it. */
const NOTHING_STORED: StoredSecrets = {
  password: false,
  clientKey: false,
  clientSecret: false,
  srPassword: false,
};

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

  // Kerberos is the one sign-in method the form can't edit: shown read-only
  // and preserved verbatim on save — never silently downgraded to plaintext.
  const unsupportedAuth =
    profile !== null && profile.auth.kind === "kerberos" ? profile.auth : null;

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

  /** Switching methods hides controls, so any message about one goes too. */
  const changeAuthKind = useCallback((authKind: EditableAuthKind) => {
    setForm((prev) => ({ ...prev, authKind }));
    setFieldError(null);
  }, []);

  // The keychain entries THIS profile points at, if any. Plain strings, so the
  // check below re-runs when the profile changes and not when React hands us a
  // new object for the same one.
  const savedAuth = profile?.auth;
  const passwordEntry =
    savedAuth &&
    (savedAuth.kind === "sasl_plain" || savedAuth.kind === "sasl_scram")
      ? savedAuth.password.entry
      : null;
  const clientKeyEntry =
    savedAuth && savedAuth.kind === "tls"
      ? (savedAuth.client_key?.entry ?? null)
      : null;
  const clientSecretEntry =
    savedAuth && savedAuth.kind === "oauth_bearer"
      ? savedAuth.client_secret.entry
      : null;
  // Independent of the sign-in method: a cluster can want mTLS and a registry
  // behind basic auth, and neither knows about the other.
  const srPasswordEntry = profile?.schema_registry?.password?.entry ?? null;

  // Secrets this profile already has in the keychain, which a blank input
  // therefore means "leave alone" rather than "clear". Asked, never assumed.
  const [stored, setStored] = useState<StoredSecrets>(NOTHING_STORED);

  useEffect(() => {
    let cancelled = false;
    // Reset first: while the answer is in flight — and if it never comes —
    // every secret counts as absent, so the form asks for input instead of
    // promising to keep something nobody has verified is there.
    setStored(NOTHING_STORED);
    const check = (entry: string | null): Promise<boolean> =>
      entry === null
        ? Promise.resolve(false)
        : // A keychain that can't answer is not a keychain that said yes.
          secretExists(entry).catch(() => false);
    void Promise.all([
      check(passwordEntry),
      check(clientKeyEntry),
      check(clientSecretEntry),
      check(srPasswordEntry),
    ]).then(([password, clientKey, clientSecret, srPassword]) => {
      if (!cancelled)
        setStored({ password, clientKey, clientSecret, srPassword });
    });
    return () => {
      cancelled = true;
    };
  }, [passwordEntry, clientKeyEntry, clientSecretEntry, srPasswordEntry]);

  const hasStoredPassword = stored.password;
  const hasStoredClientKey = stored.clientKey;
  const hasStoredClientSecret = stored.clientSecret;
  const hasStoredSrPassword = stored.srPassword;

  const needsPassword =
    (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") &&
    !hasStoredPassword;
  const needsClientSecret =
    form.authKind === "oauth_bearer" && !hasStoredClientSecret;

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

    // The registry is independent of the sign-in method, so it is checked
    // before the Kerberos early return below.
    const srUrl = form.srUrl.trim();
    if (srUrl.length > 0 && !isHttpUrl(srUrl))
      return {
        field: "srUrl",
        message:
          "Use the whole URL, starting with http:// or https:// — e.g. http://localhost:8081",
      };
    if (srUrl.length === 0 && form.srUsername.trim().length > 0)
      return {
        field: "srUrl",
        message:
          "Add the registry's address, or clear the username — a sign-in with nothing to sign in to can't be saved.",
      };

    // Kerberos is preserved as-is; there is nothing here to check.
    if (unsupportedAuth) return null;

    if (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") {
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

    if (form.authKind === "tls") {
      // The certificate and its key travel together: half a pair is a
      // connection that can only ever fail at the handshake.
      const certPath = form.clientCert.trim();
      const typedKey = form.clientKey.trim();
      if (certPath.length > 0 && typedKey.length === 0 && !hasStoredClientKey)
        return {
          field: "clientKey",
          message:
            "Paste the private key that goes with that certificate — Kavka needs both halves.",
        };
      if (certPath.length === 0 && typedKey.length > 0)
        return {
          field: "clientCert",
          message:
            "Add the path to the certificate this key belongs to — Kavka needs both halves.",
        };
    }

    if (form.authKind === "aws_msk_iam") {
      if (form.region.trim().length === 0)
        return {
          field: "region",
          message: "Name the region the cluster runs in — e.g. eu-west-1",
        };
    }

    if (form.authKind === "oauth_bearer") {
      if (form.tokenEndpoint.trim().length === 0)
        return {
          field: "tokenEndpoint",
          message:
            "Add the URL your identity provider issues tokens at — e.g. https://login.example.com/oauth2/token",
        };
      if (!isHttpUrl(form.tokenEndpoint.trim()))
        return {
          field: "tokenEndpoint",
          message:
            "Use the whole URL, starting with https:// — e.g. https://login.example.com/oauth2/token",
        };
      if (form.clientId.trim().length === 0)
        return {
          field: "clientId",
          message:
            "Add the client id your identity provider issued for this application.",
        };
      if (needsClientSecret && form.clientSecret.length === 0)
        return {
          field: "clientSecret",
          message: "This sign-in method needs the secret that goes with that client id.",
        };
    }

    return null;
  }, [
    form,
    needsPassword,
    needsClientSecret,
    hasStoredClientKey,
    unsupportedAuth,
  ]);

  /**
   * Persist the profile. Anything secret is written to the OS keychain first;
   * the profile itself only ever carries a SecretRef. Returns the saved
   * profile, or null on validation/IPC failure.
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
    // THE KEYCHAIN VOCABULARY. Every suffix here must also be in
    // `kavka_core::secrets::SECRET_SUFFIXES`, which is what `profiles_delete`
    // purges: an entry this editor writes and that list doesn't know about is
    // a secret that outlives the connection it belongs to. That is exactly
    // what happened to `sr_password`. Adding one here is a two-file change.
    const entry = {
      password: `${id}/password`,
      clientKey: `${id}/client_key`,
      clientSecret: `${id}/client_secret`,
      srPassword: `${id}/sr_password`,
    };

    // Cleared certificate path = the client certificate is being removed, so
    // the stored key goes with it. Stated in the hint under the key field.
    const certPath = orNull(form.clientCert);
    const typedKey = form.clientKey.trim();
    const keepsClientKey =
      certPath !== null && (typedKey.length > 0 || hasStoredClientKey);

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
            password: { entry: entry.password },
            tls: form.tls,
          };
          break;
        case "sasl_scram":
          auth = {
            kind: "sasl_scram",
            mechanism: form.mechanism,
            username: form.username.trim(),
            password: { entry: entry.password },
            tls: form.tls,
          };
          break;
        case "tls":
          auth = {
            kind: "tls",
            ca_pem_path: orNull(form.caPath),
            client_cert_pem_path: certPath,
            client_key: keepsClientKey ? { entry: entry.clientKey } : null,
          };
          break;
        case "aws_msk_iam":
          auth = {
            kind: "aws_msk_iam",
            region: form.region.trim(),
            profile: orNull(form.awsProfile),
          };
          break;
        case "oauth_bearer":
          auth = {
            kind: "oauth_bearer",
            token_endpoint: form.tokenEndpoint.trim(),
            client_id: form.clientId.trim(),
            client_secret: { entry: entry.clientSecret },
          };
          break;
      }
    }

    // The registry, if there is one. `null` — not an empty object — when the
    // URL is blank, so clearing the field genuinely removes it rather than
    // leaving a registry pointing at "".
    const srUrl = orNull(form.srUrl);
    const srTypedPassword = form.srPassword.length > 0;
    const keepsSrPassword =
      srUrl !== null && (srTypedPassword || hasStoredSrPassword);
    const schemaRegistry =
      srUrl === null
        ? null
        : {
            url: srUrl,
            username: orNull(form.srUsername),
            password: keepsSrPassword ? { entry: entry.srPassword } : null,
          };

    const next: ConnectionProfile = {
      id,
      name: form.name.trim(),
      environment: form.environment,
      bootstrap_servers: parseBootstrap(form.bootstrap),
      auth,
      read_only: form.readOnly,
      schema_registry: schemaRegistry,
    };

    // What this save puts into the keychain. A blank secret input always
    // means "keep whatever is stored" — never "clear it".
    const writes: Array<[string, string]> = [];
    if (!unsupportedAuth) {
      if (
        (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") &&
        form.password.length > 0
      )
        writes.push([entry.password, form.password]);
      if (form.authKind === "tls" && certPath !== null && typedKey.length > 0)
        writes.push([entry.clientKey, typedKey]);
      if (form.authKind === "oauth_bearer" && form.clientSecret.length > 0)
        writes.push([entry.clientSecret, form.clientSecret]);
    }
    // Outside the auth switch: the registry's password has nothing to do with
    // how the cluster checks who you are.
    if (srUrl !== null && srTypedPassword)
      writes.push([entry.srPassword, form.srPassword]);

    // Entries this profile still has but the new sign-in method no longer
    // references. Cleaned up best-effort after the profile is safely saved.
    const obsolete: string[] = [];
    if (!unsupportedAuth) {
      if (
        hasStoredPassword &&
        form.authKind !== "sasl_plain" &&
        form.authKind !== "sasl_scram"
      )
        obsolete.push(entry.password);
      if (hasStoredClientKey && !(form.authKind === "tls" && keepsClientKey))
        obsolete.push(entry.clientKey);
      if (hasStoredClientSecret && form.authKind !== "oauth_bearer")
        obsolete.push(entry.clientSecret);
    }
    // Removing the registry (or clearing its URL) takes its password with it —
    // the same rule the client key follows when its certificate path goes.
    if (hasStoredSrPassword && !keepsSrPassword)
      obsolete.push(entry.srPassword);

    setBusy(true);
    try {
      for (const [name, value] of writes) {
        await secretSet(name, value);
      }
      try {
        await profilesSave(next);
      } catch (err) {
        // Don't leave fresh secrets behind for a profile that never existed.
        if (isNew) for (const [name] of writes) void secretDelete(name);
        throw err;
      }
      // Best-effort cleanup; an orphaned entry is harmless.
      for (const name of obsolete) void secretDelete(name);
      // The inputs are cleared, so what is stored has to be recorded here or
      // the next Save asks for a secret this one just wrote. Written entries
      // are known present; obsolete ones count as gone even though the delete
      // is best-effort — erring towards "ask again" is the safe direction.
      const written = new Set(writes.map(([name]) => name));
      const removed = new Set(obsolete);
      const settled = (name: string, was: boolean) =>
        written.has(name) || (was && !removed.has(name));
      setStored((prev) => ({
        password: settled(entry.password, prev.password),
        clientKey: settled(entry.clientKey, prev.clientKey),
        clientSecret: settled(entry.clientSecret, prev.clientSecret),
        srPassword: settled(entry.srPassword, prev.srPassword),
      }));
      setForm((prev) => ({
        ...prev,
        password: "",
        clientKey: "",
        clientSecret: "",
        srPassword: "",
      }));
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
    hasStoredClientKey,
    hasStoredClientSecret,
    hasStoredSrPassword,
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
  const showMtls = form.authKind === "tls";
  const showAws = form.authKind === "aws_msk_iam";
  const showOauth = form.authKind === "oauth_bearer";
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
            This connection signs in with Kerberos (
            <code>{unsupportedAuth.service_name}</code> as{" "}
            <code>{unsupportedAuth.principal}</code>), which Kavka can't set up
            yet. Saving keeps it exactly as it is; every other field here still
            works.
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
                changeAuthKind(e.target.value as EditableAuthKind)
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
              <option value="tls">
                A certificate this machine presents — mTLS
              </option>
              <option value="aws_msk_iam">
                The AWS credentials on this machine — MSK IAM
              </option>
              <option value="oauth_bearer">
                A token from your identity provider — OAuth 2.0 / OIDC
              </option>
              {/* Every disabled control says why. No dead ends. */}
              <option value="kerberos" disabled title={NOT_YET}>
                A Kerberos ticket — GSSAPI (not yet)
              </option>
            </select>
            <span className="field-hint">
              Managed Kafka usually wants SASL/SCRAM with TLS on. A local
              broker usually wants nothing at all. Kerberos is the one method
              Kavka can't set up yet.
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

        {!unsupportedAuth && showMtls && (
          <>
            <span className="field-hint">
              Kavka reads PEM files exactly as they are — there is no JKS or
              PKCS#12 keystore to convert first.
            </span>

            <div className="field">
              <label className="field-label" htmlFor="pe-ca-path">
                CA certificate
              </label>
              <input
                id="pe-ca-path"
                ref={bind("caPath")}
                type="text"
                className={cls("caPath", "input-mono")}
                value={form.caPath}
                placeholder="/etc/kafka/ca.pem"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("caPath")}
                aria-describedby={describe("caPath", "pe-ca-path-hint")}
                onChange={(e) => edit("caPath", { caPath: e.target.value })}
              />
              {fieldMessage("caPath")}
              <span className="field-hint" id="pe-ca-path-hint">
                Path to the CA .pem — leave empty to use the system trust
                store.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-cert">
                Client certificate
              </label>
              <input
                id="pe-client-cert"
                ref={bind("clientCert")}
                type="text"
                className={cls("clientCert", "input-mono")}
                value={form.clientCert}
                placeholder="/etc/kafka/client.pem"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("clientCert")}
                aria-describedby={describe(
                  "clientCert",
                  "pe-client-cert-hint",
                )}
                onChange={(e) =>
                  edit("clientCert", { clientCert: e.target.value })
                }
              />
              {fieldMessage("clientCert")}
              <span className="field-hint" id="pe-client-cert-hint">
                Path to the certificate this machine shows the broker — leave
                empty if the broker doesn't ask for one.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-key">
                Client private key
              </label>
              <textarea
                id="pe-client-key"
                ref={bind("clientKey")}
                rows={5}
                className={cls("clientKey")}
                value={form.clientKey}
                placeholder={
                  hasStoredClientKey
                    ? "••••••••  (unchanged)"
                    : "-----BEGIN PRIVATE KEY-----\n…"
                }
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("clientKey")}
                aria-describedby={describe("clientKey", "pe-client-key-hint")}
                onChange={(e) =>
                  edit("clientKey", { clientKey: e.target.value })
                }
              />
              {fieldMessage("clientKey")}
              <span className="field-hint" id="pe-client-key-hint">
                Paste the key itself, not a path to it. It goes to your
                operating system's keychain — never into the connection file,
                and never off this machine.
                {hasStoredClientKey
                  ? " Leave it empty to keep the stored key; clearing the certificate path above removes it."
                  : ""}
              </span>
            </div>
          </>
        )}

        {!unsupportedAuth && showAws && (
          <>
            <span className="field-hint">
              Kavka signs each request with the AWS credentials already on this
              machine. The bootstrap servers above have to be this cluster's
              IAM endpoint — the <code>.amazonaws.com</code> hosts from the MSK
              console, usually on port 9098.
            </span>

            <div className="field">
              <label className="field-label" htmlFor="pe-region">
                Region
              </label>
              <input
                id="pe-region"
                ref={bind("region")}
                type="text"
                className={cls("region", "input-mono")}
                value={form.region}
                placeholder="eu-west-1"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("region")}
                aria-describedby={describe("region", "pe-region-hint")}
                onChange={(e) => edit("region", { region: e.target.value })}
              />
              {fieldMessage("region")}
              <span className="field-hint" id="pe-region-hint">
                The AWS region the cluster runs in. It has to match the
                bootstrap hosts, or the signature won't be accepted.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-aws-profile">
                AWS profile name
              </label>
              <input
                id="pe-aws-profile"
                ref={bind("awsProfile")}
                type="text"
                className={cls("awsProfile", "input-mono")}
                value={form.awsProfile}
                placeholder="default"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("awsProfile")}
                aria-describedby={describe("awsProfile", "pe-aws-profile-hint")}
                onChange={(e) =>
                  edit("awsProfile", { awsProfile: e.target.value })
                }
              />
              {fieldMessage("awsProfile")}
              <span className="field-hint" id="pe-aws-profile-hint">
                A named profile from <code>~/.aws/config</code>. Leave empty to
                use the default credential chain — environment variables, then{" "}
                <code>~/.aws</code>, then SSO.
              </span>
            </div>
          </>
        )}

        {!unsupportedAuth && showOauth && (
          <>
            <span className="field-hint">
              Kavka asks your identity provider for a token with the client
              credentials grant, then presents it to the broker as
              SASL/OAUTHBEARER.
            </span>

            <div className="field">
              <label className="field-label" htmlFor="pe-token-endpoint">
                Token endpoint
              </label>
              <input
                id="pe-token-endpoint"
                ref={bind("tokenEndpoint")}
                type="text"
                className={cls("tokenEndpoint", "input-mono")}
                value={form.tokenEndpoint}
                placeholder="https://login.example.com/oauth2/token"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("tokenEndpoint")}
                aria-describedby={describe(
                  "tokenEndpoint",
                  "pe-token-endpoint-hint",
                )}
                onChange={(e) =>
                  edit("tokenEndpoint", { tokenEndpoint: e.target.value })
                }
              />
              {fieldMessage("tokenEndpoint")}
              <span className="field-hint" id="pe-token-endpoint-hint">
                The URL that issues the token, not the sign-in page a browser
                would use.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-id">
                Client id
              </label>
              <input
                id="pe-client-id"
                ref={bind("clientId")}
                type="text"
                className={cls("clientId", "input-mono")}
                value={form.clientId}
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("clientId")}
                aria-describedby={describe("clientId", "pe-client-id-hint")}
                onChange={(e) => edit("clientId", { clientId: e.target.value })}
              />
              {fieldMessage("clientId")}
              <span className="field-hint" id="pe-client-id-hint">
                The application your identity provider registered for Kafka —
                not your own user account.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-secret">
                Client secret
              </label>
              <input
                id="pe-client-secret"
                ref={bind("clientSecret")}
                type="password"
                className={cls("clientSecret")}
                value={form.clientSecret}
                autoComplete="new-password"
                placeholder={
                  hasStoredClientSecret
                    ? "••••••••  (unchanged)"
                    : "Client secret"
                }
                aria-invalid={invalid("clientSecret")}
                aria-describedby={describe(
                  "clientSecret",
                  "pe-client-secret-hint",
                )}
                onChange={(e) =>
                  edit("clientSecret", { clientSecret: e.target.value })
                }
              />
              {fieldMessage("clientSecret")}
              <span className="field-hint" id="pe-client-secret-hint">
                Goes to your operating system's keychain — never into the
                connection file, and never off this machine.
              </span>
            </div>
          </>
        )}
      </fieldset>

      {/* Schema Registry — optional, and independent of how the cluster checks
          who you are: a broker on mTLS can sit in front of a registry behind
          basic auth. Leaving the address empty is the same as having no
          registry, and clearing it removes the stored password with it. */}
      <fieldset className="fieldset">
        <legend className="eyebrow">Schema Registry (optional)</legend>

        <span className="field-hint">
          If this cluster's messages are Avro, Protobuf or JSON Schema, Kavka
          reads the schema from here to decode them — and shows the subject,
          version and id beside each message. Without it those payloads are
          shown as raw bytes.
        </span>

        <div className="field">
          <label className="field-label" htmlFor="pe-sr-url">
            Registry address
          </label>
          <input
            id="pe-sr-url"
            ref={bind("srUrl")}
            type="text"
            className={cls("srUrl", "input-mono")}
            value={form.srUrl}
            placeholder="http://localhost:8081"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("srUrl")}
            aria-describedby={describe("srUrl", "pe-sr-url-hint")}
            onChange={(e) => edit("srUrl", { srUrl: e.target.value })}
          />
          {fieldMessage("srUrl")}
          <span className="field-hint" id="pe-sr-url-hint">
            The whole URL, including the scheme. Confluent, Apicurio and Glue
            all speak the same read API here. Leave it empty if this cluster
            has no registry.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-sr-username">
            Registry username
          </label>
          <input
            id="pe-sr-username"
            ref={bind("srUsername")}
            type="text"
            className={cls("srUsername", "input-mono")}
            value={form.srUsername}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("srUsername")}
            aria-describedby={describe("srUsername", "pe-sr-username-hint")}
            onChange={(e) => edit("srUsername", { srUsername: e.target.value })}
          />
          {fieldMessage("srUsername")}
          <span className="field-hint" id="pe-sr-username-hint">
            Only if the registry asks for one. Managed registries usually do;
            a registry inside your own network usually doesn't.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-sr-password">
            Registry password
          </label>
          <input
            id="pe-sr-password"
            ref={bind("srPassword")}
            type="password"
            className={cls("srPassword")}
            value={form.srPassword}
            autoComplete="new-password"
            placeholder={
              hasStoredSrPassword ? "••••••••  (unchanged)" : "Password"
            }
            aria-invalid={invalid("srPassword")}
            aria-describedby={describe("srPassword", "pe-sr-password-hint")}
            onChange={(e) => edit("srPassword", { srPassword: e.target.value })}
          />
          {fieldMessage("srPassword")}
          <span className="field-hint" id="pe-sr-password-hint">
            Goes to your operating system's keychain — never into the
            connection file, and never off this machine.
            {hasStoredSrPassword
              ? " Leave it empty to keep the stored one; clearing the address above removes it."
              : ""}
          </span>
        </div>
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
