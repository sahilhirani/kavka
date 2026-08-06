import { useCallback, useMemo, useRef, useState } from "react";
import Overlay from "./Overlay";
import { Term } from "./Glossary";
import { errorMessage, topicCreate, type TopicConfigInput } from "./api";
import { classifyError } from "./errors";

/** Kafka's own rule: letters, digits, dot, underscore, hyphen; 249 max. */
const LEGAL_NAME = /^[A-Za-z0-9._-]+$/;

type FieldKey = "name" | "partitions" | "replication";

interface FieldError {
  field: FieldKey;
  message: string;
}

interface ConfigRow {
  /** Stable across reorders so React never reuses one row's input for another. */
  id: number;
  name: string;
  value: string;
}

interface CreateTopicModalProps {
  profileId: string;
  /**
   * A PROTECTED environment renders the write action as danger-outlined
   * (§6 layer 7). Passed in rather than resolved here: the caller already
   * holds the profile, and this modal takes an id.
   */
  isProtected: boolean;
  clusterName: string;
  /** How many brokers there are — an RF above this can never be placed. */
  brokerCount: number;
  /** Names already on the cluster, so a collision is caught before the broker. */
  existing: string[];
  onCreated: (name: string) => void;
  onClose: () => void;
}

let nextRowId = 1;

export default function CreateTopicModal({
  profileId,
  isProtected,
  clusterName,
  brokerCount,
  existing,
  onCreated,
  onClose,
}: CreateTopicModalProps) {
  const [name, setName] = useState("");
  const [partitions, setPartitions] = useState("1");
  const [replication, setReplication] = useState(
    String(Math.min(3, Math.max(1, brokerCount))),
  );
  const [configs, setConfigs] = useState<ConfigRow[]>([]);
  const [fieldError, setFieldError] = useState<FieldError | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const nameRef = useRef<HTMLInputElement | null>(null);
  const controls = useRef<Partial<Record<FieldKey, HTMLElement | null>>>({});
  // Stable per-field ref callbacks: a fresh closure each render would detach
  // and reattach every control on every keystroke.
  const bind = useMemo(() => {
    const cache: Partial<Record<FieldKey, (el: HTMLElement | null) => void>> =
      {};
    return (field: FieldKey) =>
      (cache[field] ??= (el: HTMLElement | null) => {
        controls.current[field] = el;
        // Overlay wants a RefObject for initial focus; the name field is it.
        if (field === "name") nameRef.current = el as HTMLInputElement | null;
      });
  }, []);

  /** Editing a field clears its own message. Never adds one mid-keystroke. */
  const clear = (field: FieldKey) =>
    setFieldError((prev) => (prev?.field === field ? null : prev));

  const validate = useCallback((): FieldError | null => {
    const trimmed = name.trim();
    if (trimmed.length === 0)
      return { field: "name", message: "Give the topic a name — e.g. orders.v2" };
    if (trimmed === "." || trimmed === "..")
      return {
        field: "name",
        message: "Kafka won't accept “.” or “..” as a topic name. Pick another.",
      };
    if (!LEGAL_NAME.test(trimmed))
      return {
        field: "name",
        message:
          "Use letters, digits, dots, underscores or hyphens — e.g. orders.v2",
      };
    if (trimmed.length > 249)
      return {
        field: "name",
        message: "Kafka caps topic names at 249 characters. Shorten it.",
      };
    if (existing.includes(trimmed))
      return {
        field: "name",
        message: "This cluster already has a topic with that name.",
      };

    const p = Number.parseInt(partitions, 10);
    if (!Number.isFinite(p) || p < 1)
      return {
        field: "partitions",
        message: "A topic needs at least one partition.",
      };
    if (p > 10_000)
      return {
        field: "partitions",
        message:
          "That is more partitions than a cluster this size will place. Try a few hundred at most.",
      };

    const rf = Number.parseInt(replication, 10);
    if (!Number.isFinite(rf) || rf < 1)
      return {
        field: "replication",
        message: "Each partition needs at least one copy.",
      };
    if (brokerCount > 0 && rf > brokerCount)
      return {
        field: "replication",
        message: `This cluster has ${brokerCount} broker${
          brokerCount === 1 ? "" : "s"
        }, so it can't keep ${rf} copies of a partition. Use ${brokerCount} or fewer.`,
      };
    return null;
  }, [name, partitions, replication, existing, brokerCount]);

  const submit = useCallback(async () => {
    const problem = validate();
    if (problem) {
      setFieldError(problem);
      controls.current[problem.field]?.focus();
      return;
    }
    setFieldError(null);
    setFailure(null);
    const clean: TopicConfigInput[] = configs
      .map((row) => ({ name: row.name.trim(), value: row.value.trim() }))
      .filter((row) => row.name.length > 0);
    setBusy(true);
    try {
      await topicCreate(
        profileId,
        name.trim(),
        Number.parseInt(partitions, 10),
        Number.parseInt(replication, 10),
        clean,
      );
      onCreated(name.trim());
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [
    validate,
    configs,
    profileId,
    name,
    partitions,
    replication,
    onCreated,
  ]);

  const message = (field: FieldKey) =>
    fieldError?.field === field ? (
      <span className="field-error" id={`ct-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;
  const invalid = (field: FieldKey) =>
    fieldError?.field === field ? true : undefined;
  const cls = (field: FieldKey, base = "") =>
    `${base}${fieldError?.field === field ? " input-invalid" : ""}`.trim() ||
    undefined;
  const describe = (field: FieldKey, hintId?: string) =>
    [hintId, fieldError?.field === field ? `ct-${field}-error` : null]
      .filter(Boolean)
      .join(" ") || undefined;

  const classified = failure === null ? null : classifyError(failure);

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="ct-title"
      initialFocus={nameRef}
      onClose={onClose}
    >
      {/* ⏎ submits: creating a topic is not destructive, so the key a user is
          already pressing is allowed to finish the job (§5.8). */}
      <form
        className="modal-panel"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <h2 className="modal-title" id="ct-title">
          Create a topic
        </h2>

        {isProtected && (
          <div className="banner banner-warn" role="note">
            <span className="banner-glyph" aria-hidden="true">
              !
            </span>
            <div className="banner-body">
              <p className="banner-title">
                This creates a topic on {clusterName}, a production cluster.
              </p>
              <p className="banner-detail">
                Nothing here is destructive, but the topic and everything
                written to it will be real.
              </p>
            </div>
          </div>
        )}

        <div className="field">
          <label className="field-label" htmlFor="ct-name">
            Topic name
          </label>
          <input
            id="ct-name"
            ref={bind("name")}
            type="text"
            className={cls("name", "input-mono")}
            value={name}
            placeholder="orders.v2"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={invalid("name")}
            aria-describedby={describe("name", "ct-name-hint")}
            onChange={(e) => {
              setName(e.target.value);
              clear("name");
            }}
          />
          {message("name")}
          <span className="field-hint" id="ct-name-hint">
            Letters, digits, dots, underscores and hyphens. Kafka treats
            <code> a.b </code> and <code> a_b </code> as colliding names, so
            pick one separator and keep to it.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="ct-partitions">
            <Term name="partition">Partitions</Term>
          </label>
          <input
            id="ct-partitions"
            ref={bind("partitions")}
            type="number"
            min={1}
            step={1}
            className={cls("partitions", "input-num")}
            value={partitions}
            aria-invalid={invalid("partitions")}
            aria-describedby={describe("partitions", "ct-partitions-hint")}
            onChange={(e) => {
              setPartitions(e.target.value);
              clear("partitions");
            }}
          />
          {message("partitions")}
          <span className="field-hint" id="ct-partitions-hint">
            How much of this topic can be read in parallel. You can add
            partitions later, but never remove them — and adding them changes
            which partition a key lands on.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="ct-replication">
            <Term name="replication-factor">Replication factor</Term>
          </label>
          <input
            id="ct-replication"
            ref={bind("replication")}
            type="number"
            min={1}
            step={1}
            className={cls("replication", "input-num")}
            value={replication}
            aria-invalid={invalid("replication")}
            aria-describedby={describe("replication", "ct-replication-hint")}
            onChange={(e) => {
              setReplication(e.target.value);
              clear("replication");
            }}
          />
          {message("replication")}
          <span className="field-hint" id="ct-replication-hint">
            How many brokers keep a copy. This cluster has {brokerCount} broker
            {brokerCount === 1 ? "" : "s"}, so that is the ceiling.
          </span>
        </div>

        <fieldset className="fieldset">
          <legend className="eyebrow">Configuration</legend>
          <span className="field-hint">
            Anything left out uses the broker's default — <Term name="retention">
              retention
            </Term>{" "}
            and cleanup policy are the two worth setting up front.
          </span>

          {configs.map((row, index) => (
            <div className="config-row" key={row.id}>
              <input
                type="text"
                className="input-mono"
                value={row.name}
                placeholder="retention.ms"
                autoComplete="off"
                spellCheck={false}
                aria-label={`Setting ${index + 1} name`}
                onChange={(e) =>
                  setConfigs((prev) =>
                    prev.map((r) =>
                      r.id === row.id ? { ...r, name: e.target.value } : r,
                    ),
                  )
                }
              />
              <input
                type="text"
                className="input-mono"
                value={row.value}
                placeholder="604800000"
                autoComplete="off"
                spellCheck={false}
                aria-label={`Setting ${index + 1} value`}
                onChange={(e) =>
                  setConfigs((prev) =>
                    prev.map((r) =>
                      r.id === row.id ? { ...r, value: e.target.value } : r,
                    ),
                  )
                }
              />
              <button
                type="button"
                className="btn btn-ghost"
                title="Remove this setting"
                onClick={() =>
                  setConfigs((prev) => prev.filter((r) => r.id !== row.id))
                }
              >
                Remove
              </button>
            </div>
          ))}

          <button
            type="button"
            className="btn"
            onClick={() =>
              setConfigs((prev) => [
                ...prev,
                { id: nextRowId++, name: "", value: "" },
              ])
            }
          >
            Add a setting
          </button>
        </fieldset>

        {/* A failed create belongs beside the form whose fields have to
            change, not in the workspace banner behind the modal. */}
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
            title={busy ? "Kavka is creating the topic" : undefined}
          >
            Cancel
          </button>
          <button
            type="submit"
            className={`btn ${isProtected ? "btn-danger" : "btn-primary"} btn-swap`}
            disabled={busy}
            aria-busy={busy || undefined}
            title={busy ? "Kavka is creating the topic" : undefined}
          >
            <span className="btn-swap-face">
              Create topic
            </span>
            <span className="btn-swap-face btn-swap-busy">
              <span className="spinner" aria-hidden="true" />
              Create topic
            </span>
          </button>
        </div>
      </form>
    </Overlay>
  );
}
