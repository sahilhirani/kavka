import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  clusterConnect,
  configDiff,
  errorMessage,
  profilesList,
  topicsList,
  type ConfigDiffRow,
  type ConnectionProfile,
} from "./api";
import { copyText } from "./clipboard";
import { useDangerSignal, type DangerReport } from "./danger";
import { ErrorBanner } from "./ProfileEditor";
import { EnvChip } from "./environments";

/**
 * CONFIG DIFF — "why does it work in staging".
 *
 * The screen is one table, and everything interesting about it is in what the
 * table is allowed to say.
 *
 *  - A SETTING HAS THREE STATES, NOT TWO. `retention.ms = 604800000` set on the
 *    topic and `retention.ms = 604800000` inherited from the broker are the
 *    same value and a different fact: copying the first to another cluster
 *    keeps it, copying the second gets whatever THAT cluster's brokers think.
 *    So a default is rendered muted with the words "broker default" beside it,
 *    exactly as the topic's own config table does — never as a bare value.
 *  - `differs` IS THE CORE'S VERDICT, not a string comparison here. One rule,
 *    one place; a second opinion in the UI is how the row that is highlighted
 *    and the row that is counted come to disagree.
 *  - THE DEFAULT VIEW IS THE DIFFERENCES. A topic has dozens of settings and
 *    two topics usually agree about nearly all of them; leading with 60 matching
 *    rows buries the four that matter. The rest are one checkbox away, and the
 *    count of what is hidden is always on screen.
 *
 * V1 IS TOPICS ONLY. The wire type allows a null topic — a broker-level diff —
 * and the core rejects it by name; `configDiff` in api.ts does not offer the
 * shape at all, and neither does this form.
 */

interface ConfigDiffViewProps {
  /** The connection this workspace is showing — side A's default. */
  profile: ConnectionProfile;
  /** The topic list already loaded for side A, for its suggestions. */
  topics: string[];
  /** Preselected side-A topic, when the user came from one. */
  initialTopic: string | null;
  onBack: () => void;
  onDanger: DangerReport;
}

interface Side {
  profileId: string;
  topic: string;
}

export default function ConfigDiffView({
  profile,
  topics,
  initialTopic,
  onBack,
  onDanger,
}: ConfigDiffViewProps) {
  const [profiles, setProfiles] = useState<ConnectionProfile[] | null>(null);
  const [a, setA] = useState<Side>({
    profileId: profile.id,
    topic: initialTopic ?? "",
  });
  const [b, setB] = useState<Side>({
    profileId: profile.id,
    topic: "",
  });
  const [rows, setRows] = useState<ConfigDiffRow[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showSame, setShowSame] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<number | null>(null);
  /** Topic suggestions per profile, best-effort — a picker, never a gate. */
  const [suggestions, setSuggestions] = useState<Record<string, string[]>>({
    [profile.id]: topics,
  });

  useDangerSignal(error !== null, onDanger);

  useEffect(() => {
    let live = true;
    void profilesList()
      .then((list) => {
        if (live) setProfiles(list);
      })
      .catch((err: unknown) => {
        if (live) setError(errorMessage(err));
      });
    return () => {
      live = false;
    };
  }, []);

  useEffect(
    () => () => {
      if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    },
    [],
  );

  /**
   * Best-effort topic names for a connection the user just picked.
   *
   * A connection this window has never opened cannot answer, so this opens it
   * first — and if that fails it stays quiet: the topic box is a free-text
   * field and works without suggestions. A picker that raises a banner because
   * it could not offer autocomplete would be reporting its own convenience as
   * the user's problem.
   */
  const loadSuggestions = useCallback(async (profileId: string) => {
    try {
      const list = await topicsList(profileId);
      setSuggestions((prev) => ({ ...prev, [profileId]: list.map((t) => t.name) }));
      return;
    } catch {
      /* not connected in this window, probably */
    }
    try {
      await clusterConnect(profileId);
      const list = await topicsList(profileId);
      setSuggestions((prev) => ({ ...prev, [profileId]: list.map((t) => t.name) }));
    } catch {
      /* stays a free-text field */
    }
  }, []);

  const nameOf = useCallback(
    (id: string) => profiles?.find((p) => p.id === id)?.name ?? id,
    [profiles],
  );

  const compare = useCallback(async () => {
    if (a.topic.trim().length === 0 || b.topic.trim().length === 0) {
      setError(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setRows(await configDiff(a.profileId, a.topic.trim(), b.profileId, b.topic.trim()));
    } catch (err) {
      setRows(null);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [a, b]);

  const differing = useMemo(
    () => (rows ?? []).filter((r) => r.differs),
    [rows],
  );
  const shown = showSame ? (rows ?? []) : differing;
  const hidden = (rows?.length ?? 0) - differing.length;

  /**
   * The whole table as text, aligned with spaces so it survives a paste into
   * a ticket, a terminal or a chat window. Copy-as-text rather than a file:
   * this is a thing you put in an incident channel, not a thing you archive.
   */
  const asText = useCallback(() => {
    const head = [
      `# ${nameOf(a.profileId)} / ${a.topic}  vs  ${nameOf(b.profileId)} / ${b.topic}`,
      "",
    ];
    const width = shown.reduce((max, r) => Math.max(max, r.name.length), 8);
    const cell = (value: string | null, isDefault: boolean) =>
      value === null ? "(unset)" : isDefault ? `${value} (broker default)` : value;
    const body = shown.map(
      (r) =>
        `${r.differs ? "≠" : " "} ${r.name.padEnd(width)}  ${cell(
          r.a,
          r.a_is_default,
        )}  |  ${cell(r.b, r.b_is_default)}`,
    );
    return head.concat(body).join("\n");
  }, [shown, a, b, nameOf]);

  const copyAll = useCallback(async () => {
    const ok = await copyText(asText());
    setCopied(ok ? "Copied as text" : "Kavka couldn't reach the clipboard");
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => setCopied(null), 2400);
  }, [asText]);

  const ready = a.topic.trim().length > 0 && b.topic.trim().length > 0;
  const sameTopic =
    a.profileId === b.profileId && a.topic.trim() === b.topic.trim();

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
              onClick={onBack}
            >
              ← Topics
            </button>
            <span className="topic-name">Compare configurations</span>
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={rows === null || shown.length === 0}
              title={
                rows === null
                  ? "Compare two topics first"
                  : shown.length === 0
                    ? "There is nothing on screen to copy"
                    : "Copy this table as plain text, for a ticket or a chat"
              }
              onClick={() => void copyAll()}
            >
              Copy as text
            </button>
          </div>
        </div>

        <p className="table-note">
          Two topics, side by side, however far apart their clusters are. A
          value Kavka shows as a <em>broker default</em> is not set on the topic
          at all — it is whatever that cluster's brokers currently compute, so
          it can change under you without anyone editing the topic.
        </p>

        <div className="diff-picker">
          <SidePicker
            legend="Left"
            side={a}
            profiles={profiles}
            suggestions={suggestions}
            listId="cd-a-topics"
            onChange={(next) => {
              setA(next);
              setRows(null);
              if (next.profileId !== a.profileId)
                void loadSuggestions(next.profileId);
            }}
          />
          <SidePicker
            legend="Right"
            side={b}
            profiles={profiles}
            suggestions={suggestions}
            listId="cd-b-topics"
            onChange={(next) => {
              setB(next);
              setRows(null);
              if (next.profileId !== b.profileId)
                void loadSuggestions(next.profileId);
            }}
          />
          <div className="seekbar-field seekbar-actions">
            <span className="seekbar-label" aria-hidden="true">
              &nbsp;
            </span>
            <button
              type="button"
              className="btn btn-primary btn-swap"
              disabled={!ready || busy || sameTopic}
              aria-busy={busy || undefined}
              title={
                !ready
                  ? "Name a topic on both sides"
                  : sameTopic
                    ? "Those are the same topic on the same connection — pick two different ones"
                    : busy
                      ? "Kavka is reading both topics' settings"
                      : undefined
              }
              onClick={() => void compare()}
            >
              <span className="btn-swap-face">
                Compare
              </span>
              <span className="btn-swap-face btn-swap-busy">
                <span className="spinner" aria-hidden="true" />
                Compare
              </span>
            </button>
          </div>
        </div>

        <span className="inspector-copied" role="status">
          {copied ?? ""}
        </span>
      </section>

      {rows !== null && (
        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              Settings
              <span className="panel-count">
                {differing.length} differ
                {!showSame && hidden > 0 ? ` · ${hidden} identical hidden` : ""}
              </span>
            </h2>
            <div className="panel-tools">
              <label className="check-row">
                <input
                  type="checkbox"
                  checked={showSame}
                  onChange={(e) => setShowSame(e.target.checked)}
                />
                <span>Show settings that match</span>
              </label>
            </div>
          </div>

          {/* The gutter carries the sign, like every other diff in the app:
              `≠` on a row the two topics disagree about. Static table, so no
              role="grid". */}
          <div className="table-wrap">
            <table className="data-table data-table-sign">
              <caption className="sr-only">
                Configuration of {a.topic} on {nameOf(a.profileId)} compared with{" "}
                {b.topic} on {nameOf(b.profileId)}
              </caption>
              <thead>
                <tr>
                  <th scope="col" className="ledger-gutter">
                    <span className="sr-only">Differs</span>
                  </th>
                  <th scope="col">Setting</th>
                  <th scope="col">
                    {nameOf(a.profileId)} · {a.topic}
                  </th>
                  <th scope="col">
                    {nameOf(b.profileId)} · {b.topic}
                  </th>
                </tr>
              </thead>
              <tbody>
                {shown.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="cell-empty">
                      {rows.length === 0
                        ? "Neither topic reported any settings. That usually means the account can't describe topic configs on one of the two clusters."
                        : "These two topics agree about every setting either of them reports. Turn on “Show settings that match” to see them."}
                    </td>
                  </tr>
                ) : (
                  shown.map((row) => (
                    <tr key={row.name} className={row.differs ? "diff-change" : ""}>
                      <td className="ledger-gutter" aria-hidden="true">
                        {row.differs ? "≠" : ""}
                      </td>
                      <td className="cell-mono">{row.name}</td>
                      <ValueCell value={row.a} isDefault={row.a_is_default} />
                      <ValueCell value={row.b} isDefault={row.b_is_default} />
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>
      )}
    </>
  );
}

/** One half of the picker: a connection and a topic on it. */
function SidePicker({
  legend,
  side,
  profiles,
  suggestions,
  listId,
  onChange,
}: {
  legend: string;
  side: Side;
  profiles: ConnectionProfile[] | null;
  suggestions: Record<string, string[]>;
  listId: string;
  onChange: (next: Side) => void;
}) {
  const names = suggestions[side.profileId] ?? [];
  const chosen = profiles?.find((p) => p.id === side.profileId) ?? null;
  return (
    <fieldset className="diff-side">
      <legend className="diff-side-legend">{legend}</legend>
      <label className="seekbar-field">
        <span className="seekbar-label">Connection</span>
        <select
          value={side.profileId}
          onChange={(e) => onChange({ ...side, profileId: e.target.value })}
        >
          {(profiles ?? []).map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} — {p.environment}
            </option>
          ))}
          {profiles === null && <option value={side.profileId}>Loading…</option>}
        </select>
      </label>
      <label className="seekbar-field seekbar-field-wide">
        <span className="seekbar-label">Topic</span>
        <input
          type="text"
          className="input-mono"
          value={side.topic}
          list={listId}
          placeholder="orders.v2"
          autoComplete="off"
          spellCheck={false}
          onChange={(e) => onChange({ ...side, topic: e.target.value })}
        />
        <datalist id={listId}>
          {names.map((n) => (
            <option key={n} value={n} />
          ))}
        </datalist>
      </label>
      {chosen !== null && <EnvChip env={chosen.environment} />}
    </fieldset>
  );
}

/**
 * One side's value. A default is muted and says so; an absent value is `∅`,
 * never the word "null" and never an empty cell — an empty cell is a rendering
 * failure, and this one is a fact.
 */
function ValueCell({
  value,
  isDefault,
}: {
  value: string | null;
  isDefault: boolean;
}) {
  return (
    <td className={`cell-mono${isDefault ? " config-inherited" : ""}`}>
      {value === null ? (
        <span
          className="absent"
          title="This topic reports no value for this setting at all."
        >
          ∅
        </span>
      ) : (
        value
      )}
      {isDefault && value !== null && (
        <span
          className="cell-tag"
          title="Not set on the topic — this is what the cluster's brokers compute right now, and it can change without anyone editing the topic."
        >
          {" "}
          broker default
        </span>
      )}
    </td>
  );
}
