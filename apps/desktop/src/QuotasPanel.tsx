import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  quotasAlter,
  quotasList,
  type ConnectionProfile,
  type QuotaEntity,
  type QuotaEntityPart,
  type QuotaEntityType,
  type QuotaOp,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { classifyError } from "./errors";
import { formatBytes, groupDigits } from "./format";
import Overlay from "./Overlay";
import { ErrorBanner } from "./ProfileEditor";
import type { ToastSpec } from "./Toast";

/**
 * QUOTAS — the ceiling Kafka puts on one client.
 *
 * The feature is small and the vocabulary is awful: `producer_byte_rate` on
 * `(user=alice, client-id=<default>)` is four ideas in one string, and none of
 * them says what actually happens to alice's application when it goes over.
 * So every number on this screen is followed by the sentence it means —
 * "may write 1 MB a second" — and the empty state explains the whole feature,
 * because a cluster with no quotas is exactly where someone is standing when
 * they first need one.
 *
 * The one Kafka rule worth stating out loud, and stated on screen: quotas do
 * not reject anything. The broker delays its answers until the client is back
 * inside the limit, so a throttled application gets slower, never an error.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/** Kafka's own literal for "everything of this type without its own quota". */
const DEFAULT_WORD = "default";

interface QuotaKeyMeta {
  key: string;
  /** Sentence case, per §5.3. The Kafka key is never hidden — it is the code. */
  label: string;
  unit: string;
  /** The plain-language meaning of a value. This is the whole feature. */
  means: (value: number) => string;
  hint: string;
}

const QUOTA_KEYS: ReadonlyArray<QuotaKeyMeta> = [
  {
    key: "producer_byte_rate",
    label: "Write rate",
    unit: "bytes a second",
    means: (v) => `may write ${formatBytes(v)} a second`,
    hint: "How fast anything matching this entity may produce. Over the limit, the broker holds its acknowledgements back until the client slows down — it never refuses a record.",
  },
  {
    key: "consumer_byte_rate",
    label: "Read rate",
    unit: "bytes a second",
    means: (v) => `may read ${formatBytes(v)} a second`,
    hint: "How fast it may consume. Same mechanism: the broker delays fetch responses rather than failing them, so a throttled consumer looks slow, not broken.",
  },
  {
    key: "request_percentage",
    label: "Share of a broker's time",
    unit: "percent",
    means: (v) =>
      `may use ${v}% of one broker's request-handling time${
        v >= 100 ? ` — that is ${(v / 100).toFixed(v % 100 === 0 ? 0 : 1)} full threads` : ""
      }`,
    hint: "A percentage of ONE request-handler thread, so 100 is one whole thread and 200 is two. This is the quota that catches an application making millions of tiny requests rather than a few big ones.",
  },
  {
    key: "controller_mutation_rate",
    label: "Topic and partition changes",
    unit: "partitions a second",
    means: (v) =>
      `may create or delete about ${v} partition${v === 1 ? "" : "s"} a second, averaged over time`,
    hint: "Creating and deleting topics is work for the controller, and a loop that creates topics can take a cluster down on its own. Kafka averages this over a window, so short bursts are fine.",
  },
];

function keyMeta(key: string): QuotaKeyMeta | null {
  return QUOTA_KEYS.find((k) => k.key === key) ?? null;
}

/**
 * A quota is a double on the wire, and most of them are whole numbers — but
 * not all: `request_percentage` is routinely 12.5, and a rate can be a
 * fraction of one. `groupDigits` truncates, which is exactly right for an
 * offset and silently wrong for a quota, so a fractional value is shown
 * verbatim rather than rounded into a different limit than the one in force.
 */
function quotaNumber(value: number): string {
  return Number.isInteger(value) ? groupDigits(value) : String(value);
}

/** Kafka's entity type in the user's vocabulary. Unknown types pass through. */
function typeWord(type: string): string {
  if (type === "user") return "user";
  if (type === "client-id") return "client id";
  if (type === "ip") return "IP address";
  return type;
}

/** The heading a group of entities sits under. */
function groupTitle(types: string[]): string {
  const key = [...types].sort().join("+");
  if (key === "user") return "Users";
  if (key === "client-id") return "Client ids";
  if (key === "ip") return "IP addresses";
  if (key === "client-id+user") return "One user with one client id";
  return types.map(typeWord).join(" and ");
}

/** What that whole group of quotas actually catches, in one sentence. */
function groupNote(types: string[]): string {
  const key = [...types].sort().join("+");
  if (key === "user")
    return "Applies to whoever signs in as that user, whatever application they are running and however many connections they open.";
  if (key === "client-id")
    return "Applies to every connection that sets that client.id, whoever signed in. Applications choose their own client.id, so this is a limit they can step around by changing one setting — useful for keeping a well-behaved app honest, not for containing a hostile one.";
  if (key === "ip")
    return "Limits how fast new connections may be made from that address. It is the only quota here that isn't about throughput — it is the one that stops a reconnect loop.";
  if (key === "client-id+user")
    return "The most specific quota Kafka has: it applies only when that user connects with that client id. Kafka picks the most specific match it can find and uses only that one — quotas are never added together.";
  return "Kafka reports this combination of entity types.";
}

/** Stable identity for an entity, and its React key. Order-independent. */
function entityKey(parts: QuotaEntityPart[]): string {
  return [...parts]
    .map((p) => `${p.entity_type}=${p.name ?? "<default>"}`)
    .sort()
    .join("|");
}

/** `user alice`, or the default entity said out loud. */
function entitySentence(parts: QuotaEntityPart[]): string {
  return parts
    .map((p) =>
      p.name === null
        ? `every ${typeWord(p.entity_type)} without a quota of its own`
        : `${typeWord(p.entity_type)} ${p.name}`,
    )
    .join(", and ");
}

/** What the user types in a prod confirmation for this entity. */
function confirmWord(parts: QuotaEntityPart[]): string {
  return parts[0]?.name ?? DEFAULT_WORD;
}

/**
 * A cluster whose brokers are too old to answer DescribeClientQuotas says so
 * with a protocol version number, which tells a novice nothing they can use.
 */
function noQuotasNote(raw: string): string | null {
  const text = raw.toLowerCase();
  return text.includes("unsupported_version") ||
    text.includes("unsupported version") ||
    text.includes("describeclientquotas")
    ? "These brokers are too old to describe quotas over the protocol — the API arrived in Kafka 2.6. Quotas set on this cluster still work; Kavka just can't read or write them from here."
    : null;
}

interface QuotasPanelProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
  /** One toast stack per screen, so the owner's is passed in rather than made. */
  push: (spec: ToastSpec) => void;
}

/** What an editor is open for: a brand new entity, or one that exists. */
type EditorTarget = { kind: "new" } | { kind: "edit"; entity: QuotaEntity };

export default function QuotasPanel({
  profile,
  onDanger,
  push,
}: QuotasPanelProps) {
  const [entities, setEntities] = useState<QuotaEntity[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorTarget | null>(null);
  const [removing, setRemoving] = useState<QuotaEntity | null>(null);
  const [busy, setBusy] = useState(false);
  const seq = useRef(0);

  const note = failed && error !== null ? noQuotasNote(error) : null;
  useDangerSignal(error !== null && note === null, onDanger);

  const isProd = profile.environment === "prod";
  const readOnly = profile.read_only;

  const fetchQuotas = useCallback(async () => {
    const mine = ++seq.current;
    setLoading(true);
    try {
      const list = await quotasList(profile.id);
      if (seq.current !== mine) return;
      setEntities(list);
      setFailed(false);
      setError(null);
    } catch (err) {
      if (seq.current !== mine) return;
      setFailed(true);
      setError(errorMessage(err));
    } finally {
      if (seq.current === mine) setLoading(false);
    }
  }, [profile.id]);

  useEffect(() => {
    void fetchQuotas();
    return () => {
      seq.current += 1;
    };
  }, [fetchQuotas]);

  /**
   * Grouped by which entity TYPES a quota is attached to, because that is the
   * thing that decides what it catches — and defaults sort first inside each
   * group, since they are the rule everything else is an exception to.
   */
  const groups = useMemo(() => {
    const by = new Map<string, { types: string[]; items: QuotaEntity[] }>();
    for (const entity of entities ?? []) {
      const types = entity.entity.map((p) => p.entity_type);
      const key = [...types].sort().join("+");
      const bucket = by.get(key);
      if (bucket) bucket.items.push(entity);
      else by.set(key, { types, items: [entity] });
    }
    for (const bucket of by.values()) {
      bucket.items.sort((a, b) => {
        const an = a.entity[0]?.name;
        const bn = b.entity[0]?.name;
        if (an === null && bn !== null) return -1;
        if (bn === null && an !== null) return 1;
        return (an ?? "").localeCompare(bn ?? "");
      });
    }
    return [...by.values()].sort((a, b) =>
      groupTitle(a.types).localeCompare(groupTitle(b.types)),
    );
  }, [entities]);

  const total = entities?.length ?? 0;

  const removeAll = useCallback(async () => {
    if (removing === null) return;
    setBusy(true);
    try {
      const ops: QuotaOp[] = removing.values.map((v) => ({
        key: v.key,
        value: null,
      }));
      await quotasAlter(profile.id, removing.entity, ops);
      const said = entitySentence(removing.entity);
      setRemoving(null);
      push({
        kind: "ok",
        title: `Removed every quota on ${said}`,
        detail:
          "Nothing throttles it now, unless a less specific quota still applies — a default one, say.",
      });
      await fetchQuotas();
    } catch (err) {
      setRemoving(null);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [removing, profile.id, push, fetchQuotas]);

  return (
    <>
      {error !== null && note === null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Quotas
            {entities !== null && <span className="panel-count">{total}</span>}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={loading}
              aria-busy={loading || undefined}
              title={
                loading
                  ? "Kavka is already asking the cluster for quotas"
                  : "Read the cluster's quotas again"
              }
              onClick={() => void fetchQuotas()}
            >
              Refresh
            </button>
            {/* A write action renders danger-outlined on prod even when it is
                routine (§6 layer 7). */}
            <button
              type="button"
              className={`btn ${isProd ? "btn-danger" : ""}`}
              disabled={readOnly || failed}
              title={
                readOnly
                  ? READ_ONLY_WHY
                  : failed
                    ? "Kavka couldn't read this cluster's quotas, so it won't write one either"
                    : "Put a ceiling on one user, client id or address"
              }
              onClick={() => setEditor({ kind: "new" })}
            >
              Add a quota
            </button>
          </div>
        </div>

        {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {entities === null ? (
          <p className="table-note">
            {loading
              ? "Asking the cluster for its quotas…"
              : (note ??
                "Kavka couldn't read this cluster's quotas. Describing them needs DescribeConfigs on the cluster — ask whoever issued the credentials for that permission.")}
          </p>
        ) : total === 0 ? (
          <EmptyQuotas
            readOnly={readOnly}
            onCreate={() => setEditor({ kind: "new" })}
          />
        ) : (
          <>
            <p className="table-note">
              A quota is a ceiling, not a gate: over the limit Kafka delays its
              answers until the client is back inside it, so a throttled
              application gets slower and never sees an error. When more than
              one quota could apply, Kafka uses the most specific one — it does
              not add them together.
            </p>

            {groups.map((group) => (
              <div className="quota-group" key={group.types.join("+")}>
                <h3 className="quota-group-title">
                  {groupTitle(group.types)}
                  <span className="panel-count">{group.items.length}</span>
                </h3>
                <p className="table-note">{groupNote(group.types)}</p>

                {group.items.map((entity) => (
                  <EntityBlock
                    key={entityKey(entity.entity)}
                    entity={entity}
                    readOnly={readOnly}
                    isProd={isProd}
                    onEdit={() => setEditor({ kind: "edit", entity })}
                    onRemove={() => setRemoving(entity)}
                  />
                ))}
              </div>
            ))}
          </>
        )}
      </section>

      {editor !== null && (
        <QuotaEditor
          profile={profile}
          target={editor}
          existing={entities ?? []}
          onClose={() => setEditor(null)}
          onSaved={(said, count) => {
            setEditor(null);
            push({
              kind: "ok",
              title: `Set ${count} quota${count === 1 ? "" : "s"} on ${said}`,
              detail:
                "The brokers apply it to connections that are already open, not just new ones.",
            });
            void fetchQuotas();
          }}
        />
      )}

      {removing !== null && (
        <ConfirmModal
          title={
            isProd
              ? `Remove every quota on ${confirmWord(removing.entity)} on ${profile.name}?`
              : `Remove every quota on ${confirmWord(removing.entity)}?`
          }
          body={
            <>
              <p className="reset-preview">
                {entitySentence(removing.entity)} — {removing.values.length}{" "}
                quota
                {removing.values.length === 1 ? "" : "s"} removed.
              </p>
              Nothing throttles it after this, unless a less specific quota
              still catches it — the default for its type, if there is one. That
              can be the difference between one application slowing down and one
              application taking the cluster with it.
            </>
          }
          confirmLabel="Remove quotas"
          // Environment-gated, not action-gated: prod always asks, dev never
          // does (§6 layer 4).
          typeToConfirm={isProd ? confirmWord(removing.entity) : null}
          busy={busy}
          busyLabel="Kavka is removing the quotas"
          onCancel={() => setRemoving(null)}
          onConfirm={() => void removeAll()}
        />
      )}
    </>
  );
}

/** The empty state has to teach the whole feature — see §7's empty states. */
function EmptyQuotas({
  readOnly,
  onCreate,
}: {
  readOnly: boolean;
  onCreate: () => void;
}) {
  return (
    <>
      <p className="table-note">
        This cluster has no quotas, so nothing is throttled: every client may
        produce and consume as fast as the brokers will let it.
      </p>
      <p className="table-note">
        A quota is a ceiling Kafka applies to one <em>user</em>, one{" "}
        <em>client id</em> or one <em>IP address</em>. Over the limit the broker
        holds its answers back until the client slows down — it never refuses a
        record, so the application gets slower rather than broken. This is the
        usual way one runaway job stops taking the whole cluster with it.
      </p>
      <div className="empty-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={readOnly}
          title={readOnly ? READ_ONLY_WHY : undefined}
          onClick={onCreate}
        >
          Add a quota
        </button>
      </div>
    </>
  );
}

/**
 * One entity and everything set on it. Grouping is whitespace plus a top
 * hairline — Kavka ships zero cards (law 1) — and each value carries the
 * sentence it means beside the exact number it came from.
 */
function EntityBlock({
  entity,
  readOnly,
  isProd,
  onEdit,
  onRemove,
}: {
  entity: QuotaEntity;
  readOnly: boolean;
  isProd: boolean;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const primary = entity.entity[0];
  const isDefault = primary?.name === null;
  return (
    <div className="quota-entity">
      <div className="quota-entity-head">
        <span className="quota-entity-name">
          {entity.entity.map((part, i) => (
            <span className="quota-part" key={`${part.entity_type}:${i}`}>
              {i > 0 && <span className="cell-tag"> with </span>}
              {part.name === null ? (
                <span
                  className="absent"
                  title={`Kafka's <default> entity: it applies to every ${typeWord(
                    part.entity_type,
                  )} that has no quota of its own.`}
                >
                  &lt;default for every {typeWord(part.entity_type)}&gt;
                </span>
              ) : (
                <>
                  <span className="cell-tag">{typeWord(part.entity_type)} </span>
                  <span className="cell-mono">{part.name}</span>
                </>
              )}
            </span>
          ))}
        </span>
        <div className="quota-entity-actions">
          <button
            type="button"
            className={`btn btn-row ${isProd ? "btn-danger" : ""}`}
            disabled={readOnly}
            title={
              readOnly
                ? READ_ONLY_WHY
                : `Change what ${entitySentence(entity.entity)} may do`
            }
            onClick={onEdit}
          >
            Edit
          </button>
          {/* Danger OUTLINE: it only ever opens the confirmation. */}
          <button
            type="button"
            className="btn btn-danger btn-row"
            disabled={readOnly}
            title={readOnly ? READ_ONLY_WHY : "Take every quota off this entity"}
            onClick={onRemove}
          >
            Remove
          </button>
        </div>
      </div>

      {isDefault && (
        <p className="quota-default-note">
          This is the fallback. Anything of that type with a quota of its own
          uses that one instead — Kafka never adds the two together.
        </p>
      )}

      {entity.values.length === 0 ? (
        <p className="table-note">
          The cluster lists this entity with no quota keys on it, which normally
          means one was just removed.
        </p>
      ) : (
        <ul className="quota-values">
          {entity.values.map((value) => {
            const meta = keyMeta(value.key);
            return (
              <li className="quota-value" key={value.key}>
                {/* The key is a literal you could paste into kafka-configs.sh,
                    so it is mono. The number beside it is exact — §7 rule 5,
                    tables never round — and the sentence after it rounds,
                    because that one is prose. */}
                <code className="quota-key">{value.key}</code>
                <span className="quota-number">{quotaNumber(value.value)}</span>
                <span className="quota-means">
                  {meta === null
                    ? `Kafka reports this quota as ${value.key}. Kavka doesn't have a plain-language meaning for it, so the number is shown as the cluster gave it.`
                    : meta.means(value.value)}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

// ───────────────────────────────────────────────────────────────────────────
// The editor
// ───────────────────────────────────────────────────────────────────────────

type FieldKey = "name" | "second" | string;

/**
 * ONE MODAL THAT CARRIES ITS OWN PROD CONFIRMATION.
 *
 * Same reason as ReassignModal: opening a ConfirmModal from inside an Overlay
 * is two focus traps fighting over one Tab key, so §5.8's typed gate is a
 * field in this form rather than a second dialog.
 */
function QuotaEditor({
  profile,
  target,
  existing,
  onClose,
  onSaved,
}: {
  profile: ConnectionProfile;
  target: EditorTarget;
  existing: QuotaEntity[];
  onClose: () => void;
  /** `said` is the entity in words, `count` how many keys changed. */
  onSaved: (said: string, count: number) => void;
}) {
  const editing = target.kind === "edit" ? target.entity : null;
  const isProd = profile.environment === "prod";

  const [entityType, setEntityType] = useState<QuotaEntityType>("user");
  const [name, setName] = useState("");
  const [isDefault, setIsDefault] = useState(false);
  const [withClient, setWithClient] = useState(false);
  const [clientName, setClientName] = useState("");
  const [clientDefault, setClientDefault] = useState(false);

  /** Every key the form can write: the four known ones, plus anything the
      cluster already holds on this entity that Kavka has no name for. */
  const keys = useMemo(() => {
    const known = QUOTA_KEYS.map((k) => k.key);
    const extra = (editing?.values ?? [])
      .map((v) => v.key)
      .filter((k) => !known.includes(k));
    return [...known, ...extra];
  }, [editing]);

  const [drafts, setDrafts] = useState<Record<string, string>>(() => {
    const initial: Record<string, string> = {};
    for (const value of editing?.values ?? [])
      initial[value.key] = String(value.value);
    return initial;
  });
  const [typed, setTyped] = useState("");
  const [problem, setProblem] = useState<{ field: FieldKey; message: string } | null>(
    null,
  );
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const nameRef = useRef<HTMLInputElement | null>(null);
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const controls = useRef<Record<string, HTMLElement | null>>({});

  const entityParts = useMemo<QuotaEntityPart[]>(() => {
    if (editing !== null) return editing.entity;
    const parts: QuotaEntityPart[] = [
      {
        entity_type: entityType,
        name: isDefault ? null : name.trim(),
      },
    ];
    if (entityType === "user" && withClient)
      parts.push({
        entity_type: "client-id",
        name: clientDefault ? null : clientName.trim(),
      });
    return parts;
  }, [
    editing,
    entityType,
    name,
    isDefault,
    withClient,
    clientName,
    clientDefault,
  ]);

  const before = useMemo(() => {
    const map = new Map<string, number>();
    for (const value of editing?.values ?? []) map.set(value.key, value.value);
    return map;
  }, [editing]);

  /** What will be sent, and — just as importantly — what will not. */
  const ops = useMemo<QuotaOp[]>(() => {
    const out: QuotaOp[] = [];
    for (const key of keys) {
      const raw = (drafts[key] ?? "").trim();
      const had = before.has(key) ? (before.get(key) as number) : null;
      if (raw.length === 0) {
        if (had !== null) out.push({ key, value: null });
        continue;
      }
      const parsed = Number(raw);
      if (!Number.isFinite(parsed)) continue;
      if (had === null || had !== parsed) out.push({ key, value: parsed });
    }
    return out;
  }, [keys, drafts, before]);

  const said = entitySentence(entityParts);
  const word = confirmWord(entityParts);
  const matches = !isProd || typed === word;

  const validate = useCallback((): { field: FieldKey; message: string } | null => {
    if (editing === null) {
      if (!isDefault && name.trim().length === 0)
        return {
          field: "name",
          message:
            "Name the thing this limits — the username, the client.id, or the IP address. Or tick the box to make it the default for everything of that type.",
        };
      if (
        entityType === "user" &&
        withClient &&
        !clientDefault &&
        clientName.trim().length === 0
      )
        return {
          field: "second",
          message:
            "Name the client.id this applies to, or tick the box to cover every client id that user connects with.",
        };
      const key = entityKey(entityParts);
      if (existing.some((e) => entityKey(e.entity) === key))
        return {
          field: "name",
          message:
            "This cluster already has a quota on that entity. Open it from the list and edit it — adding it again would just overwrite what is there.",
        };
    }
    for (const key of keys) {
      const raw = (drafts[key] ?? "").trim();
      if (raw.length === 0) continue;
      const parsed = Number(raw);
      if (!Number.isFinite(parsed))
        return {
          field: key,
          message: "Use a plain number, e.g. 1048576. No units and no commas.",
        };
      if (parsed < 0)
        return {
          field: key,
          message:
            "A quota can't be negative. Leave the box empty to take the limit off entirely — 0 is a real quota meaning “throttled to a standstill”.",
        };
    }
    if (ops.length === 0)
      return {
        field: keys[0],
        message:
          "Nothing has changed yet. Put a number in one of these, or clear one to take that limit off.",
      };
    return null;
  }, [
    editing,
    isDefault,
    name,
    entityType,
    withClient,
    clientDefault,
    clientName,
    entityParts,
    existing,
    keys,
    drafts,
    ops,
  ]);

  const submit = useCallback(async () => {
    // ⏎ submits this form, which is right for a write that isn't destructive
    // (§5.8) — but it must not walk past the prod gate. The button is already
    // disabled when the typed name doesn't match; this is the other door.
    if (!matches || busy) return;
    const found = validate();
    if (found !== null) {
      setProblem(found);
      controls.current[found.field]?.focus();
      return;
    }
    setProblem(null);
    setFailure(null);
    setBusy(true);
    try {
      await quotasAlter(profile.id, entityParts, ops);
      onSaved(said, ops.length);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [matches, busy, validate, profile.id, entityParts, ops, onSaved, said]);

  const classified = failure === null ? null : classifyError(failure);
  const blocked = busy
    ? "Kavka is writing the quota"
    : !matches
      ? `Type ${word} exactly to confirm this on ${profile.name}`
      : undefined;

  const bind = (field: FieldKey) => (el: HTMLElement | null) => {
    controls.current[field] = el;
    if (field === "name") nameRef.current = el as HTMLInputElement | null;
  };

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="quota-title"
      initialFocus={editing === null ? nameRef : cancelRef}
      onClose={onClose}
    >
      <form
        className="modal-panel"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <h2 className="modal-title" id="quota-title">
          {editing === null ? "Add a quota" : `Change the quota on ${word}`}
        </h2>

        {/* THE SENTENCE, live as the controls change — the device the ACL
            editor and the offset reset modal both use. */}
        <p className="reset-preview" aria-live="polite">
          {ops.length === 0
            ? `Nothing set yet for ${said}.`
            : `${said.charAt(0).toUpperCase()}${said.slice(1)} ${ops
                .map((op) => {
                  const meta = keyMeta(op.key);
                  if (op.value === null) return `no longer has a ${op.key} limit`;
                  return meta === null
                    ? `has ${op.key} set to ${quotaNumber(op.value)}`
                    : meta.means(op.value);
                })
                .join(", and ")}.`}
        </p>

        {isProd && (
          <div className="banner banner-warn" role="note">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                {profile.name} is a production cluster.
              </p>
              <p className="banner-detail">
                The brokers apply this to connections that are already open, not
                only to new ones — an application inside the limit right now can
                start being throttled a second from now.
              </p>
            </div>
          </div>
        )}

        {editing === null ? (
          <>
            <div className="field">
              <label className="field-label" htmlFor="quota-type">
                What is being limited?
              </label>
              <select
                id="quota-type"
                value={entityType}
                onChange={(e) => {
                  setEntityType(e.target.value as QuotaEntityType);
                  setProblem(null);
                }}
              >
                <option value="user">A user — whoever signs in as that name</option>
                <option value="client-id">
                  A client id — whatever sets that client.id
                </option>
                <option value="ip">An IP address — connections from there</option>
              </select>
              <span className="field-hint">
                Kafka's own names for these are <code>user</code>,{" "}
                <code>client-id</code> and <code>ip</code>.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="quota-name">
                {entityType === "user"
                  ? "Which user?"
                  : entityType === "client-id"
                    ? "Which client id?"
                    : "Which address?"}
              </label>
              <input
                id="quota-name"
                ref={bind("name")}
                type="text"
                className={`input-mono${
                  problem?.field === "name" ? " input-invalid" : ""
                }`}
                value={isDefault ? "" : name}
                disabled={isDefault}
                placeholder={
                  entityType === "user"
                    ? "alice"
                    : entityType === "client-id"
                      ? "orders-service"
                      : "10.0.4.19"
                }
                autoComplete="off"
                spellCheck={false}
                aria-invalid={problem?.field === "name" || undefined}
                aria-describedby={
                  problem?.field === "name" ? "quota-name-error" : undefined
                }
                title={
                  isDefault
                    ? "The default entity has no name — that is what makes it the default."
                    : undefined
                }
                onChange={(e) => {
                  setName(e.target.value);
                  setProblem(null);
                }}
              />
              {problem?.field === "name" && (
                <span className="field-error" id="quota-name-error">
                  {problem.message}
                </span>
              )}
              <div className="check-field">
                <input
                  id="quota-default"
                  type="checkbox"
                  checked={isDefault}
                  aria-describedby="quota-default-hint"
                  onChange={(e) => {
                    setIsDefault(e.target.checked);
                    setProblem(null);
                  }}
                />
                <label className="check-label" htmlFor="quota-default">
                  Make this the default for every {typeWord(entityType)}
                </label>
                <span className="field-hint" id="quota-default-hint">
                  Kafka calls it the <code>&lt;default&gt;</code> entity. It
                  catches everything of that type that has no quota of its own —
                  the usual way to give a cluster a floor without naming
                  everybody on it.
                </span>
              </div>
            </div>

            {entityType === "user" && (
              <div className="field">
                <div className="check-field">
                  <input
                    id="quota-with-client"
                    type="checkbox"
                    checked={withClient}
                    aria-describedby="quota-with-client-hint"
                    onChange={(e) => {
                      setWithClient(e.target.checked);
                      setProblem(null);
                    }}
                  />
                  <label className="check-label" htmlFor="quota-with-client">
                    …and only for one client id
                  </label>
                  <span className="field-hint" id="quota-with-client-hint">
                    The most specific quota Kafka has. Use it to hold one of a
                    user's applications down without touching the others.
                  </span>
                </div>
                {withClient && (
                  <>
                    <input
                      ref={bind("second")}
                      type="text"
                      className={`input-mono${
                        problem?.field === "second" ? " input-invalid" : ""
                      }`}
                      value={clientDefault ? "" : clientName}
                      disabled={clientDefault}
                      placeholder="orders-service"
                      autoComplete="off"
                      spellCheck={false}
                      aria-label="Client id"
                      aria-invalid={problem?.field === "second" || undefined}
                      onChange={(e) => {
                        setClientName(e.target.value);
                        setProblem(null);
                      }}
                    />
                    {problem?.field === "second" && (
                      <span className="field-error">{problem.message}</span>
                    )}
                    <div className="check-field">
                      <input
                        id="quota-client-default"
                        type="checkbox"
                        checked={clientDefault}
                        onChange={(e) => {
                          setClientDefault(e.target.checked);
                          setProblem(null);
                        }}
                      />
                      <label className="check-label" htmlFor="quota-client-default">
                        Any client id that user connects with
                      </label>
                    </div>
                  </>
                )}
              </div>
            )}
          </>
        ) : (
          <p className="dialog-note">
            Changing the quota on <strong>{said}</strong>. Clear a box to take
            that limit off entirely; leave one alone and it stays exactly as it
            is.
          </p>
        )}

        {keys.map((key) => {
          const meta = keyMeta(key);
          const raw = (drafts[key] ?? "").trim();
          const parsed = raw.length === 0 ? null : Number(raw);
          const invalid = problem?.field === key;
          return (
            <div className="field" key={key}>
              <label className="field-label" htmlFor={`quota-k-${key}`}>
                {meta?.label ?? key}
                <span className="cell-tag"> {key}</span>
              </label>
              <input
                id={`quota-k-${key}`}
                ref={bind(key)}
                type="text"
                inputMode="decimal"
                className={`input-mono${invalid ? " input-invalid" : ""}`}
                value={drafts[key] ?? ""}
                placeholder={
                  before.has(key) ? undefined : "leave empty for no limit"
                }
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid || undefined}
                onChange={(e) => {
                  setDrafts((prev) => ({ ...prev, [key]: e.target.value }));
                  setProblem(null);
                }}
              />
              {invalid && <span className="field-error">{problem.message}</span>}
              <span className="field-hint">
                {parsed !== null && Number.isFinite(parsed) && meta !== null
                  ? `In ${meta.unit}. At this value, ${said} ${meta.means(parsed)}.`
                  : (meta?.hint ??
                    `Kavka has no plain-language meaning for ${key}; it is written to the cluster exactly as typed.`)}
              </span>
            </div>
          );
        })}

        {isProd && (
          <div className="field confirm-type">
            <label className="field-label" htmlFor="quota-confirm">
              Type <code>{word}</code> to confirm you are changing this on{" "}
              {profile.name}
            </label>
            <input
              id="quota-confirm"
              type="text"
              className="input-mono"
              value={typed}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setTyped(e.target.value)}
            />
          </div>
        )}

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
            ref={cancelRef}
            onClick={onClose}
            disabled={busy}
            title={busy ? "Kavka is writing the quota" : undefined}
          >
            Cancel
          </button>
          <button
            type="submit"
            className={`btn ${isProd ? "btn-danger-confirm" : "btn-primary"}`}
            disabled={busy || !matches}
            aria-busy={busy || undefined}
            title={blocked}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {busy ? <span className="spinner" /> : null}
            </span>
            {editing === null ? "Add quota" : "Change quota"}
          </button>
        </div>
      </form>
    </Overlay>
  );
}
