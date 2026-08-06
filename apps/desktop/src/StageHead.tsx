import type { ReactNode } from "react";
import type { MessageKey } from "./i18n";
import type { TabKey } from "./ClusterView";

/**
 * THE STAGE HEAD — how a screen introduces itself.
 *
 * Every Jackdaw screen opens in three registers before the Perch says how it
 * is going: WHERE YOU ARE (a middot trail), WHAT THIS IS (a 26px title, and
 * the chips that identify the particular thing you are looking at), and WHAT
 * YOU CAN DO (screen-level actions, pushed right). The app shipped with none
 * of them — the fidelity audit's second-largest gap after the deleted sidebar
 * — so the 26px title tier was absent from the entire product, the wayfinding
 * trail did not exist, and screen-level buttons were scattered into whichever
 * panel head happened to need one.
 *
 * IT IS THE FIRST CHILD OF THE STAGE, ABOVE THE PERCH. The order is the
 * argument: the head says what this screen is, and then the Perch says what
 * Kafka can actually tell you about it. A verdict that arrives before its
 * subject is a verdict about nothing.
 *
 * THE TRAIL IS PLAIN TEXT, NOT LINKS. Nothing in it navigates today, and a
 * breadcrumb whose crumbs are not clickable should not pretend to be one —
 * `role="navigation"` on a list of dead words is an announcement with no
 * destination. It reads out in order immediately before the heading, which is
 * where a screen reader user wants it. When a crumb earns a destination (the
 * message browser's "Topics · orders" is the first candidate), it becomes a
 * button and this comment changes with it.
 *
 * `<h2>`, NOT `<h1>`. The rail's cluster card already carries the document's
 * `<h1>` — the cluster is the thing the whole window is about, and the screen
 * is a view of it. Panel titles inside the screens are `<h2>` too, which is
 * flat and was flat before this component existed; demoting them to `<h3>` is
 * a per-screen edit that belongs with the per-screen adoption below.
 */

interface StageHeadProps {
  /**
   * The wayfinding trail, outermost first, ending with this screen. Joined
   * with middots. Always opens with the cluster identity or the rail group —
   * a trail that starts at the screen is not a trail.
   */
  trail: readonly string[];
  /** The screen, in the user's words. */
  title: string;
  /**
   * What you are looking at, beside the title on the same baseline: an
   * `<EnvChip>`, a LIVE badge, a partition count. Identity, never status a
   * sentence should be carrying.
   */
  chips?: ReactNode;
  /** One plain sentence of context. Capped at 82ch by the stylesheet. */
  sub?: ReactNode;
  /**
   * Screen-level actions, pushed right, rightmost usually `.btn-primary`.
   * Two or three. This is where the tool buttons that ended up inside panel
   * heads come home.
   */
  actions?: ReactNode;
}

/** The 13px right-arrow the mockup opens its trail with. */
function TrailArrow() {
  return (
    <svg
      className="wa-arrow"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M5 12h14M13 6l6 6-6 6" />
    </svg>
  );
}

export default function StageHead({
  trail,
  title,
  chips,
  sub,
  actions,
}: StageHeadProps) {
  return (
    <header className="stage-head">
      <div className="sh-text">
        {trail.length > 0 && (
          <p className="whereami">
            <TrailArrow />
            {/* One string, not one element per crumb: the separator is
                punctuation inside a sentence, and a screen reader that
                announced "list, 4 items" here would be describing furniture
                the user cannot use. */}
            {trail.join(" · ")}
          </p>
        )}
        <h2 className="sh-title">
          {title}
          {chips}
        </h2>
        {sub !== undefined && sub !== null && sub !== "" && (
          <p className="sh-sub">{sub}</p>
        )}
      </div>
      {actions !== undefined && <div className="stage-actions">{actions}</div>}
    </header>
  );
}

/**
 * THE REGISTRY — every cluster screen's default head.
 *
 * `Record<TabKey, …>` is the point: adding a screen to `CLUSTER_RAIL` without
 * writing its title and its one sentence of context is a COMPILE ERROR, the
 * same guard `TABS` gives the rail. No screen can ship headless by omission.
 *
 * THE SUBTITLE IS NOT DECORATION. It is one honest sentence about what this
 * screen can and cannot tell you — the same voice as a Perch caveat, said
 * before the numbers rather than after them. "Charts drawn only from readings
 * Kavka took while it was open" is a limitation, and stating it in the header
 * is cheaper than explaining it in a support thread.
 *
 * ACTIONS ARE NOT HERE and never will be. A registry can hold a message key; it
 * cannot hold a handler bound to the cluster on screen without becoming a
 * second copy of the screen's own props. Screens pass `actions` when they take
 * their head over — see `ownHead`.
 *
 * `ownHead` IS THE HANDOVER. While it is absent, `ClusterView` draws the head
 * from this table. A screen that has grown its own — with its chips, its
 * actions and a trail that knows which topic is open — sets it and renders
 * `<StageHead>` itself, and the shell stops. It is a per-screen switch rather
 * than a big-bang migration precisely so the ten screens can convert one at a
 * time without a single frame of the app rendering two heads or none.
 */
export interface ScreenHead {
  /**
   * Usually the rail's own label — the rail and the title agreeing is the
   * wayfinding, not a duplication to be optimised away.
   */
  titleKey: MessageKey;
  /** One sentence, 82ch, plain English, honest about the limits. */
  subKey: MessageKey;
  /** True once the screen renders its own `<StageHead>`. See above. */
  ownHead?: true;
}

export const CLUSTER_HEADS: Record<TabKey, ScreenHead> = {
  // "Home" is what the rail says under the group "Cluster"; on its own, as a
  // 26px title, it needs the noun back.
  overview: { titleKey: "stage.overview.title", subKey: "stage.overview.sub" },
  topics: { titleKey: "rail.item.topics", subKey: "stage.topics.sub" },
  groups: { titleKey: "rail.item.groups", subKey: "stage.groups.sub" },
  brokers: { titleKey: "rail.item.brokers", subKey: "stage.brokers.sub" },
  monitoring: {
    titleKey: "rail.item.monitoring",
    subKey: "stage.monitoring.sub",
  },
  alerts: { titleKey: "rail.item.alerts", subKey: "stage.alerts.sub" },
  streams: { titleKey: "rail.item.streams", subKey: "stage.streams.sub" },
  acls: { titleKey: "rail.item.acls", subKey: "stage.acls.sub" },
  masking: { titleKey: "rail.item.masking", subKey: "stage.masking.sub" },
  connect: { titleKey: "rail.item.connect", subKey: "stage.connect.sub" },
};
