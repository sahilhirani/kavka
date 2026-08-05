import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  historyGroups,
  historyQuery,
  metricsQuery,
  metricsStatus,
  samplerStatus,
  splitSeries,
  topicSeries,
  HISTORY_RETENTION_DAYS,
  METRIC_SERIES,
  type ConnectionProfile,
  type HistoryGroup,
  type LagSample,
  type MetricPoint,
  type MetricsStatus,
  type SamplerStatus,
} from "./api";
import LineChart, { readoutTime, type ChartPoint, type ChartSeries } from "./Chart";
import { useDangerSignal, type DangerReport } from "./danger";
import { approxCount, formatAge, groupDigits } from "./format";
import { Term } from "./Glossary";
import { useI18n } from "./i18n";
import Perch from "./Perch";
import {
  bucketWidth,
  formatSeriesValue,
  formatSpan,
  HISTORY_RANGES,
  MAX_POINTS,
  METRICS_RANGES,
  METRICS_WINDOW_MS,
  seriesGloss,
  seriesLabel,
  type HistoryRange,
} from "./monitoring";
import { ErrorBanner } from "./ProfileEditor";

/**
 * MONITORING — the tab that answers "is this getting worse?"
 *
 * Everything else in Kavka reads the cluster as it is right now. This tab is the
 * only one that reads TIME, and time is the one axis Kafka does not keep for
 * anybody: a broker will tell you the lag this second and has no memory of the
 * lag an hour ago. So the lag history here is Kavka's own — sampled while this
 * connection was up, stored locally, pruned at seven days.
 *
 * That has an honesty cost this whole tab is built around paying:
 *
 *  - **A gap in a line is a gap in Kavka's attendance, not an outage.** The
 *    sampler panel leads, before any chart, because "the line stops at 4pm"
 *    means "the laptop was closed at 4pm" far more often than it means anything
 *    about the cluster.
 *  - **A downsampled point is the WORST reading in its bucket, never the mean.**
 *    Stated on the axis of every lag chart. An averaged spike is an erased
 *    spike, and the spike is the entire reason anyone opens this page.
 *  - **Metrics are a different source with a different failure mode.** They come
 *    from an HTTP exporter, not the brokers, so "no throughput data" has four
 *    distinct causes and this tab tells them apart rather than showing one
 *    shrug for all of them.
 *
 * §2's status-adjacency law applies throughout: `--accent` means live, so it
 * never appears next to a health reading here. The charts use `--series-*`,
 * which is a different token family on purpose.
 */

/** Milliseconds Kavka waits before asking the sampler how it's doing again. */
const SAMPLER_POLL_MS = 15_000;

/** Above this, a topic's partitions start hidden — see `defaultHidden`. */
const AUTO_SHOW_PARTITIONS = 6;
const CROWDED_PARTITIONS = 12;

interface MonitoringTabProps {
  profile: ConnectionProfile;
  onDanger: DangerReport;
  /** Opens the connection form — which means disconnecting. Said out loud. */
  onEditConnection: () => void;
}

/** One partition's series, keyed the way the toggle set keys it. */
function seriesKey(topic: string, partition: number): string {
  return `${topic}:${partition}`;
}

interface LagSummary {
  lastTs: number | null;
  /** Sum of each partition's most recent lag in the window. */
  lagNow: number;
  /** Sum of each partition's oldest lag in the window — the trend's baseline. */
  lagThen: number;
  peak: LagSample | null;
  partitions: number;
  samples: number;
  /** True when at least one partition has never committed in this window. */
  anyUncommitted: boolean;
}

/**
 * The current-versus-history reading, computed per partition and then summed.
 *
 * Per partition and not per sample, because the samples are downsampled
 * independently: taking "the newest sample overall" would report one
 * partition's lag as the group's. Summing each partition's own latest reading is
 * the same arithmetic the group detail page does against live offsets.
 */
function summarise(samples: LagSample[]): LagSummary {
  const newest = new Map<string, LagSample>();
  const oldest = new Map<string, LagSample>();
  let peak: LagSample | null = null;
  let lastTs: number | null = null;
  let anyUncommitted = false;

  for (const s of samples) {
    const key = seriesKey(s.topic, s.partition);
    const n = newest.get(key);
    if (n === undefined || s.ts_ms > n.ts_ms) newest.set(key, s);
    const o = oldest.get(key);
    if (o === undefined || s.ts_ms < o.ts_ms) oldest.set(key, s);
    if (s.lag === null) anyUncommitted = true;
    else if (peak === null || s.lag > (peak.lag ?? -1)) peak = s;
    if (lastTs === null || s.ts_ms > lastTs) lastTs = s.ts_ms;
  }

  const sum = (map: Map<string, LagSample>) =>
    [...map.values()].reduce((total, s) => total + (s.lag ?? 0), 0);

  return {
    lastTs,
    lagNow: sum(newest),
    lagThen: sum(oldest),
    peak,
    partitions: newest.size,
    samples: samples.length,
    anyUncommitted,
  };
}

/** Law 2 again: a trend is a word and an arrow, never a colour. */
function trendWord(summary: LagSummary): {
  word: string;
  glyph: string;
  tone: "ok" | "warn" | "quiet";
} {
  if (summary.partitions === 0)
    return { word: "nothing recorded", glyph: "—", tone: "quiet" };
  const { lagNow, lagThen } = summary;
  if (lagNow > lagThen * 1.1 && lagNow - lagThen > 1)
    return { word: "rising", glyph: "↑", tone: "warn" };
  if (lagNow < lagThen * 0.9)
    return { word: "falling", glyph: "↓", tone: "ok" };
  return { word: "steady", glyph: "→", tone: "ok" };
}

/**
 * Which partitions start hidden on a crowded topic.
 *
 * A 64-partition topic drawn as 64 lines is a solid block, so the six with the
 * worst peak lag are shown and the rest start off — but every one of them stays
 * in the legend with its own toggle, because hiding a series without saying so
 * is the "where did it go" bug §5.2 keeps out of the topics table.
 */
function defaultHidden(samples: LagSample[]): Set<string> {
  const peakByKey = new Map<string, { topic: string; partition: number; peak: number }>();
  for (const s of samples) {
    const key = seriesKey(s.topic, s.partition);
    const seen = peakByKey.get(key);
    const lag = s.lag ?? 0;
    if (seen === undefined)
      peakByKey.set(key, { topic: s.topic, partition: s.partition, peak: lag });
    else if (lag > seen.peak) seen.peak = lag;
  }
  const byTopic = new Map<string, Array<{ key: string; peak: number }>>();
  for (const [key, v] of peakByKey) {
    const list = byTopic.get(v.topic) ?? [];
    list.push({ key, peak: v.peak });
    byTopic.set(v.topic, list);
  }
  const hidden = new Set<string>();
  for (const list of byTopic.values()) {
    if (list.length <= CROWDED_PARTITIONS) continue;
    list.sort((a, b) => b.peak - a.peak);
    for (const item of list.slice(AUTO_SHOW_PARTITIONS)) hidden.add(item.key);
  }
  return hidden;
}

export default function MonitoringTab({
  profile,
  onDanger,
  onEditConnection,
}: MonitoringTabProps) {
  const [range, setRange] = useState<HistoryRange>(HISTORY_RANGES[0]);
  const [groups, setGroups] = useState<HistoryGroup[] | null>(null);
  /**
   * The history read FAILED, which is not the same fact as a history file
   * with nothing in it. Kept separate from `error` because that one is shared
   * with the metrics status call and is cleared by dismissing the banner —
   * neither of which makes the readings appear.
   */
  const [historyFailed, setHistoryFailed] = useState(false);
  const [group, setGroup] = useState<string | null>(null);
  const [samples, setSamples] = useState<LagSample[] | null>(null);
  /** The same distinction one level down: this window's read, not the index. */
  const [samplesFailed, setSamplesFailed] = useState(false);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [loadingLag, setLoadingLag] = useState(false);

  const [sampler, setSampler] = useState<SamplerStatus | null>(null);
  const [metricRange, setMetricRange] = useState<HistoryRange>(METRICS_RANGES[0]);
  const [metrics, setMetrics] = useState<MetricsStatus | null>(null);
  const [metricTopic, setMetricTopic] = useState<string | null>(null);
  const [points, setPoints] = useState<Record<string, MetricPoint[]>>({});
  const [loadingMetrics, setLoadingMetrics] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);

  const groupsSeq = useRef(0);
  const lagSeq = useRef(0);
  const metricsSeq = useRef(0);
  const pointsSeq = useRef(0);
  /** What the auto-hide decision was last made for. See `defaultHidden`. */
  const hiddenFor = useRef<string>("");

  const { t } = useI18n();
  useDangerSignal(error !== null, onDanger);

  /**
   * The window every LAG query on this page shares, so the charts line up on
   * one x axis rather than each ending at its own `Date.now()`. Recomputed
   * exactly when the range changes or Refresh is pressed — a moving window
   * would refetch on every render.
   */
  // `nonce` is the Refresh button and `range` is the picker; nothing else may
  // move the window, or the charts under it drift apart. Reading the clock
  // inside a memo is deliberate: React is free to recompute it, and a window
  // that shifts by a few milliseconds costs nothing, where a window recomputed
  // on every render would refetch forever.
  const frame = useMemo(() => {
    const to = Date.now();
    return { from: to - range.ms, to };
  }, [range, nonce]);

  /**
   * The metrics window, which is deliberately NOT the same one.
   *
   * The two halves of this page keep different amounts of time and there is no
   * honest way to pretend otherwise: lag is a week on disk, metrics are
   * `METRICS_WINDOW_MS` in memory that start empty at every connect. Sharing
   * one picker meant a lag chart showing seven days beside a throughput chart
   * showing one day of line and six of blank — which reads as an outage. So
   * metrics get their own picker, capped at what the ring can actually hold
   * (`METRICS_RANGES`), and the two axes are allowed to differ where the data
   * does.
   */
  const metricFrame = useMemo(() => {
    const to = Date.now();
    return { from: to - metricRange.ms, to };
  }, [metricRange, nonce]);

  // ── The sampler, polled ─────────────────────────────────────────────────
  useEffect(() => {
    let cancelled = false;
    const ask = () => {
      samplerStatus(profile.id)
        .then((next) => {
          if (!cancelled) setSampler(next);
        })
        // A status Kavka can't read is reported by the panel's own empty state,
        // not as a banner: the sampler failing to answer is not something the
        // user can act on from here, and a banner they can't clear is worse
        // than a sentence they can read.
        .catch(() => undefined);
    };
    ask();
    const timer = globalThis.setInterval(ask, SAMPLER_POLL_MS);
    return () => {
      cancelled = true;
      globalThis.clearInterval(timer);
    };
  }, [profile.id, nonce]);

  // ── Which groups have history ───────────────────────────────────────────
  useEffect(() => {
    const mine = ++groupsSeq.current;
    historyGroups(profile.id)
      .then((list) => {
        if (groupsSeq.current !== mine) return;
        setGroups(list);
        setHistoryFailed(false);
        // The group with the freshest sample is the one someone opening this
        // tab is most likely to be asking about.
        setGroup((prev) => {
          if (prev !== null && list.some((g) => g.group_id === prev)) return prev;
          const best = [...list].sort((a, b) => b.last_ts_ms - a.last_ts_ms)[0];
          return best?.group_id ?? null;
        });
      })
      .catch((err: unknown) => {
        if (groupsSeq.current !== mine) return;
        // The empty array settles "Still checking…"; the flag is what keeps
        // the verdict from reading it as an empty history file and telling
        // someone to wait for samples that are failing to arrive.
        setGroups([]);
        setHistoryFailed(true);
        setError(errorMessage(err));
      });
    return () => {
      groupsSeq.current += 1;
    };
  }, [profile.id, nonce]);

  // ── The lag history for the chosen group and window ─────────────────────
  useEffect(() => {
    if (group === null) {
      setSamples(null);
      return;
    }
    const mine = ++lagSeq.current;
    setLoadingLag(true);
    historyQuery(profile.id, group, null, frame.from, frame.to, MAX_POINTS)
      .then((list) => {
        if (lagSeq.current !== mine) return;
        setSamples(list);
        setSamplesFailed(false);
        const key = `${group}:${range.key}`;
        if (hiddenFor.current !== key) {
          hiddenFor.current = key;
          setHidden(defaultHidden(list));
        }
      })
      .catch((err: unknown) => {
        if (lagSeq.current !== mine) return;
        // Same rule as the index above: no readings and no read are different
        // facts, and "try a longer window" is advice that cannot work when the
        // query is the thing that failed.
        setSamples([]);
        setSamplesFailed(true);
        setError(errorMessage(err));
      })
      .finally(() => {
        if (lagSeq.current === mine) setLoadingLag(false);
      });
    return () => {
      lagSeq.current += 1;
    };
  }, [profile.id, group, range.key, frame]);

  // ── The metrics endpoint's own state ────────────────────────────────────
  useEffect(() => {
    const mine = ++metricsSeq.current;
    metricsStatus(profile.id)
      .then((next) => {
        if (metricsSeq.current !== mine) return;
        setMetrics(next);
      })
      .catch((err: unknown) => {
        if (metricsSeq.current !== mine) return;
        // A status call that fails outright is different from an endpoint that
        // answered badly — the latter comes back as `reachable: false` with the
        // endpoint's own words, and never lands here.
        setMetrics(null);
        setError(errorMessage(err));
      });
    return () => {
      metricsSeq.current += 1;
    };
  }, [profile.id, nonce]);

  /** Per-topic variants the scrape actually publishes, deduped and sorted. */
  const metricTopics = useMemo(() => {
    if (metrics === null) return [];
    const set = new Set<string>();
    for (const name of metrics.series_available) {
      const { topic } = splitSeries(name);
      if (topic !== null) set.add(topic);
    }
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [metrics]);

  /** Every series this page will draw, cluster-level first. */
  const wanted = useMemo(() => {
    if (metrics === null || !metrics.configured || !metrics.reachable) return [];
    const have = new Set(metrics.series_available);
    const out = METRIC_SERIES.filter((s) => have.has(s)) as string[];
    if (metricTopic !== null) {
      for (const base of [
        "bytes_in_per_sec",
        "bytes_out_per_sec",
        "messages_in_per_sec",
        "log_size_bytes",
      ] as const) {
        const name = topicSeries(base, metricTopic);
        if (have.has(name)) out.push(name);
      }
    }
    return out;
  }, [metrics, metricTopic]);

  // JSON, not a joined string, for the same reason ProfileEditor keys its
  // Connect entries that way: a series name ends in a TOPIC name, which is
  // user text, so there is no separator that is safe to split back on. The
  // key is a value to compare; the array is recovered by parsing it.
  const wantedKey = JSON.stringify(wanted);

  useEffect(() => {
    const names = JSON.parse(wantedKey) as string[];
    if (names.length === 0) {
      setPoints({});
      return;
    }
    const mine = ++pointsSeq.current;
    setLoadingMetrics(true);
    Promise.all(
      names.map((name) =>
        metricsQuery(profile.id, name, metricFrame.from, metricFrame.to, MAX_POINTS)
          .then((list) => [name, list] as const)
          // One series the exporter dropped between the status call and this
          // one must not take the other five down with it.
          .catch(() => [name, [] as MetricPoint[]] as const),
      ),
    )
      .then((pairs) => {
        if (pointsSeq.current !== mine) return;
        const next: Record<string, MetricPoint[]> = {};
        for (const [name, list] of pairs) next[name] = list;
        setPoints(next);
      })
      .finally(() => {
        if (pointsSeq.current === mine) setLoadingMetrics(false);
      });
    return () => {
      pointsSeq.current += 1;
    };
  }, [profile.id, wantedKey, metricFrame]);

  const toggleSeries = useCallback((key: string) => {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const refresh = useCallback(() => setNonce((n) => n + 1), []);

  // ── Lag, grouped into one chart per topic ───────────────────────────────

  const byTopic = useMemo(() => {
    const map = new Map<string, Map<number, ChartPoint[]>>();
    for (const s of samples ?? []) {
      const topic = map.get(s.topic) ?? new Map<number, ChartPoint[]>();
      const list = topic.get(s.partition) ?? [];
      list.push({ x: s.ts_ms, y: s.lag });
      topic.set(s.partition, list);
      map.set(s.topic, topic);
    }
    return [...map.entries()]
      .map(([topic, parts]) => ({
        topic,
        series: [...parts.entries()]
          .sort((a, b) => a[0] - b[0])
          .map(([partition, list], i): ChartSeries => ({
            id: seriesKey(topic, partition),
            label: `Partition ${partition}`,
            points: list.sort((a, b) => a.x - b.x),
            tone: (i % 6) + 1,
            hidden: hidden.has(seriesKey(topic, partition)),
          })),
      }))
      .sort((a, b) => a.topic.localeCompare(b.topic));
  }, [samples, hidden]);

  const summary = useMemo(() => summarise(samples ?? []), [samples]);
  const trend = trendWord(summary);
  const bucket = bucketWidth(range.ms);

  const lagNote = (
    <>
      One point is the <strong>highest</strong> lag Kavka recorded in that{" "}
      {formatSpan(bucket)} slice, never an average — a spike that lasted a single
      sample still shows at full height. A break in a line is a stretch where
      Kavka has no reading, which usually means this connection was down.
    </>
  );

  const chosenGroup = groups?.find((g) => g.group_id === group) ?? null;

  /**
   * WHAT THE VERDICT DOESN'T COVER, in the order that matters.
   *
   * A sampler that has stopped outranks the downsampling note, because a chart
   * nobody is feeding is a chart whose last point is not "now" — and every
   * sentence above it is about a moment that may be hours old.
   */
  const samplerCaveat = (() => {
    if (sampler === null) return t("perch.monitoring.caveat.unknownSampler");
    if (!sampler.running) return t("perch.monitoring.caveat.stopped");
    if (
      sampler.last_sample_ms !== null &&
      Date.now() - sampler.last_sample_ms > sampler.interval_ms * 3
    )
      return t("perch.monitoring.caveat.stale", {
        ago: formatAge(Math.max(0, Date.now() - sampler.last_sample_ms)),
      });
    return t("perch.monitoring.caveat.sampled");
  })();

  const lagVerdict = (() => {
    const shared = {
      group: group ?? "",
      lag: approxCount(summary.lagNow),
      partitions: summary.partitions,
      topic: summary.peak?.topic ?? "",
      partition: summary.peak?.partition ?? 0,
      peak: groupDigits(summary.peak?.lag ?? 0),
    };
    // Outranks every branch below it. `noHistory` tells the reader the first
    // points will appear within one interval, which is a promise Kavka cannot
    // keep for readings it could not read — and `noWindow` would blame the
    // window for a file that never opened.
    if (historyFailed || samplesFailed)
      return { tone: "unknown" as const, text: t("perch.monitoring.unread") };
    if (groups !== null && groups.length === 0)
      return {
        tone: "unknown" as const,
        text: t("perch.monitoring.noHistory", {
          interval: formatSpan(sampler?.interval_ms ?? 15_000),
        }),
      };
    if (group === null || summary.partitions === 0)
      return {
        tone: "unknown" as const,
        text: t("perch.monitoring.noWindow", { group: group ?? "" }),
      };
    if (summary.lagNow <= 0)
      return { tone: "ok" as const, text: t("perch.monitoring.caughtUp", shared) };
    if (trend.word === "rising")
      return { tone: "problem" as const, text: t("perch.monitoring.rising", shared) };
    if (trend.word === "falling")
      return { tone: "ok" as const, text: t("perch.monitoring.falling", shared) };
    return { tone: "watch" as const, text: t("perch.monitoring.steady", shared) };
  })();

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      {/* THE SHOWCASE VERDICT. Two facts, in this order: what the numbers say,
          and where they came from. The second is not decoration — a lag chart
          is the one screen in Kavka whose data Kafka does not hold, and a
          reader who doesn't know that will read a gap as an outage. */}
      <Perch
        screen={t("rail.item.monitoring")}
        loading={groups === null}
        tone={lagVerdict.tone}
        caveat={
          <>
            {t("perch.monitoring.origin")} {samplerCaveat}
          </>
        }
      >
        {lagVerdict.text}
      </Perch>

      {/* THE SAMPLER COMES FIRST. Every chart below it is only as trustworthy
          as this panel says it is. */}
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Lag history
            <span className="panel-count">
              kept for {HISTORY_RETENTION_DAYS} days
            </span>
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={loadingLag}
              aria-busy={loadingLag || undefined}
              title={
                loadingLag
                  ? "Kavka is already reading its history file"
                  : "Read the history again, up to this moment"
              }
              onClick={refresh}
            >
              Refresh
            </button>
          </div>
        </div>

        <p className="table-note">
          Kafka doesn't remember lag — a broker can only tell you where a group
          stands right now. So Kavka takes its own reading every{" "}
          {formatSpan(sampler?.interval_ms ?? 15_000)} while this connection is
          up, writes it to a file on this machine, and deletes anything older
          than {HISTORY_RETENTION_DAYS} days. Nothing is collected while Kavka is
          closed or this cluster is disconnected, and nothing leaves this
          machine.
        </p>

        <SamplerLine status={sampler} />
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <Term name="lag">Lag</Term> over time
            {chosenGroup !== null && (
              <span className="panel-count">
                {readoutTime(chosenGroup.first_ts_ms)} onwards
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <div
              className="range-picker"
              role="group"
              aria-label="How far back to look"
            >
              {HISTORY_RANGES.map((r) => (
                <button
                  key={r.key}
                  type="button"
                  className={`btn${range.key === r.key ? " btn-latched" : ""}`}
                  aria-pressed={range.key === r.key}
                  onClick={() => setRange(r)}
                >
                  {r.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        {groups === null ? (
          <p className="table-note">Reading Kavka's history file…</p>
        ) : historyFailed ? (
          // Same sentence as the verdict, for the same reason: the empty
          // state below says the first points arrive within one interval,
          // and that is a promise about a read that did not happen.
          <p className="table-note">{t("perch.monitoring.unread")}</p>
        ) : groups.length === 0 ? (
          <p className="table-note">
            No lag history yet. Kavka starts sampling as soon as a connection is
            up, so the first points appear within{" "}
            {formatSpan(sampler?.interval_ms ?? 15_000)} of connecting — and a
            group only appears here once it has committed an offset at least
            once.
          </p>
        ) : (
          <>
            <div className="field field-inline">
              <label className="field-label" htmlFor="mon-group">
                <Term name="consumer-group">Consumer group</Term>
              </label>
              <select
                id="mon-group"
                className="input-mono"
                value={group ?? ""}
                onChange={(e) => setGroup(e.target.value)}
              >
                {groups.map((g) => (
                  <option key={g.group_id} value={g.group_id}>
                    {g.group_id}
                  </option>
                ))}
              </select>
              {chosenGroup !== null && (
                <span className="field-hint">
                  Kavka last sampled this group{" "}
                  {formatAge(Math.max(0, Date.now() - chosenGroup.last_ts_ms))}.
                </span>
              )}
            </div>

            {/* A FAILED QUERY GETS NEITHER TILES NOR ADVICE. Every figure in
                them is computed from the empty array the catch left behind, so
                a tile reading 0 is a measurement Kavka never took — and "try a
                longer window" cannot work when the query is what failed. */}
            {samplesFailed ? (
              <p className="table-note">{t("perch.monitoring.unread")}</p>
            ) : (
              <>
                {/* Jackdaw tiles: the number, then the WORD for what it counts.
                    `stat-grid-tiles` is an opt-in modifier — the bare
                    `.stat-grid` stays a floating row everywhere else. */}
                <div className="stat-grid stat-grid-tiles">
                  <div className="stat">
                    <span className="stat-label">
                      {t("monitoring.tile.lagNow")}
                    </span>
                    <span className="stat-value">
                      {groupDigits(summary.lagNow)}
                    </span>
                    <span className="stat-sub">
                      {t("monitoring.tile.lagNowSub")}
                    </span>
                  </div>
                  <div className="stat">
                    <span className="stat-label">{t("monitoring.tile.peak")}</span>
                    <span className="stat-value">
                      {summary.peak === null ? (
                        <span className="absent">∅</span>
                      ) : (
                        groupDigits(summary.peak.lag ?? 0)
                      )}
                    </span>
                    <span className="stat-sub">
                      {t("monitoring.tile.peakSub")}
                    </span>
                  </div>
                  <div
                    className={`stat${trend.tone === "warn" ? " stat-attn" : ""}`}
                  >
                    <span className="stat-label">
                      {t("monitoring.tile.trend")}
                    </span>
                    <span
                      className={`stat-value stat-value-word trend-${trend.tone}`}
                    >
                      <span aria-hidden="true">{trend.glyph}</span> {trend.word}
                    </span>
                    <span className="stat-sub">
                      {t("monitoring.tile.trendSub")}
                    </span>
                  </div>
                  <div className="stat">
                    <span className="stat-label">
                      <Term name="partition">Partitions</Term> tracked
                    </span>
                    <span className="stat-value">{summary.partitions}</span>
                    <span className="stat-sub">
                      {t("monitoring.tile.partitionsSub")}
                    </span>
                  </div>
                </div>

                <p className="table-note">
                  {summary.partitions === 0 ? (
                    <>
                      Kavka has no readings for this group in the last{" "}
                      {formatSpan(range.ms).replace(/^1 /, "")}. Try a longer
                      window, or check the sampler above.
                    </>
                  ) : (
                    <>
                      Right now this group is about{" "}
                      <strong>{approxCount(summary.lagNow)} messages</strong>{" "}
                      behind across {summary.partitions} partition
                      {summary.partitions === 1 ? "" : "s"}, and it is{" "}
                      <strong>{trend.word}</strong> compared with the start of
                      this window
                      {summary.peak !== null &&
                      (summary.peak.lag ?? 0) > summary.lagNow
                        ? `. The worst point was ${groupDigits(
                            summary.peak.lag ?? 0,
                          )} on ${summary.peak.topic} partition ${
                            summary.peak.partition
                          }, at ${readoutTime(summary.peak.ts_ms)}`
                        : ""}
                      .
                      {summary.anyUncommitted
                        ? " Some partitions have no committed offset in this window, so they contribute no lag rather than zero lag — they show as gaps."
                        : ""}
                    </>
                  )}
                </p>
              </>
            )}

            {byTopic.map(({ topic, series }) => (
              <div className="chart-block" key={topic}>
                <h3 className="chart-title">
                  <span className="chart-title-name">{topic}</span>
                  <span className="chart-title-count">
                    {series.length} partition{series.length === 1 ? "" : "s"}
                    {series.filter((s) => s.hidden === true).length > 0
                      ? ` · ${series.filter((s) => s.hidden === true).length} hidden`
                      : ""}
                  </span>
                </h3>
                <LineChart
                  series={series}
                  fromMs={frame.from}
                  toMs={frame.to}
                  label={`Lag over time for ${group ?? ""} on ${topic}, by partition`}
                  formatValue={groupDigits}
                  valueHeading="Lag in messages"
                  axisNote={lagNote}
                  tableCaption={`Lag readings for ${group ?? ""} on ${topic}`}
                  onToggleSeries={toggleSeries}
                />
              </div>
            ))}
          </>
        )}
      </section>

      <MetricsSection
        status={metrics}
        endpointUrl={profile.metrics_endpoint?.url ?? null}
        topics={metricTopics}
        topic={metricTopic}
        onTopic={setMetricTopic}
        points={points}
        frame={metricFrame}
        range={metricRange}
        onRange={setMetricRange}
        loading={loadingMetrics}
        onEditConnection={onEditConnection}
      />
    </>
  );
}

/**
 * One line about the sampler, with a dot AND a word AND the numbers behind it.
 *
 * `running: false` is the state this exists for. A chart that simply stops is
 * indistinguishable from a cluster that went quiet, and the difference between
 * those two is the difference between "nothing is wrong" and "you are not
 * watching".
 */
function SamplerLine({ status }: { status: SamplerStatus | null }) {
  if (status === null)
    return (
      <p className="table-note">
        Kavka can't say what its sampler is doing right now. The history below is
        whatever was already on disk.
      </p>
    );

  const stale =
    status.last_sample_ms !== null &&
    Date.now() - status.last_sample_ms > status.interval_ms * 3;
  const tone = !status.running ? "warn" : stale ? "warn" : "ok";
  const word = !status.running
    ? "Not sampling"
    : stale
      ? "Sampling, but behind"
      : "Sampling";

  return (
    <>
      {/* The dot pulses only while readings are genuinely arriving. A heartbeat
          on a stopped sampler is the one animation this app must never show. */}
      <p className={`sampler-line${tone === "ok" ? " sampler-line-live" : ""}`}>
        <span className={`health health-${tone}`}>
          <i className="dot" aria-hidden="true" />
          {word}
        </span>
        <span className="sampler-detail">
          every {formatSpan(status.interval_ms)}
        </span>
        <span className="sampler-detail">
          {status.last_sample_ms === null
            ? "no reading taken yet"
            : `last reading ${formatAge(
                Math.max(0, Date.now() - status.last_sample_ms),
              )}`}
        </span>
      </p>
      {!status.running && (
        <p className="table-note">
          Nothing is being recorded for this connection at the moment, so the
          charts below stop where the last reading did. The sampler runs while a
          connection is up — reconnecting starts it again.
        </p>
      )}
      {status.last_error !== null && (
        <div className="banner banner-warn" role="status">
          <span className="banner-glyph" aria-hidden="true">
            !
          </span>
          <div className="banner-body">
            <p className="banner-title">
              The last sample didn't complete
            </p>
            <p className="banner-detail">
              The sampler keeps trying on its own schedule, so this may already
              have cleared. It usually means the account lost{" "}
              <code>Describe</code> on the group, or the cluster stopped
              answering mid-reading.
            </p>
            <details className="banner-details">
              <summary>Show details</summary>
              <pre className="banner-raw">{status.last_error}</pre>
            </details>
          </div>
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Broker metrics
// ---------------------------------------------------------------------------

interface MetricsSectionProps {
  status: MetricsStatus | null;
  endpointUrl: string | null;
  topics: string[];
  topic: string | null;
  onTopic: (topic: string | null) => void;
  points: Record<string, MetricPoint[]>;
  frame: { from: number; to: number };
  /** The metrics window — one of `METRICS_RANGES`, not the lag one. */
  range: HistoryRange;
  onRange: (range: HistoryRange) => void;
  loading: boolean;
  onEditConnection: () => void;
}

function MetricsSection({
  status,
  endpointUrl,
  topics,
  topic,
  onTopic,
  points,
  frame,
  range,
  onRange,
  loading,
  onEditConnection,
}: MetricsSectionProps) {
  const have = useMemo(
    () => new Set(status?.series_available ?? []),
    [status],
  );

  /**
   * Kavka's earliest reading anywhere in this window — where the line can
   * start.
   *
   * This is what tells the two causes of an empty chart apart. These readings
   * live in memory and start again from nothing every time the connection
   * opens, so a window longer than the connection has been up is mostly a
   * window Kavka was not there for; that is a different fact from an exporter
   * that stopped answering, and the copy below must not blame the second for
   * the first.
   */
  const collectingSince = useMemo(() => {
    let earliest: number | null = null;
    for (const list of Object.values(points))
      for (const point of list)
        if (earliest === null || point.ts_ms < earliest) earliest = point.ts_ms;
    return earliest;
  }, [points]);

  const chart = useCallback(
    (names: string[], tones: number[]): ChartSeries[] =>
      names
        .filter((name) => have.has(name))
        .map((name, i) => ({
          id: name,
          label: seriesLabel(name),
          tone: tones[i] ?? i + 1,
          points: (points[name] ?? []).map((p) => ({ x: p.ts_ms, y: p.value })),
        })),
    [have, points],
  );

  // ── The four states, told apart ─────────────────────────────────────────

  if (status === null)
    return (
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Throughput and storage</h2>
        </div>
        <p className="table-note">Asking Kavka about this cluster's metrics endpoint…</p>
      </section>
    );

  if (!status.configured)
    return (
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Throughput and storage
            <span className="panel-count">not available</span>
          </h2>
        </div>

        {/* THE TEACHING EMPTY STATE. Not "no data": what you would get, how to
            switch it on, and what does not depend on it. The mark is a broken
            chart line — decorative, and every word beside it is the signal. */}
        <div className="teach">
          <div className="teach-art" aria-hidden="true">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.7"
              strokeLinecap="round"
              strokeLinejoin="round"
              focusable="false"
            >
              <path d="M3 18l5-6 4 3 5-8 4 5" />
              <path d="M4 4l16 16" />
            </svg>
          </div>
          <div className="teach-body">
            <h3 className="teach-title">
              This connection has no metrics endpoint, so there is nothing to
              draw
            </h3>
            <p className="teach-text">
              Kafka's brokers don't serve throughput, storage growth or
              replication health over the Kafka protocol — they publish them as
              JMX, and almost everyone puts a Prometheus exporter in front of
              that. Kavka would rather show you nothing than draw a line it made
              up.
            </p>
            <ul className="teach-list">
              <li>
                What you would get: bytes in and out per second, messages in per
                second, log size on disk, and the under-replicated and offline
                partition counts.
              </li>
              <li>
                How to switch it on: the <code>jmx_exporter</code> Java agent on
                each broker (
                <code>-javaagent:jmx_prometheus_javaagent.jar=7071:kafka.yml</code>
                ), which answers on something like{" "}
                <code>http://broker-1.internal:7071/metrics</code>. A Prometheus
                server that already scrapes those brokers works too — give Kavka
                its address instead.
              </li>
              <li>
                Nothing above depends on it: Kavka reads the lag history from
                the brokers itself.
              </li>
            </ul>
            <div className="empty-actions">
              <button
                type="button"
                className="btn"
                title="Kavka only edits a connection while it is disconnected, so this disconnects first"
                onClick={onEditConnection}
              >
                Open connection settings
              </button>
            </div>
          </div>
        </div>
      </section>
    );

  if (!status.reachable)
    return (
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Throughput and storage</h2>
        </div>
        <div className="banner banner-warn" role="status">
          <span className="banner-glyph" aria-hidden="true">
            !
          </span>
          <div className="banner-body">
            <p className="banner-title">
              {endpointUrl === null
                ? "The metrics endpoint didn't answer"
                : `${endpointUrl} didn't answer`}
            </p>
            <p className="banner-detail">
              The cluster itself is fine — this is a separate HTTP address, and
              nothing else in Kavka goes through it. Check that the exporter is
              running on that host and port, that the path is right (most publish
              at <code>/metrics</code>), and that this machine can reach it —
              exporters are often bound to the broker's private network.
            </p>
            {status.last_error !== null && (
              <details className="banner-details">
                <summary>Show details</summary>
                <pre className="banner-raw">{status.last_error}</pre>
              </details>
            )}
          </div>
          <div className="banner-actions">
            <button type="button" className="btn btn-ghost" onClick={onEditConnection}>
              Open connection settings
            </button>
          </div>
        </div>
        {status.last_scrape_ms !== null && (
          <p className="table-note">
            Kavka last tried this address{" "}
            {formatAge(Math.max(0, Date.now() - status.last_scrape_ms))} — when
            it tried, not when it last got an answer. It keeps trying on its own
            schedule while this connection is up, so this may already have
            cleared.
          </p>
        )}
      </section>
    );

  if (status.series_available.length === 0)
    return (
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Throughput and storage</h2>
        </div>
        <p className="table-note">
          {endpointUrl ?? "The metrics endpoint"} answered, but Kavka didn't
          recognise any Kafka broker metrics in what it published. That is an
          exporter configuration, not a permission: a jmx_exporter running with
          an empty or non-Kafka rule set answers happily and exposes nothing
          Kavka can map.
        </p>
        <p className="table-note">
          Kavka looks for {METRIC_SERIES.length} readings —{" "}
          {METRIC_SERIES.map((s) => seriesLabel(s).toLowerCase()).join(", ")} —
          and draws whichever of them it finds. Point it at the exporter that
          fronts the brokers themselves rather than one in front of a client, and
          check the rule set is the Kafka one.
        </p>
      </section>
    );

  const throughput = chart(["bytes_in_per_sec", "bytes_out_per_sec"], [2, 3]);
  const messages = chart(["messages_in_per_sec"], [4]);
  const storage = chart(["log_size_bytes"], [6]);
  const perTopic =
    topic === null
      ? []
      : chart(
          [
            topicSeries("bytes_in_per_sec", topic),
            topicSeries("bytes_out_per_sec", topic),
            topicSeries("messages_in_per_sec", topic),
            topicSeries("log_size_bytes", topic),
          ],
          [2, 3, 4, 6],
        );

  const urp = points["under_replicated_partitions"] ?? [];
  const offline = points["offline_partitions"] ?? [];
  const bucket = bucketWidth(range.ms);

  /**
   * When Kavka started collecting, but only when that is later than the window
   * itself starts — otherwise the earliest point is just the edge of the window
   * and says nothing about when collection began.
   */
  const beganAt =
    collectingSince !== null && collectingSince - frame.from > bucket
      ? collectingSince
      : null;

  const metricNote = (
    <>
      One point covers {formatSpan(bucket)}, scraped from{" "}
      {endpointUrl ?? "the metrics endpoint"} rather than from the brokers. A gap
      in a line is a stretch where the endpoint didn't answer.
      {beganAt !== null && (
        <>
          {" "}
          The blank <em>before</em> {readoutTime(beganAt)} is a different thing:
          Kavka keeps these readings in memory only and has been collecting them
          since then, so there is nothing earlier to draw.
        </>
      )}
    </>
  );

  return (
    <>
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Cluster health
            <span className="panel-count">from the metrics endpoint</span>
          </h2>
          <div className="panel-tools">
            {/* The metrics window, and only the metrics window: it governs
                every chart in this section and none of the lag charts above,
                which keep their own because they can reach back a week and
                these cannot. */}
            <div
              className="range-picker"
              role="group"
              aria-label="How far back the metrics charts look"
            >
              {METRICS_RANGES.map((r) => (
                <button
                  key={r.key}
                  type="button"
                  className={`btn${range.key === r.key ? " btn-latched" : ""}`}
                  aria-pressed={range.key === r.key}
                  title={`Draw the metrics charts over ${r.label.replace(
                    /^Last /,
                    "the last ",
                  )} — the lag charts above keep their own window`}
                  onClick={() => onRange(r)}
                >
                  {r.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="health-grid">
          <HealthReading
            name="under_replicated_partitions"
            title={<Term name="under-replicated">Under-replicated partitions</Term>}
            points={urp}
            available={have.has("under_replicated_partitions")}
            tone={(v) => (v > 0 ? "warn" : "ok")}
            word={(v) =>
              v > 0 ? "Copies behind" : "Every copy in sync"
            }
            explain={(v) =>
              v > 0
                ? "One or more partitions don't have all their replicas in sync. A few minutes after a broker restart this is normal; an hour later it is not."
                : "Every partition has all its copies in sync, which is what you want to see here."
            }
          />
          <HealthReading
            name="offline_partitions"
            title="Offline partitions"
            points={offline}
            available={have.has("offline_partitions")}
            tone={(v) => (v > 0 ? "danger" : "ok")}
            word={(v) => (v > 0 ? "No leader" : "All partitions led")}
            explain={(v) =>
              v > 0
                ? "These partitions have no leader, so nothing can be produced to them and nothing can be read from them. This is never a normal steady state."
                : "Every partition has a leader, so every partition can be written to and read from."
            }
          />
        </div>

        {have.has("under_replicated_partitions") && (
          <div className="chart-block">
            <h3 className="chart-title">
              <span className="chart-title-name">
                Under-replicated partitions
              </span>
              <span className="chart-title-count">
                {seriesGloss("under_replicated_partitions")}
              </span>
            </h3>
            <LineChart
              series={chart(["under_replicated_partitions"], [5])}
              fromMs={frame.from}
              toMs={frame.to}
              label="Under-replicated partitions over time"
              formatValue={(v) => formatSeriesValue("under_replicated_partitions", v)}
              valueHeading="Partitions"
              axisNote={metricNote}
              tableCaption="Under-replicated partition readings"
              height={150}
            />
          </div>
        )}

        {have.has("offline_partitions") && (
          <div className="chart-block">
            <h3 className="chart-title">
              <span className="chart-title-name">Offline partitions</span>
              <span className="chart-title-count">
                {seriesGloss("offline_partitions")}
              </span>
            </h3>
            <LineChart
              series={chart(["offline_partitions"], [5])}
              fromMs={frame.from}
              toMs={frame.to}
              label="Offline partitions over time"
              formatValue={(v) => formatSeriesValue("offline_partitions", v)}
              valueHeading="Partitions"
              axisNote={metricNote}
              tableCaption="Offline partition readings"
              height={150}
            />
          </div>
        )}
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Throughput and storage
            {loading && <span className="panel-count">reading…</span>}
          </h2>
          {topics.length > 0 && (
            <div className="panel-tools">
              <label className="inline-label" htmlFor="mon-metric-topic">
                One topic
              </label>
              <select
                id="mon-metric-topic"
                className="input-mono"
                value={topic ?? ""}
                onChange={(e) =>
                  onTopic(e.target.value.length === 0 ? null : e.target.value)
                }
              >
                <option value="">Whole cluster only</option>
                {topics.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>

        <p className="table-note">
          {status.last_scrape_ms === null
            ? "Kavka hasn't recorded a scrape time yet."
            : `Last scraped ${formatAge(
                Math.max(0, Date.now() - status.last_scrape_ms),
              )} from ${endpointUrl ?? "the metrics endpoint"}.`}{" "}
          These figures are the exporter's, passed through unchanged — Kavka maps
          the names and nothing else. Unlike the lag history above, they are kept
          in memory rather than on disk: {formatSpan(METRICS_WINDOW_MS)} at most,
          starting again from empty every time this connection opens.
          {beganAt !== null && (
            <>
              {" "}
              That is why these charts begin at {readoutTime(beganAt)} rather
              than at the start of the window — Kavka had not been collecting
              before then. It is not a quiet cluster, and it is not the exporter
              failing to answer.
            </>
          )}
        </p>

        {throughput.length > 0 && (
          <div className="chart-block">
            <h3 className="chart-title">
              <span className="chart-title-name">Bytes in and out</span>
              <span className="chart-title-count">
                {seriesGloss("bytes_out_per_sec")}
              </span>
            </h3>
            <LineChart
              series={throughput}
              fromMs={frame.from}
              toMs={frame.to}
              label="Bytes in and out per second, cluster-wide"
              formatValue={(v) => formatSeriesValue("bytes_in_per_sec", v)}
              valueHeading="Bytes per second"
              axisNote={metricNote}
              tableCaption="Cluster-wide bytes in and out per second"
            />
          </div>
        )}

        {messages.length > 0 && (
          <div className="chart-block">
            <h3 className="chart-title">
              <span className="chart-title-name">Messages in</span>
              <span className="chart-title-count">
                {seriesGloss("messages_in_per_sec")}
              </span>
            </h3>
            <LineChart
              series={messages}
              fromMs={frame.from}
              toMs={frame.to}
              label="Messages produced per second, cluster-wide"
              formatValue={(v) => formatSeriesValue("messages_in_per_sec", v)}
              valueHeading="Messages per second"
              axisNote={metricNote}
              tableCaption="Cluster-wide messages in per second"
              height={170}
            />
          </div>
        )}

        {storage.length > 0 && (
          <div className="chart-block">
            <h3 className="chart-title">
              <span className="chart-title-name">Log size on disk</span>
              <span className="chart-title-count">
                {seriesGloss("log_size_bytes")}
              </span>
            </h3>
            <LineChart
              series={storage}
              fromMs={frame.from}
              toMs={frame.to}
              label="Total log size on disk"
              formatValue={(v) => formatSeriesValue("log_size_bytes", v)}
              valueHeading="Bytes on disk"
              axisNote={metricNote}
              tableCaption="Total log size on disk over time"
              height={170}
            />
          </div>
        )}

        {topic !== null && (
          <div className="chart-block">
            <h3 className="chart-title">
              <span className="chart-title-name">{topic}</span>
              <span className="chart-title-count">
                {perTopic.length === 0
                  ? "this scrape publishes no per-topic series for it"
                  : `${perTopic.length} series`}
              </span>
            </h3>
            {perTopic.length === 0 ? (
              <p className="table-note">
                The exporter lists per-topic series for other topics but none for
                this one. That normally means the topic has had no traffic since
                the brokers last restarted — JMX only creates a topic's meters
                once it has been used.
              </p>
            ) : (
              <LineChart
                series={perTopic}
                fromMs={frame.from}
                toMs={frame.to}
                label={`Throughput and storage for ${topic}`}
                formatValue={(v) => formatSeriesValue("bytes_in_per_sec", v)}
                valueHeading="Per-topic readings"
                axisNote={
                  <>
                    {metricNote} Bytes and message counts share one axis here, so
                    read the values rather than the heights when both are on
                    screen.
                  </>
                }
                tableCaption={`Per-topic readings for ${topic}`}
              />
            )}
          </div>
        )}
      </section>
    </>
  );
}

/**
 * One health reading: the number, a dot, a word and a sentence. Never a colour
 * on its own, and never a bare figure — "3" is meaningless without "partitions
 * with no leader" beside it.
 */
function HealthReading({
  name,
  title,
  points,
  available,
  tone,
  word,
  explain,
}: {
  name: string;
  title: React.ReactNode;
  points: MetricPoint[];
  available: boolean;
  tone: (value: number) => "ok" | "warn" | "danger";
  word: (value: number) => string;
  explain: (value: number) => string;
}) {
  if (!available)
    return (
      <div className="health-reading">
        <span className="stat-label">{title}</span>
        <span className="health health-quiet">
          <i className="dot" aria-hidden="true" />
          Not published
        </span>
        <span className="health-explain">
          This scrape doesn't expose that reading, so Kavka has nothing to show
          rather than a zero it made up.
        </span>
      </div>
    );

  const latest = points.length === 0 ? null : points[points.length - 1];
  if (latest === null)
    return (
      <div className="health-reading">
        <span className="stat-label">{title}</span>
        <span className="health health-quiet">
          <i className="dot" aria-hidden="true" />
          No reading yet
        </span>
        <span className="health-explain">
          The endpoint publishes it, but Kavka has no reading inside this
          window. These are held in memory from the moment this connection
          opened, so a window longer than the connection has been up starts
          empty — try a shorter one before suspecting the exporter.
        </span>
      </div>
    );

  const value = latest.value;
  return (
    <div className="health-reading">
      <span className="stat-label">{title}</span>
      <span className={`health health-${tone(value)}`}>
        <i className="dot" aria-hidden="true" />
        {word(value)}
        <span className="health-number">{formatSeriesValue(name, value)}</span>
      </span>
      <span className="health-explain">
        {explain(value)} Read{" "}
        {formatAge(Math.max(0, Date.now() - latest.ts_ms))}.
      </span>
    </div>
  );
}
