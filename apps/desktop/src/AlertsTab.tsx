import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  alertsChannelsGet,
  alertsChannelsSet,
  alertsChannelsTest,
  alertsDelete,
  alertsHistory,
  alertsList,
  alertsSave,
  errorMessage,
  groupsList,
  metricsStatus,
  METRIC_SERIES,
  type AlertChannels,
  type AlertEvent,
  type AlertKind,
  type AlertRule,
  type ConnectionProfile,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { classifyError } from "./errors";
import { groupDigits } from "./format";
import { readoutTime } from "./Chart";
import {
  formatDuration,
  formatSpan,
  formatSeriesValue,
  seriesGloss,
  seriesLabel,
} from "./monitoring";
import Overlay from "./Overlay";
import { ErrorBanner } from "./ProfileEditor";
import { ToastStack, useToasts } from "./Toast";

/**
 * ALERTS — the one surface a read-only connection is not limited on, and the
 * one place in Kavka that deliberately diverges from §6's read-only doctrine.
 *
 * Everywhere else, "read-only" disables every mutating control with the same
 * sentence. An alert rule mutates nothing on the cluster: it is a note Kavka
 * keeps about what to watch for, stored on this machine, acted on by reading.
 * Disabling it on a read-only connection would block the one thing a read-only
 * operator most obviously wants to do — watch production without being able to
 * touch it. So the rules are editable and the page says so out loud, because a
 * divergence nobody explains reads as a bug.
 *
 * THE SENTENCE IS THE FEATURE, exactly as it is for an ACL. A rule is four or
 * five fields that mean one English sentence, and if the sentence and the fields
 * ever disagree, Kavka is lying about what it will wake someone up for. The
 * parity table below is the same device `AclsTab` uses and exists for the same
 * reason: this repo has no TS test runner, so the claim is checked in dev and
 * fails loudly in the console.
 */

// ---------------------------------------------------------------------------
// THE SENTENCE
// ---------------------------------------------------------------------------

/** `orders-service` → the group half of the sentence, never blank. */
function nameOr(value: string, fallback: string): string {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : fallback;
}

/**
 * The dwell clause. `for_ms: 0` is a real, useful setting — "the moment it
 * happens" — and it must not read as "for 0 seconds", which is both wrong and
 * unreadable.
 */
function dwellClause(forMs: number): string {
  return forMs <= 0 ? "" : ` for ${formatSpan(forMs)}`;
}

/**
 * ONE LINE OF ENGLISH THAT MEANS EXACTLY WHAT THE FIELDS MEAN.
 *
 * No hedging, no "may". Every rule renders through this: the builder writes it
 * live, the list carries it under the rule's name, and the delete confirmation
 * leads with it.
 */
export function alertSentence(rule: AlertRule): string {
  switch (rule.kind) {
    case "lag_threshold": {
      const group = nameOr(rule.group_id, "a group");
      const where =
        rule.topic === null || rule.topic.trim().length === 0
          ? "on any topic it reads"
          : `on ${rule.topic.trim()}`;
      const dwell = dwellClause(rule.for_ms);
      return dwell.length === 0
        ? `Alert me the moment group ${group} goes more than ${groupDigits(
            rule.threshold,
          )} messages behind ${where}.`
        : `Alert me when group ${group} is more than ${groupDigits(
            rule.threshold,
          )} messages behind ${where}${dwell}.`;
    }
    case "under_replicated": {
      const dwell = dwellClause(rule.for_ms);
      return dwell.length === 0
        ? "Alert me the moment any partition on this cluster is missing a copy."
        : `Alert me when any partition on this cluster has been missing a copy${dwell}.`;
    }
    case "offline_partitions":
      return "Alert me the moment any partition on this cluster has no leader at all.";
    case "throughput_floor": {
      const what = seriesLabel(rule.series);
      const first = what.charAt(0).toLowerCase() + what.slice(1);
      const dwell = dwellClause(rule.for_ms);
      return dwell.length === 0
        ? `Alert me the moment ${first} drops below ${formatSeriesValue(
            rule.series,
            rule.below,
          )}.`
        : `Alert me when ${first} stays below ${formatSeriesValue(
            rule.series,
            rule.below,
          )}${dwell}.`;
    }
  }
}

// ---------------------------------------------------------------------------
// THE PARITY TABLE — see AclsTab, which this mirrors deliberately.
// ---------------------------------------------------------------------------

/**
 * U+202F NARROW NO-BREAK SPACE — the digit separator `groupDigits` puts inside
 * every grouped number, and therefore inside every sentence with a threshold
 * in it. Written as an escape rather than pasted: an invisible character in an
 * expected string is a claim nobody can read, and a plain space here would make
 * every numeric row fail for a reason that has nothing to do with the sentence.
 */
const GROUP_SEP = "\u202F";

interface ParityRow {
  what: string;
  got: () => string;
  want: string;
}

function parityRows(): ParityRow[] {
  const lag = (over: Partial<Extract<AlertRule, { kind: "lag_threshold" }>> = {}) =>
    alertSentence({
      kind: "lag_threshold",
      id: "r1",
      name: "n",
      group_id: "orders-service",
      topic: null,
      threshold: 10000,
      for_ms: 300_000,
      ...over,
    });

  return [
    {
      what: "a lag rule with no topic covers every topic the group reads",
      got: () => lag(),
      want: `Alert me when group orders-service is more than 10${GROUP_SEP}000 messages behind on any topic it reads for 5 minutes.`,
    },
    {
      what: "a lag rule with a topic names it",
      got: () => lag({ topic: "orders.v2" }),
      want: `Alert me when group orders-service is more than 10${GROUP_SEP}000 messages behind on orders.v2 for 5 minutes.`,
    },
    {
      what: "a zero dwell reads as 'the moment', never 'for 0 seconds'",
      got: () => lag({ for_ms: 0 }),
      want: `Alert me the moment group orders-service goes more than 10${GROUP_SEP}000 messages behind on any topic it reads.`,
    },
    {
      what: "an empty topic string is the same as no topic",
      got: () => lag({ topic: "   " }),
      want: `Alert me when group orders-service is more than 10${GROUP_SEP}000 messages behind on any topic it reads for 5 minutes.`,
    },
    {
      what: "under-replicated states the dwell",
      got: () =>
        alertSentence({
          kind: "under_replicated",
          id: "r2",
          name: "n",
          for_ms: 120_000,
        }),
      want: "Alert me when any partition on this cluster has been missing a copy for 2 minutes.",
    },
    {
      what: "offline partitions never wait",
      got: () =>
        alertSentence({ kind: "offline_partitions", id: "r3", name: "n" }),
      want: "Alert me the moment any partition on this cluster has no leader at all.",
    },
    {
      what: "a throughput floor lowercases the series inside the sentence",
      got: () =>
        alertSentence({
          kind: "throughput_floor",
          id: "r4",
          name: "n",
          series: "messages_in_per_sec",
          below: 1000,
          for_ms: 600_000,
        }),
      want: `Alert me when messages in per second stays below 1${GROUP_SEP}000 for 10 minutes.`,
    },
  ];
}

/** Exported so it can be called from a console, or a test runner one day. */
export function alertSentenceParityFailures(): string[] {
  return parityRows()
    .filter((row) => row.got() !== row.want)
    .map(
      (row) =>
        `${row.what}: expected ${JSON.stringify(row.want)}, got ${JSON.stringify(
          row.got(),
        )}`,
    );
}

if (import.meta.env.DEV) {
  const failures = alertSentenceParityFailures();
  if (failures.length > 0) {
    console.error(
      "[kavka] an alert rule no longer says what it will do — the sentence is " +
        "what the builder, the rule list and the delete confirmation all show, " +
        "so this is a wrong claim about when someone gets woken up:\n" +
        failures.join("\n"),
    );
  }
}

// ---------------------------------------------------------------------------
// The tab
// ---------------------------------------------------------------------------

const KIND_LABEL: Record<AlertKind, string> = {
  lag_threshold: "Consumer group falls behind",
  under_replicated: "Partitions missing a copy",
  offline_partitions: "Partitions with no leader",
  throughput_floor: "Throughput drops away",
};

const KIND_GLOSS: Record<AlertKind, string> = {
  lag_threshold:
    "Watches Kavka's own lag samples for one group. It needs this connection to be up, because that is when the sampling happens.",
  under_replicated:
    "Watches the under-replicated partition count from the metrics endpoint. Needs a metrics endpoint on this connection.",
  offline_partitions:
    "Watches the offline partition count from the metrics endpoint. Anything above zero means records are being refused somewhere.",
  throughput_floor:
    "Watches one metrics series for a floor — the way to catch a producer that quietly stopped, which no error will ever tell you about.",
};

const HISTORY_LIMIT = 100;

interface AlertsTabProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
  /** Bumped by ClusterView on every fire and resolve, so history stays live. */
  eventNonce: number;
}

export default function AlertsTab({
  profile,
  onDanger,
  eventNonce,
}: AlertsTabProps) {
  const [rules, setRules] = useState<AlertRule[] | null>(null);
  const [history, setHistory] = useState<AlertEvent[] | null>(null);
  const [channels, setChannels] = useState<AlertChannels | null>(null);
  const [groups, setGroups] = useState<string[]>([]);
  const [available, setAvailable] = useState<string[]>([]);
  const [editing, setEditing] = useState<AlertRule | "new" | null>(null);
  const [removing, setRemoving] = useState<AlertRule | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [nonce, setNonce] = useState(0);

  const toaster = useToasts();
  const push = toaster.push;
  const rulesSeq = useRef(0);
  const historySeq = useRef(0);

  useDangerSignal(error !== null, onDanger);

  const reloadRules = useCallback(() => {
    const mine = ++rulesSeq.current;
    alertsList(profile.id)
      .then((list) => {
        if (rulesSeq.current === mine) setRules(list);
      })
      .catch((err: unknown) => {
        if (rulesSeq.current !== mine) return;
        setRules([]);
        setError(errorMessage(err));
      });
  }, [profile.id]);

  useEffect(() => {
    reloadRules();
    return () => {
      rulesSeq.current += 1;
    };
  }, [reloadRules, nonce]);

  // History reloads on its own nonce AND on every live event, so a firing that
  // arrives while this tab is open lands in the list without a click.
  useEffect(() => {
    const mine = ++historySeq.current;
    alertsHistory(profile.id, HISTORY_LIMIT)
      .then((list) => {
        if (historySeq.current === mine) setHistory(list);
      })
      .catch((err: unknown) => {
        if (historySeq.current !== mine) return;
        setHistory([]);
        setError(errorMessage(err));
      });
    return () => {
      historySeq.current += 1;
    };
  }, [profile.id, nonce, eventNonce]);

  useEffect(() => {
    let cancelled = false;
    alertsChannelsGet(profile.id)
      .then((next) => {
        if (!cancelled) setChannels(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [profile.id, nonce]);

  // The two pickers' vocabularies. Both are conveniences: a rule may name a
  // group that has not started yet, or a series this scrape doesn't publish
  // today, and neither is an error — so a failure here is silent.
  useEffect(() => {
    let cancelled = false;
    groupsList(profile.id)
      .then((list) => {
        if (!cancelled) setGroups(list.map((g) => g.group_id));
      })
      .catch(() => undefined);
    metricsStatus(profile.id)
      .then((status) => {
        if (!cancelled) setAvailable(status.series_available);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [profile.id]);

  const saveRule = useCallback(
    async (rule: AlertRule) => {
      setBusy(true);
      try {
        await alertsSave(profile.id, rule);
        setEditing(null);
        setNonce((n) => n + 1);
        push({
          kind: "ok",
          title: `Saved ${rule.name}`,
          detail: alertSentence(rule),
        });
      } catch (err) {
        setError(errorMessage(err));
      } finally {
        setBusy(false);
      }
    },
    [profile.id, push],
  );

  const deleteRule = useCallback(async () => {
    if (removing === null) return;
    setBusy(true);
    try {
      await alertsDelete(profile.id, removing.id);
      setRemoving(null);
      setNonce((n) => n + 1);
      push({ kind: "ok", title: `Deleted ${removing.name}` });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [profile.id, removing, push]);

  /** Rules currently firing, by id — drives the "firing now" mark in the list. */
  const firing = useMemo(() => {
    const set = new Set<string>();
    for (const event of history ?? [])
      if (event.resolved_ms === null) set.add(event.rule_id);
    return set;
  }, [history]);

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Alert rules
            {rules !== null && <span className="panel-count">{rules.length}</span>}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy}
              title={busy ? "Kavka is saving a rule" : "Describe something worth being told about"}
              onClick={() => setEditing("new")}
            >
              Add a rule
            </button>
          </div>
        </div>

        <p className="table-note">
          A rule watches one number and tells you when it crosses a line and{" "}
          <em>stays</em> there. The waiting period is the whole point: lag jumps
          during every rebalance, and a rule that fires on the jump is a rule
          people learn to ignore.
        </p>

        {/* THE ONE DELIBERATE DIVERGENCE FROM READ-ONLY. Said out loud, because
            a rule that behaves differently from every other control on a
            read-only connection needs a reason on screen. */}
        {profile.read_only && (
          <p className="readonly-note">
            This connection is read-only, and alerts still work. Rules are
            Kavka's own notes about what to watch — they read, they never write
            to the cluster, so nothing here is blocked.
          </p>
        )}

        {rules === null ? (
          <p className="table-note">Reading this connection's alert rules…</p>
        ) : rules.length === 0 ? (
          <p className="table-note">
            No rules yet. The two most people start with are "tell me when this
            group falls a long way behind and stays there" and "tell me the
            moment a partition has no leader" — the first needs nothing but this
            connection, the second needs a metrics endpoint.
          </p>
        ) : (
          <div className="table-wrap">
            <table className="data-table data-table-flush data-table-tall">
              <caption className="sr-only">
                Alert rules for {profile.name}
              </caption>
              <thead>
                <tr>
                  <th scope="col">Rule</th>
                  <th scope="col">What it watches</th>
                  <th scope="col">State</th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {rules.map((rule) => (
                  <tr key={rule.id}>
                    <td>
                      <span className="rule-name">{rule.name}</span>
                      <span className="rule-kind">{KIND_LABEL[rule.kind]}</span>
                    </td>
                    <td className="rule-sentence">{alertSentence(rule)}</td>
                    <td>
                      {firing.has(rule.id) ? (
                        <span
                          className="health health-danger"
                          title="This rule is firing right now — see the history below for what tripped it."
                        >
                          <i className="dot" aria-hidden="true" />
                          Firing
                        </span>
                      ) : (
                        <span
                          className="health health-ok"
                          title="Nothing has tripped this rule."
                        >
                          <i className="dot" aria-hidden="true" />
                          Quiet
                        </span>
                      )}
                    </td>
                    <td className="col-affordance">
                      <span className="rule-actions">
                        <button
                          type="button"
                          className="btn btn-row"
                          title={`Change what ${rule.name} watches for`}
                          onClick={() => setEditing(rule)}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          className="btn btn-danger btn-row"
                          title={`Stop watching for ${rule.name}`}
                          onClick={() => setRemoving(rule)}
                        >
                          Delete
                        </button>
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <ChannelsPanel
        profileId={profile.id}
        channels={channels}
        onChannels={setChannels}
        onError={setError}
        onToast={push}
      />

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            What has fired
            {history !== null && (
              <span className="panel-count">
                {history.length === HISTORY_LIMIT
                  ? `last ${HISTORY_LIMIT}`
                  : history.length}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              onClick={() => setNonce((n) => n + 1)}
              title="Read the alert log again"
            >
              Refresh
            </button>
          </div>
        </div>

        {history === null ? (
          <p className="table-note">Reading the alert log…</p>
        ) : history.length === 0 ? (
          <p className="table-note">
            Nothing has fired yet. Firings are kept on this machine alongside the
            lag history, so this list survives a restart — and it is the record
            to reconstruct an incident from afterwards.
          </p>
        ) : (
          <div className="table-wrap">
            <table className="data-table data-table-flush data-table-tall">
              <caption className="sr-only">
                Alerts that have fired on {profile.name}
              </caption>
              <thead>
                <tr>
                  <th scope="col">Rule</th>
                  <th scope="col">Fired</th>
                  <th scope="col">Resolved</th>
                  <th scope="col" className="col-num">
                    Lasted
                  </th>
                  <th scope="col">What tripped it</th>
                </tr>
              </thead>
              <tbody>
                {history.map((event, i) => (
                  <tr key={`${event.rule_id}:${event.fired_ms}:${i}`}>
                    <td>{event.rule_name}</td>
                    <td className="cell-mono">{readoutTime(event.fired_ms)}</td>
                    <td className="cell-mono">
                      {event.resolved_ms === null ? (
                        <span
                          className="health health-danger"
                          title="This condition is still true."
                        >
                          <i className="dot" aria-hidden="true" />
                          Still firing
                        </span>
                      ) : (
                        readoutTime(event.resolved_ms)
                      )}
                    </td>
                    <td className="col-num cell-num">
                      {formatDuration(
                        (event.resolved_ms ?? Date.now()) - event.fired_ms,
                      )}
                      {event.resolved_ms === null ? " so far" : ""}
                    </td>
                    <td className="rule-sentence">{event.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {editing !== null && (
        <AlertRuleModal
          rule={editing === "new" ? null : editing}
          groups={groups}
          available={available}
          busy={busy}
          onSave={saveRule}
          onClose={() => setEditing(null)}
        />
      )}

      {removing !== null && (
        // `plain`, not destructive, and no type-to-confirm even on prod. §5.5's
        // guardrail rule is that a filled red button lives one click from a
        // destructive action — and deleting a rule destroys nothing on the
        // cluster. Making this wear the red wire would be crying wolf, which is
        // exactly how the wire stops meaning anything on the modals that need
        // it.
        <ConfirmModal
          title={`Delete ${removing.name}?`}
          body={
            <>
              <p>{alertSentence(removing)}</p>
              <p>
                Kavka stops watching for that. Everything this rule has already
                recorded stays in the log below.
              </p>
            </>
          }
          confirmLabel="Delete rule"
          tone="plain"
          busy={busy}
          busyLabel="Kavka is deleting this rule"
          onConfirm={() => void deleteRule()}
          onCancel={() => setRemoving(null)}
        />
      )}

      <ToastStack {...toaster} />
    </>
  );
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

function ChannelsPanel({
  profileId,
  channels,
  onChannels,
  onError,
  onToast,
}: {
  profileId: string;
  channels: AlertChannels | null;
  onChannels: (channels: AlertChannels) => void;
  onError: (message: string) => void;
  onToast: ReturnType<typeof useToasts>["push"];
}) {
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [urlError, setUrlError] = useState<string | null>(null);
  const urlRef = useRef<HTMLInputElement | null>(null);

  if (channels === null)
    return (
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Where a firing goes</h2>
        </div>
        <p className="table-note">Reading this connection's alert channels…</p>
      </section>
    );

  const url = channels.webhook_url ?? "";
  const hasUrl = url.trim().length > 0;

  const validate = (): string | null => {
    if (!hasUrl) return null;
    try {
      const parsed = new URL(url.trim());
      if (parsed.protocol !== "https:" && parsed.protocol !== "http:")
        return "Use an http:// or https:// address — that is all Kavka can POST to.";
      return null;
    } catch {
      return "Use the whole URL, starting with https:// — e.g. https://hooks.slack.com/services/…";
    }
  };

  const persist = async (next: AlertChannels): Promise<boolean> => {
    const problem = validate();
    if (problem !== null) {
      setUrlError(problem);
      urlRef.current?.focus();
      return false;
    }
    setUrlError(null);
    setBusy(true);
    try {
      await alertsChannelsSet(profileId, next);
      onChannels(next);
      return true;
    } catch (err) {
      onError(errorMessage(err));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const runTest = async () => {
    // Saved first, deliberately: a test that ran against the stored settings
    // while the form shows different ones proves nothing about what is on
    // screen, which is the only thing the person clicking it cares about.
    const saved = await persist({
      ...channels,
      webhook_url: hasUrl ? url.trim() : null,
    });
    if (!saved) return;
    setTesting(true);
    try {
      await alertsChannelsTest(profileId);
      onToast({
        kind: "ok",
        title: "Sent a test alert",
        detail: channels.os_notification
          ? "Check for a desktop notification, and for a message at the webhook if you set one."
          : "Check for a message at the webhook.",
      });
    } catch (err) {
      const raw = errorMessage(err);
      const { title, detail } = classifyError(raw);
      onToast({ kind: "danger", title, detail });
    } finally {
      setTesting(false);
    }
  };

  const nothingConfigured = !channels.os_notification && !hasUrl;

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">Where a firing goes</h2>
      </div>

      <p className="table-note">
        A firing always lands in the log below and as a toast in this window.
        These two send it somewhere you'll see when Kavka isn't in front of you.
      </p>

      <div className="check-field">
        <input
          id="al-os"
          type="checkbox"
          checked={channels.os_notification}
          disabled={busy}
          aria-describedby="al-os-hint"
          onChange={(e) =>
            void persist({ ...channels, os_notification: e.target.checked })
          }
        />
        <label className="check-label" htmlFor="al-os">
          Show a desktop notification
        </label>
        <span className="field-hint" id="al-os-hint">
          Your operating system's own notification, with the same words as the
          toast — so the two never say different things about the same firing.
        </span>
      </div>

      <div className="field">
        <label className="field-label" htmlFor="al-webhook">
          Webhook address
        </label>
        <input
          id="al-webhook"
          ref={urlRef}
          type="text"
          className={`input-mono${urlError !== null ? " input-invalid" : ""}`}
          value={url}
          placeholder="https://hooks.slack.com/services/T000/B000/xxxx"
          autoComplete="off"
          spellCheck={false}
          disabled={busy}
          aria-invalid={urlError !== null ? true : undefined}
          aria-describedby={
            urlError !== null ? "al-webhook-error al-webhook-hint" : "al-webhook-hint"
          }
          onChange={(e) => {
            setUrlError(null);
            onChannels({
              ...channels,
              webhook_url: e.target.value.length === 0 ? null : e.target.value,
            });
          }}
          onBlur={() =>
            void persist({
              ...channels,
              webhook_url: hasUrl ? url.trim() : null,
            })
          }
        />
        {urlError !== null && (
          <span className="field-error" id="al-webhook-error">
            {urlError}
          </span>
        )}
        <span className="field-hint" id="al-webhook-hint">
          Kavka POSTs one JSON body per firing and one more when it resolves.
          Leave it empty for no webhook. The address usually contains a secret,
          so treat it like one.
        </span>
      </div>

      <div className="check-field">
        <input
          id="al-slack"
          type="checkbox"
          checked={channels.webhook_is_slack}
          disabled={busy || !hasUrl}
          title={
            hasUrl
              ? undefined
              : "Add a webhook address first — there is nothing to shape the message for yet."
          }
          aria-describedby="al-slack-hint"
          onChange={(e) =>
            void persist({ ...channels, webhook_is_slack: e.target.checked })
          }
        />
        <label className="check-label" htmlFor="al-slack">
          Send it in Slack's shape
        </label>
        <span className="field-hint" id="al-slack-hint">
          Slack wants <code>{'{"text": "…"}'}</code> and ignores anything else;
          a generic endpoint gets Kavka's own JSON with the rule, the timestamps
          and the numbers as separate fields. Posting the wrong shape to the
          right address fails quietly at the far end, which is what the test
          below is for.
        </span>
      </div>

      <div className="panel-tools">
        <button
          type="button"
          className="btn"
          disabled={busy || testing || nothingConfigured}
          aria-busy={testing || undefined}
          title={
            nothingConfigured
              ? "Turn on notifications or add a webhook address first — there is nothing to test yet"
              : "Send one test firing through everything above, exactly as a real one would go"
          }
          onClick={() => void runTest()}
        >
          <span className="btn-busy-slot" aria-hidden="true">
            {testing ? <span className="spinner" /> : null}
          </span>
          Send a test alert
        </button>
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// The builder
// ---------------------------------------------------------------------------

type RuleField =
  | "name"
  | "groupId"
  | "topic"
  | "threshold"
  | "series"
  | "below"
  | "forMinutes";

interface RuleFieldError {
  field: RuleField;
  message: string;
}

/** Everything the form can hold, whichever kind is selected. */
interface RuleDraft {
  kind: AlertKind;
  name: string;
  groupId: string;
  topic: string;
  threshold: string;
  series: string;
  below: string;
  forMinutes: string;
}

function draftFrom(rule: AlertRule | null): RuleDraft {
  const base: RuleDraft = {
    kind: "lag_threshold",
    name: "",
    groupId: "",
    topic: "",
    threshold: "10000",
    series: "messages_in_per_sec",
    below: "1",
    forMinutes: "5",
  };
  if (rule === null) return base;
  base.kind = rule.kind;
  base.name = rule.name;
  if (rule.kind === "lag_threshold") {
    base.groupId = rule.group_id;
    base.topic = rule.topic ?? "";
    base.threshold = String(rule.threshold);
    base.forMinutes = String(rule.for_ms / 60_000);
  } else if (rule.kind === "under_replicated") {
    base.forMinutes = String(rule.for_ms / 60_000);
  } else if (rule.kind === "throughput_floor") {
    base.series = rule.series;
    base.below = String(rule.below);
    base.forMinutes = String(rule.for_ms / 60_000);
  }
  return base;
}

/** A number of minutes, as typed. Blank and 0 both mean "no waiting". */
function minutesToMs(value: string): number {
  const n = Number(value.trim());
  return Number.isFinite(n) && n > 0 ? Math.round(n * 60_000) : 0;
}

/**
 * The rule the sentence is written from, always complete enough to render.
 * Placeholders stand in for empty fields so the sentence is readable from the
 * first keystroke rather than appearing only once the form validates.
 */
function ruleFrom(draft: RuleDraft, id: string): AlertRule {
  const forMs = minutesToMs(draft.forMinutes);
  const name = draft.name.trim();
  switch (draft.kind) {
    case "lag_threshold":
      return {
        kind: "lag_threshold",
        id,
        name,
        group_id: draft.groupId.trim(),
        topic: draft.topic.trim().length === 0 ? null : draft.topic.trim(),
        threshold: Math.max(0, Math.round(Number(draft.threshold) || 0)),
        for_ms: forMs,
      };
    case "under_replicated":
      return { kind: "under_replicated", id, name, for_ms: forMs };
    case "offline_partitions":
      return { kind: "offline_partitions", id, name };
    case "throughput_floor":
      return {
        kind: "throughput_floor",
        id,
        name,
        series: draft.series,
        below: Number(draft.below) || 0,
        for_ms: forMs,
      };
  }
}

/** What Kavka calls a rule the user didn't name. Never blank, never "Rule 1". */
function suggestName(rule: AlertRule): string {
  switch (rule.kind) {
    case "lag_threshold":
      return `${rule.group_id || "group"} falling behind`;
    case "under_replicated":
      return "Partitions missing a copy";
    case "offline_partitions":
      return "Partitions with no leader";
    case "throughput_floor":
      return `${seriesLabel(rule.series)} floor`;
  }
}

function AlertRuleModal({
  rule,
  groups,
  available,
  busy,
  onSave,
  onClose,
}: {
  rule: AlertRule | null;
  groups: string[];
  available: string[];
  busy: boolean;
  onSave: (rule: AlertRule) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<RuleDraft>(() => draftFrom(rule));
  const [fieldError, setFieldError] = useState<RuleFieldError | null>(null);
  const nameRef = useRef<HTMLInputElement | null>(null);
  const controls = useRef<Partial<Record<RuleField, HTMLElement | null>>>({});
  const bind = useMemo(() => {
    const cache: Partial<Record<RuleField, (el: HTMLElement | null) => void>> = {};
    return (field: RuleField) =>
      (cache[field] ??= (el: HTMLElement | null) => {
        controls.current[field] = el;
        if (field === "name") nameRef.current = el as HTMLInputElement | null;
      });
  }, []);

  // A new rule keeps one id across save retries, exactly like the profile
  // editor's draft id: a failed save must not leave two rules behind.
  const draftId = useRef<string | null>(null);
  const id = rule?.id ?? (draftId.current ??= crypto.randomUUID());

  const edit = (field: RuleField, partial: Partial<RuleDraft>) => {
    setDraft((prev) => ({ ...prev, ...partial }));
    setFieldError((prev) => (prev?.field === field ? null : prev));
  };

  /** What would be saved right now — the source for the name suggestion. */
  const built = ruleFrom(draft, id);
  /** The same rule with a placeholder where a field is still empty, so the
      sentence is readable from the first keystroke rather than only once the
      form validates. */
  const preview = ruleFrom(
    {
      ...draft,
      groupId: draft.groupId.trim().length === 0 ? "…" : draft.groupId,
    },
    id,
  );

  const validate = (): RuleFieldError | null => {
    if (draft.kind === "lag_threshold") {
      if (draft.groupId.trim().length === 0)
        return {
          field: "groupId",
          message:
            "Name the consumer group to watch. It doesn't have to exist yet — Kavka starts watching when it appears.",
        };
      const threshold = Number(draft.threshold);
      if (!Number.isFinite(threshold) || threshold < 0)
        return {
          field: "threshold",
          message:
            "How many messages behind is too far? A whole number — e.g. 10000.",
        };
    }
    if (draft.kind === "throughput_floor") {
      if (draft.series.trim().length === 0)
        return { field: "series", message: "Pick the reading to watch." };
      const below = Number(draft.below);
      if (!Number.isFinite(below) || below < 0)
        return {
          field: "below",
          message: "How low is too low? A number in the series' own unit.",
        };
    }
    if (draft.kind !== "offline_partitions") {
      const minutes = Number(draft.forMinutes.trim().length === 0 ? "0" : draft.forMinutes);
      if (!Number.isFinite(minutes) || minutes < 0)
        return {
          field: "forMinutes",
          message:
            "How long must it stay true before Kavka tells you? 0 means the moment it happens.",
        };
    }
    return null;
  };

  const submit = () => {
    const problem = validate();
    if (problem !== null) {
      setFieldError(problem);
      controls.current[problem.field]?.focus();
      return;
    }
    onSave({
      ...built,
      name: built.name.length > 0 ? built.name : suggestName(built),
    });
  };

  const message = (field: RuleField) =>
    fieldError?.field === field ? (
      <span className="field-error" id={`al-${field}-error`}>
        {fieldError.message}
      </span>
    ) : null;
  const invalid = (field: RuleField) =>
    fieldError?.field === field ? true : undefined;
  const cls = (field: RuleField, base = "") =>
    `${base}${fieldError?.field === field ? " input-invalid" : ""}`.trim() ||
    undefined;
  const describe = (field: RuleField, hintId?: string) =>
    [hintId, fieldError?.field === field ? `al-${field}-error` : null]
      .filter(Boolean)
      .join(" ") || undefined;

  /** Cluster-level series first, then whatever per-topic ones this scrape has. */
  const seriesOptions = useMemo(() => {
    const out = [...METRIC_SERIES] as string[];
    for (const name of available) if (!out.includes(name)) out.push(name);
    if (!out.includes(draft.series)) out.push(draft.series);
    return out;
  }, [available, draft.series]);

  return (
    <Overlay
      surfaceClass="modal modal-wide"
      labelledBy="al-rule-title"
      initialFocus={nameRef}
      onClose={onClose}
    >
      {/* ⏎ submits: saving a rule changes nothing on the cluster, so the key
          the user is already pressing is allowed to finish the job (§5.8). */}
      <form
        className="modal-panel"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <h2 className="modal-title" id="al-rule-title">
          {rule === null ? "Add an alert rule" : `Edit ${rule.name}`}
        </h2>

        {/* THE SENTENCE, live. Same device as the ACL modal and the offset
            reset modal, for the same reason: it is the entire feature's UX. */}
        <p className="reset-preview" aria-live="polite">
          {alertSentence(preview)}
        </p>

        <div className="field">
          <label className="field-label" htmlFor="al-kind">
            What should Kavka watch?
          </label>
          <select
            id="al-kind"
            value={draft.kind}
            onChange={(e) => {
              setDraft((prev) => ({ ...prev, kind: e.target.value as AlertKind }));
              setFieldError(null);
            }}
          >
            {(Object.keys(KIND_LABEL) as AlertKind[]).map((kind) => (
              <option key={kind} value={kind}>
                {KIND_LABEL[kind]}
              </option>
            ))}
          </select>
          <span className="field-hint">{KIND_GLOSS[draft.kind]}</span>
        </div>

        {draft.kind === "lag_threshold" && (
          <>
            <div className="field">
              <label className="field-label" htmlFor="al-group">
                Which consumer group?
              </label>
              <input
                id="al-group"
                ref={bind("groupId")}
                type="text"
                className={cls("groupId", "input-mono")}
                list="al-group-list"
                value={draft.groupId}
                placeholder="orders-service"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={invalid("groupId")}
                aria-describedby={describe("groupId", "al-group-hint")}
                onChange={(e) => edit("groupId", { groupId: e.target.value })}
              />
              <datalist id="al-group-list">
                {groups.map((g) => (
                  <option key={g} value={g} />
                ))}
              </datalist>
              {message("groupId")}
              <span className="field-hint" id="al-group-hint">
                The list offers the groups this cluster has right now. A group
                that hasn't started yet is a perfectly good thing to watch for —
                type its id.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="al-topic">
                Which topic?
              </label>
              <input
                id="al-topic"
                ref={bind("topic")}
                type="text"
                className={cls("topic", "input-mono")}
                value={draft.topic}
                placeholder="Leave empty for any topic this group reads"
                autoComplete="off"
                spellCheck={false}
                aria-describedby="al-topic-hint"
                onChange={(e) => edit("topic", { topic: e.target.value })}
              />
              <span className="field-hint" id="al-topic-hint">
                Empty means any of them, and the rule fires on the first
                partition that crosses the line — it is not a total across
                topics.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="al-threshold">
                How far behind is too far?
              </label>
              <input
                id="al-threshold"
                ref={bind("threshold")}
                type="number"
                min={0}
                step={1}
                className={cls("threshold")}
                value={draft.threshold}
                aria-invalid={invalid("threshold")}
                aria-describedby={describe("threshold", "al-threshold-hint")}
                onChange={(e) => edit("threshold", { threshold: e.target.value })}
              />
              {message("threshold")}
              <span className="field-hint" id="al-threshold-hint">
                In messages, per partition. A busy topic sits a few thousand
                behind all day without anything being wrong, so start higher
                than feels right and lower it once you've watched the chart.
              </span>
            </div>
          </>
        )}

        {draft.kind === "throughput_floor" && (
          <>
            <div className="field">
              <label className="field-label" htmlFor="al-series">
                Which reading?
              </label>
              <select
                id="al-series"
                ref={bind("series")}
                className={cls("series", "input-mono")}
                value={draft.series}
                aria-invalid={invalid("series")}
                aria-describedby={describe("series", "al-series-hint")}
                onChange={(e) => edit("series", { series: e.target.value })}
              >
                {seriesOptions.map((name) => (
                  <option key={name} value={name}>
                    {seriesLabel(name)}
                  </option>
                ))}
              </select>
              {message("series")}
              <span className="field-hint" id="al-series-hint">
                {seriesGloss(draft.series) ??
                  "Kavka doesn't have a description for this series — it will watch whatever the exporter publishes under that name."}{" "}
                It comes from the metrics endpoint, so this rule needs one
                configured on this connection.
              </span>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="al-below">
                How low is too low?
              </label>
              <input
                id="al-below"
                ref={bind("below")}
                type="number"
                min={0}
                className={cls("below")}
                value={draft.below}
                aria-invalid={invalid("below")}
                aria-describedby={describe("below", "al-below-hint")}
                onChange={(e) => edit("below", { below: e.target.value })}
              />
              {message("below")}
              <span className="field-hint" id="al-below-hint">
                In the series' own unit. A floor of 1 catches "this stopped
                entirely", which is the case no error message will ever tell you
                about.
              </span>
            </div>
          </>
        )}

        {draft.kind !== "offline_partitions" && (
          <div className="field">
            <label className="field-label" htmlFor="al-for">
              How long must it stay that way?
            </label>
            <input
              id="al-for"
              ref={bind("forMinutes")}
              type="number"
              min={0}
              step={1}
              className={cls("forMinutes")}
              value={draft.forMinutes}
              aria-invalid={invalid("forMinutes")}
              aria-describedby={describe("forMinutes", "al-for-hint")}
              onChange={(e) => edit("forMinutes", { forMinutes: e.target.value })}
            />
            {message("forMinutes")}
            <span className="field-hint" id="al-for-hint">
              In minutes. 0 fires the moment the condition is true. This is the
              setting that decides whether anyone still reads these alerts in a
              month — lag jumps on every rebalance, and a rule that fires on the
              jump gets muted.
            </span>
          </div>
        )}

        {draft.kind === "offline_partitions" && (
          <p className="table-note">
            This one has no waiting period on purpose. A partition with no leader
            is refusing writes right now, and there is no version of that worth
            smoothing over.
          </p>
        )}

        <div className="field">
          <label className="field-label" htmlFor="al-name">
            What should Kavka call it?
          </label>
          <input
            id="al-name"
            ref={bind("name")}
            type="text"
            className={cls("name")}
            value={draft.name}
            placeholder={suggestName(built)}
            autoComplete="off"
            aria-describedby="al-name-hint"
            onChange={(e) => edit("name", { name: e.target.value })}
          />
          <span className="field-hint" id="al-name-hint">
            This is what the notification says and what the log lists. Leave it
            empty and Kavka names it after what it watches.
          </span>
        </div>

        <div className="modal-actions">
          <button
            type="button"
            className="btn"
            disabled={busy}
            title={busy ? "Kavka is saving this rule" : undefined}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={busy}
            aria-busy={busy}
            title={busy ? "Kavka is saving this rule" : undefined}
          >
            <span className="btn-busy-slot" aria-hidden="true">
              {busy ? <span className="spinner" /> : null}
            </span>
            {rule === null ? "Add rule" : "Save rule"}
          </button>
        </div>
      </form>
    </Overlay>
  );
}
