import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  errorMessage,
  groupsList,
  streamsTopology,
  type ConnectionProfile,
  type StreamsTopology,
  type TopologyEdge,
  type TopologyNode,
} from "./api";
import { useDangerSignal, type DangerReport } from "./danger";
import { Term } from "./Glossary";
import { useI18n } from "./i18n";
import Perch from "./Perch";
import { ErrorBanner } from "./ProfileEditor";

/**
 * KAFKA STREAMS TOPOLOGY — a picture Kafka cannot give us, drawn honestly.
 *
 * There is no API for this. A Streams application's topology exists inside the
 * application's own JVM and is never published anywhere a client can read it.
 * What a broker CAN tell us is which topics a consumer group subscribes to, and
 * what the internal topics are called — and Kafka Streams names those to a
 * convention (`<app-id>-<store>-changelog`, `<app-id>-<name>-repartition`) that
 * gives away most of the shape.
 *
 * So this whole view is an inference, and it says so in three places rather than
 * one: a permanent note above the picture, the `caveats` the core itself
 * attaches, and a `title` on every node. **A topology picture that looks
 * authoritative and is a guess is worse than no picture at all** — someone will
 * debug against it for an hour before discovering the processor they were
 * looking for was never in the data.
 *
 * Layout is a hand-rolled longest-path layering: sources on the left, sinks on
 * the right, one column per layer. Streams topologies genuinely contain cycles
 * (a repartition topic feeds a sub-topology that writes back to another), so the
 * relaxation is bounded by the node count rather than assuming a DAG — an
 * unbounded one would hang on exactly the applications this view is for.
 *
 * The a11y law applies here as it does to the charts: the SVG is `role="img"`,
 * and the same graph is available as a real table one keystroke away.
 */

const NODE_W = 180;
const NODE_H = 54;
const GAP_X = 72;
const GAP_Y = 18;
const PAD = 12;
/** Longer labels are clipped in the box and kept whole in its `title`. */
const LABEL_CHARS = 24;

interface KindStyle {
  /** The word under the label. Law 2: never shape or colour alone. */
  word: string;
  /** Which `--series-N` token paints the node's edge. */
  tone: number;
  dashed: boolean;
  /** Which side gets the 3px bar, if any. Sources point in, sinks point out. */
  bar: "left" | "right" | null;
  gloss: string;
}

const KINDS: Record<string, KindStyle> = {
  source_topic: {
    word: "source topic",
    tone: 2,
    dashed: false,
    bar: "left",
    gloss:
      "A topic this application reads from. Something else produces it — Streams only consumes it.",
  },
  sub_topology: {
    word: "your code",
    tone: 0,
    dashed: false,
    bar: null,
    gloss:
      "A group of processors that run together in one task. Kavka can see that it exists and cannot see what is inside it — that lives in the application, not in Kafka.",
  },
  repartition: {
    word: "repartition topic",
    tone: 4,
    dashed: true,
    bar: null,
    gloss:
      "Streams wrote records back to Kafka here so it could re-key them, then reads them again. Every record crossing this is a round trip through the brokers.",
  },
  changelog: {
    word: "changelog topic",
    tone: 3,
    dashed: true,
    bar: null,
    gloss:
      "The backup of a state store. Streams replays it to rebuild the store after a restart, which is why a large one makes recovery slow.",
  },
  sink_topic: {
    word: "sink topic",
    tone: 6,
    dashed: false,
    bar: "right",
    gloss: "A topic this application writes to. Something else consumes it.",
  },
};

function kindStyle(kind: string): KindStyle {
  return (
    KINDS[kind] ?? {
      word: kind,
      tone: 0,
      dashed: false,
      bar: null,
      gloss:
        "Kavka doesn't have a description for this kind of node — it is shown exactly as the core named it.",
    }
  );
}

function clip(label: string): string {
  return label.length <= LABEL_CHARS ? label : `${label.slice(0, LABEL_CHARS - 1)}…`;
}

interface Placed {
  node: TopologyNode;
  layer: number;
  row: number;
  x: number;
  y: number;
}

/**
 * Longest-path layering, bounded.
 *
 * Sources — nodes nothing points at, plus anything the core called a source
 * topic — start at layer 0. Every other node is pushed one column right of the
 * furthest thing that feeds it. The relaxation runs at most `nodes.length`
 * passes, so a cycle settles instead of looping forever: the result is not
 * "correct" for a cyclic graph, because no left-to-right layout is, but it is
 * stable and it terminates.
 */
function layout(nodes: TopologyNode[], edges: TopologyEdge[]): {
  placed: Placed[];
  width: number;
  height: number;
} {
  const byId = new Map(nodes.map((n) => [n.id, n]));

  // Everything starts in column 0 and is pushed right by whatever feeds it, so
  // a node nothing points at simply never moves — which is the definition of a
  // source, without having to compute in-degrees to find one.
  const layer = new Map<string, number>();
  for (const n of nodes) layer.set(n.id, 0);

  for (let pass = 0; pass < nodes.length; pass += 1) {
    let moved = false;
    for (const e of edges) {
      if (!byId.has(e.from) || !byId.has(e.to)) continue;
      const from = layer.get(e.from) ?? 0;
      const to = layer.get(e.to) ?? 0;
      // A source topic never moves right: it is where the data enters, and an
      // application that feeds one of its own inputs would otherwise drag the
      // whole picture out of shape.
      if (byId.get(e.to)?.kind === "source_topic") continue;
      if (to < from + 1) {
        layer.set(e.to, from + 1);
        moved = true;
      }
    }
    if (!moved) break;
  }

  const columns = new Map<number, TopologyNode[]>();
  for (const n of nodes) {
    const l = layer.get(n.id) ?? 0;
    const list = columns.get(l) ?? [];
    list.push(n);
    columns.set(l, list);
  }

  const placed: Placed[] = [];
  let maxRows = 0;
  for (const [l, list] of [...columns.entries()].sort((a, b) => a[0] - b[0])) {
    list.sort((a, b) => a.label.localeCompare(b.label));
    list.forEach((node, row) => {
      placed.push({
        node,
        layer: l,
        row,
        x: PAD + l * (NODE_W + GAP_X),
        y: PAD + row * (NODE_H + GAP_Y),
      });
    });
    maxRows = Math.max(maxRows, list.length);
  }

  const maxLayer = placed.reduce((m, p) => Math.max(m, p.layer), 0);
  return {
    placed,
    width: PAD * 2 + (maxLayer + 1) * NODE_W + maxLayer * GAP_X,
    height: PAD * 2 + maxRows * NODE_H + Math.max(0, maxRows - 1) * GAP_Y,
  };
}

/**
 * A group that isn't a Streams application refuses in its own vocabulary, and
 * "no internal topics matched" is true and useless. The fact the user needs is
 * that this group is an ordinary consumer, which is not a fault.
 */
function notStreamsNote(raw: string): string | null {
  const text = raw.toLowerCase();
  return text.includes("not a streams") ||
    text.includes("no streams") ||
    text.includes("no topology") ||
    text.includes("no internal topics")
    ? "This group doesn't look like a Kafka Streams application. Kavka recognises one by its internal topics — names ending in -repartition or -changelog, prefixed with the application id — and this group has none. An ordinary consumer group has no topology to draw, which is not a fault."
    : null;
}

interface StreamsTabProps {
  profile: ConnectionProfile;
  group: string | null;
  onSelectGroup: (group: string | null) => void;
  onDanger: DangerReport;
}

export default function StreamsTab({
  profile,
  group,
  onSelectGroup,
  onDanger,
}: StreamsTabProps) {
  const [groups, setGroups] = useState<string[] | null>(null);
  /**
   * The group list FAILED, which is a different fact from a cluster with no
   * groups on it — and the one the picker cannot show. `failure` is the
   * topology's; this is the list's, and it survives dismissing the banner.
   */
  const [listFailed, setListFailed] = useState(false);
  const [topology, setTopology] = useState<StreamsTopology | null>(null);
  const [loading, setLoading] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  const { t } = useI18n();
  const note = failure === null ? null : notStreamsNote(failure);
  // A group that simply isn't a Streams app is a fact, not a danger — only an
  // unexplained failure reaches the prod damper. Same rule as QuorumPanel.
  useDangerSignal(error !== null, onDanger);

  useEffect(() => {
    let cancelled = false;
    groupsList(profile.id)
      .then((list) => {
        if (cancelled) return;
        setGroups(list.map((g) => g.group_id));
        setListFailed(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setGroups([]);
        setListFailed(true);
        setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [profile.id]);

  const fetchTopology = useCallback(
    (groupId: string) => {
      const mine = ++seq.current;
      setLoading(true);
      setTopology(null);
      setFailure(null);
      streamsTopology(profile.id, groupId)
        .then((next) => {
          if (seq.current !== mine) return;
          setTopology(next);
        })
        .catch((err: unknown) => {
          if (seq.current !== mine) return;
          setFailure(errorMessage(err));
        })
        .finally(() => {
          if (seq.current === mine) setLoading(false);
        });
    },
    [profile.id],
  );

  useEffect(() => {
    if (group === null) {
      setTopology(null);
      setFailure(null);
      return;
    }
    fetchTopology(group);
    return () => {
      seq.current += 1;
    };
  }, [group, fetchTopology]);

  const graph = useMemo(
    () =>
      topology === null
        ? null
        : layout(topology.nodes, topology.edges),
    [topology],
  );

  return (
    <>
      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      {/* THE INFERENCE, SAID FIRST. The whole screen is a guess, so the guess
          belongs in the verdict rather than only in the note above the picture:
          a caveat under a diagram is a caveat people quote the diagram without.

          An unexplained failure is NOT passed in as `error`. The panel below
          already renders it through `classifyError` with its raw text and a
          dismiss, and two banners carrying the same classified sentence is one
          banner too many — so the verdict says it can't answer and points at
          the one place that says why. */}
      <Perch
        screen={t("rail.item.streams")}
        loading={loading}
        tone={topology !== null && topology.nodes.length > 0 ? "watch" : "unknown"}
        caveat={
          topology !== null && topology.nodes.length > 0
            ? t("perch.streams.caveat")
            : undefined
        }
      >
        {/* A group list Kavka never got is not a cluster with no groups on it.
            The sentence is the Groups screen's because the fact is that
            screen's — this read IS `groupsList` — and borrowing the translated
            one beats a second string saying the same thing in six catalogs. */}
        {group === null
          ? listFailed
            ? t("perch.groups.unread")
            : groups !== null && groups.length === 0
              ? t("perch.streams.noGroups")
              : t("perch.streams.pick")
          : note !== null
            ? t("perch.streams.notStreams", { group })
            : failure !== null
              ? t("perch.streams.unread", { group })
              : topology === null || topology.nodes.length === 0
                ? t("perch.streams.notStreams", { group })
                : t("perch.streams.inferred", {
                  app: topology.app_id,
                  nodes: topology.nodes.length,
                  edges: topology.edges.length,
                })}
      </Perch>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Streams topology
            {topology !== null && (
              <span className="panel-count">
                {topology.nodes.length} node
                {topology.nodes.length === 1 ? "" : "s"} ·{" "}
                {topology.edges.length} link
                {topology.edges.length === 1 ? "" : "s"}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={group === null || loading}
              aria-busy={loading || undefined}
              title={
                group === null
                  ? "Pick an application first"
                  : loading
                    ? "Kavka is working it out"
                    : "Work the shape out again from what the cluster reports now"
              }
              onClick={() => group !== null && fetchTopology(group)}
            >
              Refresh
            </button>
          </div>
        </div>

        <p className="table-note">
          Kafka Streams applications join topics together through topics of their
          own. Kavka reconstructs the shape from what this{" "}
          <Term name="consumer-group">consumer group</Term> subscribes to and
          from the names of those internal topics.
        </p>

        <div className="field field-inline">
          <label className="field-label" htmlFor="streams-group">
            Which application?
          </label>
          <select
            id="streams-group"
            className="input-mono"
            value={group ?? ""}
            onChange={(e) =>
              onSelectGroup(e.target.value.length === 0 ? null : e.target.value)
            }
          >
            <option value="">Pick a group…</option>
            {(groups ?? []).map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
          <span className="field-hint">
            A Streams application's group id is its <code>application.id</code>.
            Groups that aren't Streams applications are listed too, and Kavka
            says so rather than drawing an empty picture.
          </span>
        </div>

        {group === null ? (
          <p className="table-note">
            {listFailed
              ? t("perch.groups.unread")
              : groups !== null && groups.length === 0
                ? "This cluster has no consumer groups yet, so there is nothing to work a topology out from."
                : "Pick an application above and Kavka will work out what it reads, what it writes and what it keeps in between."}
          </p>
        ) : loading ? (
          <p className="table-note">
            Working out what {group} is connected to…
          </p>
        ) : failure !== null ? (
          note !== null ? (
            <p className="table-note">{note}</p>
          ) : (
            <ErrorBanner raw={failure} />
          )
        ) : topology === null ? null : topology.nodes.length === 0 ? (
          <p className="table-note">
            Kavka found nothing to draw for {group}. That normally means it is an
            ordinary consumer group rather than a Streams application — Kavka
            recognises one by its internal topics, named{" "}
            <code>{group}-…-repartition</code> and{" "}
            <code>{group}-…-changelog</code>, and this cluster has none of them.
          </p>
        ) : (
          <>
            {/* THE HONESTY NOTE. Permanent, not a disclosure — the whole
                picture is a guess, and a guess behind a chevron is a claim. */}
            <div className="inferred-note" role="note">
              <p className="inferred-title">
                Inferred from topic names — Kafka doesn't expose the real
                topology.
              </p>
              <p className="inferred-detail">
                Every box below was worked out from what{" "}
                <span className="cell-mono">{topology.app_id}</span> subscribes
                to and how its internal topics are named. Nothing here was read
                from the application itself, because there is nowhere to read it
                from.
              </p>
              {topology.caveats.length > 0 && (
                <ul className="inferred-caveats">
                  {topology.caveats.map((caveat) => (
                    <li key={caveat}>{caveat}</li>
                  ))}
                </ul>
              )}
            </div>

            {graph !== null && (
              <TopologyGraph topology={topology} graph={graph} />
            )}

            <TopologyLegend />

            {/* Same law as the charts: the picture is never the only way in. */}
            <details className="chart-table">
              <summary>Show this topology as a list</summary>
              <div className="table-wrap">
                <table className="data-table data-table-flush">
                  <caption className="sr-only">
                    Inferred topology of {topology.app_id}, node by node
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">Node</th>
                      <th scope="col">Kind</th>
                      <th scope="col">Topics</th>
                      <th scope="col">Feeds into</th>
                    </tr>
                  </thead>
                  <tbody>
                    {topology.nodes.map((node) => {
                      const downstream = topology.edges
                        .filter((e) => e.from === node.id)
                        .map(
                          (e) =>
                            topology.nodes.find((n) => n.id === e.to)?.label ??
                            e.to,
                        );
                      return (
                        <tr key={node.id}>
                          <td>{node.label}</td>
                          <td>{kindStyle(node.kind).word}</td>
                          <td className="cell-mono">
                            {node.topics.length === 0 ? (
                              <span className="absent">∅</span>
                            ) : (
                              node.topics.join(", ")
                            )}
                          </td>
                          <td>
                            {downstream.length === 0 ? (
                              <span
                                className="absent"
                                title="Nothing downstream — this is where the data leaves the application."
                              >
                                ∅
                              </span>
                            ) : (
                              downstream.join(", ")
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </details>
          </>
        )}
      </section>
    </>
  );
}

function TopologyGraph({
  topology,
  graph,
}: {
  topology: StreamsTopology;
  graph: { placed: Placed[]; width: number; height: number };
}) {
  const byId = useMemo(
    () => new Map(graph.placed.map((p) => [p.node.id, p])),
    [graph],
  );

  return (
    <div className="topology-scroll">
      <svg
        className="topology"
        width={graph.width}
        height={graph.height}
        role="img"
        aria-label={`Inferred topology of ${topology.app_id}: ${topology.nodes.length} nodes and ${topology.edges.length} links, sources on the left and sinks on the right. The same information is in the list below.`}
      >
        <defs>
          <marker
            id="topology-arrow"
            viewBox="0 0 8 8"
            refX="7"
            refY="4"
            markerWidth="7"
            markerHeight="7"
            orient="auto-start-reverse"
          >
            <path className="topology-arrowhead" d="M0 0 L8 4 L0 8 z" />
          </marker>
        </defs>

        {topology.edges.map((edge, i) => {
          const from = byId.get(edge.from);
          const to = byId.get(edge.to);
          if (from === undefined || to === undefined) return null;
          const x1 = from.x + NODE_W;
          const y1 = from.y + NODE_H / 2;
          const x2 = to.x;
          const y2 = to.y + NODE_H / 2;
          const bend = Math.max(24, Math.abs(x2 - x1) / 2);
          return (
            <path
              key={`${edge.from}->${edge.to}:${i}`}
              className="topology-edge"
              markerEnd="url(#topology-arrow)"
              d={`M${x1} ${y1} C${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`}
            />
          );
        })}

        {graph.placed.map((p) => {
          const style = kindStyle(p.node.kind);
          const stroke =
            style.tone === 0
              ? "var(--border-control)"
              : `var(--series-${((style.tone - 1) % 6) + 1})`;
          return (
            <g key={p.node.id} className="topology-node">
              <title>
                {`${p.node.label} — ${style.word}. ${style.gloss}${
                  p.node.topics.length > 0
                    ? ` Topics: ${p.node.topics.join(", ")}.`
                    : ""
                }`}
              </title>
              <rect
                className={`topology-box${style.dashed ? " topology-box-dashed" : ""}${
                  p.node.kind === "sub_topology" ? " topology-box-code" : ""
                }`}
                x={p.x}
                y={p.y}
                width={NODE_W}
                height={NODE_H}
                style={{ stroke }}
              />
              {style.bar !== null && (
                <rect
                  className="topology-bar"
                  x={style.bar === "left" ? p.x : p.x + NODE_W - 3}
                  y={p.y}
                  width={3}
                  height={NODE_H}
                  style={{ fill: stroke }}
                />
              )}
              <text className="topology-label" x={p.x + 12} y={p.y + 22}>
                {clip(p.node.label)}
              </text>
              <text className="topology-kind" x={p.x + 12} y={p.y + 39}>
                {style.word}
                {p.node.topics.length > 1 ? ` · ${p.node.topics.length} topics` : ""}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

/** Shape and colour both mean something, so both are spelled out in words. */
function TopologyLegend() {
  return (
    <ul className="topology-legend">
      {Object.entries(KINDS).map(([kind, style]) => (
        <li key={kind}>
          <svg
            className="topology-legend-mark"
            width="26"
            height="14"
            aria-hidden="true"
            focusable="false"
          >
            <rect
              className={`topology-box${style.dashed ? " topology-box-dashed" : ""}${
                kind === "sub_topology" ? " topology-box-code" : ""
              }`}
              x="0.5"
              y="0.5"
              width="25"
              height="13"
              style={{
                stroke:
                  style.tone === 0
                    ? "var(--border-control)"
                    : `var(--series-${((style.tone - 1) % 6) + 1})`,
              }}
            />
            {style.bar !== null && (
              <rect
                x={style.bar === "left" ? 0.5 : 23}
                y="0.5"
                width="3"
                height="13"
                style={{
                  fill:
                    style.tone === 0
                      ? "var(--border-control)"
                      : `var(--series-${((style.tone - 1) % 6) + 1})`,
                }}
              />
            )}
          </svg>
          <span className="topology-legend-word">{style.word}</span>
          <span className="topology-legend-gloss">{style.gloss}</span>
        </li>
      ))}
    </ul>
  );
}
