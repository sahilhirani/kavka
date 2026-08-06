import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  aclsCreate,
  aclsDelete,
  aclsList,
  errorMessage,
  ACL_OPERATIONS,
  type AclBinding,
  type AclFilter,
  type AclOperation,
  type AclPatternType,
  type AclPermission,
  type AclResourceType,
  type ConnectionProfile,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { useIsProtected } from "./environments";
import { classifyError } from "./errors";
import { useI18n } from "./i18n";
import Overlay from "./Overlay";
import Perch from "./Perch";
import { ErrorBanner } from "./ProfileEditor";
import { ToastStack, useToasts } from "./Toast";

/**
 * ACCESS RULES.
 *
 * An ACL is one sentence Kafka enforces, and this whole view is built around
 * saying that sentence out loud: the create modal writes it live as the form
 * changes, every row carries it on hover, and the delete confirmation leads
 * with it. Seven dropdowns that produce "ALLOW User:alice READ TOPIC orders
 * LITERAL *" teach nobody anything; the same seven controls under a line that
 * reads "Allow user alice to read topic orders from any host" teach the model
 * on the way past.
 *
 * The one asymmetry worth knowing: a DENY rule beats every ALLOW that matches
 * the same request, so REMOVING a deny can WIDEN access. That is the opposite
 * of what "delete" usually means, and it is the reason this view's delete
 * confirmation says something a topic delete never has to.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/** Kafka's own word for each resource type, in the user's vocabulary. */
const RESOURCE_WORD: Record<string, string> = {
  topic: "topic",
  group: "consumer group",
  cluster: "this cluster",
  transactional_id: "transactional id",
  delegation_token: "delegation token",
  user: "user",
};

/** The four Kavka's editor writes. The table renders anything the cluster has. */
const RESOURCE_TYPES: ReadonlyArray<{ value: AclResourceType; label: string }> = [
  { value: "topic", label: "Topic" },
  { value: "group", label: "Consumer group" },
  { value: "cluster", label: "The cluster itself" },
  { value: "transactional_id", label: "Transactional id" },
];

/**
 * The verb phrase each operation becomes inside the sentence. Kafka's own name
 * is never hidden — it is what the table cell and the `kafka-acls.sh` command
 * both show — but it is not what the sentence says.
 */
const OPERATION_PHRASE: Record<string, string> = {
  read: "read",
  write: "write to",
  create: "create",
  delete: "delete",
  alter: "change",
  describe: "see",
  describe_configs: "read the settings of",
  alter_configs: "change the settings of",
  cluster_action: "run internal cluster operations on",
  idempotent_write: "write to, idempotently,",
  all: "do anything to",
};

/** One line under the operation picker, so nobody has to guess what it covers. */
const OPERATION_HINT: Record<string, string> = {
  read: "Consume records, and read committed offsets.",
  write: "Produce records.",
  create: "Create the topic (or the group) if it doesn't exist yet.",
  delete: "Delete the topic, or delete a group's committed offsets.",
  alter: "Change the resource itself — a topic's partition count, say.",
  describe: "See that the resource exists, and read its metadata.",
  describe_configs: "Read the resource's configuration.",
  alter_configs: "Change the resource's configuration.",
  cluster_action:
    "Inter-broker operations. Brokers need it; applications almost never do.",
  idempotent_write:
    "Produce with idempotence on. On Kafka 3.0 and later, Write already covers it.",
  all: "Every operation, including ones added by a future Kafka release.",
};

function resourceWord(type: string): string {
  return RESOURCE_WORD[type] ?? type;
}

function operationPhrase(operation: string): string {
  return OPERATION_PHRASE[operation] ?? `perform ${operation} on`;
}

/** `User:alice` → `user alice`. Anything unfamiliar is shown verbatim. */
function principalWords(principal: string): string {
  const trimmed = principal.trim();
  if (trimmed === "User:*" || trimmed === "*") return "anyone";
  const colon = trimmed.indexOf(":");
  if (colon <= 0) return trimmed;
  return `${trimmed.slice(0, colon).toLowerCase()} ${trimmed.slice(colon + 1)}`;
}

/**
 * The resource half of the sentence.
 *
 * **The pattern is decided first, and that order is load-bearing.** `*` is only
 * the wildcard under a LITERAL pattern; under PREFIXED it is an ordinary
 * character, and `PREFIXED *` matches names that literally begin with an
 * asterisk — a rule that in practice grants nothing. Testing the name first
 * read that binding out as "every topic", which is the widest sentence this
 * screen can say about the narrowest rule Kafka can hold.
 */
function resourcePhrase(type: string, name: string, pattern: string): string {
  if (type === "cluster") return "this cluster";
  const word = resourceWord(type);
  if (pattern === "prefixed")
    return `every ${word} whose name starts with ${name}`;
  if (name === "*") return `every ${word}`;
  return `${word} ${name}`;
}

function hostPhrase(host: string): string {
  return host.trim() === "*" ? "from any host" : `from ${host.trim()}`;
}

/**
 * THE SENTENCE. One line of English that means exactly what the seven fields
 * mean, and nothing more — no hedging, no "may be able to".
 */
export function aclSentence(binding: AclBinding): string {
  const who = principalWords(binding.principal);
  const what = resourcePhrase(
    binding.resource_type,
    binding.resource_name,
    binding.pattern_type,
  );
  const verb = operationPhrase(binding.operation);
  const where = hostPhrase(binding.host);
  return binding.permission === "deny"
    ? `Deny ${who} permission to ${verb} ${what} ${where}.`
    : `Allow ${who} to ${verb} ${what} ${where}.`;
}

// ---------------------------------------------------------------------------
// THE PARITY TABLE
//
// The sentence is the feature — the create modal writes it live, every row
// carries it on hover, and the delete confirmation leads with it — so a wrong
// sentence is not a cosmetic bug, it is Kavka lying about what a rule does.
// This repo has no TS test runner (see apps/desktop/package.json); the answer
// it already gives to that, in template.ts and virtual.ts, is to check in dev
// and fail loudly in the console. This is that pattern.
//
// Every row is a claim about Kafka's own matching rules. The two PREFIXED-`*`
// rows are why the table exists: `*` is the wildcard only under a LITERAL
// pattern, and reading `PREFIXED *` as "every topic" describes the widest rule
// there is where Kafka is holding one of the narrowest.
// ---------------------------------------------------------------------------

interface ParityRow {
  what: string;
  got: () => string;
  want: string;
}

/** An ordinary allow rule, for a row to vary one field of. */
function sampleBinding(over: Partial<AclBinding> = {}): AclBinding {
  return {
    resource_type: "topic",
    resource_name: "orders.v2",
    pattern_type: "literal",
    principal: "User:alice",
    host: "*",
    operation: "read",
    permission: "allow",
    ...over,
  };
}

function parityRows(): ParityRow[] {
  const phrase = (name: string, pattern: string, type = "topic") =>
    resourcePhrase(type, name, pattern);

  return [
    // --- the pattern decides first: `*` is the wildcard only under LITERAL
    {
      what: "LITERAL * is the wildcard",
      got: () => phrase("*", "literal"),
      want: "every topic",
    },
    {
      what: "PREFIXED * is an asterisk, not the wildcard",
      got: () => phrase("*", "prefixed"),
      want: "every topic whose name starts with *",
    },
    {
      what: "PREFIXED names read as a prefix",
      got: () => phrase("orders", "prefixed"),
      want: "every topic whose name starts with orders",
    },
    {
      what: "LITERAL names read as one name",
      got: () => phrase("orders.v2", "literal"),
      want: "topic orders.v2",
    },
    {
      what: "the resource word follows the type",
      got: () => phrase("*", "prefixed", "group"),
      want: "every consumer group whose name starts with *",
    },
    {
      what: "the cluster has no name and no pattern",
      got: () => phrase("kafka-cluster", "prefixed", "cluster"),
      want: "this cluster",
    },

    // --- and the sentence is built from that one function, not a second copy
    {
      what: "the sentence uses the phrase, PREFIXED *",
      got: () =>
        aclSentence(sampleBinding({ resource_name: "*", pattern_type: "prefixed" })),
      want: "Allow user alice to read every topic whose name starts with * from any host.",
    },
    {
      what: "the sentence uses the phrase, LITERAL *",
      got: () => aclSentence(sampleBinding({ resource_name: "*" })),
      want: "Allow user alice to read every topic from any host.",
    },
    {
      what: "a deny sentence says deny",
      got: () =>
        aclSentence(
          sampleBinding({
            permission: "deny",
            principal: "User:*",
            operation: "write",
            host: "10.0.4.19",
          }),
        ),
      want: "Deny anyone permission to write to topic orders.v2 from 10.0.4.19.",
    },
  ];
}

/**
 * Runs the parity table and returns what disagreed. Exported so it can be
 * called from a console, or from a test runner the day this repo grows one.
 */
export function aclSentenceParityFailures(): string[] {
  return parityRows()
    .filter((row) => row.got() !== row.want)
    .map((row) => `${row.what}: expected ${JSON.stringify(row.want)}, got ${JSON.stringify(row.got())}`);
}

if (import.meta.env.DEV) {
  const failures = aclSentenceParityFailures();
  if (failures.length > 0) {
    console.error(
      "[kavka] the ACL sentence no longer says what the binding means — the " +
        "sentence is what the create modal, every row's tooltip and the delete " +
        "confirmation all show, so this is a wrong claim about access:\n" +
        failures.join("\n"),
    );
  }
}

/**
 * A cluster with no authorizer refuses to answer at all rather than returning
 * an empty list, and Kafka's word for that ("security disabled") reads like a
 * warning about the user's own credentials. It isn't one.
 */
function noAuthorizerNote(raw: string): string | null {
  const text = raw.toLowerCase();
  return text.includes("security_disabled") ||
    text.includes("security disabled") ||
    text.includes("no authorizer") ||
    text.includes("authorizer is not configured")
    ? "This cluster has no authorizer configured, so it has no access rules to list — that is a broker setting (authorizer.class.name), not a permission you are missing. Kafka refuses the request outright rather than answering with an empty list."
    : null;
}

interface AclsTabProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
}

const EMPTY_FILTER: AclFilter = {
  resource_type: null,
  resource_name: null,
  principal: null,
};

export default function AclsTab({ profile, onDanger }: AclsTabProps) {
  const [acls, setAcls] = useState<AclBinding[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [listFailed, setListFailed] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The form's fields, and the filter actually in force. They are separate on
  // purpose: refetching per keystroke would hammer the broker, and a filter
  // that changes under you as you type is not a filter.
  const [formType, setFormType] = useState<string>("");
  const [formName, setFormName] = useState("");
  const [formPrincipal, setFormPrincipal] = useState("");
  const [filter, setFilter] = useState<AclFilter>(EMPTY_FILTER);

  const [creating, setCreating] = useState(false);
  const [removing, setRemoving] = useState<AclBinding | null>(null);
  const [busy, setBusy] = useState(false);
  /**
   * The filter form folds away. It is an expert control on a screen a novice
   * opens to read one sentence — but the SUMMARY says when a filter is in
   * force, so folding it can never hide the reason the list looks short.
   */
  const [filterOpen, setFilterOpen] = useState(false);

  const toaster = useToasts();
  const push = toaster.push;
  const seq = useRef(0);

  const { t } = useI18n();
  useDangerSignal(error !== null, onDanger);

  const isProtected = useIsProtected(profile.environment);
  const readOnly = profile.read_only;

  const fetchAcls = useCallback(
    async (which: AclFilter) => {
      const mine = ++seq.current;
      setLoading(true);
      try {
        const list = await aclsList(profile.id, which);
        if (seq.current !== mine) return;
        setAcls(list);
        setListFailed(false);
      } catch (err) {
        if (seq.current !== mine) return;
        setListFailed(true);
        setError(errorMessage(err));
      } finally {
        if (seq.current === mine) setLoading(false);
      }
    },
    [profile.id],
  );

  useEffect(() => {
    void fetchAcls(filter);
    return () => {
      seq.current += 1;
    };
  }, [fetchAcls, filter]);

  const applyFilter = useCallback(() => {
    setFilter({
      resource_type: formType.length > 0 ? formType : null,
      resource_name: formName.trim().length > 0 ? formName.trim() : null,
      principal: formPrincipal.trim().length > 0 ? formPrincipal.trim() : null,
    });
  }, [formType, formName, formPrincipal]);

  const clearFilter = useCallback(() => {
    setFormType("");
    setFormName("");
    setFormPrincipal("");
    setFilter(EMPTY_FILTER);
  }, []);

  const filtered =
    filter.resource_type !== null ||
    filter.resource_name !== null ||
    filter.principal !== null;

  const handleDelete = useCallback(async () => {
    if (removing === null) return;
    setBusy(true);
    try {
      const gone = await aclsDelete(profile.id, removing);
      setRemoving(null);
      if (gone.length === 0) {
        // Not an error, and not a success either: the rule was already gone,
        // and saying "removed" would be a lie about what this click did.
        push({
          kind: "warn",
          title: "Nothing was removed — that rule was already gone",
          detail:
            "Kafka matched no rule for it. Someone else may have removed it since this list was read.",
        });
      } else {
        push({
          kind: "ok",
          title: `Removed ${gone.length} access rule${gone.length === 1 ? "" : "s"}`,
          detail: aclSentence(gone[0]),
        });
      }
      await fetchAcls(filter);
    } catch (err) {
      setRemoving(null);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [removing, profile.id, push, fetchAcls, filter]);

  const authorizerNote = listFailed && error !== null ? noAuthorizerNote(error) : null;

  const denies = useMemo(
    () => acls?.filter((a) => a.permission === "deny").length ?? 0,
    [acls],
  );

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      {/* A cluster with no authorizer is not a cluster with no rules, and the
          verdict keeps those two apart — the first means the brokers' default
          decides every request, which is a fact about the cluster nobody should
          have to infer from an empty table. */}
      <Perch
        screen={t("rail.item.acls")}
        loading={acls === null && loading}
        tone={
          authorizerNote !== null || acls === null || listFailed
            ? "unknown"
            : acls.length === 0 || denies > 0
              ? "watch"
              : "ok"
        }
        caveat={
          authorizerNote !== null
            ? t("perch.acls.noAuthorizerNext")
            : acls === null || listFailed
              ? undefined
              : filtered
                ? t("perch.acls.caveat.filtered")
                : denies > 0
                  ? t("perch.acls.caveat.removing")
                  : undefined
        }
      >
        {authorizerNote !== null
          ? t("perch.acls.noAuthorizer")
          : acls === null || listFailed
            ? t("perch.acls.unread")
            : filtered
              ? t("perch.acls.filtered", { count: acls.length })
              : acls.length === 0
                ? t("perch.acls.none")
                : denies > 0
                  ? t("perch.acls.someDeny", { count: acls.length, denies })
                  : t("perch.acls.allAllow", { count: acls.length })}
      </Perch>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Access rules
            {acls !== null && (
              <span className="panel-count">
                {acls.length}
                {denies > 0 ? ` · ${denies} deny` : ""}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={loading}
              aria-busy={loading || undefined}
              title={
                loading ? "Kavka is already asking the cluster for rules" : undefined
              }
              onClick={() => void fetchAcls(filter)}
            >
              Refresh
            </button>
            {/* A write action renders danger-outlined on prod even when it is
                routine (§6 layer 7). */}
            <button
              type="button"
              className={`btn ${isProtected ? "btn-danger" : ""}`}
              disabled={readOnly}
              title={readOnly ? READ_ONLY_WHY : "Write a new access rule"}
              onClick={() => setCreating(true)}
            >
              Add a rule
            </button>
          </div>
        </div>

        {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {/* PROGRESSIVE DISCLOSURE, with the honesty kept outside the fold: the
            summary carries "a filter is in force" whenever one is, so a
            collapsed form can never be the reason a list looks empty. */}
        <details
          className="disclose disclose-inline"
          open={filterOpen || filtered}
          onToggle={(e) => setFilterOpen(e.currentTarget.open)}
        >
          <summary>
            <svg
              className="caret"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              aria-hidden="true"
              focusable="false"
            >
              <path d="M6 4l4 4-4 4" />
            </svg>
            {t("acls.filter.summary")}
            <span className="sum-note">
              {filtered ? t("acls.filter.active") : t("acls.filter.note")}
            </span>
          </summary>

        {/* The filter is a form: it applies on submit, never on keystroke. */}
        <form
          className="seekbar"
          onSubmit={(e) => {
            e.preventDefault();
            applyFilter();
          }}
        >
          <div className="seekbar-field">
            <label className="seekbar-label" htmlFor="acl-f-type">
              Resource type
            </label>
            <select
              id="acl-f-type"
              value={formType}
              onChange={(e) => setFormType(e.target.value)}
            >
              <option value="">Any</option>
              {RESOURCE_TYPES.map((r) => (
                <option key={r.value} value={r.value}>
                  {r.label}
                </option>
              ))}
            </select>
          </div>
          <div className="seekbar-field seekbar-field-wide">
            <label className="seekbar-label" htmlFor="acl-f-name">
              Resource name
            </label>
            <input
              id="acl-f-name"
              type="text"
              className="input-mono"
              value={formName}
              placeholder="orders.v2"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setFormName(e.target.value)}
            />
          </div>
          <div className="seekbar-field seekbar-field-wide">
            <label className="seekbar-label" htmlFor="acl-f-principal">
              Principal
            </label>
            <input
              id="acl-f-principal"
              type="text"
              className="input-mono"
              value={formPrincipal}
              placeholder="User:alice"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setFormPrincipal(e.target.value)}
            />
          </div>
          <div className="seekbar-field seekbar-actions">
            <button type="submit" className="btn">
              Apply filter
            </button>
          </div>
          {filtered && (
            <div className="seekbar-field seekbar-actions">
              <button type="button" className="btn btn-ghost" onClick={clearFilter}>
                Clear
              </button>
            </div>
          )}
        </form>
        </details>

        {acls === null && loading ? (
          <>
            <div className="table-note">Asking the cluster for access rules…</div>
            <div className="skeleton-table" aria-hidden="true">
              {[44, 32, 50, 38].map((w, i) => (
                <div className="skeleton-row" key={i}>
                  <div className="skeleton-cell" style={{ width: `${w}%` }} />
                  <div className="skeleton-cell" style={{ width: "56px" }} />
                </div>
              ))}
            </div>
          </>
        ) : acls === null || listFailed ? (
          <p className="table-note">
            {authorizerNote ??
              "Kavka couldn't read this cluster's access rules. The account needs Describe on the cluster to list them — ask whoever issued the credentials for that permission."}
          </p>
        ) : acls.length === 0 ? (
          <EmptyAcls
            filtered={filtered}
            readOnly={readOnly}
            onClear={clearFilter}
            onCreate={() => setCreating(true)}
          />
        ) : (
          <div className="table-wrap">
            {loading && <div className="table-loading" role="presentation" />}
            {/* An access rule has no address of its own, so the gutter is zero
                and the rule sits flush at the table's left edge (§2). */}
            <table className="data-table data-table-flush">
              <caption className="sr-only">
                Access rules on this cluster
                {filtered ? ", filtered" : ""}
              </caption>
              <thead>
                <tr>
                  <th scope="col">Principal</th>
                  <th scope="col">Permission</th>
                  <th scope="col">Operation</th>
                  <th scope="col">Resource</th>
                  <th scope="col">Pattern</th>
                  <th scope="col">Host</th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Remove</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {acls.map((acl) => (
                  <AclRow
                    key={`${acl.principal}|${acl.permission}|${acl.operation}|${acl.resource_type}|${acl.pattern_type}|${acl.resource_name}|${acl.host}`}
                    acl={acl}
                    readOnly={readOnly}
                    onRemove={() => setRemoving(acl)}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}

        {/* THE PANEL'S CAVEAT, IN THE PANEL'S OWN SLOT. This sentence was
            always the right words in the wrong container — a `.table-note`
            reads as commentary between elements, and this qualifies every row
            above it. The second half is the limitation it never stated: this
            is the authorizer's list, and a cluster running without one lists
            nothing while allowing everything. */}
        {acls !== null && acls.length > 0 && (
          <p className="panel-foot">
            A deny rule beats every allow rule that matches the same request, so
            removing one can widen access rather than narrow it. Kavka says so
            again before it removes one. {t("acls.foot.authorizer")}
          </p>
        )}
      </section>

      {creating && (
        <AclCreateModal
          profile={profile}
          onClose={() => setCreating(false)}
          onCreated={(binding) => {
            setCreating(false);
            push({
              kind: "ok",
              title: "Added an access rule",
              detail: aclSentence(binding),
            });
            void fetchAcls(filter);
          }}
        />
      )}

      {removing !== null && (
        <ConfirmModal
          title={
            isProtected
              ? `Remove this rule on ${profile.name}?`
              : "Remove this access rule?"
          }
          body={
            <>
              <p className="reset-preview">{aclSentence(removing)}</p>
              {removing.permission === "deny" ? (
                <>
                  Kafka stops enforcing this refusal. Anything that was blocked
                  only by it becomes allowed again the moment any allow rule
                  matches.
                </>
              ) : (
                <>
                  {principalWords(removing.principal)} loses this permission
                  unless another rule still grants it. Applications using it will
                  start failing with authorization errors, not with a warning.
                </>
              )}
            </>
          }
          confirmLabel="Remove rule"
          // Environment-gated, not action-gated: a protected environment
          // always asks, an unprotected one never
          // does (§6 layer 4).
          typeToConfirm={isProtected ? removing.resource_name : null}
          extra={
            removing.permission === "deny" ? (
              <div className="banner banner-warn" role="note">
                <span className="banner-glyph" aria-hidden="true">
                  !
                </span>
                <div className="banner-body">
                  <p className="banner-title">
                    Removing a deny rule WIDENS access.
                  </p>
                  <p className="banner-detail">
                    A deny outranks every allow that matches the same request, so
                    this rule may be the only thing stopping something right now.
                    Check what allows exist for{" "}
                    {resourcePhrase(
                      removing.resource_type,
                      removing.resource_name,
                      removing.pattern_type,
                    )}{" "}
                    before you remove it.
                  </p>
                </div>
              </div>
            ) : undefined
          }
          busy={busy}
          busyLabel="Kavka is removing the rule"
          onCancel={() => setRemoving(null)}
          onConfirm={() => void handleDelete()}
        />
      )}

      <ToastStack {...toaster} />
    </>
  );
}

/** Empty is two different situations, and they need two different sentences. */
function EmptyAcls({
  filtered,
  readOnly,
  onClear,
  onCreate,
}: {
  filtered: boolean;
  readOnly: boolean;
  onClear: () => void;
  onCreate: () => void;
}) {
  if (filtered) {
    return (
      <>
        <p className="table-note">
          No rules match this filter. The cluster may still have others — the
          filter narrows by resource type, resource name and principal, and all
          three have to match.
        </p>
        <div className="empty-actions">
          <button type="button" className="btn" onClick={onClear}>
            Clear the filter
          </button>
        </div>
      </>
    );
  }
  return (
    <>
      <p className="table-note">
        This cluster has an authorizer, but no access rules yet. An ACL is one
        sentence Kafka enforces: <em>allow</em> or <em>deny</em> one principal
        one operation on one resource, from one host.
      </p>
      <p className="table-note">
        With no rules at all, what happens next is the broker's own default:
        with <code>allow.everyone.if.no.acl.found=true</code> — which is what
        this repo's dev cluster sets — everything is permitted until the first
        rule exists. On a cluster where it is false, nothing is, except for the
        super users.
      </p>
      <div className="empty-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={readOnly}
          title={readOnly ? READ_ONLY_WHY : undefined}
          onClick={onCreate}
        >
          Add a rule
        </button>
      </div>
    </>
  );
}

/**
 * One rule. A deny row takes the danger wash and the word "Deny" with a glyph:
 * three channels, because "the row that is subtly pinker" is exactly the state
 * law 2 exists to stop anyone shipping.
 */
function AclRow({
  acl,
  readOnly,
  onRemove,
}: {
  acl: AclBinding;
  readOnly: boolean;
  onRemove: () => void;
}) {
  const deny = acl.permission === "deny";
  return (
    <tr className={deny ? "row-deny" : undefined} title={aclSentence(acl)}>
      <td className="cell-mono">{acl.principal}</td>
      <td>
        <span className={`acl-perm${deny ? " acl-perm-deny" : ""}`}>
          <span aria-hidden="true">{deny ? "✕" : "✓"}</span>
          {deny ? "Deny" : "Allow"}
        </span>
      </td>
      <td className="cell-mono">{acl.operation}</td>
      <td className="cell-mono">
        {acl.resource_name}
        <span className="cell-tag">
          {" "}
          {acl.resource_type === "cluster"
            ? "the cluster"
            : resourceWord(acl.resource_type)}
        </span>
      </td>
      {/* The pattern is a claim about the FUTURE — a prefixed rule covers every
          topic created later whose name starts the same way — so it gets a
          column of its own rather than a footnote on the name. */}
      <td className="cell-tag">
        {acl.pattern_type === "prefixed" ? (
          <span title="Every resource whose name starts with this, including ones that don't exist yet.">
            starts with
          </span>
        ) : acl.pattern_type === "literal" ? (
          <span title="This exact name, and nothing else.">exactly</span>
        ) : (
          acl.pattern_type
        )}
      </td>
      <td className="cell-mono">
        {acl.host === "*" ? (
          <>
            *<span className="cell-tag"> any host</span>
          </>
        ) : (
          acl.host
        )}
      </td>
      <td className="col-affordance">
        {/* Danger OUTLINE: it only ever opens the confirmation. */}
        <button
          type="button"
          className="btn btn-danger btn-row"
          disabled={readOnly}
          title={readOnly ? READ_ONLY_WHY : "Remove this rule"}
          onClick={onRemove}
        >
          Remove
        </button>
      </td>
    </tr>
  );
}

// ───────────────────────────────────────────────────────────────────────────
// The create modal — the one that explains as it goes
// ───────────────────────────────────────────────────────────────────────────

type FieldKey = "principal" | "resourceName" | "host";

interface FieldError {
  field: FieldKey;
  message: string;
}

function AclCreateModal({
  profile,
  onClose,
  onCreated,
}: {
  profile: ConnectionProfile;
  onClose: () => void;
  onCreated: (binding: AclBinding) => void;
}) {
  const [principal, setPrincipal] = useState("User:");
  const [resourceType, setResourceType] = useState<AclResourceType>("topic");
  const [patternType, setPatternType] = useState<AclPatternType>("literal");
  const [resourceName, setResourceName] = useState("");
  const [operation, setOperation] = useState<AclOperation>("read");
  const [permission, setPermission] = useState<AclPermission>("allow");
  const [host, setHost] = useState("*");
  const [fieldError, setFieldError] = useState<FieldError | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const principalRef = useRef<HTMLInputElement | null>(null);
  const controls = useRef<Partial<Record<FieldKey, HTMLElement | null>>>({});
  const bind = useMemo(() => {
    const cache: Partial<Record<FieldKey, (el: HTMLElement | null) => void>> = {};
    return (field: FieldKey) =>
      (cache[field] ??= (el: HTMLElement | null) => {
        controls.current[field] = el;
        if (field === "principal")
          principalRef.current = el as HTMLInputElement | null;
      });
  }, []);

  /**
   * The pattern picker's keyboard model — the same fix as the connection
   * form's environment picker, for the same reason. `role="radiogroup"`
   * promises one tab stop walked with the arrows, with selection following
   * focus; two plain buttons in the tab order kept neither half of that
   * (SC 2.1.1, SC 4.1.2).
   */
  const patternRefs = useRef<Partial<Record<AclPatternType, HTMLButtonElement | null>>>(
    {},
  );
  const onPatternKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>) => {
      const order: AclPatternType[] = ["literal", "prefixed"];
      const index = order.indexOf(patternType);
      let next: number | null = null;
      if (e.key === "ArrowRight" || e.key === "ArrowDown")
        next = (index + 1) % order.length;
      else if (e.key === "ArrowLeft" || e.key === "ArrowUp")
        next = (index - 1 + order.length) % order.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = order.length - 1;
      if (next === null) return;
      e.preventDefault();
      setPatternType(order[next]);
      patternRefs.current[order[next]]?.focus();
    },
    [patternType],
  );

  const isProtected = useIsProtected(profile.environment);
  const isCluster = resourceType === "cluster";
  // Kafka's own literal for the cluster resource. It is not a name the user
  // gets to pick, so the field is filled in and explained rather than left
  // blank for them to guess at.
  const effectiveName = isCluster ? "kafka-cluster" : resourceName.trim();
  const effectivePattern: AclPatternType = isCluster ? "literal" : patternType;

  const draft: AclBinding = {
    resource_type: resourceType,
    resource_name: effectiveName.length > 0 ? effectiveName : "…",
    pattern_type: effectivePattern,
    principal: principal.trim().length > 0 ? principal.trim() : "…",
    host: host.trim().length > 0 ? host.trim() : "*",
    operation,
    permission,
  };

  const clear = (field: FieldKey) =>
    setFieldError((prev) => (prev?.field === field ? null : prev));

  const validate = useCallback((): FieldError | null => {
    const who = principal.trim();
    if (who.length === 0 || who.endsWith(":"))
      return {
        field: "principal",
        message:
          "Name the principal the way Kafka stores it — e.g. User:alice. The prefix is part of it.",
      };
    if (!who.includes(":"))
      return {
        field: "principal",
        message:
          "Kafka wants a type and a name — e.g. User:alice. Without the prefix the rule will never match.",
      };
    if (!isCluster && resourceName.trim().length === 0)
      return {
        field: "resourceName",
        message: `Name the ${resourceWord(resourceType)} this rule is about, or use * for every one of them.`,
      };
    if (host.trim().length === 0)
      return {
        field: "host",
        message: "Use * for any host, or one IP address — Kafka matches it exactly.",
      };
    return null;
  }, [principal, resourceName, host, isCluster, resourceType]);

  const submit = useCallback(async () => {
    const problem = validate();
    if (problem) {
      setFieldError(problem);
      controls.current[problem.field]?.focus();
      return;
    }
    setFieldError(null);
    setFailure(null);
    const binding: AclBinding = {
      resource_type: resourceType,
      resource_name: effectiveName,
      pattern_type: effectivePattern,
      principal: principal.trim(),
      host: host.trim(),
      operation,
      permission,
    };
    setBusy(true);
    try {
      await aclsCreate(profile.id, [binding]);
      onCreated(binding);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [
    validate,
    resourceType,
    effectiveName,
    effectivePattern,
    principal,
    host,
    operation,
    permission,
    profile.id,
    onCreated,
  ]);

  const message = (field: FieldKey) =>
    fieldError?.field === field ? (
      <span className="field-error" id={`acl-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;
  const invalid = (field: FieldKey) =>
    fieldError?.field === field ? true : undefined;
  const cls = (field: FieldKey, base = "") =>
    `${base}${fieldError?.field === field ? " input-invalid" : ""}`.trim() ||
    undefined;
  const describe = (field: FieldKey, hintId?: string) =>
    [hintId, fieldError?.field === field ? `acl-${field}-error` : null]
      .filter(Boolean)
      .join(" ") || undefined;

  const classified = failure === null ? null : classifyError(failure);

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="acl-new-title"
      initialFocus={principalRef}
      onClose={onClose}
    >
      {/* ⏎ submits: adding a rule is not destructive, so the key the user is
          already pressing is allowed to finish the job (§5.8). */}
      <form
        className="modal-panel"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <h2 className="modal-title" id="acl-new-title">
          Add an access rule
        </h2>

        {isProtected && (
          <div className="banner banner-warn" role="note">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                This changes who can do what on {profile.name}, a production
                cluster.
              </p>
              <p className="banner-detail">
                Kafka enforces it from the moment it is written — there is no
                staging step and no dry run.
              </p>
            </div>
          </div>
        )}

        {/* THE SENTENCE. It updates live as the controls change, and it is the
            loudest prose in the modal — the same device the offset reset modal
            uses, for the same reason: it is the entire feature's UX. */}
        <p className="reset-preview" aria-live="polite">
          {aclSentence(draft)}
        </p>

        <div className="field">
          <label className="field-label" htmlFor="acl-principal">
            Who does this apply to?
          </label>
          <input
            id="acl-principal"
            ref={bind("principal")}
            type="text"
            className={cls("principal", "input-mono")}
            value={principal}
            placeholder="User:alice"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("principal")}
            aria-describedby={describe("principal", "acl-principal-hint")}
            onChange={(e) => {
              setPrincipal(e.target.value);
              clear("principal");
            }}
          />
          {message("principal")}
          <span className="field-hint" id="acl-principal-hint">
            Kafka's principal, prefix and all. With TLS client certificates it is
            the certificate's subject (<code>User:CN=svc-orders,OU=…</code>);
            with SASL it is the username. <code>User:*</code> means everyone.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="acl-permission">
            Allow or deny?
          </label>
          <select
            id="acl-permission"
            value={permission}
            onChange={(e) => setPermission(e.target.value as AclPermission)}
          >
            <option value="allow">Allow — grant this</option>
            <option value="deny">Deny — refuse this</option>
          </select>
          <span className="field-hint">
            Deny wins: a matching deny rule refuses the request even when
            another rule allows it. Use it to carve one exception out of a
            broader allow, not as the everyday tool.
          </span>
        </div>

        {permission === "deny" && (
          <div className="banner banner-warn" role="note">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                A deny rule outranks every allow that matches the same request.
              </p>
              <p className="banner-detail">
                It takes effect immediately, and applications that were working a
                second ago will start failing with authorization errors. Removing
                it later widens access again.
              </p>
            </div>
          </div>
        )}

        <div className="field">
          <label className="field-label" htmlFor="acl-operation">
            What can they do?
          </label>
          <select
            id="acl-operation"
            value={operation}
            onChange={(e) => setOperation(e.target.value as AclOperation)}
          >
            {ACL_OPERATIONS.map((op) => (
              <option key={op} value={op}>
                {op}
              </option>
            ))}
          </select>
          <span className="field-hint">
            {OPERATION_HINT[operation] ?? "Kafka's own name for the operation."}
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="acl-restype">
            On what?
          </label>
          <select
            id="acl-restype"
            value={resourceType}
            onChange={(e) => {
              setResourceType(e.target.value as AclResourceType);
              setFieldError(null);
            }}
          >
            {RESOURCE_TYPES.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
              </option>
            ))}
          </select>
          <span className="field-hint">
            Reading a topic normally needs two rules: Read on the topic, and Read
            on the consumer group the application uses.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="acl-resname">
            {isCluster ? "Resource name" : `${resourceWord(resourceType)} name`}
          </label>
          <input
            id="acl-resname"
            ref={bind("resourceName")}
            type="text"
            className={cls("resourceName", "input-mono")}
            value={isCluster ? "kafka-cluster" : resourceName}
            placeholder="orders.v2"
            autoComplete="off"
            spellCheck={false}
            disabled={isCluster}
            aria-invalid={invalid("resourceName")}
            aria-describedby={describe("resourceName", "acl-resname-hint")}
            title={
              isCluster
                ? "The cluster has exactly one name in Kafka's model, and this is it."
                : undefined
            }
            onChange={(e) => {
              setResourceName(e.target.value);
              clear("resourceName");
            }}
          />
          {message("resourceName")}
          <span className="field-hint" id="acl-resname-hint">
            {isCluster
              ? "Kafka's own literal for the cluster resource. It isn't a name you pick."
              : "Exactly one name, or * for every one of them."}
          </span>
        </div>

        {!isCluster && (
          <div className="field">
            <span className="field-label">How does the name match?</span>
            <div className="env-picker" role="radiogroup" aria-label="Name matching">
              <button
                type="button"
                role="radio"
                aria-checked={patternType === "literal"}
                tabIndex={patternType === "literal" ? 0 : -1}
                ref={(el) => {
                  patternRefs.current.literal = el;
                }}
                className={`env-option ${
                  patternType === "literal" ? "env-option-active" : ""
                }`}
                onClick={() => setPatternType("literal")}
                onKeyDown={onPatternKeyDown}
              >
                Exactly
              </button>
              <button
                type="button"
                role="radio"
                aria-checked={patternType === "prefixed"}
                tabIndex={patternType === "prefixed" ? 0 : -1}
                ref={(el) => {
                  patternRefs.current.prefixed = el;
                }}
                className={`env-option ${
                  patternType === "prefixed" ? "env-option-active" : ""
                }`}
                onClick={() => setPatternType("prefixed")}
                onKeyDown={onPatternKeyDown}
              >
                Starts with
              </button>
            </div>
            <span className="field-hint">
              “Starts with” covers every {resourceWord(resourceType)} created
              later whose name begins the same way — which is the point, and the
              risk.
            </span>
          </div>
        )}

        <div className="field">
          <label className="field-label" htmlFor="acl-host">
            From where?
          </label>
          <input
            id="acl-host"
            ref={bind("host")}
            type="text"
            className={cls("host", "input-mono")}
            value={host}
            placeholder="*"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("host")}
            aria-describedby={describe("host", "acl-host-hint")}
            onChange={(e) => {
              setHost(e.target.value);
              clear("host");
            }}
          />
          {message("host")}
          <span className="field-hint" id="acl-host-hint">
            <code>*</code> for any host. Kafka matches a single IP address
            literally — it does not understand ranges or hostnames here.
          </span>
        </div>

        {classified !== null && (
          <div className="banner banner-danger" role="alert">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">{classified.title}</p>
              <p className="banner-detail">{classified.detail}</p>
              <details className="banner-details">
                <summary>Show details</summary>
                <pre className="banner-raw">{failure}</pre>
              </details>
            </div>
          </div>
        )}

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            onClick={onClose}
            disabled={busy}
            title={busy ? "Kavka is writing the rule" : undefined}
          >
            Cancel
          </button>
          <button
            type="submit"
            className={`btn ${isProtected ? "btn-danger" : "btn-primary"} btn-swap`}
            disabled={busy}
            aria-busy={busy || undefined}
            title={busy ? "Kavka is writing the rule" : undefined}
          >
            <span className="btn-swap-face">
              Add rule
            </span>
            <span className="btn-swap-face btn-swap-busy">
              <span className="spinner" aria-hidden="true" />
              Add rule
            </span>
          </button>
        </div>
      </form>
    </Overlay>
  );
}
