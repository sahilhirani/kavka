import { useEffect } from "react";

/**
 * THE STAGE — one scrollport, and one place that puts it back at the top.
 *
 * Arriving on a screen halfway down it is the single most disorienting thing a
 * persistent scrollport does, and it is invisible in review: it only happens
 * when the screen you LEFT was longer than the one you arrived on. Every
 * report of it comes from the field, never from a PR.
 *
 * WHY A MODULE AND NOT A PROP. Two components own navigation and neither can
 * see the other's state. `App` owns which screen and which rail item; the
 * placement inside a cluster screen — which topic, which pane, which broker,
 * which connector — belongs to `ClusterView`, and threading it back up would
 * put a second copy of a record that has exactly one writer (see
 * `readPlacement`) into the shell. So the SCROLLPORT is what they share: the
 * stage registers itself here once, and anything that knows it has moved the
 * user says so through `useStageTop`.
 *
 * THAT IS THE WHOLE CONTRACT. No screen scrolls itself. A per-screen
 * `useEffect(() => window.scrollTo(0))` is the shape this exists to prevent —
 * ten copies, nine of them right, and the tenth found by a user.
 *
 * THE THREE FULL-HEIGHT PANES DO NOT USE IT. The message browser, search and
 * SQL own their own scrollports (see `.cluster-view-full`); the stage under
 * them does not scroll, so scrolling it is a no-op rather than a conflict.
 */

/**
 * The element `App` hands over. Module-level rather than a context because it
 * is one DOM node for the process's whole life and a context would make every
 * screen re-render when the shell re-mounted its own stage.
 */
let stage: HTMLElement | null = null;

/**
 * Called by the stage element's `ref`. React runs ref callbacks during commit,
 * BEFORE effects, so a `useStageTop` firing on the very first paint already
 * has somewhere to scroll.
 *
 * It is passed the element directly (`ref={registerStage}`), which is also how
 * it gets `null` on unmount — nothing has to remember to clear it.
 */
export function registerStage(el: HTMLElement | null): void {
  stage = el;
}

/**
 * Put the stage back at the top. Silent when there is no stage: the palette,
 * the dialogs and the tests all run without one, and none of them should have
 * to know that.
 *
 * `scrollTo` and not `scrollTop = 0` because the horizontal offset matters
 * too — a wide table left the stage scrolled right, and arriving on the next
 * screen with its title off the left edge is the same bug in the other axis.
 * No `behavior`, so it is instant: this is not a movement the user asked for,
 * and animating it would mean animating it under `prefers-reduced-motion`.
 */
export function scrollStageToTop(): void {
  stage?.scrollTo({ top: 0, left: 0 });
}

/**
 * Scroll the stage whenever `token` changes.
 *
 * The token is a STRING the caller builds out of everything that means
 * "somewhere else" — screen, rail item, cluster, topic, pane. A string rather
 * than a dependency array so the two callers can each describe their own idea
 * of a location without this module knowing what either contains, and so
 * pressing the SAME rail item twice can still count as a navigation (the rail
 * folds a counter into its token for exactly that).
 *
 * It fires on mount as well. That is a no-op on a stage that is already at the
 * top, and it is correct on the one that is not: a remount with the scrollport
 * reused is still an arrival.
 */
export function useStageTop(token: string): void {
  useEffect(() => {
    scrollStageToTop();
  }, [token]);
}
