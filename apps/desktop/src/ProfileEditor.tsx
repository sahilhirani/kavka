import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  profilesDelete,
  profilesSave,
  secretDelete,
  secretExists,
  secretSet,
  HISTORY_RETENTION_DAYS,
  SAMPLER_DEFAULT_MS,
  SAMPLER_MIN_MS,
  type AuthConfig,
  type ConnectClusterConfig,
  type ConnectionProfile,
  type ConnStatus,
  type Environment,
  type ScramMechanism,
} from "./api";
import EnvironmentsManager from "./EnvironmentsManager";
import {
  envAttrs,
  isKnownEnvironment,
  resolveEnvironment,
  sameEnvironmentName,
  useEnvironments,
} from "./environments";
import { classifyError } from "./errors";
import { Term } from "./Glossary";
import { useI18n, type TFunction } from "./i18n";
import WasmSerdesFields from "./WasmSerdesFields";

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
  const { t } = useI18n();
  // `classifyError` is `errors.ts`, outside this wave's extraction boundary:
  // the error library is still English in every locale. Said out loud in
  // docs/I18N.md rather than papered over.
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
          <summary>{t("common.showDetails")}</summary>
          <pre className="banner-raw">{raw}</pre>
        </details>
      </div>
      {onDismiss && (
        <div className="banner-actions">
          <button type="button" className="btn btn-ghost" onClick={onDismiss}>
            {t("common.dismiss")}
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * The environment picker's options come from the registry now, not from a
 * literal — which is the whole point of the change. `ENVIRONMENTS` used to
 * live here as `["dev", "staging", "prod"] as const`, and its removal is what
 * lets an org with dev/QA/UAT/production describe itself.
 */

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
  | "srPassword"
  | "connectName"
  | "connectUrl"
  | "metricsUrl"
  | "metricsUsername"
  | "metricsPassword"
  | "samplerSeconds";

interface FieldError {
  field: FieldKey;
  message: string;
  /**
   * Which Connect row the message belongs to. The Connect list is the only
   * repeated control group in this form, so the field key alone stops being an
   * address — and "validation focuses the offending control" (§5.3) needs an
   * address, not a category.
   */
  row?: number;
}

/**
 * How long a span reads, in the active language.
 *
 * The thresholds are `formatSpan` in `monitoring.ts` — change both or neither.
 * It is duplicated rather than imported because that function hard-codes
 * English pluralisation ("1 second" / "2 seconds"), and the sampler sentence
 * is the one place in this form where a raw English fragment would land in
 * the middle of a translated paragraph. Four catalog keys and four branches
 * is a smaller price than that.
 */
function spanText(t: TFunction, ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return t("unit.seconds", { count: 0 });
  if (ms < 60_000)
    return t("unit.seconds", { count: Math.max(1, Math.round(ms / 1000)) });
  if (ms < 3_600_000)
    return t("unit.minutes", { count: Math.round(ms / 60_000) });
  if (ms < 86_400_000)
    return t("unit.hours", { count: Math.round(ms / 3_600_000) });
  return t("unit.days", { count: Math.round(ms / 86_400_000) });
}

/**
 * One row of the Connect list, while it is being edited.
 *
 * `entry` is the keychain entry this cluster ALREADY points at, and it is
 * carried through a save rather than recomputed: the core names a Connect
 * password after the cluster (`{profileId}/connect_password/{cluster}`) and
 * purges by reading the stored refs, so reusing the entry is what makes
 * renaming a cluster keep its password instead of silently orphaning it.
 */
interface ConnectRow {
  /** Stable across reorders so React never reuses one row's input for another. */
  key: number;
  name: string;
  url: string;
  username: string;
  /** Raw password input. NEVER stored in the profile; sent to secret_set only. */
  password: string;
  entry: string | null;
}

let nextConnectKey = 1;

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
  /** The Kafka Connect clusters this connection can drive. Empty is normal. */
  connect: ConnectRow[];
  /** Prometheus-format metrics address. Empty = this cluster has no endpoint. */
  metricsUrl: string;
  /** Basic-auth user, if the exporter (or the Prometheus in front of it) wants one. */
  metricsUsername: string;
  /** Metrics password. NEVER stored in the profile; keychain only. */
  metricsPassword: string;
  /**
   * The lag sampler's interval, in SECONDS — the unit the field is in, not the
   * unit the wire is in. Kept as text so a half-typed value doesn't snap back
   * to a number under the caret. Empty means "the default".
   */
  samplerSeconds: string;
  readOnly: boolean;
}

/**
 * `fallbackEnvironment` is the environment a NEW connection starts in: the
 * first one the registry lists, because the registry's order is the user's
 * order and its first entry is the least dangerous place to land. It is a
 * parameter rather than the literal `"dev"` this used to hold — a machine
 * whose environments are `QA`, `UAT` and `Production` has no `dev`, and a form
 * that starts on a name nothing defines would open showing the unknown-
 * environment hint.
 */
function initialForm(
  profile: ConnectionProfile | null,
  fallbackEnvironment: string,
): FormState {
  const base: FormState = {
    name: "",
    environment: fallbackEnvironment,
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
    connect: [],
    metricsUrl: "",
    metricsUsername: "",
    metricsPassword: "",
    samplerSeconds: String(SAMPLER_DEFAULT_MS / 1000),
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
  // Absent on every profile written before Phase 3a; the Rust side defaults it
  // to an empty list, so missing and empty mean the same thing here too.
  base.connect = (profile.connect_clusters ?? []).map((cluster) => ({
    key: nextConnectKey++,
    name: cluster.name,
    url: cluster.url,
    username: cluster.username ?? "",
    // Like every other secret: never read back into the form. Blank means
    // "leave the stored one alone".
    password: "",
    entry: cluster.password?.entry ?? null,
  }));
  // Absent on every profile written before Phase 4 — serde-defaulted to None,
  // so `?.` here is the same statement the Rust side makes. The password is
  // never read back into the form, like every other secret.
  base.metricsUrl = profile.metrics_endpoint?.url ?? "";
  base.metricsUsername = profile.metrics_endpoint?.username ?? "";
  // Missing means "the core's default", and the field shows that default rather
  // than an empty box — an interval nobody can see is an interval nobody knows
  // they can change.
  base.samplerSeconds = String(
    (profile.sampler_interval_ms ?? SAMPLER_DEFAULT_MS) / 1000,
  );
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
  metricsPassword: boolean;
}

/** Until the keychain answers, nothing is stored — so the form asks for it. */
const NOTHING_STORED: StoredSecrets = {
  password: false,
  clientKey: false,
  clientSecret: false,
  srPassword: false,
  metricsPassword: false,
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
  /**
   * Something other than this form changed the stored connections — today,
   * only the environment manager's reassign-then-delete flow, which rewrites
   * every profile that named the environment being removed. The editor has no
   * business reloading the sidebar itself, so it reports up.
   */
  onProfilesChanged: () => void;
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
  onProfilesChanged,
}: ProfileEditorProps) {
  const { t, tx } = useI18n();
  const isNew = profile === null;
  const envDefs = useEnvironments();
  // Read once at mount, deliberately NOT reactive: if the registry loads a
  // moment after the form does, a new connection's environment must not slide
  // out from under a user who has already picked one. The registry landing
  // late only ever affects a form opened before it — and that form's initial
  // value is the neutral first entry either way.
  const fallbackEnv = useRef(envDefs[0]?.name ?? "dev").current;
  const [form, setForm] = useState<FormState>(() =>
    initialForm(profile, fallbackEnv),
  );
  const [busy, setBusy] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  /** The "Manage environments…" dialog. One at a time, like every overlay. */
  const [managingEnvs, setManagingEnvs] = useState(false);
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
  // The Connect list is repeated, so its controls are addressed by row as well
  // as by field. Same cached-callback rule as `bind`, keyed by both.
  const connectControls = useRef<Record<string, HTMLElement | null>>({});
  const connectBind = useMemo(() => {
    const cache: Record<string, (el: HTMLElement | null) => void> = {};
    return (row: number, which: "name" | "url") => {
      const key = `${row}:${which}`;
      return (cache[key] ??= (el: HTMLElement | null) => {
        connectControls.current[key] = el;
      });
    };
  }, []);

  // Kerberos is the one sign-in method the form can't edit: shown read-only
  // and preserved verbatim on save — never silently downgraded to plaintext.
  const unsupportedAuth =
    profile !== null && profile.auth.kind === "kerberos" ? profile.auth : null;

  const patch = useCallback((partial: Partial<FormState>) => {
    setForm((prev) => ({ ...prev, ...partial }));
  }, []);

  /**
   * The environment picker's keyboard model.
   *
   * `role="radiogroup"` is a promise, and it was one nothing kept: three
   * buttons all in the tab order and arrow keys that did nothing. A radio
   * group is ONE tab stop whose members are walked with the arrows, and
   * selection follows focus — so the roving `tabIndex` below and this
   * handler are two halves of the same fix (SC 2.1.1, SC 4.1.2).
   *
   * Keyed by name rather than by a closed union now, because the members are
   * whatever the user defined. The keys are the registry's names verbatim, so
   * two environments differing only in case cannot collide here — the store
   * refuses to hold both.
   */
  const envRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const envOptions = useMemo(() => {
    // A profile can name an environment the registry no longer holds — deleted
    // in another window, or written by a colleague's export. It gets a segment
    // of its own at the end so the picker still shows the current value as
    // selected, rather than silently reading as "none of these".
    if (form.environment === "" || isKnownEnvironment(form.environment, envDefs))
      return envDefs;
    return [...envDefs, resolveEnvironment(form.environment, envDefs)];
  }, [envDefs, form.environment]);
  /** The definition behind whatever the form currently says. */
  const formEnv = resolveEnvironment(form.environment, envDefs);
  /**
   * True when the profile names something the registry does not hold. Renders
   * slate and unprotected with a hint pointing at the manager — never an
   * error, because the profile is not wrong: the registry is just incomplete
   * on THIS machine, which is the normal state after importing a colleague's
   * connections.
   */
  const envUnknown =
    form.environment !== "" && !isKnownEnvironment(form.environment, envDefs);
  /**
   * Which segment carries the group's single tab stop. The checked one — or
   * the first, when nothing is checked, so the radiogroup never falls out of
   * the tab order entirely.
   */
  const rovingIndex = Math.max(
    envOptions.findIndex((d) => sameEnvironmentName(d.name, form.environment)),
    0,
  );
  const onEnvKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
      let next: number | null = null;
      if (e.key === "ArrowRight" || e.key === "ArrowDown")
        next = (index + 1) % envOptions.length;
      else if (e.key === "ArrowLeft" || e.key === "ArrowUp")
        next = (index - 1 + envOptions.length) % envOptions.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = envOptions.length - 1;
      if (next === null) return;
      e.preventDefault();
      const target = envOptions[next].name;
      patch({ environment: target });
      envRefs.current[target]?.focus();
    },
    [patch, envOptions],
  );

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

  /** Editing any field of a Connect row clears that row's message. */
  const editConnect = useCallback(
    (row: number, partial: Partial<ConnectRow>) => {
      setForm((prev) => ({
        ...prev,
        connect: prev.connect.map((entry, i) =>
          i === row ? { ...entry, ...partial } : entry,
        ),
      }));
      setFieldError((prev) => (prev?.row === row ? null : prev));
    },
    [],
  );

  const addConnect = useCallback(() => {
    setForm((prev) => ({
      ...prev,
      connect: [
        ...prev.connect,
        {
          key: nextConnectKey++,
          name: "",
          url: "",
          username: "",
          password: "",
          entry: null,
        },
      ],
    }));
  }, []);

  /** Removing a row renumbers the rest, so any row-addressed message goes. */
  const removeConnect = useCallback((row: number) => {
    setForm((prev) => ({
      ...prev,
      connect: prev.connect.filter((_, i) => i !== row),
    }));
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
  // Independent of everything above it, for the same reason: the exporter is a
  // separate HTTP service that has never heard of the brokers' auth.
  const metricsPasswordEntry =
    profile?.metrics_endpoint?.password?.entry ?? null;

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
      check(metricsPasswordEntry),
    ]).then(
      ([password, clientKey, clientSecret, srPassword, metricsPassword]) => {
        if (!cancelled)
          setStored({
            password,
            clientKey,
            clientSecret,
            srPassword,
            metricsPassword,
          });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [
    passwordEntry,
    clientKeyEntry,
    clientSecretEntry,
    srPasswordEntry,
    metricsPasswordEntry,
  ]);

  const hasStoredPassword = stored.password;
  const hasStoredClientKey = stored.clientKey;
  const hasStoredClientSecret = stored.clientSecret;
  const hasStoredSrPassword = stored.srPassword;
  const hasStoredMetricsPassword = stored.metricsPassword;

  // The Connect passwords, asked about the same way and for the same reason —
  // except there are N of them, so the answer is a map keyed by entry name
  // rather than four booleans. The effect keys on the JOINED entry list: it
  // has to re-run when the set of entries changes and not when React hands us
  // a new array for the same set.
  // JSON, not a joined string: an entry name ends in the cluster's own name,
  // which is user text, so there is no separator character that is safe to
  // split back on. The key is a value to compare, and the array is recovered
  // by parsing it.
  const connectEntryKey = JSON.stringify(
    (profile?.connect_clusters ?? [])
      .map((cluster) => cluster.password?.entry ?? "")
      .filter((entry) => entry.length > 0),
  );
  const [connectStored, setConnectStored] = useState<Record<string, boolean>>(
    {},
  );

  useEffect(() => {
    let cancelled = false;
    // Reset first, for the same reason as the four above: while the answer is
    // in flight nothing counts as stored, so the form asks rather than
    // promising to keep something nobody has verified is there.
    setConnectStored({});
    const entries = JSON.parse(connectEntryKey) as string[];
    if (entries.length === 0) return;
    void Promise.all(
      entries.map((entry) => secretExists(entry).catch(() => false)),
    ).then((results) => {
      if (cancelled) return;
      const map: Record<string, boolean> = {};
      entries.forEach((entry, i) => {
        map[entry] = results[i];
      });
      setConnectStored(map);
    });
    return () => {
      cancelled = true;
    };
  }, [connectEntryKey]);

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
          t("editor.err.name"),
      };
    if (parseBootstrap(form.bootstrap).length === 0)
      return {
        field: "bootstrap",
        message: t("editor.err.bootstrap"),
      };

    // The registry is independent of the sign-in method, so it is checked
    // before the Kerberos early return below.
    const srUrl = form.srUrl.trim();
    if (srUrl.length > 0 && !isHttpUrl(srUrl))
      return {
        field: "srUrl",
        message:
          t("editor.err.srUrl"),
      };
    if (srUrl.length === 0 && form.srUsername.trim().length > 0)
      return {
        field: "srUrl",
        message:
          t("editor.err.srUserNoUrl"),
      };

    // The metrics endpoint is independent of everything else here too — it is a
    // plain HTTP address in front of JMX — so it is checked before the Kerberos
    // early return, like the registry and Connect.
    const metricsUrl = form.metricsUrl.trim();
    if (metricsUrl.length > 0 && !isHttpUrl(metricsUrl))
      return {
        field: "metricsUrl",
        message:
          t("editor.err.metricsUrl"),
      };
    if (metricsUrl.length === 0 && form.metricsUsername.trim().length > 0)
      return {
        field: "metricsUrl",
        message:
          t("editor.err.metricsUserNoUrl"),
      };

    // The sampler interval has a floor in the core, so the form refuses the
    // value here rather than letting the save come back with a refusal about a
    // field the user can no longer see.
    const seconds = Number(form.samplerSeconds.trim());
    if (
      form.samplerSeconds.trim().length === 0 ||
      !Number.isFinite(seconds) ||
      seconds * 1000 < SAMPLER_MIN_MS
    )
      return {
        field: "samplerSeconds",
        message: t("editor.err.sampler", {
          seconds: SAMPLER_MIN_MS / 1000,
        }),
      };

    // Connect is independent of the sign-in method too — the workers have their
    // own address and their own credentials — so it is checked here, before the
    // Kerberos early return.
    const seen = new Set<string>();
    for (let row = 0; row < form.connect.length; row += 1) {
      const cluster = form.connect[row];
      const clusterName = cluster.name.trim();
      const url = cluster.url.trim();
      // A row nobody has touched is dropped on save rather than refused: an
      // empty row is what "Add a Connect cluster" produces, and being told off
      // for the thing the button just did is absurd.
      if (
        clusterName.length === 0 &&
        url.length === 0 &&
        cluster.username.trim().length === 0 &&
        cluster.password.length === 0
      )
        continue;
      if (clusterName.length === 0)
        return {
          field: "connectName",
          row,
          message:
            t("editor.err.connectName"),
        };
      if (seen.has(clusterName))
        return {
          field: "connectName",
          row,
          message:
            t("editor.err.connectDuplicate"),
        };
      seen.add(clusterName);
      if (url.length === 0)
        return {
          field: "connectUrl",
          row,
          message:
            t("editor.err.connectUrlMissing"),
        };
      if (!isHttpUrl(url))
        return {
          field: "connectUrl",
          row,
          message:
            t("editor.err.connectUrl"),
        };
    }

    // Kerberos is preserved as-is; there is nothing here to check.
    if (unsupportedAuth) return null;

    if (form.authKind === "sasl_plain" || form.authKind === "sasl_scram") {
      if (form.username.trim().length === 0)
        return {
          field: "username",
          message: t("editor.err.username"),
        };
      if (needsPassword && form.password.length === 0)
        return {
          field: "password",
          message: t("editor.err.password"),
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
            t("editor.err.clientKey"),
        };
      if (certPath.length === 0 && typedKey.length > 0)
        return {
          field: "clientCert",
          message:
            t("editor.err.clientCert"),
        };
    }

    if (form.authKind === "aws_msk_iam") {
      if (form.region.trim().length === 0)
        return {
          field: "region",
          message: t("editor.err.region"),
        };
    }

    if (form.authKind === "oauth_bearer") {
      if (form.tokenEndpoint.trim().length === 0)
        return {
          field: "tokenEndpoint",
          message:
            t("editor.err.tokenEndpoint"),
        };
      if (!isHttpUrl(form.tokenEndpoint.trim()))
        return {
          field: "tokenEndpoint",
          message:
            t("editor.err.tokenEndpointUrl"),
        };
      if (form.clientId.trim().length === 0)
        return {
          field: "clientId",
          message:
            t("editor.err.clientId"),
        };
      if (needsClientSecret && form.clientSecret.length === 0)
        return {
          field: "clientSecret",
          message: t("editor.err.clientSecret"),
        };
    }

    return null;
    // `t` is memoized on the locale (see i18n/index.ts), so validation
    // re-derives when the language changes and on no other render.
  }, [
    form,
    needsPassword,
    needsClientSecret,
    hasStoredClientKey,
    unsupportedAuth,
    t,
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
      if (problem.row === undefined) controls.current[problem.field]?.focus();
      else
        connectControls.current[
          `${problem.row}:${problem.field === "connectName" ? "name" : "url"}`
        ]?.focus();
      return null;
    }
    setFieldError(null);
    const id = profile?.id ?? (draftIdRef.current ??= crypto.randomUUID());
    // THE KEYCHAIN VOCABULARY. Every suffix here must also be in
    // `kavka_core::secrets::SECRET_SUFFIXES`, which is what `profiles_delete`
    // purges: an entry this editor writes and that list doesn't know about is
    // a secret that outlives the connection it belongs to. That is exactly
    // what happened to `sr_password`. Adding one here is a two-file change.
    //
    // `metrics_password` IS THE PHASE 4 ADDITION, and the second file is not
    // this one: the core's constant has to grow it too, or a deleted connection
    // leaves its exporter credentials in the keychain forever. The Rust side
    // carries a test that fails when the two lists disagree — that failure is
    // the contract working, not a broken build.
    const entry = {
      password: `${id}/password`,
      clientKey: `${id}/client_key`,
      clientSecret: `${id}/client_secret`,
      srPassword: `${id}/sr_password`,
      metricsPassword: `${id}/metrics_password`,
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

    // The metrics endpoint, if there is one. Same rule as the registry: `null`
    // and not an empty object when the URL is blank, so clearing the field
    // genuinely removes it — and takes its stored password with it.
    const metricsUrl = orNull(form.metricsUrl);
    const metricsTypedPassword = form.metricsPassword.length > 0;
    const keepsMetricsPassword =
      metricsUrl !== null && (metricsTypedPassword || hasStoredMetricsPassword);
    const metricsEndpoint =
      metricsUrl === null
        ? null
        : {
            url: metricsUrl,
            username: orNull(form.metricsUsername),
            password: keepsMetricsPassword
              ? { entry: entry.metricsPassword }
              : null,
          };

    // Seconds in the form, milliseconds on the wire. validate() has already
    // refused anything below the core's floor.
    const samplerIntervalMs = Math.round(
      Number(form.samplerSeconds.trim()) * 1000,
    );

    // The Connect clusters. A row with no name or no URL is dropped rather
    // than saved half-written — validate() has already refused any row that
    // has one and not the other, so what falls out here is only the empty row
    // "Add a Connect cluster" leaves behind.
    //
    // THE ENTRY NAME IS REUSED WHEN THERE IS ONE. The core names a Connect
    // password after the cluster (`{id}/connect_password/{cluster}`) and purges
    // by reading the SecretRefs off the profile, never by rebuilding the name —
    // so carrying the stored entry through a rename keeps the password with the
    // cluster it belongs to. Only a cluster that has never had one gets a name
    // minted for it.
    const connectWrites: Array<[string, string]> = [];
    const connectEntryByKey = new Map<number, string>();
    const connectClusters: ConnectClusterConfig[] = form.connect
      .filter(
        (row) => row.name.trim().length > 0 && row.url.trim().length > 0,
      )
      .map((row) => {
        const clusterName = row.name.trim();
        const entryName = row.entry ?? `${id}/connect_password/${clusterName}`;
        const typed = row.password.length > 0;
        const keeps =
          typed || (row.entry !== null && connectStored[row.entry] === true);
        if (typed) connectWrites.push([entryName, row.password]);
        if (keeps) connectEntryByKey.set(row.key, entryName);
        return {
          name: clusterName,
          url: row.url.trim(),
          username: orNull(row.username),
          password: keeps ? { entry: entryName } : null,
        };
      });

    const next: ConnectionProfile = {
      id,
      name: form.name.trim(),
      environment: form.environment,
      bootstrap_servers: parseBootstrap(form.bootstrap),
      auth,
      read_only: form.readOnly,
      schema_registry: schemaRegistry,
      connect_clusters: connectClusters,
      metrics_endpoint: metricsEndpoint,
      sampler_interval_ms: samplerIntervalMs,
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
    // And again for the metrics endpoint, which knows nothing about either.
    if (metricsUrl !== null && metricsTypedPassword)
      writes.push([entry.metricsPassword, form.metricsPassword]);
    // Same again for every Connect cluster that had a password typed into it.
    writes.push(...connectWrites);

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
    // Same rule once more for the exporter's password.
    if (hasStoredMetricsPassword && !keepsMetricsPassword)
      obsolete.push(entry.metricsPassword);
    // A Connect cluster that was removed — or that lost its password — takes
    // its keychain entry with it. Read off the SAVED profile, because that is
    // the only record of what this connection used to reference.
    const keptConnectEntries = new Set(
      connectClusters
        .map((cluster) => cluster.password?.entry)
        .filter((name): name is string => typeof name === "string"),
    );
    for (const cluster of profile?.connect_clusters ?? []) {
      const name = cluster.password?.entry;
      if (typeof name === "string" && !keptConnectEntries.has(name))
        obsolete.push(name);
    }

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
        metricsPassword: settled(entry.metricsPassword, prev.metricsPassword),
      }));
      // The Connect passwords, recorded the same way: an entry this save wrote
      // is known present, and one it dropped counts as gone.
      setConnectStored((prev) => {
        const map = { ...prev };
        for (const [name] of connectWrites) map[name] = true;
        for (const name of removed) delete map[name];
        return map;
      });
      setForm((prev) => ({
        ...prev,
        password: "",
        clientKey: "",
        clientSecret: "",
        srPassword: "",
        metricsPassword: "",
        // Each row keeps the entry this save actually used, so the next one
        // reuses it rather than minting a second name for the same password.
        connect: prev.connect.map((row) => ({
          ...row,
          password: "",
          entry: connectEntryByKey.get(row.key) ?? row.entry,
        })),
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
    hasStoredMetricsPassword,
    connectStored,
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
          setForm(initialForm(profile, fallbackEnv));
          setFieldError(null);
          setConfirmingDelete(false);
        }
      }
    },
    [isNew, profile, onCancelNew, fallbackEnv],
  );

  const connecting = connStatus === "connecting";
  const showSasl =
    form.authKind === "sasl_plain" || form.authKind === "sasl_scram";
  const showMtls = form.authKind === "tls";
  const showAws = form.authKind === "aws_msk_iam";
  const showOauth = form.authKind === "oauth_bearer";
  const waitReason = connecting
    ? t("editor.busy.connecting")
    : busy
      ? t("editor.busy.saving")
      : undefined;
  const savingReason = busy ? t("editor.busy.saving") : undefined;

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
    fieldError?.field === field && fieldError.row === undefined ? (
      <span className="field-error" id={`pe-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;

  /** The same four helpers, addressed by row — see FieldError.row. */
  const connInvalid = (row: number, field: FieldKey) =>
    fieldError?.field === field && fieldError.row === row ? true : undefined;
  const connCls = (row: number, field: FieldKey, base = "") =>
    `${base}${connInvalid(row, field) ? " input-invalid" : ""}`.trim() ||
    undefined;
  const connDescribe = (row: number, field: FieldKey, hintId?: string) =>
    [hintId, connInvalid(row, field) ? `pe-connect-${row}-${field}-error` : null]
      .filter(Boolean)
      .join(" ") || undefined;
  const connMessage = (row: number, field: FieldKey) =>
    connInvalid(row, field) ? (
      <span className="field-error" id={`pe-connect-${row}-${field}-error`}>
        {fieldError?.message}
      </span>
    ) : null;

  return (
    // Picking a PROTECTED environment swaps this form's substrate live — the
    // rule, the tints and the picker all turn warm. That is the single best
    // moment in the product to teach the guardrail, and it now fires for
    // whatever the user marked protected instead of for one hard-coded name.
    <form
      className="editor"
      {...envAttrs(formEnv)}
      onKeyDown={handleKeyDown}
      onSubmit={(e) => {
        e.preventDefault();
        void handleConnect();
      }}
    >
      <div className="view-header">
        <h1 className="view-title">
          {isNew ? t("editor.new.title") : profile.name}
        </h1>
        <span className="view-subtitle">
          {isNew ? t("editor.new.subtitle") : t("editor.saved.subtitle")}
        </span>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="pe-name">
          {t("editor.name.label")}
        </label>
        <input
          id="pe-name"
          ref={bind("name")}
          type="text"
          className={cls("name")}
          value={form.name}
          placeholder={t("editor.name.placeholder")}
          autoFocus={isNew}
          aria-invalid={invalid("name")}
          aria-describedby={describe("name", "pe-name-hint")}
          onChange={(e) => edit("name", { name: e.target.value })}
        />
        {fieldMessage("name")}
        <span className="field-hint" id="pe-name-hint">
          {t("editor.name.hint")}
        </span>
      </div>

      <div className="field">
        <span className="field-label">{t("editor.env.label")}</span>
        <div className="env-row">
          <div
            className="env-picker"
            role="radiogroup"
            aria-label={t("editor.env.label")}
          >
            {envOptions.map((def, index) => {
              // The registry's own uniqueness rule, not `===`: a profile
              // stored as `Prod` names the same environment the manager holds
              // as `prod`, and an exact comparison would leave the picker
              // showing nothing checked for a value it is already carrying.
              const active = sameEnvironmentName(def.name, form.environment);
              return (
                <button
                  key={def.name}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  // One tab stop for the group; the arrows walk it. When
                  // NOTHING is checked — a profile whose environment is the
                  // empty string, which a hand-edited profiles.json can
                  // produce — the first segment takes the stop anyway, or the
                  // whole group drops out of the tab order (SC 2.1.1).
                  tabIndex={index === rovingIndex ? 0 : -1}
                  ref={(el) => {
                    envRefs.current[def.name] = el;
                  }}
                  className={`env-option ${active ? "env-option-active" : ""}`}
                  {...envAttrs(def)}
                  onClick={() => patch({ environment: def.name })}
                  onKeyDown={(e) => onEnvKeyDown(e, index)}
                >
                  {/* Law 2: the chosen segment is never chosen by colour
                      alone. `aria-checked` says it to assistive tech; this
                      glyph says it to everyone else. */}
                  {active && (
                    <span className="env-option-check" aria-hidden="true">
                      ✓
                    </span>
                  )}
                  {def.name}
                </button>
              );
            })}
          </div>
          {/* Not inside the radio group: it is not one of the choices, and a
              radiogroup with a non-radio child is a broken promise about what
              the arrow keys reach. */}
          <button
            type="button"
            className="btn btn-ghost env-manage-btn"
            onClick={() => setManagingEnvs(true)}
          >
            {t("editor.env.manage")}
          </button>
        </div>
        {/* The segment labels are NOT translated. They are user data now, and
            they are the same strings as `data-env-color`'s sibling attribute,
            the forced-colors wire label and the CLI's refusal — a guardrail
            that reads differently per locale is two signals where the design
            specifies one. */}
        <span className="field-hint">
          {envUnknown
            ? t("editor.env.hint.unknown", { name: form.environment })
            : formEnv.protected
              ? t("editor.env.hint.protected")
              : t("editor.env.hint.other")}
        </span>
      </div>

      {managingEnvs && (
        <EnvironmentsManager
          onClose={() => setManagingEnvs(false)}
          onProfilesChanged={onProfilesChanged}
          // The form holds an environment NAME in local state, and the manager
          // rewriting the stored profiles does not reach it. Follow the move,
          // or renaming the environment you are looking at leaves the picker
          // reading "unknown" about a change you just made.
          onEnvironmentMoved={(from, to) => {
            setForm((prev) =>
              sameEnvironmentName(prev.environment, from)
                ? { ...prev, environment: to }
                : prev,
            );
          }}
        />
      )}

      <div className="field">
        <label className="field-label" htmlFor="pe-bootstrap">
          <Term name="bootstrap-server">{t("editor.bootstrap.label")}</Term>
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
          {tx("editor.bootstrap.hint", { local: <code>localhost:9092</code> })}
        </span>
      </div>

      <fieldset className="fieldset">
        <legend className="eyebrow">{t("editor.auth.legend")}</legend>

        {unsupportedAuth && (
          <span className="field-hint">
            {tx("editor.auth.kerberos", {
              service: <code>{unsupportedAuth.service_name}</code>,
              principal: <code>{unsupportedAuth.principal}</code>,
            })}
          </span>
        )}

        {!unsupportedAuth && (
          <div className="field">
            <label className="field-label" htmlFor="pe-auth-kind">
              {t("editor.auth.label")}
            </label>
            <select
              id="pe-auth-kind"
              value={form.authKind}
              onChange={(e) =>
                changeAuthKind(e.target.value as EditableAuthKind)
              }
            >
              <option value="plaintext">{t("editor.auth.plaintext")}</option>
              <option value="sasl_plain">{t("editor.auth.saslPlain")}</option>
              <option value="sasl_scram">{t("editor.auth.saslScram")}</option>
              <option value="tls">{t("editor.auth.mtls")}</option>
              <option value="aws_msk_iam">{t("editor.auth.mskIam")}</option>
              <option value="oauth_bearer">{t("editor.auth.oauth")}</option>
              {/* Every disabled control says why. No dead ends. */}
              <option
                value="kerberos"
                disabled
                title={t("editor.auth.notYet")}
              >
                {t("editor.auth.kerberosOption")}
              </option>
            </select>
            <span className="field-hint">{t("editor.auth.hint")}</span>
          </div>
        )}

        {!unsupportedAuth && form.authKind === "sasl_scram" && (
          <div className="field">
            <label className="field-label" htmlFor="pe-mechanism">
              {t("editor.mechanism.label")}
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
            <span className="field-hint">{t("editor.mechanism.hint")}</span>
          </div>
        )}

        {!unsupportedAuth && showSasl && (
          <>
            <div className="field">
              <label className="field-label" htmlFor="pe-username">
                {t("editor.username.label")}
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
                {t("editor.password.label")}
              </label>
              <input
                id="pe-password"
                ref={bind("password")}
                type="password"
                className={cls("password")}
                value={form.password}
                autoComplete="new-password"
                placeholder={
                  hasStoredPassword
                    ? t("editor.secret.unchanged")
                    : t("editor.password.placeholder")
                }
                aria-invalid={invalid("password")}
                aria-describedby={describe("password", "pe-password-hint")}
                onChange={(e) => edit("password", { password: e.target.value })}
              />
              {fieldMessage("password")}
              <span className="field-hint" id="pe-password-hint">
                {t("editor.password.hint")}
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
                {t("editor.tls.label")}
              </label>
              <span className="field-hint" id="pe-tls-hint">
                {t("editor.tls.hint")}
              </span>
            </div>
          </>
        )}

        {!unsupportedAuth && showMtls && (
          <>
            <span className="field-hint">{t("editor.mtls.hint")}</span>

            <div className="field">
              <label className="field-label" htmlFor="pe-ca-path">
                {t("editor.caPath.label")}
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
                {t("editor.caPath.hint")}
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-cert">
                {t("editor.clientCert.label")}
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
                {t("editor.clientCert.hint")}
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-key">
                {t("editor.clientKey.label")}
              </label>
              <textarea
                id="pe-client-key"
                ref={bind("clientKey")}
                rows={5}
                className={cls("clientKey")}
                value={form.clientKey}
                placeholder={
                  hasStoredClientKey
                    ? t("editor.secret.unchanged")
                    : // A PEM header is a literal, not prose: it stays
                      // verbatim in every locale (§4).
                      "-----BEGIN PRIVATE KEY-----\n…"
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
                {t("editor.clientKey.hint")}
                {hasStoredClientKey ? ` ${t("editor.clientKey.storedHint")}` : ""}
              </span>
            </div>
          </>
        )}

        {!unsupportedAuth && showAws && (
          <>
            <span className="field-hint">
              {tx("editor.aws.hint", {
                host: <code>.amazonaws.com</code>,
              })}
            </span>

            <div className="field">
              <label className="field-label" htmlFor="pe-region">
                {t("editor.region.label")}
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
                {t("editor.region.hint")}
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-aws-profile">
                {t("editor.awsProfile.label")}
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
                {tx("editor.awsProfile.hint", {
                  config: <code>~/.aws/config</code>,
                  dir: <code>~/.aws</code>,
                })}
              </span>
            </div>
          </>
        )}

        {!unsupportedAuth && showOauth && (
          <>
            <span className="field-hint">{t("editor.oauth.hint")}</span>

            <div className="field">
              <label className="field-label" htmlFor="pe-token-endpoint">
                {t("editor.tokenEndpoint.label")}
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
                {t("editor.tokenEndpoint.hint")}
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-id">
                {t("editor.clientId.label")}
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
                {t("editor.clientId.hint")}
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pe-client-secret">
                {t("editor.clientSecret.label")}
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
                    ? t("editor.secret.unchanged")
                    : t("editor.clientSecret.placeholder")
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
                {t("editor.clientSecret.hint")}
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
        <legend className="eyebrow">{t("editor.sr.legend")}</legend>

        <span className="field-hint">{t("editor.sr.hint")}</span>

        <div className="field">
          <label className="field-label" htmlFor="pe-sr-url">
            {t("editor.srUrl.label")}
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
            {t("editor.srUrl.hint")}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-sr-username">
            {t("editor.srUsername.label")}
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
            {t("editor.srUsername.hint")}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-sr-password">
            {t("editor.srPassword.label")}
          </label>
          <input
            id="pe-sr-password"
            ref={bind("srPassword")}
            type="password"
            className={cls("srPassword")}
            value={form.srPassword}
            autoComplete="new-password"
            placeholder={
              hasStoredSrPassword
                ? t("editor.secret.unchanged")
                : t("editor.password.placeholder")
            }
            aria-invalid={invalid("srPassword")}
            aria-describedby={describe("srPassword", "pe-sr-password-hint")}
            onChange={(e) => edit("srPassword", { srPassword: e.target.value })}
          />
          {fieldMessage("srPassword")}
          <span className="field-hint" id="pe-sr-password-hint">
            {t("editor.password.hint")}
            {hasStoredSrPassword ? ` ${t("editor.srPassword.storedHint")}` : ""}
          </span>
        </div>
      </fieldset>

      {/* Kafka Connect — optional, plural, and independent of both the
          sign-in method and the registry: the workers are a separate service
          with their own address and their own credentials. Removing a cluster
          here takes its keychain entry with it, exactly like clearing the
          registry's address does. */}
      <fieldset className="fieldset">
        <legend className="eyebrow">{t("editor.connect.legend")}</legend>

        <span className="field-hint">{t("editor.connect.hint")}</span>

        {form.connect.map((cluster, row) => (
          <div className="connect-cluster" key={cluster.key}>
            <div className="connect-cluster-head">
              <span className="eyebrow">
                {cluster.name.trim().length > 0
                  ? cluster.name
                  : t("editor.connect.unnamed", { number: row + 1 })}
              </span>
              {/* Every Connect row has a Remove button, so "Remove" on its
                  own is N identically-named controls in one form. The label
                  says which row it belongs to; the visible word does not
                  change (SC 2.4.6). */}
              <button
                type="button"
                className="btn btn-ghost"
                aria-label={t("editor.connect.removeLabel", {
                  name:
                    cluster.name.trim().length > 0
                      ? cluster.name
                      : t("editor.connect.unnamedLong", { number: row + 1 }),
                })}
                title={t("editor.connect.remove")}
                onClick={() => removeConnect(row)}
              >
                {t("common.remove")}
              </button>
            </div>

            <div className="field">
              <label
                className="field-label"
                htmlFor={`pe-connect-${row}-name`}
              >
                {t("editor.connect.name.label")}
              </label>
              <input
                id={`pe-connect-${row}-name`}
                ref={connectBind(row, "name")}
                type="text"
                className={connCls(row, "connectName")}
                value={cluster.name}
                placeholder={t("editor.connect.name.placeholder")}
                autoComplete="off"
                aria-invalid={connInvalid(row, "connectName")}
                aria-describedby={connDescribe(
                  row,
                  "connectName",
                  `pe-connect-${row}-name-hint`,
                )}
                onChange={(e) => editConnect(row, { name: e.target.value })}
              />
              {connMessage(row, "connectName")}
              <span
                className="field-hint"
                id={`pe-connect-${row}-name-hint`}
              >
                {t("editor.connect.name.hint")}
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor={`pe-connect-${row}-url`}>
                {t("editor.connect.url.label")}
              </label>
              <input
                id={`pe-connect-${row}-url`}
                ref={connectBind(row, "url")}
                type="text"
                className={connCls(row, "connectUrl", "input-mono")}
                value={cluster.url}
                placeholder="http://connect-1.internal:8083"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={connInvalid(row, "connectUrl")}
                aria-describedby={connDescribe(
                  row,
                  "connectUrl",
                  `pe-connect-${row}-url-hint`,
                )}
                onChange={(e) => editConnect(row, { url: e.target.value })}
              />
              {connMessage(row, "connectUrl")}
              <span className="field-hint" id={`pe-connect-${row}-url-hint`}>
                {t("editor.connect.url.hint")}
              </span>
            </div>

            <div className="field">
              <label
                className="field-label"
                htmlFor={`pe-connect-${row}-user`}
              >
                {t("editor.username.label")}
              </label>
              <input
                id={`pe-connect-${row}-user`}
                type="text"
                className="input-mono"
                value={cluster.username}
                autoComplete="off"
                spellCheck={false}
                aria-describedby={`pe-connect-${row}-user-hint`}
                onChange={(e) => editConnect(row, { username: e.target.value })}
              />
              <span className="field-hint" id={`pe-connect-${row}-user-hint`}>
                {t("editor.connect.username.hint")}
              </span>
            </div>

            <div className="field">
              <label
                className="field-label"
                htmlFor={`pe-connect-${row}-password`}
              >
                {t("editor.password.label")}
              </label>
              <input
                id={`pe-connect-${row}-password`}
                type="password"
                value={cluster.password}
                autoComplete="new-password"
                placeholder={
                  cluster.entry !== null && connectStored[cluster.entry]
                    ? t("editor.secret.unchanged")
                    : t("editor.password.placeholder")
                }
                aria-describedby={`pe-connect-${row}-password-hint`}
                onChange={(e) => editConnect(row, { password: e.target.value })}
              />
              <span
                className="field-hint"
                id={`pe-connect-${row}-password-hint`}
              >
                {t("editor.password.hint")}
                {cluster.entry !== null && connectStored[cluster.entry]
                  ? ` ${t("editor.connect.password.storedHint")}`
                  : ""}
              </span>
            </div>
          </div>
        ))}

        <button type="button" className="btn" onClick={addConnect}>
          {t("editor.connect.add")}
        </button>
      </fieldset>

      {/* Monitoring — optional, and independent of everything above it. The
          metrics endpoint is a plain HTTP address in front of JMX, and the
          sampler is Kavka's own clock. Neither has anything to do with how the
          cluster checks who you are. */}
      <fieldset className="fieldset">
        <legend className="eyebrow">{t("editor.monitoring.legend")}</legend>

        <span className="field-hint">{t("editor.monitoring.hint")}</span>

        <div className="field">
          <label className="field-label" htmlFor="pe-metrics-url">
            {t("editor.metricsUrl.label")}
          </label>
          <input
            id="pe-metrics-url"
            ref={bind("metricsUrl")}
            type="text"
            className={cls("metricsUrl", "input-mono")}
            value={form.metricsUrl}
            placeholder="http://broker-1.internal:7071/metrics"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("metricsUrl")}
            aria-describedby={describe("metricsUrl", "pe-metrics-url-hint")}
            onChange={(e) => edit("metricsUrl", { metricsUrl: e.target.value })}
          />
          {fieldMessage("metricsUrl")}
          <span className="field-hint" id="pe-metrics-url-hint">
            {tx("editor.metricsUrl.hint", {
              agent: <code>jmx_exporter</code>,
              flag: (
                <code>
                  -javaagent:jmx_prometheus_javaagent.jar=7071:kafka.yml
                </code>
              ),
            })}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-metrics-user">
            {t("editor.metricsUsername.label")}
          </label>
          <input
            id="pe-metrics-user"
            ref={bind("metricsUsername")}
            type="text"
            className={cls("metricsUsername", "input-mono")}
            value={form.metricsUsername}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("metricsUsername")}
            aria-describedby={describe(
              "metricsUsername",
              "pe-metrics-user-hint",
            )}
            onChange={(e) =>
              edit("metricsUsername", { metricsUsername: e.target.value })
            }
          />
          {fieldMessage("metricsUsername")}
          <span className="field-hint" id="pe-metrics-user-hint">
            {t("editor.metricsUsername.hint")}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-metrics-password">
            {t("editor.metricsPassword.label")}
          </label>
          <input
            id="pe-metrics-password"
            ref={bind("metricsPassword")}
            type="password"
            className={cls("metricsPassword")}
            value={form.metricsPassword}
            autoComplete="new-password"
            placeholder={
              hasStoredMetricsPassword
                ? t("editor.secret.unchanged")
                : t("editor.password.placeholder")
            }
            aria-invalid={invalid("metricsPassword")}
            aria-describedby={describe(
              "metricsPassword",
              "pe-metrics-password-hint",
            )}
            onChange={(e) =>
              edit("metricsPassword", { metricsPassword: e.target.value })
            }
          />
          {fieldMessage("metricsPassword")}
          <span className="field-hint" id="pe-metrics-password-hint">
            {t("editor.password.hint")}
            {hasStoredMetricsPassword
              ? ` ${t("editor.metricsPassword.storedHint")}`
              : ""}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="pe-sampler">
            {t("editor.sampler.label")}
          </label>
          <input
            id="pe-sampler"
            ref={bind("samplerSeconds")}
            type="number"
            min={SAMPLER_MIN_MS / 1000}
            step={1}
            className={cls("samplerSeconds")}
            value={form.samplerSeconds}
            aria-invalid={invalid("samplerSeconds")}
            aria-describedby={describe("samplerSeconds", "pe-sampler-hint")}
            onChange={(e) =>
              edit("samplerSeconds", { samplerSeconds: e.target.value })
            }
          />
          {fieldMessage("samplerSeconds")}
          <span className="field-hint" id="pe-sampler-hint">
            {tx("editor.sampler.hint", {
              days: t("unit.days", { count: HISTORY_RETENTION_DAYS }),
              // The <strong> is a param, so the emphasis travels with the
              // sentence rather than a translator having to guess which
              // clause it wrapped.
              warning: <strong>{t("editor.sampler.warning")}</strong>,
              floor: spanText(t, SAMPLER_MIN_MS),
              default: spanText(t, SAMPLER_DEFAULT_MS),
            })}
          </span>
        </div>
      </fieldset>

      {/* Custom decoders — independent of everything above, and stored beside
          the connection rather than inside it. The section states that, since
          it is the one part of this form that writes as you type. */}
      <WasmSerdesFields profileId={profile?.id ?? null} />

      <div className="check-field">
        <input
          id="pe-readonly"
          type="checkbox"
          checked={form.readOnly}
          aria-describedby="pe-readonly-hint"
          onChange={(e) => patch({ readOnly: e.target.checked })}
        />
        <label className="check-label" htmlFor="pe-readonly">
          {t("editor.readonly.label")}
        </label>
        <span className="field-hint" id="pe-readonly-hint">
          {t("editor.readonly.hint")}
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
            {t("common.save")}
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
            {t("common.connect")}
          </button>
          {isNew && (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              title={savingReason}
              onClick={onCancelNew}
            >
              {t("common.cancel")}
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
                  {t("editor.delete.confirm", { name: profile.name })}
                </span>
                <button
                  type="button"
                  className="btn"
                  disabled={busy}
                  title={savingReason}
                  onClick={() => setConfirmingDelete(false)}
                >
                  {t("common.cancel")}
                </button>
                <button
                  type="button"
                  className="btn btn-danger-confirm"
                  disabled={busy || connecting}
                  title={waitReason}
                  onClick={() => void handleDelete()}
                >
                  {t("editor.delete")}
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
                {t("editor.delete")}
              </button>
            )}
          </div>
        )}
      </div>

      <span className="editor-kbd-hint">
        <span className="kbd">Enter</span> {t("editor.kbd.connect")}
        <span aria-hidden="true">·</span>
        <span className="kbd">Esc</span>{" "}
        {isNew ? t("editor.kbd.cancel") : t("editor.kbd.undo")}
      </span>
    </form>
  );
}
