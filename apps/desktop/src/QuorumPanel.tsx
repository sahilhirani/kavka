import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  quorumDescribe,
  type ConnectionProfile,
  type QuorumInfo,
  type ReplicaState,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { formatAge, groupDigits } from "./format";
import { ErrorBanner } from "./ProfileEditor";

/**
 * THE KRAFT QUORUM, ON THE CLUSTER OVERVIEW.
 *
 * Kafka's own metadata lives in a log, and a small group of servers holds it.
 * That is a thing most Kafka GUIs either hide completely or render as a wall of
 * raw DescribeQuorum fields, and both are wrong for the same reason: the
 * quorum is the first thing to look at when "the cluster is up but nothing
 * works", and it is also the piece of Kafka a support engineer is least likely
 * to have been taught. So this panel leads with a sentence saying what the
 * quorum IS, and only then shows the numbers.
 *
 * Three rules it inherits rather than invents:
 *
 *  - Law 2. Every replica's state is a dot AND a word AND the numbers it was
 *    derived from. The lag against the leader gets the same three channels the
 *    consumer-group lag column has — a figure, a proportional track and a word.
 *  - §2's status-adjacency law. `--accent` may not appear beside a health or
 *    lag column, so the leader badge is neutral. It is a badge, not a status.
 *  - §7. A cluster that cannot answer at all gets a sentence about why, not a
 *    broker string: a ZooKeeper-mode cluster has no quorum to describe and
 *    saying "unsupported version" to a novice is saying nothing.
 */

/** How stale a fetch has to get before it stops meaning "healthy". */
const CONTACT_STALE_MS = 30_000;

/**
 * The lag vocabulary for a METADATA log, which is deliberately not the one
 * GroupsTab uses for a data topic.
 *
 * A busy topic takes a million records a minute, so a consumer 1 000 behind is
 * "keeping up". The metadata log takes a handful of records when someone
 * creates a topic, so a voter 1 000 behind is a voter that has been out of
 * contact for a long time. Same words, different scale, on purpose — and the
 * two live apart rather than sharing a constant that would have to be wrong
 * for one of them.
 */
function quorumLagWord(lag: number): {
  word: string;
  tone: "ok" | "warn" | "danger";
} {
  if (lag <= 0) return { word: "caught up", tone: "ok" };
  if (lag <= 10) return { word: "keeping up", tone: "ok" };
  if (lag <= 1_000) return { word: "behind", tone: "warn" };
  return { word: "far behind", tone: "danger" };
}

interface ReplicaHealth {
  word: string;
  tone: "ok" | "warn" | "danger" | "quiet";
  why: string;
}

/** Dot, word, and one sentence on hover AND focus — never colour alone. */
function replicaHealth(
  replica: ReplicaState,
  leaderOffset: number,
  isLeader: boolean,
): ReplicaHealth {
  if (isLeader)
    return {
      word: "Leading",
      tone: "ok",
      why: "This is the server the rest of the quorum copies from right now.",
    };
  const fetchAge = replica.last_fetch_age_ms;
  if (fetchAge === null)
    return {
      word: "Not reported",
      tone: "quiet",
      why: "The leader didn't say when it last heard from this replica, so Kavka can't tell you whether it is keeping up. That is a gap in the answer, not a fault in the replica.",
    };
  if (fetchAge > CONTACT_STALE_MS)
    return {
      word: "Out of contact",
      tone: "danger",
      why: `The leader hasn't heard from it in ${formatAge(fetchAge)}. If it is a voter, the quorum is running with less margin than it looks — check whether that server is up.`,
    };
  const lag = Math.max(0, leaderOffset - replica.log_end_offset);
  if (lag === 0)
    return {
      word: "In sync",
      tone: "ok",
      why: "It holds exactly what the leader holds, and it answered within the last few seconds.",
    };
  const caughtUp = replica.last_caught_up_age_ms;
  if (caughtUp !== null && caughtUp > CONTACT_STALE_MS)
    return {
      word: "Falling behind",
      tone: "warn",
      why: `It is still answering the leader, but it was last level with it ${formatAge(caughtUp)}. It is copying more slowly than the log is growing.`,
    };
  return {
    word: "Catching up",
    tone: "warn",
    why: "It is behind the leader but still fetching, which is the normal state for a few seconds after a restart.",
  };
}

/**
 * A cluster with no KRaft quorum refuses the call rather than answering with
 * an empty one, and the broker's word for that is a protocol version number.
 * That is true and useless — the fact the user needs is that this cluster is
 * still on ZooKeeper, which is not a fault and not something to fix from here.
 */
function noQuorumNote(raw: string): string | null {
  const text = raw.toLowerCase();
  return text.includes("unsupported_version") ||
    text.includes("unsupported version") ||
    text.includes("describequorum") ||
    text.includes("not a kraft") ||
    text.includes("zookeeper")
    ? "This cluster doesn't report a KRaft quorum. That normally means it still keeps its metadata in ZooKeeper, which is a cluster-wide choice made when it was built — not a permission you're missing and not something to change from here. Everything else in Kavka works the same either way."
    : null;
}

interface QuorumPanelProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
}

export default function QuorumPanel({ profile, onDanger }: QuorumPanelProps) {
  const [quorum, setQuorum] = useState<QuorumInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  // A quorum Kavka simply can't describe is a fact about the cluster, not a
  // danger the prod damper should react to — so only an unexplained failure
  // reaches the signal.
  const note = failed && error !== null ? noQuorumNote(error) : null;
  useDangerSignal(error !== null && note === null, onDanger);

  const fetchQuorum = useCallback(async () => {
    const mine = ++seq.current;
    setLoading(true);
    try {
      const next = await quorumDescribe(profile.id);
      if (seq.current !== mine) return;
      setQuorum(next);
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
    void fetchQuorum();
    return () => {
      seq.current += 1;
    };
  }, [fetchQuorum]);

  /** Voters first, then observers, each with the role it plays. */
  const rows = useMemo(() => {
    if (quorum === null) return [];
    return [
      ...quorum.voters.map((r) => ({ replica: r, voter: true })),
      ...quorum.observers.map((r) => ({ replica: r, voter: false })),
    ];
  }, [quorum]);

  const leaderOffset = useMemo(() => {
    if (quorum === null) return 0;
    const leader = quorum.voters.find((v) => v.replica_id === quorum.leader_id);
    // Without the leader in its own voter list the high watermark is the best
    // honest baseline: it is the offset the quorum has already agreed on.
    return leader?.log_end_offset ?? quorum.high_watermark;
  }, [quorum]);

  /** The widest gap on screen, so the tracks are proportional to each other. */
  const maxLag = useMemo(
    () =>
      rows.reduce(
        (worst, r) =>
          Math.max(worst, Math.max(0, leaderOffset - r.replica.log_end_offset)),
        0,
      ),
    [rows, leaderOffset],
  );

  const outOfContact = rows.filter(
    (r) =>
      r.replica.replica_id !== quorum?.leader_id &&
      r.replica.last_fetch_age_ms !== null &&
      r.replica.last_fetch_age_ms > CONTACT_STALE_MS,
  ).length;

  return (
    <>
      {/* An unrecognised failure renders where its fix is — inside the panel
          it is about, not in the app's global banner. */}
      {error !== null && note === null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Metadata quorum
            {quorum !== null && (
              <span className="panel-count">
                {quorum.voters.length} voter
                {quorum.voters.length === 1 ? "" : "s"}
                {quorum.observers.length > 0
                  ? ` · ${quorum.observers.length} observer${
                      quorum.observers.length === 1 ? "" : "s"
                    }`
                  : ""}
                {outOfContact > 0 ? ` · ${outOfContact} out of contact` : ""}
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
                loading
                  ? "Kavka is already asking the controller about the quorum"
                  : "Ask the controller where the quorum stands right now"
              }
              onClick={() => void fetchQuorum()}
            >
              Refresh
            </button>
          </div>
        </div>

        {/* The explainer comes BEFORE the numbers, because the numbers mean
            nothing to someone who has never had to look at them. */}
        <p className="table-note">
          Kafka keeps its own bookkeeping — which topics exist, which broker
          leads which partition — in a log of its own. The quorum is the small
          group of servers holding that log: the <strong>voters</strong> elect
          one leader among themselves and copy from it, and{" "}
          <strong>observers</strong> copy it without ever getting a vote.
        </p>
        <p className="table-note">
          The high watermark is how far into that log the voters have agreed. A
          replica whose log end offset sits behind it hasn't caught up yet —
          which is normal for a few seconds after a restart, and worth looking
          into if it stays that way.
        </p>

        {quorum === null ? (
          <p className="table-note">
            {loading
              ? "Asking the controller about the quorum…"
              : (note ??
                "Kavka couldn't read this cluster's quorum. Describing it needs Describe on the cluster, and it is answered by the active controller — ask whoever issued the credentials for that permission.")}
          </p>
        ) : (
          <>
            <div className="stat-grid">
              <div className="stat">
                <span className="stat-label">Quorum leader</span>
                <span className="stat-value">{quorum.leader_id}</span>
              </div>
              <div className="stat">
                <span className="stat-label">Leader epoch</span>
                <span className="stat-value">{quorum.leader_epoch}</span>
              </div>
              <div className="stat">
                <span className="stat-label">High watermark</span>
                <span className="stat-value">
                  {groupDigits(quorum.high_watermark)}
                </span>
              </div>
              <div className="stat">
                <span className="stat-label">Voters</span>
                <span className="stat-value">{quorum.voters.length}</span>
              </div>
            </div>

            <p className="table-note">
              An epoch counts elections, not time: it goes up by one every time
              the voters pick a new leader. A number that keeps climbing means
              the quorum keeps re-electing, which is worth chasing down.
            </p>

            {/* The gutter carries the replica id — the row's address in
                Kafka's own vocabulary (§2). Static table, so no role="grid". */}
            <div className="table-wrap">
              {loading && <div className="table-loading" role="presentation" />}
              <table className="data-table">
                <caption className="sr-only">
                  The metadata quorum: {quorum.voters.length} voters and{" "}
                  {quorum.observers.length} observers, with how far each is
                  behind the leader
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="ledger-gutter">
                      ID
                    </th>
                    <th scope="col">Role</th>
                    <th scope="col" className="col-num">
                      Log end offset
                    </th>
                    <th scope="col" className="col-lag">
                      Behind leader
                    </th>
                    <th scope="col">Last heard from</th>
                    <th scope="col">Last level</th>
                    <th scope="col">Health</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.length === 0 ? (
                    <tr>
                      <td colSpan={7} className="cell-empty">
                        The controller answered with no voters at all, which
                        shouldn't be possible on a running cluster — try
                        refreshing, then reconnecting.
                      </td>
                    </tr>
                  ) : (
                    rows.map(({ replica, voter }) => (
                      <QuorumRow
                        key={`${voter ? "v" : "o"}:${replica.replica_id}`}
                        replica={replica}
                        voter={voter}
                        isLeader={replica.replica_id === quorum.leader_id}
                        leaderOffset={leaderOffset}
                        maxLag={maxLag}
                      />
                    ))
                  )}
                </tbody>
              </table>
            </div>

            <p className="table-note">
              Losing one voter out of {quorum.voters.length} is survivable; the
              quorum needs more than half of them answering to elect a leader or
              write anything new. Observers never count towards that — they only
              read.
            </p>
          </>
        )}
      </section>
    </>
  );
}

/**
 * One replica. The role is a neutral badge rather than an accent one: §2's
 * status-adjacency law forbids `--accent` in a column beside a health or lag
 * column, and this row has both.
 */
function QuorumRow({
  replica,
  voter,
  isLeader,
  leaderOffset,
  maxLag,
}: {
  replica: ReplicaState;
  voter: boolean;
  isLeader: boolean;
  leaderOffset: number;
  maxLag: number;
}) {
  const lag = Math.max(0, leaderOffset - replica.log_end_offset);
  const { word, tone } = quorumLagWord(lag);
  const pct = maxLag > 0 ? Math.min(100, (lag / maxLag) * 100) : 0;
  const health = replicaHealth(replica, leaderOffset, isLeader);

  return (
    <tr>
      <td className="ledger-gutter">{replica.replica_id}</td>
      <td>
        <span className="role-word">{voter ? "Voter" : "Observer"}</span>
        {isLeader && (
          <span
            className="role-chip"
            title="The voter the others copy from. Kafka picked it in an election, and it can change without anyone doing anything."
          >
            leader
          </span>
        )}
      </td>
      {/* An offset is a literal you could paste into a command, so it is mono
          with tabular figures. The lag beside it is a quantity Kavka computed
          from two of them, so it is not (§4). */}
      <td className="col-num cell-mono cell-mono-num">
        {groupDigits(replica.log_end_offset)}
      </td>
      <td className="col-lag">
        {isLeader ? (
          <span className="cell-tag">the leader</span>
        ) : (
          <span className={`lag lag-${tone}`}>
            <span className="lag-number">{groupDigits(lag)}</span>
            <span className="lag-track" aria-hidden="true">
              <span className="lag-fill" style={{ width: `${pct}%` }} />
            </span>
            <span className="lag-word">{word}</span>
          </span>
        )}
      </td>
      <td className="cell-tag">
        {replica.last_fetch_age_ms === null ? (
          <span
            className="absent"
            title="The leader didn't report when it last heard from this replica."
          >
            ∅
          </span>
        ) : (
          formatAge(replica.last_fetch_age_ms)
        )}
      </td>
      <td className="cell-tag">
        {replica.last_caught_up_age_ms === null ? (
          <span
            className="absent"
            title="The leader didn't report when this replica was last level with it."
          >
            ∅
          </span>
        ) : (
          formatAge(replica.last_caught_up_age_ms)
        )}
      </td>
      <td>
        {/* Focusable, so the sentence behind the word reaches a keyboard user
            as well as a pointer one. */}
        <span
          className={`health health-${health.tone}`}
          title={health.why}
          tabIndex={0}
        >
          <i className="dot" aria-hidden="true" />
          {health.word}
        </span>
      </td>
    </tr>
  );
}
