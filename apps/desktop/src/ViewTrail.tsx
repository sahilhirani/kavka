/**
 * WHERE YOU ARE, INSIDE A FULL-HEIGHT PANE.
 *
 * The message browser, search and SQL own the whole stage — their own
 * scrollport, their own status line, their own docked inspector — so
 * `ClusterView` deliberately draws no `<StageHead>` above them: a 26px title
 * band would take that height from the table and duplicate the `← orders`
 * crumb these panes already own (see StageHead.tsx's registry notes).
 *
 * What they still owed was the WAYFINDING half of a stage head. The rail says
 * "Topics" for as long as you are reading messages — the fidelity audit's own
 * example of the drill-down confusion — and the owner chose to keep the
 * drill-down. So the trail arrives without the band: one 12px line in the
 * pane's existing head, reusing the shell's `.whereami` treatment so it is the
 * same wayfinding device the other ten screens draw, at a cost of about
 * eighteen pixels rather than a hundred.
 *
 * PLAIN TEXT, NOT LINKS, for the same reason `StageHead`'s trail is: nothing
 * in it navigates, the crumb button beside it is the working way back, and
 * `role="navigation"` over a list of dead words is an announcement with no
 * destination. It reads out immediately before the pane's own heading, which
 * is where a screen-reader user wants it.
 */

/** The 13px right-arrow the shell's trail opens with. Same class, same size. */
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

export default function ViewTrail({
  /** Outermost first, ending with this pane. Joined with middots. */
  crumbs,
}: {
  crumbs: readonly string[];
}) {
  return (
    <p className="whereami view-trail">
      <TrailArrow />
      {/* One string, not one element per crumb: the separator is punctuation
          inside a sentence, and a screen reader announcing "list, 3 items"
          here would be describing furniture the user cannot use. */}
      {crumbs.join(" · ")}
    </p>
  );
}
