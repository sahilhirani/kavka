import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  connectApply,
  connectConfig,
  connectDelete,
  connectList,
  connectPause,
  connectRestart,
  connectResume,
  connectValidate,
  errorMessage,
  type ConfigValidation,
  type ConnectClusterConfig,
  type ConnectionProfile,
  type ConnectorSummary,
  type TaskStatus,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { classifyError } from "./errors";
import { ErrorBanner } from "./ProfileEditor";
import { ToastStack, useToasts } from "./Toast";

/**
 * KAFKA CONNECT — three screens, one tab: the connector list, one connector,
 * and the config editor.
 *
 * The editor is a SCREEN and not a modal, deliberately. A JSON config in a
 * 560px dialog is a keyhole, and the validate step needs room to list a
 * plugin's field errors beside the text they are about — but the real reason is
 * that Apply has to be able to raise a confirmation, and a modal inside a modal
 * is two focus traps fighting over the same Tab key.
 *
 * The one thing this view refuses to do is pretend validation is a guarantee.
 * `connect_validate` asks the worker that answered, against the plugin it has
 * loaded, at that moment. It is a snapshot, so a clean result unlocks Apply and
 * a stale or failing one only makes Apply ask first — never blocks it. An
 * expert with a config the worker's config-def doesn't understand must still be
 * able to ship it.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/** The worker's states, in plain words. Every dot has a word (law 2). */
const STATE_WORD: Record<string, string> = {
  RUNNING: "Running",
  PAUSED: "Paused",
  FAILED: "Failed",
  UNASSIGNED: "Not assigned",
  RESTARTING: "Restarting",
  STOPPED: "Stopped",
  DESTROYED: "Destroyed",
};

const STATE_GLOSS: Record<string, string> = {
  RUNNING: "A worker has it and it is moving records right now.",
  PAUSED:
    "Someone paused it. It keeps its config and its offsets, and carries on from where it stopped when you resume it.",
  FAILED:
    "It stopped with an error. The trace is below — a restart is the usual first move once the cause is fixed.",
  UNASSIGNED:
    "No worker has picked it up yet. Usually a rebalance in progress; if it stays this way, the Connect cluster is short of workers.",
  RESTARTING: "A worker is starting it again right now.",
  STOPPED:
    "Stopped: the connector keeps its config and its offsets but has no tasks running.",
  DESTROYED: "The worker no longer has this connector at all.",
};

function stateWord(state: string): string {
  return STATE_WORD[state.toUpperCase()] ?? state;
}

function stateGloss(state: string): string {
  return (
    STATE_GLOSS[state.toUpperCase()] ??
    `The Connect worker reports this as ${state}.`
  );
}

function stateTone(state: string): "ok" | "warn" | "danger" | "quiet" {
  switch (state.toUpperCase()) {
    case "RUNNING":
      return "ok";
    case "FAILED":
      return "danger";
    case "UNASSIGNED":
    case "RESTARTING":
      return "warn";
    default:
      return "quiet";
  }
}

/** Dot plus word plus a sentence on hover and focus. Never colour alone. */
function StateChip({ state }: { state: string }) {
  const tone = stateTone(state);
  return (
    <span
      className={`health health-${tone}`}
      title={stateGloss(state)}
      tabIndex={0}
    >
      <i className="dot" aria-hidden="true" />
      {stateWord(state)}
    </span>
  );
}

function typeWord(type: string): string {
  const t = type.toLowerCase();
  if (t === "source") return "Source — brings data into Kafka";
  if (t === "sink") return "Sink — sends data out of Kafka";
  return type;
}

/**
 * The §7 doctrine applied to a Java stack trace: a plain title, then the next
 * click, then the verbatim trace behind Show details.
 *
 * Deliberately NOT a branch inside errors.ts. That module classifies what a
 * BROKER said, its whole table is written in librdkafka's vocabulary, and it is
 * pure so it can be checked against captured broker strings. A Connect trace is
 * a different language from a different process; giving it its own small table
 * here keeps both honest.
 */
export function summarizeTrace(trace: string): { title: string; detail: string } {
  const first =
    trace
      .split("\n")
      .map((line) => line.trim())
      .find((line) => line.length > 0) ?? "";
  const match = first.match(/^([\w.$]*?([\w$]+(?:Exception|Error|Throwable)))(?::\s*(.*))?$/);
  const short = match ? match[2] : null;
  const message = match ? (match[3] ?? "").trim() : first;
  const titleRaw = short
    ? message.length > 0
      ? `${short}: ${message}`
      : short
    : first;
  const title = titleRaw.length > 160 ? `${titleRaw.slice(0, 157)}…` : titleRaw;

  const has = (...needles: string[]) => {
    const text = trace.toLowerCase();
    return needles.some((n) => text.includes(n.toLowerCase()));
  };

  if (has("tolerance exceeded in error handler"))
    return {
      title: title || "The connector gave up on a record",
      detail:
        "It hit more record-level errors than errors.tolerance allows and stopped. Set errors.tolerance to all and errors.deadletterqueue.topic.name to send the bad records somewhere instead of taking the connector down.",
    };
  if (has("classnotfound", "failed to find any class that implements connector"))
    return {
      title: title || "The worker doesn't have this plugin",
      detail:
        "connector.class has to name a class that is on the worker's plugin path. Check the spelling first — then whether that plugin is installed on the workers, which is not something Kavka can do from here.",
    };
  if (has("unknownhost", "connection refused", "unable to connect", "connect timed out"))
    return {
      title: title || "The worker couldn't reach the other system",
      detail:
        "The address in this connector's config has to be reachable from the WORKER, not from this machine. Check the host and port, then whether the workers sit behind a different network.",
    };
  if (has("authenticationexception", "authorizationexception", "not authorized", "access denied", "401", "403"))
    return {
      title: title || "The other system refused the worker's credentials",
      detail:
        "The credentials live in this connector's config on the worker, not in Kavka. Check the username and secret in the config below.",
    };
  if (has("serializationexception", "dataexception", "converter", "deserializ"))
    return {
      title: title || "A record didn't match the converter",
      detail:
        "key.converter and value.converter have to match what is actually in the topic. A JSON converter pointed at Avro bytes fails on the first record, which is what this usually is.",
    };
  return {
    title: title.length > 0 ? title : "The worker reported a failure",
    detail:
      "Kavka doesn't recognise this one. The worker's full trace is under Show details — the first line names the exception, and the last “Caused by:” in it usually names the real cause.",
  };
}

/** One failure, three layers: plain title → next click → verbatim trace. */
function TraceBlock({ label, trace }: { label: string; trace: string }) {
  const { title, detail } = summarizeTrace(trace);
  return (
    <div className="trace-item">
      <p className="trace-title">
        {label} — {title}
      </p>
      <p className="trace-detail">{detail}</p>
      <details className="banner-details">
        <summary>Show details</summary>
        <pre className="banner-raw">{trace}</pre>
      </details>
    </div>
  );
}

function countTasks(tasks: TaskStatus[]) {
  let running = 0;
  let failed = 0;
  let paused = 0;
  for (const t of tasks) {
    const s = t.state.toUpperCase();
    if (s === "RUNNING") running += 1;
    else if (s === "FAILED") failed += 1;
    else if (s === "PAUSED") paused += 1;
  }
  return { running, failed, paused, total: tasks.length };
}

/** What a confirmation is about to do. `task` is set only for a task restart. */
interface PendingAction {
  kind: "pause" | "resume" | "restart" | "delete" | "task";
  name: string;
  task?: number;
}

interface ConnectTabProps {
  profile: ConnectionProfile;
  /** Which Connect cluster is selected; null = whichever comes first. */
  cluster: string | null;
  connector: string | null;
  onSelectCluster: (name: string) => void;
  onSelectConnector: (name: string | null) => void;
  onDanger: DangerReport;
  /** Opens this connection's settings, which means disconnecting first. */
  onEditConnection: () => void;
}

export default function ConnectTab({
  profile,
  cluster,
  connector,
  onSelectCluster,
  onSelectConnector,
  onDanger,
  onEditConnection,
}: ConnectTabProps) {
  const clusters = useMemo<ConnectClusterConfig[]>(
    () => profile.connect_clusters ?? [],
    [profile.connect_clusters],
  );
  // A persisted selection can name a cluster that has since been removed from
  // the profile, so the list is the authority and the persisted name is a hint.
  const active =
    clusters.find((c) => c.name === cluster) ?? clusters[0] ?? null;
  const activeName = active?.name ?? null;

  const [connectors, setConnectors] = useState<ConnectorSummary[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [busy, setBusy] = useState(false);
  /** null = not editing; { name: null } = a new connector. */
  const [editing, setEditing] = useState<{ name: string | null } | null>(null);

  const toaster = useToasts();
  const push = toaster.push;
  const seq = useRef(0);

  useDangerSignal(error !== null, onDanger);

  const isProd = profile.environment === "prod";
  const readOnly = profile.read_only;

  const fetchConnectors = useCallback(
    async (name: string) => {
      const mine = ++seq.current;
      setLoading(true);
      try {
        const list = await connectList(profile.id, name);
        if (seq.current !== mine) return;
        setConnectors(list);
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
    if (activeName === null) return;
    setConnectors(null);
    setEditing(null);
    void fetchConnectors(activeName);
    return () => {
      seq.current += 1;
    };
  }, [activeName, fetchConnectors]);

  const runAction = useCallback(
    async (action: PendingAction) => {
      if (activeName === null) return;
      setBusy(true);
      try {
        switch (action.kind) {
          case "pause":
            await connectPause(profile.id, activeName, action.name);
            push({
              kind: "ok",
              title: `Paused ${action.name}`,
              detail:
                "Its tasks stop moving records. Nothing is lost — it resumes from its committed offsets.",
            });
            break;
          case "resume":
            await connectResume(profile.id, activeName, action.name);
            push({
              kind: "ok",
              title: `Resumed ${action.name}`,
              detail: "The workers pick it up again within a few seconds.",
            });
            break;
          case "restart":
            await connectRestart(profile.id, activeName, action.name, null, true);
            push({
              kind: "ok",
              title: `Restarted ${action.name} and its tasks`,
              detail:
                "Coming back can take a few seconds — refresh to see where it got to.",
            });
            break;
          case "task":
            await connectRestart(
              profile.id,
              activeName,
              action.name,
              action.task ?? 0,
              false,
            );
            push({
              kind: "ok",
              title: `Restarted task ${action.task} of ${action.name}`,
              detail:
                "Coming back can take a few seconds — refresh to see where it got to.",
            });
            break;
          case "delete":
            await connectDelete(profile.id, activeName, action.name);
            push({
              kind: "ok",
              title: `Deleted ${action.name}`,
              detail: `Removed from ${activeName}. Its offsets stay in Connect's own topics.`,
            });
            onSelectConnector(null);
            break;
        }
        setPending(null);
        await fetchConnectors(activeName);
      } catch (err) {
        setPending(null);
        setError(errorMessage(err));
      } finally {
        setBusy(false);
      }
    },
    [activeName, profile.id, push, fetchConnectors, onSelectConnector],
  );

  /** Prod asks before every write; dev only asks before the irreversible ones. */
  const ask = useCallback(
    (action: PendingAction) => {
      if (action.kind === "task" && !isProd) {
        void runAction(action);
        return;
      }
      setPending(action);
    },
    [isProd, runAction],
  );

  // ── No Connect on this connection ──────────────────────────────────────

  if (clusters.length === 0) {
    return (
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Kafka Connect</h2>
        </div>
        <p className="table-note">
          This connection has no Connect clusters yet. Kafka Connect is a
          separate set of workers with their own REST address — Kavka drives them
          over that address rather than through the brokers, so it has to be told
          where they are.
        </p>
        <p className="table-note">
          Add one under <em>Kafka Connect clusters</em> in this connection's
          settings — a name you'll recognise and the workers' URL, usually
          something like <code>http://connect-1.internal:8083</code>.
        </p>
        <div className="empty-actions">
          <button type="button" className="btn" onClick={onEditConnection}>
            Open connection settings
          </button>
        </div>
        <p className="table-note">
          Opening the settings disconnects this cluster — Kavka reconnects when
          you press Connect again.
        </p>
      </section>
    );
  }

  const current =
    connector === null
      ? null
      : (connectors?.find((c) => c.name === connector) ?? null);

  const clusterPicker = (
    <>
      {clusters.length > 1 && (
        <select
          className="connect-picker"
          value={activeName ?? ""}
          aria-label="Connect cluster"
          onChange={(e) => {
            onSelectConnector(null);
            onSelectCluster(e.target.value);
          }}
        >
          {clusters.map((c) => (
            <option key={c.name} value={c.name}>
              {c.name}
            </option>
          ))}
        </select>
      )}
      <button
        type="button"
        className="btn"
        disabled={loading || activeName === null}
        aria-busy={loading || undefined}
        title={loading ? "Kavka is already asking the workers" : undefined}
        onClick={() => activeName !== null && void fetchConnectors(activeName)}
      >
        Refresh
      </button>
    </>
  );

  // ── The config editor ──────────────────────────────────────────────────

  if (editing !== null && activeName !== null) {
    return (
      <>
        {error !== null && (
          <ErrorBanner raw={error} onDismiss={() => setError(null)} />
        )}
        <ConnectorEditor
          profile={profile}
          cluster={activeName}
          name={editing.name}
          existing={connectors?.map((c) => c.name) ?? []}
          onClose={() => setEditing(null)}
          onApplied={(name) => {
            setEditing(null);
            push({
              kind: "ok",
              title: `Applied ${name}`,
              detail: `${activeName} has the new config. The workers restart the connector's tasks to pick it up.`,
            });
            onSelectConnector(name);
            void fetchConnectors(activeName);
          }}
        />
        <ToastStack {...toaster} />
      </>
    );
  }

  // ── One connector ──────────────────────────────────────────────────────

  if (connector !== null) {
    const tasks = current?.tasks ?? [];
    const counts = countTasks(tasks);
    const failedTasks = tasks.filter((t) => t.state.toUpperCase() === "FAILED");
    const paused = (current?.connector_state ?? "").toUpperCase() === "PAUSED";

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
                onClick={() => onSelectConnector(null)}
              >
                ← Connectors
              </button>
              <span className="topic-name">{connector}</span>
              {current !== null && <StateChip state={current.connector_state} />}
            </h2>
            <div className="panel-tools">
              <button
                type="button"
                className={`btn ${isProd ? "btn-danger" : ""}`}
                disabled={readOnly || current === null}
                title={
                  readOnly
                    ? READ_ONLY_WHY
                    : current === null
                      ? "Kavka is still reading this connector"
                      : paused
                        ? "Start moving records again"
                        : "Stop moving records, keeping the config and the offsets"
                }
                onClick={() =>
                  ask({ kind: paused ? "resume" : "pause", name: connector })
                }
              >
                {paused ? "Resume" : "Pause"}
              </button>
              <button
                type="button"
                className={`btn ${isProd ? "btn-danger" : ""}`}
                disabled={readOnly || current === null}
                title={
                  readOnly ? READ_ONLY_WHY : "Restart the connector and every task"
                }
                onClick={() => ask({ kind: "restart", name: connector })}
              >
                Restart
              </button>
              <button
                type="button"
                className="btn"
                disabled={readOnly}
                title={readOnly ? READ_ONLY_WHY : "Read and change this connector's config"}
                onClick={() => setEditing({ name: connector })}
              >
                Edit config
              </button>
              {/* Danger OUTLINE: it only ever opens the confirmation. */}
              <button
                type="button"
                className="btn btn-danger"
                disabled={readOnly}
                title={readOnly ? READ_ONLY_WHY : undefined}
                onClick={() => ask({ kind: "delete", name: connector })}
              >
                Delete
              </button>
            </div>
          </div>

          {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

          {current === null ? (
            <p className="table-note">
              {loading
                ? "Asking the workers about this connector…"
                : `${activeName} doesn't have a connector called ${connector} any more. It may have been deleted from another window.`}
            </p>
          ) : (
            <div className="stat-grid">
              <div className="stat">
                <span className="stat-label">Tasks</span>
                <span className="stat-value">{counts.total}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Running</span>
                <span className="stat-value">{counts.running}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Failed</span>
                <span className="stat-value">{counts.failed}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Kind</span>
                <span className="stat-value stat-value-mono">
                  {current.connector_type}
                </span>
              </div>
            </div>
          )}

          {current !== null && (
            <p className="table-note">
              {typeWord(current.connector_type)}. The worker holding it is{" "}
              <code>{current.worker_id}</code>.
            </p>
          )}
        </section>

        {current !== null && (
          <section className="panel">
            <div className="panel-head">
              <h2 className="panel-title">
                Tasks
                <span className="panel-count">
                  {counts.running} of {counts.total} running
                  {counts.failed > 0 ? ` · ${counts.failed} failed` : ""}
                </span>
              </h2>
            </div>

            {tasks.length === 0 ? (
              <p className="table-note">
                This connector has no tasks right now. A connector with{" "}
                <code>tasks.max</code> above zero and no tasks is usually one the
                workers haven't assigned yet — or one that failed before it could
                create any.
              </p>
            ) : (
              <div className="table-wrap">
                {/* The gutter carries the task id — the row's address in the
                    worker's own vocabulary. */}
                <table className="data-table">
                  <caption className="sr-only">Tasks of {connector}</caption>
                  <thead>
                    <tr>
                      <th scope="col" className="ledger-gutter">
                        Task
                      </th>
                      <th scope="col">State</th>
                      <th scope="col">Worker</th>
                      <th scope="col" className="col-affordance">
                        <span className="sr-only">Restart</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {tasks.map((task) => (
                      <tr key={task.id}>
                        <td className="ledger-gutter">{task.id}</td>
                        <td>
                          <StateChip state={task.state} />
                        </td>
                        <td className="cell-mono">{task.worker_id}</td>
                        <td className="col-affordance">
                          <button
                            type="button"
                            className={`btn btn-row ${isProd ? "btn-danger" : ""}`}
                            disabled={readOnly || busy}
                            title={
                              readOnly
                                ? READ_ONLY_WHY
                                : "Restart just this task — the others keep running"
                            }
                            onClick={() =>
                              ask({ kind: "task", name: connector, task: task.id })
                            }
                          >
                            Restart
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        )}

        {current !== null &&
          (failedTasks.length > 0 ||
            current.connector_state.toUpperCase() === "FAILED") && (
            <section className="panel">
              <div className="panel-head">
                <h2 className="panel-title">
                  What failed
                  {failedTasks.length > 0 && (
                    <span className="panel-count">
                      {failedTasks.length} task
                      {failedTasks.length === 1 ? "" : "s"}
                    </span>
                  )}
                </h2>
              </div>

              {failedTasks.length === 0 ? (
                <p className="table-note">
                  The workers report the connector itself as failed, with no
                  failed task to point at — which normally means it couldn't
                  start at all. Restarting it is the usual first move; the
                  worker's own log has the trace this view doesn't get.
                </p>
              ) : (
                failedTasks.map((task) =>
                  task.trace === null ? (
                    <p className="table-note" key={task.id}>
                      Task {task.id} is failed, and the worker didn't send a
                      trace with it. The worker's log has it.
                    </p>
                  ) : (
                    <TraceBlock
                      key={task.id}
                      label={`Task ${task.id}`}
                      trace={task.trace}
                    />
                  ),
                )
              )}
            </section>
          )}

        {pending !== null && (
          <ActionConfirm
            action={pending}
            clusterName={activeName ?? ""}
            profileName={profile.name}
            isProd={isProd}
            taskCount={counts.total}
            busy={busy}
            onCancel={() => setPending(null)}
            onConfirm={() => void runAction(pending)}
          />
        )}

        <ToastStack {...toaster} />
      </>
    );
  }

  // ── The connector list ─────────────────────────────────────────────────

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Connectors
            {connectors !== null && (
              <span className="panel-count">{connectors.length}</span>
            )}
          </h2>
          <div className="panel-tools">
            {clusterPicker}
            <button
              type="button"
              className={`btn ${isProd ? "btn-danger" : ""}`}
              disabled={readOnly}
              title={readOnly ? READ_ONLY_WHY : "Write a new connector config"}
              onClick={() => setEditing({ name: null })}
            >
              New connector
            </button>
          </div>
        </div>

        {/* Which workers these connectors belong to — the Connect equivalent of
            prod guardrail layer 3: the address is always on screen. */}
        {active !== null && (
          <span className="view-address">
            {active.name} · {active.url}
          </span>
        )}

        {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {connectors === null && loading ? (
          <>
            <div className="table-note">Asking the workers for connectors…</div>
            <div className="skeleton-table" aria-hidden="true">
              {[36, 48, 30].map((w, i) => (
                <div className="skeleton-row" key={i}>
                  <div className="skeleton-cell" style={{ width: `${w}%` }} />
                  <div className="skeleton-cell" style={{ width: "64px" }} />
                </div>
              ))}
            </div>
          </>
        ) : connectors === null || failed ? (
          <p className="table-note">
            Kavka couldn't reach {active?.url ?? "the Connect workers"}. Check
            the address in this connection's settings, and whether the workers'
            REST port is reachable from this machine — it is a different port
            from the brokers', usually 8083.
          </p>
        ) : connectors.length === 0 ? (
          <>
            <p className="table-note">
              No connectors on {activeName}. A connector is a running job the
              workers own: a <em>source</em> brings data into Kafka, a{" "}
              <em>sink</em> sends it out. Its config is a flat set of keys, and
              the workers keep it — Kavka just writes it.
            </p>
            <div className="empty-actions">
              <button
                type="button"
                className="btn btn-primary"
                disabled={readOnly}
                title={readOnly ? READ_ONLY_WHY : undefined}
                onClick={() => setEditing({ name: null })}
              >
                New connector
              </button>
            </div>
          </>
        ) : (
          <div className="table-wrap">
            {loading && <div className="table-loading" role="presentation" />}
            {/* A connector has no address of its own, so the gutter is zero and
                the rule sits flush at the table's left edge (§2). */}
            <table className="data-table data-table-flush">
              <caption className="sr-only">Connectors on {activeName}</caption>
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col">State</th>
                  <th scope="col">Kind</th>
                  <th scope="col">Tasks</th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Open</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {connectors.map((c) => {
                  const counts = countTasks(c.tasks);
                  return (
                    <tr
                      key={c.name}
                      className="row-click"
                      onClick={() => onSelectConnector(c.name)}
                    >
                      <td className="cell-mono">{c.name}</td>
                      <td>
                        <StateChip state={c.connector_state} />
                      </td>
                      <td className="cell-tag" title={typeWord(c.connector_type)}>
                        {c.connector_type}
                      </td>
                      <td>
                        {/* A count Kavka computed, so sans — and the failure is
                            a word, not a shade of the number. */}
                        <span className="task-count">
                          {counts.running} of {counts.total} running
                        </span>
                        {counts.failed > 0 && (
                          <span className="task-failed">
                            {" "}
                            · {counts.failed} failed
                          </span>
                        )}
                        {counts.paused > 0 && (
                          <span className="cell-tag"> · {counts.paused} paused</span>
                        )}
                      </td>
                      {/* The row's keyboard path, and the only element here
                          with a role that says it opens something (SC
                          4.1.2). */}
                      <td className="col-affordance">
                        <button
                          type="button"
                          className="row-affordance"
                          aria-label={`View tasks for ${c.name}`}
                          // The row is still clickable for the pointer;
                          // without this the handler runs twice per click.
                          onClick={(e) => {
                            e.stopPropagation();
                            onSelectConnector(c.name);
                          }}
                        >
                          View tasks →
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {pending !== null && (
        <ActionConfirm
          action={pending}
          clusterName={activeName ?? ""}
          profileName={profile.name}
          isProd={isProd}
          taskCount={0}
          busy={busy}
          onCancel={() => setPending(null)}
          onConfirm={() => void runAction(pending)}
        />
      )}

      <ToastStack {...toaster} />
    </>
  );
}

/**
 * One confirmation for five actions, so the blast radius of each is written
 * down exactly once. Only `delete` is destructive; the rest get the plain tone,
 * because a red wire over "Pause" is the boy who cried wolf (§5.5).
 */
function ActionConfirm({
  action,
  clusterName,
  profileName,
  isProd,
  taskCount,
  busy,
  onCancel,
  onConfirm,
}: {
  action: PendingAction;
  clusterName: string;
  profileName: string;
  isProd: boolean;
  taskCount: number;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const onCluster = isProd ? ` on ${profileName}` : "";
  const spec: Record<
    PendingAction["kind"],
    { title: string; body: React.ReactNode; label: string; typed: boolean }
  > = {
    pause: {
      title: `Pause ${action.name}${onCluster}?`,
      body: (
        <>
          Its {taskCount} task{taskCount === 1 ? "" : "s"} stop moving records.
          The connector keeps its config and its committed offsets, and carries
          on from exactly where it stopped when you resume it — nothing is lost,
          but nothing flows in the meantime either.
        </>
      ),
      label: "Pause connector",
      typed: isProd,
    },
    resume: {
      title: `Resume ${action.name}?`,
      body: (
        <>
          The workers pick it up again and carry on from its committed offsets.
          Anything that piled up while it was paused is processed now.
        </>
      ),
      label: "Resume connector",
      typed: false,
    },
    restart: {
      title: `Restart ${action.name} and its tasks${onCluster}?`,
      body: (
        <>
          The workers stop and start the connector and all {taskCount} of its
          tasks. Records in flight are re-attempted from the last committed
          offset, so a sink can write a few of them twice — which matters if
          whatever it writes to isn't idempotent.
        </>
      ),
      label: "Restart connector",
      typed: isProd,
    },
    task: {
      title: `Restart task ${action.task} of ${action.name}${onCluster}?`,
      body: (
        <>
          Only this task stops and starts; the others keep running. It resumes
          from its last committed offset, so a few records may be processed
          twice.
        </>
      ),
      label: "Restart task",
      typed: isProd,
    },
    delete: {
      title: isProd
        ? `Delete ${action.name} on ${profileName}?`
        : `Delete ${action.name}?`,
      body: (
        <>
          This removes the connector and its config from {clusterName}. Connect
          keeps its offsets in its own topics, so re-creating it with the same
          name usually carries on where it left off — but the config itself is
          gone, and Kavka has no copy of it.
        </>
      ),
      label: "Delete connector",
      typed: isProd,
    },
  };
  const it = spec[action.kind];
  return (
    <ConfirmModal
      tone={action.kind === "delete" ? "destructive" : "plain"}
      title={it.title}
      body={it.body}
      confirmLabel={it.label}
      typeToConfirm={it.typed ? action.name : null}
      busy={busy}
      busyLabel="Kavka is talking to the workers"
      onCancel={onCancel}
      onConfirm={onConfirm}
    />
  );
}

// ───────────────────────────────────────────────────────────────────────────
// The config editor
// ───────────────────────────────────────────────────────────────────────────

const NEW_TEMPLATE = `{
  "connector.class": "",
  "tasks.max": "1",
  "topics": ""
}`;

type ParsedConfig =
  | { ok: true; config: Record<string, string> }
  | { ok: false; message: string; index: number | null };

/**
 * Connect's config is a flat map of strings. JSON numbers and booleans are
 * accepted and stringified — every example on the internet writes
 * `"tasks.max": 1` — but a nested object is refused by NAME rather than by a
 * parser error four screens away from the key that caused it.
 */
function parseConfig(text: string): ParsedConfig {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const at = message.match(/position (\d+)/);
    return {
      ok: false,
      message,
      index: at ? Number.parseInt(at[1], 10) : null,
    };
  }
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return {
      ok: false,
      message:
        "A connector config is a JSON object of keys and values — it has to start with { and end with }.",
      index: null,
    };
  const config: Record<string, string> = {};
  for (const [key, raw] of Object.entries(value as Record<string, unknown>)) {
    if (typeof raw === "string") config[key] = raw;
    else if (typeof raw === "number" || typeof raw === "boolean")
      config[key] = String(raw);
    else if (raw === null)
      return {
        ok: false,
        message: `"${key}" has no value. Connect stores every setting as text, so use "" for an empty one.`,
        index: null,
      };
    else
      return {
        ok: false,
        message: `"${key}" is a nested object or list. Connect configs are flat — write it as a single string, e.g. "a,b,c".`,
        index: null,
      };
  }
  return { ok: true, config };
}

/** 1-based line of a character offset, for a parser message that gives one. */
function lineOf(text: string, index: number): number {
  let line = 1;
  for (let i = 0; i < index && i < text.length; i += 1) {
    if (text[i] === "\n") line += 1;
  }
  return line;
}

function ConnectorEditor({
  profile,
  cluster,
  name,
  existing,
  onClose,
  onApplied,
}: {
  profile: ConnectionProfile;
  cluster: string;
  /** null = creating. A name = editing that connector's config in place. */
  name: string | null;
  existing: string[];
  onClose: () => void;
  onApplied: (name: string) => void;
}) {
  const creating = name === null;
  const [connectorName, setConnectorName] = useState(name ?? "");
  const [text, setText] = useState(creating ? NEW_TEMPLATE : "");
  const [loading, setLoading] = useState(!creating);
  const [loadFailed, setLoadFailed] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [validation, setValidation] = useState<ConfigValidation | null>(null);
  /** The exact text a validation was computed from — anything else is stale. */
  const [validatedText, setValidatedText] = useState<string | null>(null);
  const [validating, setValidating] = useState(false);
  const [applying, setApplying] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const textRef = useRef<HTMLTextAreaElement | null>(null);
  const nameRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (creating || name === null) return;
    let cancelled = false;
    setLoading(true);
    connectConfig(profile.id, cluster, name)
      .then((config) => {
        if (cancelled) return;
        // Pretty-printed and key-sorted: a config the worker returns in hash
        // order is a config nobody can diff by eye.
        const sorted: Record<string, string> = {};
        for (const key of Object.keys(config).sort()) sorted[key] = config[key];
        setText(`${JSON.stringify(sorted, null, 2)}\n`);
        setLoadFailed(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setLoadFailed(true);
        setFailure(errorMessage(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [creating, name, profile.id, cluster]);

  const parsed = useMemo(() => parseConfig(text), [text]);
  const connectorClass = parsed.ok ? (parsed.config["connector.class"] ?? "") : "";
  const stale = validatedText !== null && validatedText !== text;

  /** Put the caret on a key, so a field error is one click from the text. */
  const jumpTo = useCallback((key: string) => {
    const el = textRef.current;
    if (!el) return;
    const needle = `"${key}"`;
    const at = el.value.indexOf(needle);
    el.focus();
    if (at < 0) return;
    el.setSelectionRange(at, at + needle.length);
    // Textareas do not scroll to a programmatic selection, so it is done by
    // hand: the line index times the computed line height, a third of a
    // viewport up so the key is not welded to the top edge.
    const line = lineOf(el.value, at) - 1;
    const lineHeight = Number.parseFloat(
      window.getComputedStyle(el).lineHeight || "18",
    );
    if (Number.isFinite(lineHeight))
      el.scrollTop = Math.max(0, line * lineHeight - el.clientHeight / 3);
  }, []);

  const validate = useCallback(async () => {
    if (!parsed.ok) {
      setProblem(
        parsed.index === null
          ? parsed.message
          : `Line ${lineOf(text, parsed.index)}: ${parsed.message}`,
      );
      textRef.current?.focus();
      return;
    }
    if (connectorClass.trim().length === 0) {
      setProblem(
        "Add connector.class first — the workers validate against that plugin's own rules, so there is nothing to check without it.",
      );
      jumpTo("connector.class");
      return;
    }
    setProblem(null);
    setFailure(null);
    setValidating(true);
    try {
      const result = await connectValidate(
        profile.id,
        cluster,
        connectorClass.trim(),
        parsed.config,
      );
      setValidation(result);
      setValidatedText(text);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setValidating(false);
    }
  }, [parsed, text, connectorClass, profile.id, cluster, jumpTo]);

  const apply = useCallback(async () => {
    if (!parsed.ok) {
      setProblem(
        parsed.index === null
          ? parsed.message
          : `Line ${lineOf(text, parsed.index)}: ${parsed.message}`,
      );
      textRef.current?.focus();
      return;
    }
    const finalName = connectorName.trim();
    if (finalName.length === 0) {
      setProblem("Give the connector a name — the workers key everything by it.");
      nameRef.current?.focus();
      return;
    }
    if (creating && existing.includes(finalName)) {
      setProblem(
        `${cluster} already has a connector called ${finalName}. Applying would overwrite its config — open it from the list instead if that is what you meant.`,
      );
      nameRef.current?.focus();
      return;
    }
    setProblem(null);
    setFailure(null);
    setApplying(true);
    try {
      // The worker requires `name` inside the config to match the connector it
      // is being written to, so Kavka fills it in rather than letting a
      // mismatch come back as a 400 nobody can read.
      await connectApply(profile.id, cluster, finalName, {
        ...parsed.config,
        name: finalName,
      });
      onApplied(finalName);
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setApplying(false);
      setConfirming(false);
    }
  }, [
    parsed,
    text,
    connectorName,
    creating,
    existing,
    profile.id,
    cluster,
    onApplied,
  ]);

  const isProd = profile.environment === "prod";
  const clean = validation !== null && validation.error_count === 0 && !stale;
  const fieldErrors = (validation?.configs ?? []).filter(
    (c) => c.errors.length > 0,
  );
  const classified = failure === null ? null : classifyError(failure);

  return (
    <>
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <button
              type="button"
              className="btn btn-ghost crumb-btn"
              onClick={onClose}
            >
              ← {creating ? "Connectors" : name}
            </button>
            <span className="topic-name">
              {creating ? "New connector" : `${name} · config`}
            </span>
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={validating || loading}
              aria-busy={validating || undefined}
              title={
                validating
                  ? "The workers are checking this config"
                  : loading
                    ? "Kavka is still reading the connector's config"
                    : "Ask the workers what this plugin thinks of the config"
              }
              onClick={() => void validate()}
            >
              <span className="btn-busy-slot" aria-hidden="true">
                {validating ? <span className="spinner" /> : null}
              </span>
              Validate
            </button>
            <button
              type="button"
              className={`btn ${isProd || !clean ? "btn-danger" : "btn-primary"}`}
              disabled={applying || loading}
              aria-busy={applying || undefined}
              title={
                applying
                  ? "Kavka is writing the config"
                  : loading
                    ? "Kavka is still reading the connector's config"
                    : clean && !isProd
                      ? "Write this config to the workers"
                      : "Kavka will ask you to confirm first"
              }
              onClick={() => {
                if (clean && !isProd) void apply();
                else setConfirming(true);
              }}
            >
              <span className="btn-busy-slot" aria-hidden="true">
                {applying ? <span className="spinner" /> : null}
              </span>
              {creating ? "Create connector" : "Apply config"}
            </button>
          </div>
        </div>

        <p className="table-note">
          Writing to <code>{cluster}</code>. A connector config is a flat set of
          keys — <code>connector.class</code> names the plugin, and everything
          else is that plugin's own. Kavka fills in <code>name</code> for you.
        </p>

        {creating && (
          <div className="field">
            <label className="field-label" htmlFor="ce-name">
              Connector name
            </label>
            <input
              id="ce-name"
              ref={nameRef}
              type="text"
              className="input-mono"
              value={connectorName}
              placeholder="orders-to-s3"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => {
                setConnectorName(e.target.value);
                setProblem(null);
              }}
            />
            <span className="field-hint">
              How the workers will refer to it. Applying a config to a name that
              already exists replaces that connector's config in place.
            </span>
          </div>
        )}

        <div className="field">
          <div className="field-label-row">
            <label className="field-label" htmlFor="ce-json">
              Config
            </label>
            {connectorClass.length > 0 && (
              <span className="field-hint">
                Plugin: <code>{connectorClass}</code>
              </span>
            )}
          </div>
          <textarea
            id="ce-json"
            ref={textRef}
            className="io-json connect-json"
            rows={18}
            value={loading ? "" : text}
            spellCheck={false}
            disabled={loading}
            placeholder={loading ? "Reading the connector's config…" : undefined}
            onChange={(e) => {
              setText(e.target.value);
              setProblem(null);
            }}
          />
          {problem !== null && <span className="field-error">{problem}</span>}
          {!parsed.ok && problem === null && (
            <span className="field-hint">
              This isn't valid JSON yet —{" "}
              {parsed.index === null
                ? parsed.message
                : `line ${lineOf(text, parsed.index)}: ${parsed.message}`}
            </span>
          )}
          <span className="field-hint">
            Kafka ships a file connector you can try this with:{" "}
            <code>org.apache.kafka.connect.file.FileStreamSinkConnector</code>{" "}
            with <code>topics</code> and <code>file</code>.
          </span>
        </div>

        {loadFailed && (
          <p className="table-note">
            Kavka couldn't read the current config, so this box is empty rather
            than wrong. Applying now would replace the connector's whole config
            with whatever is here — go back unless that is what you want.
          </p>
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
      </section>

      {validation !== null && (
        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              What the workers say
              <span className="panel-count">
                {validation.error_count === 0
                  ? "no problems"
                  : `${validation.error_count} problem${
                      validation.error_count === 1 ? "" : "s"
                    }`}
              </span>
            </h2>
          </div>

          {stale && (
            <div className="banner banner-warn" role="note">
              <span className="banner-glyph" aria-hidden="true">
                !
              </span>
              <div className="banner-body">
                <p className="banner-title">
                  The config has changed since this check.
                </p>
                <p className="banner-detail">
                  What's below is about the version you validated, not the one in
                  the box. Validate again before you lean on it.
                </p>
              </div>
            </div>
          )}

          {fieldErrors.length === 0 ? (
            <p className="table-note">
              The plugin's own validation found nothing wrong. That is the
              worker's opinion at this moment, against the plugin it has loaded —
              it is not a promise the connector will run.
            </p>
          ) : (
            <ul className="validate-list">
              {fieldErrors.map((entry) => (
                <li className="validate-row" key={entry.name}>
                  <button
                    type="button"
                    className="btn btn-ghost validate-jump"
                    title={`Find ${entry.name} in the config`}
                    onClick={() => jumpTo(entry.name)}
                  >
                    <span className="validate-name">{entry.name}</span>
                  </button>
                  <span className="validate-msgs">
                    {entry.errors.map((message, i) => (
                      <span className="validate-msg" key={i}>
                        {message}
                      </span>
                    ))}
                    {entry.required && entry.value === null && (
                      <span className="cell-tag">
                        Required, and not in your config.
                      </span>
                    )}
                    {entry.documentation !== null &&
                      entry.documentation.length > 0 && (
                        <span className="validate-doc">{entry.documentation}</span>
                      )}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {confirming && (
        <ConfirmModal
          tone="plain"
          title={
            creating
              ? `Create ${connectorName.trim() || "this connector"} on ${cluster}?`
              : `Apply this config to ${name}?`
          }
          body={
            <>
              {clean ? (
                <>
                  The workers validated this config with no problems. They take
                  it immediately and restart the connector's tasks to pick it up.
                </>
              ) : validation === null ? (
                <>
                  This config hasn't been validated. The workers accept it or
                  refuse it on the spot, and a config they accept can still fail
                  at runtime — Validate first if you want the plugin's opinion.
                </>
              ) : stale ? (
                <>
                  The last validation was for a different version of this config,
                  so it says nothing about what is in the box now.
                </>
              ) : (
                <>
                  The workers found {validation.error_count} problem
                  {validation.error_count === 1 ? "" : "s"} with this config.
                  Validation runs against the plugin's own rules on the worker
                  that answered, and those rules are sometimes wrong about a
                  valid config — which is why this is a warning and not a wall.
                </>
              )}
              {isProd && (
                <>
                  {" "}
                  {profile.name} is a production cluster, so this changes a
                  running pipeline.
                </>
              )}
            </>
          }
          confirmLabel={creating ? "Create connector" : "Apply config"}
          typeToConfirm={isProd ? connectorName.trim() : null}
          busy={applying}
          busyLabel="Kavka is writing the config"
          onCancel={() => setConfirming(false)}
          onConfirm={() => void apply()}
        />
      )}
    </>
  );
}
