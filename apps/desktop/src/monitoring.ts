/**
 * Phase 4's shared vocabulary, as pure functions.
 *
 * No React, no imports beyond `format.ts`, no I/O — the same rule `errors.ts`
 * and `format.ts` follow, and for the same reason: these strings are what the
 * Monitoring tab, the alert builder and the alert history all say about the
 * same numbers, and three copies of "bytes in per second" is how three screens
 * come to disagree about what a series measures.
 */

import { approxCount, formatBytes, groupDigits } from "./format";
import { METRIC_SERIES, splitSeries, type MetricSeries } from "./api";

// ---------------------------------------------------------------------------
// Time windows
// ---------------------------------------------------------------------------

export interface HistoryRange {
  key: "1h" | "6h" | "24h" | "7d";
  /** Sentence case, per §7 rule 1 — this is a label, not a system token. */
  label: string;
  ms: number;
}

/**
 * The four windows, and the reason the longest one is exactly seven days: the
 * store prunes at seven days on write, so a "30 days" option could only ever
 * draw a week of line and three weeks of nothing.
 */
export const HISTORY_RANGES: readonly HistoryRange[] = [
  { key: "1h", label: "Last hour", ms: 3_600_000 },
  { key: "6h", label: "Last 6 hours", ms: 6 * 3_600_000 },
  { key: "24h", label: "Last 24 hours", ms: 24 * 3_600_000 },
  { key: "7d", label: "Last 7 days", ms: 7 * 86_400_000 },
];

/**
 * How much of each metric series the core holds, mirroring
 * `kavka_core::metrics::WINDOW_MS`.
 *
 * Mirrored for the same reason `SAMPLER_MIN_MS` and `HISTORY_RETENTION_DAYS`
 * are: it is needed to draw a control, not to answer a query, and a UI that has
 * to ask the core before it can render a picker renders the picker late.
 */
export const METRICS_WINDOW_MS = 24 * 3_600_000;

/**
 * The windows the METRIC charts offer — the history windows that fit inside the
 * ring above.
 *
 * The two halves of the Monitoring tab do not keep time the same way, and this
 * is where that stops being invisible. Lag history is a file on disk holding a
 * week; metrics are a ring in memory holding a day, and that ring starts EMPTY
 * at every connect. Offering "last 7 days" beside a throughput chart promises
 * six days of line that cannot exist, and the chart that comes back looks like
 * a cluster that was switched off — which is precisely the confusion the
 * sampler panel exists to prevent for lag.
 *
 * Derived by filtering rather than written out again, so a window added to
 * `HISTORY_RANGES` can never quietly become a metrics window the ring is too
 * small to fill.
 */
export const METRICS_RANGES: readonly HistoryRange[] = HISTORY_RANGES.filter(
  (range) => range.ms <= METRICS_WINDOW_MS,
);

/** How many points the UI asks for per partition. One per ~3px of a wide plot. */
export const MAX_POINTS = 240;

/**
 * A span of time in prose: `15 seconds`, `4 minutes`, `2 hours`.
 *
 * Used for bucket widths and alert dwells, both of which are read inside a
 * sentence — so this rounds, per §7 rule 5, and nothing that anyone acts on
 * digit-by-digit goes through it.
 *
 * # There is a second copy of these thresholds, and it is deliberate
 *
 * `ProfileEditor.tsx`'s `spanText(t, ms)` is the same four branches at the same
 * four boundaries, written against the `unit.*` catalog keys instead of English
 * literals. It exists because this function hard-codes English pluralisation
 * (`1 second` / `2 seconds`), and the connection form needs two spans *inside a
 * translated paragraph* — one raw English fragment in the middle of a German
 * sentence is worse than four catalog keys (docs/I18N.md §1).
 *
 * **Change both or neither.** A threshold moved here and not there makes the
 * sampler hint disagree with every chart axis about what "a minute" starts at,
 * in five languages and not in English — which is the hardest kind of drift to
 * notice.
 */
export function formatSpan(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "no time at all";
  if (ms < 60_000) {
    const s = Math.max(1, Math.round(ms / 1000));
    return `${s} second${s === 1 ? "" : "s"}`;
  }
  if (ms < 3_600_000) {
    const m = Math.round(ms / 60_000);
    return `${m} minute${m === 1 ? "" : "s"}`;
  }
  if (ms < 86_400_000) {
    const h = Math.round(ms / 3_600_000);
    return `${h} hour${h === 1 ? "" : "s"}`;
  }
  const d = Math.round(ms / 86_400_000);
  return `${d} day${d === 1 ? "" : "s"}`;
}

/**
 * How long a firing lasted, or has lasted so far.
 *
 * Exact rather than rounded, unlike `formatSpan`: "fired at 02:14, resolved at
 * 02:19" is a fact someone reconstructs an incident from, and `about 5 minutes`
 * loses the only precision that mattered.
 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

/**
 * The width of one downsample bucket, and therefore what one point on the chart
 * covers. Stated on every chart — see the axis notes.
 */
export function bucketWidth(rangeMs: number, points = MAX_POINTS): number {
  return Math.max(1, Math.round(rangeMs / Math.max(1, points)));
}

// ---------------------------------------------------------------------------
// The metric series vocabulary
// ---------------------------------------------------------------------------

const SERIES_LABEL: Record<MetricSeries, string> = {
  bytes_in_per_sec: "Bytes in per second",
  bytes_out_per_sec: "Bytes out per second",
  messages_in_per_sec: "Messages in per second",
  under_replicated_partitions: "Under-replicated partitions",
  offline_partitions: "Offline partitions",
  log_size_bytes: "Log size on disk",
};

const SERIES_GLOSS: Record<MetricSeries, string> = {
  bytes_in_per_sec:
    "How many bytes producers are writing to this cluster each second, as the brokers count them.",
  bytes_out_per_sec:
    "How many bytes consumers are reading each second. Replication traffic between brokers is counted separately by most exporters, so this is usually smaller than you expect.",
  messages_in_per_sec:
    "How many records producers are writing each second. A batch of 500 small records counts as 500 here and as one request on the network.",
  under_replicated_partitions:
    "Partitions that don't have all their copies in sync right now. Anything above zero for more than a few minutes means a broker is down or badly behind.",
  offline_partitions:
    "Partitions with no leader at all. Nothing can be produced to them or read from them — this is the one number that is never normal.",
  log_size_bytes:
    "How much disk the cluster's partitions are using. It grows until retention starts deleting, which is why a flat line here is healthier than a rising one.",
};

function isMetricSeries(name: string): name is MetricSeries {
  return (METRIC_SERIES as readonly string[]).includes(name);
}

/**
 * The label for a series name, per-topic variants included:
 * `bytes_in_per_sec:topic:orders.v2` → `Bytes in per second · orders.v2`.
 *
 * A name Kavka doesn't recognise is shown VERBATIM rather than prettified. An
 * exporter may publish something the core maps but this table doesn't know
 * about yet, and a series nobody can name is still a series someone has to be
 * able to see — the same rule ACLs follow for an unknown resource type.
 */
export function seriesLabel(name: string): string {
  const { base, topic } = splitSeries(name);
  const label = isMetricSeries(base) ? SERIES_LABEL[base] : base;
  return topic === null ? label : `${label} · ${topic}`;
}

/** One sentence about what a series measures, or null if Kavka can't say. */
export function seriesGloss(name: string): string | null {
  const { base } = splitSeries(name);
  return isMetricSeries(base) ? SERIES_GLOSS[base] : null;
}

/** True when a series is measured in bytes, and therefore formatted as bytes. */
export function seriesIsBytes(name: string): boolean {
  const { base } = splitSeries(name);
  return base === "bytes_in_per_sec" || base === "bytes_out_per_sec" || base === "log_size_bytes";
}

/**
 * A series value for an axis, a readout or a table cell.
 *
 * Bytes go through `formatBytes` because `1 073 741 824` on a y axis is not a
 * quantity anyone reads; everything else is a count Kavka computed, so it is
 * grouped and exact per §7 rule 5.
 */
export function formatSeriesValue(name: string, value: number): string {
  if (seriesIsBytes(name)) return formatBytes(value);
  if (!Number.isFinite(value)) return "—";
  // A per-second rate is not always a whole number once a bucket averages it.
  return Number.isInteger(value)
    ? groupDigits(value)
    : groupDigits(Math.round(value));
}

/**
 * The same value inside a sentence, where §7 rule 5 says prose rounds:
 * "about 4.2M". Never used for a lag figure someone is about to act on.
 */
export function approxSeriesValue(name: string, value: number): string {
  return seriesIsBytes(name) ? formatBytes(value) : approxCount(value);
}
