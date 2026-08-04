import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";

/**
 * THE CHART PRIMITIVE — docs/DESIGN.md §5.11, and §8's rule about it.
 *
 * "Anything not on that list — charts, config diff, topology, reassignment —
 * inherits from these primitives. It does not get its own vocabulary." So this
 * is hand-rolled SVG with no chart dependency, painted out of the same tokens
 * every table uses: 1px strokes, grid at `--grid-line`, `--t-micro` tertiary
 * axis labels, no chart junk, no shadows, no radius, no card.
 *
 * FOUR RULES IT ENFORCES RATHER THAN OFFERS.
 *
 *  1. **A chart is never the only way to read the data.** Every instance ships a
 *     keyboard-reachable `<details>` holding the same numbers as a real table.
 *     Not a nicety: an SVG polyline is invisible to a screen reader whatever
 *     `aria-label` you hang on it, and the person most likely to be reading a
 *     lag chart at 3am is the person on call, not the person who drew it.
 *  2. **Series are dash-patterned as well as coloured.** §5.11: "a 6-series line
 *     chart identified by hue alone fails SC 1.4.1 for deuteranopes." The
 *     pattern is the channel that survives greyscale, and the legend swatch
 *     draws the actual pattern rather than a colour square.
 *  3. **A null is a GAP, never a zero.** A partition a group has never committed
 *     to has no lag, and a line that dips to the axis there is a lie about a
 *     caught-up consumer. The path breaks; the table says `∅`.
 *  4. **Line-draw animation on first paint only.** The wipe element is mounted
 *     once and never re-keyed, so React reuses it across every data update and
 *     the CSS animation cannot re-run. Its resting state is fully drawn, so a
 *     browser that ignores the animation shows a correct chart.
 *
 * The crosshair is driven by pointer AND keyboard — arrow keys walk the merged
 * timeline — because a value readout only reachable with a mouse is the same
 * failure as rule 1 wearing a different hat.
 */

/** A null y is a gap in the line, not a zero. See rule 3. */
export interface ChartPoint {
  x: number;
  y: number | null;
}

export interface ChartSeries {
  /** Stable across renders — React keys, legend toggles and the readout use it. */
  id: string;
  label: string;
  points: ChartPoint[];
  /** Which `--series-N` token, 1–6. Wraps past 6. */
  tone: number;
  /** Toggled off in the legend. Still listed, so nobody wonders where it went. */
  hidden?: boolean;
}

interface LineChartProps {
  series: ChartSeries[];
  fromMs: number;
  toMs: number;
  /** Accessible name for the plot region. */
  label: string;
  /** Formats a y value for the axis, the readout and the table. */
  formatValue: (value: number) => string;
  /** Column head for the value columns in the fallback table. */
  valueHeading: string;
  /**
   * The always-visible sentence under the axis saying what ONE POINT MEANS.
   * Required, not optional: a downsampled chart whose bucket rule is unstated
   * is a chart the reader will assume is an average.
   */
  axisNote: React.ReactNode;
  /** `<caption>` for the fallback table. */
  tableCaption: string;
  /** Legend items become toggles when this is provided. */
  onToggleSeries?: (id: string) => void;
  height?: number;
}

/** Dash patterns, in order. Series 1 is solid; nothing else is. */
const DASHES = ["", "5 3", "1 3", "7 3 1 3", "9 4", "2 2"];

const PAD = { top: 10, right: 18, bottom: 26, left: 62 };
const DEFAULT_W = 720;
const DEFAULT_H = 200;

/** The candidate x-axis steps, smallest first. Seconds through a week. */
const TIME_STEPS = [
  1_000, 5_000, 15_000, 30_000, 60_000, 5 * 60_000, 15 * 60_000, 30 * 60_000,
  3_600_000, 3 * 3_600_000, 6 * 3_600_000, 12 * 3_600_000, 86_400_000,
  2 * 86_400_000, 7 * 86_400_000,
];

function pad2(n: number): string {
  return n.toString().padStart(2, "0");
}

/**
 * An axis label as short as the range allows and no shorter. Under two days a
 * clock time is unambiguous; past that it needs the date, or a 7-day chart
 * reads as seven copies of the same afternoon.
 */
export function axisTime(ms: number, rangeMs: number): string {
  const d = new Date(ms);
  const clock = `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  if (rangeMs <= 2 * 86_400_000) return clock;
  return `${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${clock}`;
}

/** The readout and the table always carry the full stamp — no ambiguity. */
export function readoutTime(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(
    d.getDate(),
  )} ${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

/** 1, 2, 2.5, 5 or 10 × a power of ten — the axis top, so ticks are readable. */
function niceCeil(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 1;
  const exp = Math.floor(Math.log10(value));
  const base = 10 ** exp;
  const n = value / base;
  const step = n <= 1 ? 1 : n <= 2 ? 2 : n <= 2.5 ? 2.5 : n <= 5 ? 5 : 10;
  return step * base;
}

function timeStep(rangeMs: number, target: number): number {
  for (const step of TIME_STEPS) if (rangeMs / step <= target) return step;
  return TIME_STEPS[TIME_STEPS.length - 1];
}

/**
 * The plot's own width, measured rather than assumed.
 *
 * A `viewBox` that stretches would give non-uniform stroke widths and squashed
 * type — the two things §5.11's "1px strokes, no chart junk" is about — so the
 * SVG is drawn in real pixels and re-drawn when the workspace resizes. Without
 * a ResizeObserver (nothing in Kavka's floor lacks one, but a jsdom might) the
 * fallback width is used and everything still renders.
 */
function useMeasuredWidth(): [React.RefObject<HTMLDivElement | null>, number] {
  const ref = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(DEFAULT_W);
  useEffect(() => {
    const el = ref.current;
    if (el === null || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver((entries) => {
      const next = Math.round(entries[0].contentRect.width);
      if (next > 0) setWidth(next);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, width];
}

export default function LineChart({
  series,
  fromMs,
  toMs,
  label,
  formatValue,
  valueHeading,
  axisNote,
  tableCaption,
  onToggleSeries,
  height = DEFAULT_H,
}: LineChartProps) {
  // `useId` has had two formats across React versions — `:r0:` and `«r0»` — and
  // both of them contain characters that are trouble in an id used inside
  // `url(#…)`. Strip to word characters rather than to one known separator, so
  // this cannot break again on the next format change.
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const clipId = `chart-clip-${uid}`;
  const noteId = `chart-note-${uid}`;
  const [wrapRef, width] = useMeasuredWidth();
  const [cursor, setCursor] = useState<number | null>(null);

  const visible = useMemo(() => series.filter((s) => s.hidden !== true), [series]);

  const rangeMs = Math.max(1, toMs - fromMs);
  const plotW = Math.max(120, width - PAD.left - PAD.right);
  const plotH = Math.max(80, height - PAD.top - PAD.bottom);

  /** Every timestamp any visible series has a point at, sorted and deduped. */
  const timeline = useMemo(() => {
    const set = new Set<number>();
    for (const s of visible) for (const p of s.points) set.add(p.x);
    return [...set].sort((a, b) => a - b);
  }, [visible]);

  const yMax = useMemo(() => {
    let max = 0;
    for (const s of visible)
      for (const p of s.points) if (p.y !== null && p.y > max) max = p.y;
    return niceCeil(max);
  }, [visible]);

  const xAt = useCallback(
    (ms: number) => PAD.left + ((ms - fromMs) / rangeMs) * plotW,
    [fromMs, rangeMs, plotW],
  );
  const yAt = useCallback(
    (value: number) => PAD.top + plotH - (value / yMax) * plotH,
    [plotH, yMax],
  );

  const yTicks = useMemo(() => {
    const steps = plotH >= 150 ? 4 : 2;
    return Array.from({ length: steps + 1 }, (_, i) => (yMax / steps) * i);
  }, [yMax, plotH]);

  const xTicks = useMemo(() => {
    const step = timeStep(rangeMs, Math.max(2, Math.floor(plotW / 110)));
    const out: number[] = [];
    for (let t = Math.ceil(fromMs / step) * step; t <= toMs; t += step)
      out.push(t);
    return out;
  }, [fromMs, toMs, rangeMs, plotW]);

  /**
   * The path, with a fresh `M` after every gap. A null y ENDS the current
   * segment; it never interpolates across it, because a straight line drawn
   * over a hole is an assertion nobody made.
   */
  const pathFor = useCallback(
    (points: ChartPoint[]): string => {
      let d = "";
      let pen = false;
      for (const p of points) {
        if (p.y === null) {
          pen = false;
          continue;
        }
        const cmd = pen ? "L" : "M";
        d += `${cmd}${xAt(p.x).toFixed(1)} ${yAt(p.y).toFixed(1)}`;
        pen = true;
      }
      return d;
    },
    [xAt, yAt],
  );

  /** The 6%-alpha wash §5.11 allows under a SINGLE series, and only then. */
  const areaFor = useCallback(
    (points: ChartPoint[]): string => {
      const drawn = points.filter((p) => p.y !== null);
      if (drawn.length < 2) return "";
      const base = (PAD.top + plotH).toFixed(1);
      const first = xAt(drawn[0].x).toFixed(1);
      const last = xAt(drawn[drawn.length - 1].x).toFixed(1);
      return `M${first} ${base}${drawn
        .map((p) => `L${xAt(p.x).toFixed(1)} ${yAt(p.y as number).toFixed(1)}`)
        .join("")}L${last} ${base}Z`;
    },
    [xAt, yAt, plotH],
  );

  /**
   * How far from the crosshair a point may sit and still be reported as the
   * value AT it. Series are downsampled independently, so their buckets rarely
   * share a timestamp; without a tolerance the readout would be empty almost
   * everywhere, and without a LIMIT on it the readout would happily quote a
   * value from twenty minutes away.
   */
  const tolerance = rangeMs / 40;

  const readout = useMemo(() => {
    if (cursor === null) return null;
    const at = timeline[cursor];
    if (at === undefined) return null;
    const values = visible.map((s) => {
      let best: ChartPoint | null = null;
      let bestGap = Infinity;
      for (const p of s.points) {
        const gap = Math.abs(p.x - at);
        if (gap < bestGap) {
          bestGap = gap;
          best = p;
        }
      }
      const usable = best !== null && bestGap <= tolerance;
      return {
        id: s.id,
        label: s.label,
        tone: s.tone,
        value: usable ? (best as ChartPoint).y : undefined,
      };
    });
    return { at, values };
  }, [cursor, timeline, visible, tolerance]);

  /** Snap to the nearest sampled moment, never to a pixel between two. */
  const snap = useCallback(
    (clientX: number, rect: DOMRect) => {
      if (timeline.length === 0) return;
      const ms = fromMs + ((clientX - rect.left - PAD.left) / plotW) * rangeMs;
      let bestIndex = 0;
      let bestGap = Infinity;
      timeline.forEach((t, i) => {
        const gap = Math.abs(t - ms);
        if (gap < bestGap) {
          bestGap = gap;
          bestIndex = i;
        }
      });
      setCursor(bestIndex);
    },
    [timeline, fromMs, rangeMs, plotW],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (timeline.length === 0) return;
      const last = timeline.length - 1;
      let next: number | null = null;
      if (e.key === "ArrowRight") next = Math.min(last, (cursor ?? -1) + 1);
      else if (e.key === "ArrowLeft") next = Math.max(0, (cursor ?? last + 1) - 1);
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = last;
      else if (e.key === "Escape" && cursor !== null) {
        e.preventDefault();
        setCursor(null);
        return;
      }
      if (next === null) return;
      e.preventDefault();
      setCursor(next);
    },
    [cursor, timeline],
  );

  const cursorX = readout === null ? null : xAt(readout.at);
  const empty = timeline.length === 0;

  return (
    <div className="chart" ref={wrapRef}>
      {series.length > 1 && (
        <ul className="chart-legend">
          {series.map((s, i) => (
            <li key={s.id}>
              {onToggleSeries === undefined ? (
                <span className="chart-key">
                  <ChartSwatch tone={s.tone} index={i} />
                  {s.label}
                </span>
              ) : (
                <button
                  type="button"
                  className={`chart-key chart-key-btn${
                    s.hidden === true ? " chart-key-off" : ""
                  }`}
                  aria-pressed={s.hidden !== true}
                  title={
                    s.hidden === true
                      ? `Show ${s.label} on the chart`
                      : `Hide ${s.label} — it stays in this list, so nothing disappears`
                  }
                  onClick={() => onToggleSeries(s.id)}
                >
                  <ChartSwatch tone={s.tone} index={i} />
                  {s.label}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      <div
        className="chart-plot"
        role="img"
        // `role="img"` makes everything inside it presentational, which is
        // right for the SVG and wrong for the one sentence that shares the
        // box: the empty state was invisible to a screen reader, so a chart
        // with no readings announced as a chart and said nothing about why
        // (SC 1.1.1). The sentence is folded into the name instead of being
        // moved, because it is positioned against this element.
        aria-label={
          empty
            ? `${label} — nothing recorded in this window. Kavka only has history it collected while this connection was up.`
            : label
        }
        aria-describedby={noteId}
        tabIndex={0}
        onKeyDown={onKeyDown}
        onMouseMove={(e) =>
          snap(e.clientX, e.currentTarget.getBoundingClientRect())
        }
        onMouseLeave={() => setCursor(null)}
        onBlur={() => setCursor(null)}
      >
        <svg width={width} height={height} aria-hidden="true" focusable="false">
          <defs>
            {/* Mounted once and never re-keyed, so the wipe cannot re-run on a
                data update. Its resting state is the full plot. */}
            <clipPath id={clipId}>
              <rect
                className="chart-wipe"
                x={PAD.left}
                y={PAD.top}
                width={plotW}
                height={plotH}
              />
            </clipPath>
          </defs>

          {yTicks.map((v) => (
            <g key={`y${v}`}>
              <line
                className="chart-grid"
                x1={PAD.left}
                x2={PAD.left + plotW}
                y1={yAt(v)}
                y2={yAt(v)}
              />
              <text className="chart-axis" x={PAD.left - 8} y={yAt(v) + 3.5} textAnchor="end">
                {formatValue(v)}
              </text>
            </g>
          ))}

          {xTicks.map((t) => (
            <text
              key={`x${t}`}
              className="chart-axis"
              x={xAt(t)}
              y={PAD.top + plotH + 16}
              textAnchor="middle"
            >
              {axisTime(t, rangeMs)}
            </text>
          ))}

          {/* Both axes, 1px. The left one is what makes a zero baseline read as
              a baseline rather than as the lowest grid line. */}
          <line
            className="chart-axis-line"
            x1={PAD.left}
            x2={PAD.left + plotW}
            y1={PAD.top + plotH}
            y2={PAD.top + plotH}
          />
          <line
            className="chart-axis-line"
            x1={PAD.left}
            x2={PAD.left}
            y1={PAD.top}
            y2={PAD.top + plotH}
          />

          <g clipPath={`url(#${clipId})`}>
            {visible.length === 1 && (
              <path
                className="chart-area"
                d={areaFor(visible[0].points)}
                style={{ fill: `var(--series-${((visible[0].tone - 1) % 6) + 1})` }}
              />
            )}
            {visible.map((s) => {
              const i = series.findIndex((x) => x.id === s.id);
              return (
                <path
                  key={s.id}
                  className="chart-line"
                  d={pathFor(s.points)}
                  strokeDasharray={DASHES[i % DASHES.length] || undefined}
                  style={{ stroke: `var(--series-${((s.tone - 1) % 6) + 1})` }}
                />
              );
            })}
          </g>

          {cursorX !== null && (
            <>
              <line
                className="chart-crosshair"
                x1={cursorX}
                x2={cursorX}
                y1={PAD.top}
                y2={PAD.top + plotH}
              />
              {readout?.values.map((v) =>
                typeof v.value === "number" ? (
                  <circle
                    key={v.id}
                    className="chart-dot"
                    cx={cursorX}
                    cy={yAt(v.value)}
                    r={2.5}
                    style={{ fill: `var(--series-${((v.tone - 1) % 6) + 1})` }}
                  />
                ) : null,
              )}
            </>
          )}
        </svg>

        {empty && (
          <p className="chart-empty">
            Nothing recorded in this window. That is Kavka's own history — it
            only has what it collected while this connection was up.
          </p>
        )}
      </div>

      {/* The readout is a live region, so the keyboard crosshair actually says
          something rather than just moving a line. */}
      <div className="chart-readout" role="status">
        {readout === null ? (
          <span className="chart-readout-hint">
            Hover the chart, or focus it and use ← →, to read a value.
          </span>
        ) : (
          <>
            <span className="chart-readout-time">{readoutTime(readout.at)}</span>
            {readout.values.map((v) => (
              <span key={v.id} className="chart-readout-item">
                <ChartSwatch
                  tone={v.tone}
                  index={series.findIndex((s) => s.id === v.id)}
                />
                <span className="chart-readout-label">{v.label}</span>
                <span className="chart-readout-value">
                  {typeof v.value === "number" ? (
                    formatValue(v.value)
                  ) : (
                    <span
                      className="absent"
                      title="No reading close enough to this moment to quote one."
                    >
                      ∅
                    </span>
                  )}
                </span>
              </span>
            ))}
          </>
        )}
      </div>

      <p className="chart-note" id={noteId}>
        {axisNote}
      </p>

      {/* DESIGN's a11y law: a chart is never the only way to read the data. */}
      <details className="chart-table">
        <summary>Show these numbers as a table</summary>
        {empty ? (
          <p className="table-note">Nothing to list — there are no readings in this window.</p>
        ) : (
          <div className="table-wrap">
            <table className="data-table data-table-flush">
              {/* The unit lives in the caption rather than in a stray line
                  beneath the table: a screen reader announces the caption as
                  the table's name, which is exactly where "in messages" or
                  "in bytes per second" needs to be heard. */}
              <caption className="sr-only">
                {tableCaption} — {valueHeading}
              </caption>
              <thead>
                <tr>
                  <th scope="col">Time</th>
                  {visible.map((s) => (
                    <th scope="col" className="col-num" key={s.id}>
                      {s.label}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {timeline.map((t) => (
                  <tr key={t}>
                    <td className="cell-mono">{readoutTime(t)}</td>
                    {visible.map((s) => {
                      const point = s.points.find((p) => p.x === t);
                      return (
                        <td className="col-num cell-num" key={s.id}>
                          {point === undefined || point.y === null ? (
                            <span
                              className="absent"
                              title={
                                point === undefined
                                  ? "This series has no reading at this moment."
                                  : "Nothing committed, so there is no value to record."
                              }
                            >
                              ∅
                            </span>
                          ) : (
                            formatValue(point.y)
                          )}
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </details>
    </div>
  );
}

/**
 * The legend mark, drawn as the actual dash pattern rather than a colour block.
 * A square of colour tells a deuteranope nothing; a dotted line versus a solid
 * one tells everybody the same thing.
 */
function ChartSwatch({ tone, index }: { tone: number; index: number }) {
  return (
    <svg
      className="chart-swatch"
      width="18"
      height="8"
      aria-hidden="true"
      focusable="false"
    >
      <line
        x1="0"
        x2="18"
        y1="4"
        y2="4"
        strokeDasharray={DASHES[Math.max(0, index) % DASHES.length] || undefined}
        style={{ stroke: `var(--series-${((tone - 1) % 6) + 1})` }}
      />
    </svg>
  );
}
