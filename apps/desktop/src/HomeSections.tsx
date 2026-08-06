import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  alertsHistory,
  alertsList,
  brokerConfigs,
  errorMessage,
  groupsList,
  type AlertEvent,
  type AlertRule,
  type ClusterOverview,
  type ConfigEntry,
  type ConnectionProfile,
  type GroupInfo,
} from "./api";
import type { DangerReport } from "./danger";
import { Term } from "./Glossary";
import { groupDigits } from "./format";
import { useI18n } from "./i18n";
import QuorumPanel from "./QuorumPanel";
import type { TabKey } from "./ClusterView";

/**
 * CLUSTER HOME — the regions the fidelity audit found missing.
 *
 * The audit's item 15 is the sharpest sentence in it: *"The app's Home reports
 * and never triages."* Home had three regions stacked full width — a floating
 * row of four numbers, a broker table, a quorum panel — where the mockup has
 * six, and the two that carry the direction's whole thesis were the two that
 * were absent: a stat row whose numbers SAY something, and a list of the
 * specific things that are wrong with a deep link beside each one.
 *
 * FOUR THINGS THIS FILE ADDS, and one it deliberately does not.
 *
 * 1. TILES WITH THE THIRD TIER. "Brokers 1" is a number; "Brokers 1 / as the
 *    cluster named them when you connected" is an answer AND its own caveat.
 *    `.stat-sub` already exists (Monitoring shipped it); Home simply never
 *    opted in. Cluster ID stays out of the tile row — it is a literal from
 *    Kafka, not a quantity Kavka counted, and it reads as mono below them.
 *
 * 2. "NEEDS A LOOK" — the triage list. Every row is something Kavka can PROVE
 *    from data it actually holds: an alert rule that is firing right now
 *    (from this connection's own alert log, not from a guess), and a consumer
 *    group Kafka itself reports as anything other than Stable. The panel head
 *    says so out loud — "Kavka only lists what it can prove from this
 *    snapshot" — because a triage list that looks exhaustive and is not is
 *    worse than no list at all.
 *
 * 3. THE TWO-UP SPLIT. Brokers beside the quorum rather than above it, which
 *    is worth adopting on width grounds alone: both are short tables on a
 *    1320px stage, and stacking them pushed the quorum — the thing you look
 *    at when "the cluster is up but nothing works" — below the fold.
 *
 * 4. PANEL FEET. The audit called this the highest fidelity-per-effort item in
 *    the whole product: one plain-English statement, under each table, of what
 *    the rows above it cannot tell you. Both tables here end in one.
 *
 * WHAT IT DOES NOT ADD: the mockup's Topics table on Home. Topics is its own
 * rail screen with filtering, creation and per-topic drill-down; a second,
 * poorer copy of it here would be two places to look for one answer. The audit
 * agrees ("do not restore that one").
 *
 * WHY THE WORST-LAG TILE IS NOT HERE. The mockup's fourth tile is "Worst lag
 * 82 / demo-checkout on payments". Kavka cannot compute that without asking
 * EVERY consumer group for its committed offsets — one admin round trip per
 * group, on a screen that opens the moment you connect. On a cluster with two
 * groups that is invisible; on one with four hundred it is a Home screen that
 * hangs, and the number would still be a snapshot the instant it arrived. So
 * the fourth tile counts the groups instead and says how many are settled, and
 * the lag question is answered where it is asked for — the Consumer groups
 * screen, and any lag rule the user has actually asked Kavka to watch, which
 * DOES appear in "Needs a look" the moment it fires. Reporting a number Kavka
 * did not measure would break the one rule this whole redesign is about.
 */

interface HomeSectionsProps {
  profile: ConnectionProfile;
  /** The metadata snapshot the connection came back with. */
  overview: ClusterOverview;
  /**
   * How many alert rules are firing, live from the workspace's subscription.
   *
   * NOT rendered as a number here — it is the REFETCH TRIGGER. The workspace
   * holds the live subscription and re-renders this component on every fire
   * and resolve, so keying the alert-log read on the count means the triage
   * list follows the cluster without a second subscription and without a poll.
   */
  firing: number;
  /** The quorum panel can raise a banner, and any danger has to reach the root. */
  onDanger: DangerReport;
  /**
   * Open another screen — the deep link beside each attention row.
   *
   * OPTIONAL, AND THE ROWS DEGRADE RATHER THAN DIE. Navigation is the shell's
   * (`App` owns the rail; `ClusterView` is controlled), so until the callback
   * is threaded down, an attention row states which screen answers it in words
   * instead of rendering a button that cannot navigate. A dead button is worse
   * than a sentence.
   */
  onOpenScreen?: (tab: TabKey) => void;
}

/** How much of the alert log the triage list reads. Stated in the panel foot. */
const ALERT_SCAN = 100;

/**
 * Kafka's own group states, reduced to the three that change what a row means.
 * Deliberately the same mapping `GroupsTab` uses — a group is "not settled"
 * on one screen and on the other, or the two screens are describing different
 * clusters.
 */
function groupTone(state: string): "ok" | "warn" | "danger" | "quiet" {
  switch (state.toLowerCase().replace(/[^a-z]/g, "")) {
    case "stable":
      return "ok";
    case "preparingrebalance":
    case "completingrebalance":
      return "warn";
    case "dead":
      return "danger";
    default:
      return "quiet";
  }
}

/** The 16px glyphs an attention row leads with. Decorative — the row says it. */
function AttIcon({ tone }: { tone: "warn" | "danger" | "ok" }) {
  return (
    <span className={`att-icon att-icon-${tone}`} aria-hidden="true">
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        focusable="false"
      >
        {tone === "ok" ? (
          <path d="M5 12.5l4.5 4.5L19 7.5" />
        ) : (
          <>
            <path d="M12 4l8 14H4z" />
            <path d="M12 10v3.6M12 15.8v.5" />
          </>
        )}
      </svg>
    </span>
  );
}

/** One thing that needs a look, in the order the panel renders them. */
interface Attention {
  key: string;
  tone: "warn" | "danger";
  title: string;
  sub: string;
  /**
   * Where the fix is. `label` is the button; `screen` is the rail's own word
   * for the same place, used when there is no button to press.
   */
  go?: { tab: TabKey; label: string; screen: string };
}

export default function HomeSections({
  profile,
  overview,
  firing,
  onDanger,
  onOpenScreen,
}: HomeSectionsProps) {
  const { t } = useI18n();

  // ── The two local reads ────────────────────────────────────────────────
  //
  // `groupsList` is one admin call and answers both the fourth tile and half
  // the triage list. The alert log and the rule list are LOCAL — neither
  // touches the cluster — so re-reading them on every fire costs nothing.

  const [groups, setGroups] = useState<GroupInfo[] | null>(null);
  const [groupsFailed, setGroupsFailed] = useState(false);
  const [events, setEvents] = useState<AlertEvent[] | null>(null);
  const [rules, setRules] = useState<AlertRule[]>([]);
  const [alertsFailed, setAlertsFailed] = useState(false);

  const groupSeq = useRef(0);
  useEffect(() => {
    const seq = ++groupSeq.current;
    groupsList(profile.id)
      .then((list) => {
        if (groupSeq.current !== seq) return;
        setGroups(list);
        setGroupsFailed(false);
      })
      .catch(() => {
        if (groupSeq.current !== seq) return;
        // The tile says "couldn't read" rather than 0. A zero Kavka never
        // measured is the one number this product may not print.
        setGroups(null);
        setGroupsFailed(true);
      });
    return () => {
      groupSeq.current += 1;
    };
  }, [profile.id]);

  const alertSeq = useRef(0);
  useEffect(() => {
    const seq = ++alertSeq.current;
    Promise.all([
      alertsHistory(profile.id, ALERT_SCAN),
      alertsList(profile.id).catch(() => [] as AlertRule[]),
    ])
      .then(([log, list]) => {
        if (alertSeq.current !== seq) return;
        setEvents(log);
        setRules(list);
        setAlertsFailed(false);
      })
      .catch(() => {
        if (alertSeq.current !== seq) return;
        setEvents(null);
        setAlertsFailed(true);
      });
    return () => {
      alertSeq.current += 1;
    };
    // `firing` is the trigger, not a value — see the prop's comment.
  }, [profile.id, firing]);

  // ── What the tiles say ─────────────────────────────────────────────────

  const brokerCount = overview.brokers.length;

  /**
   * THREE BUCKETS, NOT TWO, and the middle one is why.
   *
   * "Not stable" and "not Stable" are different claims. A group Kafka reports
   * as Empty has no members — normal for an application that is switched off,
   * and NOT something that needs a look — while PreparingRebalance and Dead
   * are. Folding Empty into "unsettled" would put the attention spine on this
   * tile on every cluster with one retired consumer, and the "Needs a look"
   * panel beside it would be empty, which is the worst of both: a warning with
   * nothing behind it teaches people to ignore warnings.
   *
   * The split is deliberately the same `groupTone` test the attention list
   * uses, so the tile and the panel can never disagree.
   */
  const groupCounts = useMemo(() => {
    if (groups === null) return null;
    let stable = 0;
    let unsettled = 0;
    let idle = 0;
    for (const g of groups) {
      const tone = groupTone(g.state);
      if (tone === "ok") stable += 1;
      else if (tone === "quiet") idle += 1;
      else unsettled += 1;
    }
    return { total: groups.length, stable, unsettled, idle };
  }, [groups]);

  let groupsSub: string;
  if (groupsFailed) groupsSub = t("home.tile.groups.unread");
  else if (groupCounts === null) groupsSub = t("home.tile.reading");
  else if (groupCounts.total === 0) groupsSub = t("home.tile.groups.none");
  else if (groupCounts.unsettled > 0)
    groupsSub = t("home.tile.groups.unsettled", {
      count: groupCounts.unsettled,
    });
  else if (groupCounts.idle > 0)
    groupsSub = t("home.tile.groups.idle", { count: groupCounts.idle });
  else groupsSub = t("home.tile.groups.allStable");

  // ── What needs a look ──────────────────────────────────────────────────

  const attention = useMemo<Attention[]>(() => {
    const out: Attention[] = [];

    // 1. Rules that are firing RIGHT NOW. `resolved_ms === null` is the whole
    //    test — the same event arrives twice, once on fire and once resolved,
    //    so a list built from fires alone shows conditions that cleared hours
    //    ago as if they were live.
    const live = new Map<string, AlertEvent>();
    for (const e of events ?? []) {
      // Newest first, so the first entry for a rule is its current state.
      if (!live.has(e.rule_id)) live.set(e.rule_id, e);
    }
    for (const e of live.values()) {
      if (e.resolved_ms !== null) continue;
      const kind = rules.find((r) => r.id === e.rule_id)?.kind;
      // A lag rule's fix is a lag chart; everything else is answered on the
      // screen that owns the rule.
      const tab: TabKey = kind === "lag_threshold" ? "monitoring" : "alerts";
      out.push({
        key: `alert:${e.rule_id}`,
        tone: "danger",
        title: e.rule_name,
        // Kavka's own words for the numbers that tripped it — shown verbatim,
        // never re-derived here.
        sub: e.detail === "" ? t("home.attention.alert.noDetail") : e.detail,
        go: {
          tab,
          label: t(
            tab === "monitoring"
              ? "home.attention.open.monitoring"
              : "home.attention.open.alerts",
          ),
          screen: t(
            tab === "monitoring" ? "rail.item.monitoring" : "rail.item.alerts",
          ),
        },
      });
    }

    // 2. Consumer groups Kafka itself reports as anything but Stable. Not a
    //    judgement Kavka made — the state is the cluster's own word, and the
    //    row prints it.
    for (const g of groups ?? []) {
      const tone = groupTone(g.state);
      if (tone === "ok" || tone === "quiet") continue;
      out.push({
        key: `group:${g.group_id}`,
        tone: tone === "danger" ? "danger" : "warn",
        title: t("home.attention.group.title", { group: g.group_id }),
        sub: t("home.attention.group.sub", {
          state: g.state,
          members: g.member_count,
        }),
        go: {
          tab: "groups",
          label: t("home.attention.open.groups"),
          screen: t("rail.item.groups"),
        },
      });
    }

    return out;
  }, [events, rules, groups, t]);

  /**
   * BOTH SOURCES, NOT ONE. The triage list is built from the alert log AND the
   * group list, so it is still "reading" while either of them is in flight —
   * a clear verdict printed while half the evidence is outstanding is the
   * cheerful-verdict-from-partial-data failure this product exists to refuse.
   * A source that FAILED is not still reading: that state says so in its own
   * words rather than spinning forever.
   */
  const reading =
    (events === null && !alertsFailed) || (groups === null && !groupsFailed);

  return (
    <>
      {/* THE TILES. Four numbers, each with the sentence that makes it an
          answer. `stat-grid-tiles` is the opt-in modifier Monitoring already
          uses — one tile idiom, two screens. */}
      <div className="stat-grid stat-grid-tiles">
        <div className="stat">
          <span className="stat-label">
            <Term name="broker">{t("rail.item.brokers")}</Term>
          </span>
          <span className="stat-value">{groupDigits(brokerCount)}</span>
          <span className="stat-sub">
            {brokerCount === 0
              ? t("home.tile.brokers.none")
              : t("home.tile.brokers.sub")}
          </span>
        </div>

        <div className="stat">
          <span className="stat-label">{t("rail.item.topics")}</span>
          <span className="stat-value">{groupDigits(overview.topic_count)}</span>
          <span className="stat-sub">
            {t("home.tile.topics.sub", { partitions: overview.partition_count })}
          </span>
        </div>

        <div className="stat">
          <span className="stat-label">
            <Term name="partition">{t("home.tile.partitions")}</Term>
          </span>
          <span className="stat-value">
            {groupDigits(overview.partition_count)}
          </span>
          <span className="stat-sub">{t("home.tile.partitions.sub")}</span>
        </div>

        {/* The one tile that can be a problem, so the one that can carry the
            attention spine — and it carries its WORD in the sub-caption, never
            the edge alone (Law 2). */}
        <div
          className={`stat${
            groupCounts !== null && groupCounts.unsettled > 0 ? " stat-attn" : ""
          }`}
        >
          <span className="stat-label">
            <Term name="consumer-group">{t("rail.item.groups")}</Term>
          </span>
          <span className="stat-value">
            {groupCounts === null ? (
              <span className="absent" title={t("home.tile.groups.unread")}>
                ∅
              </span>
            ) : (
              groupDigits(groupCounts.total)
            )}
          </span>
          <span className="stat-sub">{groupsSub}</span>
        </div>
      </div>

      {/* The cluster's own identifier. Outside the tile row on purpose: it is
          a literal Kafka handed over, not a quantity Kavka counted, so it is
          mono and it is not a card. */}
      <p className="home-clusterid">
        <span className="home-clusterid-label">{t("home.clusterId")}</span>
        <span className="home-clusterid-value">
          {overview.cluster_id ?? (
            <span className="absent" title={t("home.clusterId.absent")}>
              ∅
            </span>
          )}
        </span>
      </p>

      {/* ── NEEDS A LOOK ──────────────────────────────────────────────── */}
      <section className="panel" aria-label={t("home.attention.title")}>
        <div className="panel-head">
          <h2 className="panel-title">
            {t("home.attention.title")}
            {/* No count while the evidence is in flight or a read failed. A
                "0" beside a list that has not finished being built is the
                same lie as an empty list. */}
            {!reading && !alertsFailed && (
              <span className="panel-count">{attention.length}</span>
            )}
          </h2>
          {/* The provenance line the mockup puts in the panel head, and the
              reason this list is safe to render at all: it says what it is a
              list OF before the user reads it as a list of everything. */}
          <span className="panel-head-note">
            {t("home.attention.provenance")}
          </span>
        </div>

        {alertsFailed ? (
          <p className="table-note">{t("home.attention.unread")}</p>
        ) : reading ? (
          <p className="table-note">{t("home.attention.reading")}</p>
        ) : attention.length === 0 && groupsFailed ? (
          // Half the evidence is missing, so there is no clear state to
          // render. "Nothing needs a look" is a claim, and this screen cannot
          // make it about groups it could not read.
          <p className="table-note">{t("home.attention.partial")}</p>
        ) : attention.length === 0 ? (
          <div className="att att-clear">
            <AttIcon tone="ok" />
            <div className="att-body">
              <p className="att-title">{t("home.attention.clear")}</p>
              <p className="att-sub">{t("home.attention.clear.sub")}</p>
            </div>
          </div>
        ) : (
          <ul className="att-list">
            {attention.map((item) => {
              const go = item.go;
              return (
                <li key={item.key} className="att">
                  <AttIcon tone={item.tone} />
                  <div className="att-body">
                    <p className="att-title">{item.title}</p>
                    <p className="att-sub">{item.sub}</p>
                  </div>
                  {go !== undefined && (
                    <div className="att-act">
                      {onOpenScreen === undefined ? (
                        // No navigation threaded down yet — say where the
                        // answer lives instead of drawing a button that cannot
                        // go there.
                        <span className="att-where">
                          {t("home.attention.where", { screen: go.screen })}
                        </span>
                      ) : (
                        <button
                          type="button"
                          className="btn btn-sm"
                          onClick={() => onOpenScreen(go.tab)}
                        >
                          {go.label}
                        </button>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}

        {/* A list that is complete about alerts and silent about groups has to
            say which half is missing, or the rows it DID find make it look
            exhaustive. */}
        {!alertsFailed && !reading && groupsFailed && attention.length > 0 && (
          <p className="table-note">{t("home.attention.groupsUnread")}</p>
        )}

        <p className="panel-foot">
          {t("home.attention.foot", { limit: ALERT_SCAN })}
        </p>
      </section>

      {/* ── BROKERS │ QUORUM ─────────────────────────────────────────────
          Two short tables side by side rather than stacked. It wraps to one
          column below the split's own min-width, so the quorum's replica table
          never gets squeezed into a scroll well it does not need. */}
      <div className="home-split">
        <HomeBrokers profile={profile} overview={overview} />
        <div className="home-split-side">
          <QuorumPanel profile={profile} onDanger={onDanger} />
        </div>
      </div>
    </>
  );
}

/**
 * The broker list, with the mockup's folded Details.
 *
 * THE FOLD FETCHES ON FIRST OPEN, NEVER ON MOUNT. Broker configuration is a
 * second admin round trip and nobody arriving on Home has asked for it; a
 * disclosure that costs a request the moment the screen paints is a disclosure
 * that is not really folded. `<details>` gives us the exact event for it.
 *
 * AND IT SAYS WHOSE CONFIGURATION IT IS. These values come from ONE broker —
 * the first the cluster named — and a cluster whose brokers disagree is a real
 * and common misconfiguration. Printing "Default replication 1" without saying
 * which machine said so is the kind of quiet half-truth this redesign exists
 * to remove, so the fold ends in the sentence that says it.
 */
function HomeBrokers({
  profile,
  overview,
}: {
  profile: ConnectionProfile;
  overview: ClusterOverview;
}) {
  const { t } = useI18n();
  const first = overview.brokers[0] ?? null;
  const [configs, setConfigs] = useState<ConfigEntry[] | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [asked, setAsked] = useState(false);

  const load = useCallback(() => {
    if (asked || first === null) return;
    setAsked(true);
    brokerConfigs(profile.id, first.id)
      .then(setConfigs)
      .catch((err) => setConfigError(errorMessage(err)));
  }, [asked, first, profile.id]);

  /**
   * The five settings the mockup folds away, by their real Kafka names.
   *
   * "Protocol version" and not "Kafka version": `inter.broker.protocol.version`
   * is what the broker actually reports, and it is not the same thing as the
   * binary's version — a 3.7 broker can be speaking 3.5. Labelling it as the
   * Kafka version would be a guess dressed as a fact.
   */
  const FACTS: readonly { key: string; labelKey: Parameters<typeof t>[0] }[] = [
    { key: "inter.broker.protocol.version", labelKey: "home.brokers.fact.protocol" },
    { key: "log.dirs", labelKey: "home.brokers.fact.logDirs" },
    {
      key: "default.replication.factor",
      labelKey: "home.brokers.fact.replication",
    },
    { key: "auto.create.topics.enable", labelKey: "home.brokers.fact.autoCreate" },
    { key: "log.retention.hours", labelKey: "home.brokers.fact.retention" },
  ];

  return (
    <section className="panel home-split-main">
      <div className="panel-head">
        <h2 className="panel-title">
          <Term name="broker">{t("rail.item.brokers")}</Term>
          <span className="panel-count">{overview.brokers.length}</span>
        </h2>
      </div>

      {/* The ledger gutter carries the broker id — the row's address in
          Kafka's own vocabulary — then the rule, then the payload.

          NO role="grid" and NO aria-rowcount/aria-rowindex. This is a static,
          fully-rendered table, so the implicit <table> semantics are already
          complete and correct. */}
      <div className="table-wrap">
        <table className="data-table">
          <caption className="sr-only">{t("home.brokers.caption")}</caption>
          <thead>
            <tr>
              <th scope="col" className="ledger-gutter">
                ID
              </th>
              <th scope="col">Host</th>
              <th scope="col" className="col-num">
                Port
              </th>
            </tr>
          </thead>
          <tbody>
            {overview.brokers.length === 0 ? (
              <tr>
                <td colSpan={3} className="cell-empty">
                  {t("home.brokers.none")}
                </td>
              </tr>
            ) : (
              overview.brokers.map((b) => (
                <tr key={b.id}>
                  <td className="ledger-gutter">{b.id}</td>
                  <td className="cell-mono">{b.host}</td>
                  <td className="col-num cell-num">{b.port}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {first !== null && (
        <details className="disclose" onToggle={load}>
          <summary>
            <svg
              className="caret"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              aria-hidden="true"
              focusable="false"
            >
              <path d="M6 4l4 4-4 4" />
            </svg>
            {t("home.brokers.details")}
            <span className="sum-note">
              {t("home.brokers.details.note", { id: first.id })}
            </span>
          </summary>
          <div>
            {configError !== null ? (
              <p className="table-note">
                {t("home.brokers.details.unread", { id: first.id })}
              </p>
            ) : configs === null ? (
              <p className="table-note">
                {t("home.brokers.details.reading", { id: first.id })}
              </p>
            ) : (
              <>
                {/* Fragments, not wrapper divs: `.facts` is a two-column grid
                    whose dt and dd are its own children, and a div around a
                    pair collapses both into one cell. */}
                <dl className="facts">
                  {FACTS.map((fact) => {
                    const entry = configs.find((c) => c.name === fact.key);
                    return (
                      <Fragment key={fact.key}>
                        <dt>{t(fact.labelKey)}</dt>
                        <dd className="facts-mono">
                          {entry === undefined || entry.value === null ? (
                            <span className="absent">
                              {t("home.brokers.fact.absent")}
                            </span>
                          ) : (
                            entry.value
                          )}
                        </dd>
                      </Fragment>
                    );
                  })}
                </dl>
                <p className="table-note">
                  {t("home.brokers.details.caveat", { id: first.id })}
                </p>
              </>
            )}
          </div>
        </details>
      )}

      {/* What the rows above cannot tell you. The whole point of a panel foot. */}
      <p className="panel-foot">{t("home.brokers.foot")}</p>
    </section>
  );
}
