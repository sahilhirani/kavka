import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  brokerConfigSet,
  brokerConfigs,
  errorMessage,
  type BrokerInfo,
  type ConfigEntry,
  type ConnectionProfile,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { Term } from "./Glossary";
import { ErrorBanner } from "./ProfileEditor";
import { ToastStack, useToasts } from "./Toast";

/**
 * BROKERS, AND THE SETTINGS YOU CAN ACTUALLY CHANGE FROM HERE.
 *
 * The config table is Phase 1's topic-config table with one column added: the
 * `+` gutter means "this broker overrides the default", the same device and
 * the same washes, because a setting is a setting and learning it twice is
 * learning it wrong.
 *
 * What is new is the honesty about WHICH keys are editable. Kafka reports a
 * config as read-only when it cannot be changed at runtime — it came from the
 * broker's properties file, or it is a computed default — and those rows keep
 * their Edit button, disabled, with the reason on hover. Hiding them would
 * produce the "where did the setting go" ticket; showing them enabled would
 * produce a refusal from the broker after the user has already typed a value.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/**
 * Clearing the box is the one edit this table refuses, and the reason is the
 * gap between what it looks like and what it does: an empty input sends
 * `SET ""`, which is a real override whose value is the empty string — not the
 * absence of one. The confirm preview then renders `→ <code></code>`, so the
 * modal that exists to say what is about to change says nothing at all, one
 * click from a live broker. The row already carries the control that means
 * "stop overriding this", so the refusal names it rather than only saying no.
 */
const EMPTY_DRAFT_WHY =
  "Give this setting a value. To go back to the broker default, use Revert instead — an empty value is an override of its own, not the absence of one.";

/** Kafka's config sources, in the user's vocabulary. Raw stays on screen. */
const SOURCE_WORD: Record<string, string> = {
  DYNAMIC_BROKER_CONFIG: "Set on this broker while it was running.",
  DYNAMIC_DEFAULT_BROKER_CONFIG:
    "Set on every broker in the cluster while they were running.",
  STATIC_BROKER_CONFIG:
    "Set in this broker's properties file. It changes on restart, not from here.",
  DEFAULT_CONFIG: "Kafka's own default — nothing has overridden it.",
  DYNAMIC_TOPIC_CONFIG: "Set on the topic.",
  UNKNOWN_CONFIG: "Kafka didn't say where this value came from.",
};

function sourceWord(source: string): string {
  return SOURCE_WORD[source] ?? `Kafka reports the source as ${source}.`;
}

interface BrokersTabProps {
  profile: ConnectionProfile;
  brokers: BrokerInfo[];
  /** null = the broker list; a number = that broker's settings. */
  brokerId: number | null;
  onSelectBroker: (brokerId: number | null) => void;
  onDanger: DangerReport;
}

/** What a confirmed change is about to do. Both halves, so the modal can say. */
interface PendingChange {
  entry: ConfigEntry;
  /** null = revert to the default; a string = set it to this. */
  next: string | null;
}

export default function BrokersTab({
  profile,
  brokers,
  brokerId,
  onSelectBroker,
  onDanger,
}: BrokersTabProps) {
  const [configs, setConfigs] = useState<ConfigEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showDefaults, setShowDefaults] = useState(false);

  // Edit-in-place: which key is open, and what has been typed into it.
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState<PendingChange | null>(null);
  const [busy, setBusy] = useState(false);

  const toaster = useToasts();
  const push = toaster.push;
  const seq = useRef(0);

  useDangerSignal(error !== null, onDanger);

  const isProd = profile.environment === "prod";
  const readOnly = profile.read_only;

  const fetchConfigs = useCallback(
    async (id: number) => {
      const mine = ++seq.current;
      setLoading(true);
      try {
        const list = await brokerConfigs(profile.id, id);
        if (seq.current !== mine) return;
        setConfigs(list);
        setFailed(false);
      } catch (err) {
        if (seq.current !== mine) return;
        setFailed(true);
        setError(errorMessage(err));
      } finally {
        if (seq.current === mine) setLoading(false);
      }
    },
    [profile.id],
  );

  useEffect(() => {
    setEditing(null);
    if (brokerId === null) {
      setConfigs(null);
      return;
    }
    setConfigs(null);
    void fetchConfigs(brokerId);
    return () => {
      seq.current += 1;
    };
  }, [brokerId, fetchConfigs]);

  const overrides = useMemo(
    () => configs?.filter((c) => !c.is_default) ?? [],
    [configs],
  );
  const shown = configs === null ? [] : showDefaults ? configs : overrides;
  const hiddenDefaults = (configs?.length ?? 0) - overrides.length;

  const applyChange = useCallback(async () => {
    if (pending === null || brokerId === null) return;
    setBusy(true);
    try {
      await brokerConfigSet(
        profile.id,
        brokerId,
        pending.entry.name,
        pending.next,
      );
      const reverted = pending.next === null;
      setPending(null);
      setEditing(null);
      push({
        kind: "ok",
        title: reverted
          ? `Reverted ${pending.entry.name} on broker ${brokerId}`
          : `Changed ${pending.entry.name} on broker ${brokerId}`,
        detail: reverted
          ? "The broker is back on whatever it computes as the default for this key."
          : `Now ${pending.next}`,
        mono: !reverted,
      });
      await fetchConfigs(brokerId);
    } catch (err) {
      setPending(null);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [pending, brokerId, profile.id, push, fetchConfigs]);

  // ── The broker list ────────────────────────────────────────────────────

  if (brokerId === null) {
    return (
      <>
        {error !== null && (
          <ErrorBanner raw={error} onDismiss={() => setError(null)} />
        )}

        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              <Term name="broker">Brokers</Term>
              <span className="panel-count">{brokers.length}</span>
            </h2>
          </div>

          <p className="table-note">
            Open a broker to see every setting it is running with, and change the
            ones Kafka accepts at runtime.
          </p>

          {/* The gutter carries the broker id — the row's address in Kafka's
              own vocabulary (§2). Static table, so no role="grid". */}
          <div className="table-wrap">
            <table className="data-table">
              <caption className="sr-only">Brokers in this cluster</caption>
              <thead>
                <tr>
                  <th scope="col" className="ledger-gutter">
                    ID
                  </th>
                  <th scope="col">Host</th>
                  <th scope="col" className="col-num">
                    Port
                  </th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Open</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {brokers.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="cell-empty">
                      This cluster reported no brokers. That normally means the
                      connection is up but metadata came back empty — try
                      reconnecting.
                    </td>
                  </tr>
                ) : (
                  brokers.map((b) => (
                    <tr
                      key={b.id}
                      className="row-click"
                      tabIndex={0}
                      onClick={() => onSelectBroker(b.id)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          onSelectBroker(b.id);
                        }
                      }}
                    >
                      <td className="ledger-gutter">{b.id}</td>
                      <td className="cell-mono">{b.host}</td>
                      <td className="col-num cell-num">{b.port}</td>
                      <td className="col-affordance">
                        <span className="row-affordance">View settings →</span>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>

        <ToastStack {...toaster} />
      </>
    );
  }

  // ── One broker's settings ──────────────────────────────────────────────

  const broker = brokers.find((b) => b.id === brokerId) ?? null;

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <button
              type="button"
              className="btn btn-ghost crumb-btn"
              onClick={() => onSelectBroker(null)}
            >
              ← Brokers
            </button>
            <span className="topic-name">
              {broker ? `${broker.host}:${broker.port}` : `broker ${brokerId}`}
            </span>
            <span className="cell-tag">broker {brokerId}</span>
            {configs !== null && (
              <span className="panel-count">
                {overrides.length} set here
                {!showDefaults && hiddenDefaults > 0
                  ? ` · ${hiddenDefaults} Kafka defaults hidden`
                  : ""}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <label className="check-row">
              <input
                type="checkbox"
                checked={showDefaults}
                onChange={(e) => setShowDefaults(e.target.checked)}
              />
              <span>Show Kafka defaults</span>
            </label>
            <button
              type="button"
              className="btn"
              disabled={loading}
              aria-busy={loading || undefined}
              title={
                loading ? "Kavka is already reading this broker's settings" : undefined
              }
              onClick={() => void fetchConfigs(brokerId)}
            >
              Refresh
            </button>
          </div>
        </div>

        {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {configs === null ? (
          <p className="table-note">
            {loading
              ? "Asking the broker for its settings…"
              : failed
                ? "Kavka couldn't read this broker's settings. The account needs DescribeConfigs on the cluster — ask whoever issued the credentials for that permission."
                : "No settings to show."}
          </p>
        ) : (
          <>
            <div className="stat-grid">
              <div className="stat">
                <span className="stat-label">Set on this broker</span>
                <span className="stat-value">{overrides.length}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Settings in total</span>
                <span className="stat-value">{configs.length}</span>
              </div>
            </div>

            <p className="table-note">
              A <code>+</code> in the gutter means this broker overrides Kafka's
              default. Everything else is whatever the broker computes right now
              — which can change under you when the cluster does.
            </p>

            <div className="table-wrap">
              {loading && <div className="table-loading" role="presentation" />}
              <table className="data-table data-table-sign">
                <caption className="sr-only">
                  Settings of broker {brokerId}, overrides marked
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="ledger-gutter">
                      <span className="sr-only">Overridden</span>
                    </th>
                    <th scope="col">Setting</th>
                    <th scope="col">Value</th>
                    <th scope="col">Source</th>
                    <th scope="col" className="col-affordance">
                      <span className="sr-only">Change</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {shown.length === 0 ? (
                    <tr>
                      <td colSpan={5} className="cell-empty">
                        This broker changes nothing from Kafka's defaults. Turn
                        on “Show Kafka defaults” to see what it is running with.
                      </td>
                    </tr>
                  ) : (
                    shown.map((entry) => (
                      <ConfigRow
                        key={entry.name}
                        entry={entry}
                        readOnly={readOnly}
                        editing={editing === entry.name}
                        draft={draft}
                        onDraft={setDraft}
                        onEdit={() => {
                          setEditing(entry.name);
                          setDraft(entry.value ?? "");
                        }}
                        onCancel={() => setEditing(null)}
                        onSave={() =>
                          setPending({ entry, next: draft })
                        }
                        onRevert={() => setPending({ entry, next: null })}
                      />
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </>
        )}
      </section>

      {pending !== null && (
        <ConfirmModal
          // A broker setting change is a write, not a delete: the red wire is
          // reserved for prod, where §6 layer 7 says every write reads as one.
          tone={isProd ? "destructive" : "plain"}
          title={
            pending.next === null
              ? `Revert ${pending.entry.name}?`
              : `Change ${pending.entry.name} on broker ${brokerId}?`
          }
          body={
            <>
              <p className="reset-preview">
                {pending.entry.is_sensitive ? (
                  <>
                    The broker withholds this value, so Kavka can't show what it
                    is now — only what it is about to become:{" "}
                    <code>{pending.next ?? "the default"}</code>
                  </>
                ) : (
                  <>
                    <code>{pending.entry.value ?? "∅"}</code> →{" "}
                    <code>{pending.next ?? "Kafka's default"}</code>
                  </>
                )}
              </p>
              {pending.next === null ? (
                <>
                  Kafka drops this broker's override and goes back to whatever it
                  computes as the default. Kavka can't show you that value in
                  advance — the broker only reports it once the override is gone.
                </>
              ) : (
                <>
                  This takes effect on broker {brokerId} immediately, without a
                  restart, and only on this broker — the rest of the cluster
                  keeps whatever it had.
                </>
              )}
              {isProd && (
                <>
                  {" "}
                  {profile.name} is a production cluster, so this is a live
                  change to a running broker.
                </>
              )}
            </>
          }
          confirmLabel={
            pending.next === null ? "Revert to default" : "Change setting"
          }
          typeToConfirm={isProd ? pending.entry.name : null}
          typePrompt={
            isProd ? (
              <>
                Type <code>{pending.entry.name}</code> to confirm you are
                changing this on {profile.name}
              </>
            ) : undefined
          }
          busy={busy}
          busyLabel="Kavka is sending the change to the broker"
          onCancel={() => setPending(null)}
          onConfirm={() => void applyChange()}
        />
      )}

      <ToastStack {...toaster} />
    </>
  );
}

/**
 * One setting. Editable in place when Kafka says it can be changed at runtime;
 * disabled with the reason when it can't — never hidden, never a dead control.
 */
function ConfigRow({
  entry,
  readOnly,
  editing,
  draft,
  onDraft,
  onEdit,
  onCancel,
  onSave,
  onRevert,
}: {
  entry: ConfigEntry;
  readOnly: boolean;
  editing: boolean;
  draft: string;
  onDraft: (value: string) => void;
  onEdit: () => void;
  onCancel: () => void;
  onSave: () => void;
  onRevert: () => void;
}) {
  const staticConfig = entry.source === "STATIC_BROKER_CONFIG";
  const why = readOnly
    ? READ_ONLY_WHY
    : entry.is_read_only
      ? `Kafka reports this one as read-only for clients: ${sourceWord(entry.source)} Changing it means editing the broker's properties file and restarting it.`
      : undefined;

  // The refusal lives on the control, never in the global banner (§5.3): it is
  // a condition this input is in, not one the system is in.
  const [refused, setRefused] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const errorId = `broker-config-${entry.name}-error`;

  /**
   * Both doors into the confirmation — the button and ⏎ — go through here, so
   * an empty draft cannot get past one of them by using the other.
   */
  const attemptSave = () => {
    if (draft.trim().length === 0) {
      setRefused(true);
      inputRef.current?.focus();
      return;
    }
    setRefused(false);
    onSave();
  };

  /** Editing a field clears its message; nothing ever adds one mid-keystroke. */
  const edit = (value: string) => {
    setRefused(false);
    onDraft(value);
  };

  const leaveEdit = () => {
    setRefused(false);
    onCancel();
  };

  return (
    <tr className={entry.is_default ? undefined : "diff-add"}>
      <td className="ledger-gutter" aria-hidden="true">
        {entry.is_default ? "" : "+"}
      </td>
      <td className="cell-mono">
        {entry.name}
        {entry.is_read_only && (
          <span
            className="cell-tag"
            title={
              staticConfig
                ? "It comes from the broker's properties file, so it changes on restart."
                : "The broker won't accept a change to this one from a client."
            }
          >
            {" "}
            read-only
          </span>
        )}
      </td>
      <td
        className={`cell-mono config-value${
          editing ? " config-value-editing" : ""
        }`}
      >
        {editing ? (
          <span className="config-edit-wrap">
            <input
              ref={inputRef}
              type="text"
              className={`input-mono config-edit${refused ? " input-invalid" : ""}`}
              value={draft}
              autoFocus
              autoComplete="off"
              spellCheck={false}
              aria-label={`New value for ${entry.name}`}
              aria-invalid={refused || undefined}
              aria-describedby={refused ? errorId : undefined}
              onChange={(e) => edit(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  leaveEdit();
                }
                if (e.key === "Enter") {
                  e.preventDefault();
                  attemptSave();
                }
              }}
            />
            {refused && (
              <span className="field-error" id={errorId}>
                {EMPTY_DRAFT_WHY}
              </span>
            )}
          </span>
        ) : entry.is_sensitive ? (
          <span
            className="absent"
            title="Kafka withholds this value — it is marked sensitive, so no client ever reads it back."
          >
            —<span className="sr-only">withheld by the broker</span>
          </span>
        ) : entry.value === null ? (
          <span className="absent">∅</span>
        ) : (
          <span title={entry.value}>{entry.value}</span>
        )}
      </td>
      <td
        className="cell-tag config-source"
        title={sourceWord(entry.source)}
      >
        {entry.source}
      </td>
      <td className="col-affordance config-actions">
        {editing ? (
          <>
            <button
              type="button"
              className="btn btn-row"
              disabled={draft === (entry.value ?? "")}
              title={
                draft === (entry.value ?? "")
                  ? "This is what the setting is already"
                  : "See what will change, then confirm"
              }
              onClick={attemptSave}
            >
              Review change
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-row"
              onClick={leaveEdit}
            >
              Cancel
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              className="btn btn-row"
              disabled={readOnly || entry.is_read_only}
              title={why ?? "Change this setting on this broker"}
              onClick={() => {
                setRefused(false);
                onEdit();
              }}
            >
              Edit
            </button>
            {!entry.is_default && !entry.is_read_only && (
              <button
                type="button"
                className="btn btn-ghost btn-row"
                disabled={readOnly}
                title={
                  readOnly
                    ? READ_ONLY_WHY
                    : "Drop this override and go back to Kafka's default"
                }
                onClick={onRevert}
              >
                Revert
              </button>
            )}
          </>
        )}
      </td>
    </tr>
  );
}
