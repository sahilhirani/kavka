/**
 * Hand-rolled row windowing. No dependency, ~120 lines, and it reads its row
 * height from the design system rather than owning one.
 *
 * docs/DESIGN.md law 3 is the contract this implements: `--row-h` is a fixed
 * token "the virtualizer reads at runtime", and nothing inside a scrolling
 * table may carry a transition, filter, border-radius or box-shadow. Fixed
 * row height is what makes the arithmetic below exact — every row is at the
 * same offset it would be at if all of them were rendered, so scrollbar
 * geometry, keyboard navigation and `aria-rowindex` all agree with the DOM.
 *
 * §9 gate 2: a dev-mode assertion compares the token against a real row's
 * measured height and fails loudly, because a density change, a font change
 * or a platform override that moves the row silently would misplace every
 * row on screen.
 */

import { useCallback, useEffect, useRef, useState } from "react";

export interface RowMetrics {
  /** `--row-h`, in px. */
  rowH: number;
  /** `--row-h-head`, in px — the sticky header overlays this much of the top. */
  headH: number;
}

/** Only ever used if the custom properties cannot be read at all. */
const FALLBACK: RowMetrics = { rowH: 30, headH: 28 };

/** Rows rendered above and below the viewport. Eight covers a fast flick. */
export const OVERSCAN = 8;

function readPx(style: CSSStyleDeclaration, name: string, fallback: number): number {
  const raw = style.getPropertyValue(name).trim();
  const parsed = Number.parseFloat(raw);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

/**
 * The current row metrics for a subtree, re-measured whenever anything that
 * could move them changes: the density attribute, the theme attribute, or a
 * window resize (which is where a platform font override shows up).
 */
export function useRowMetrics(
  ref: React.RefObject<HTMLElement | null>,
): RowMetrics {
  const [metrics, setMetrics] = useState<RowMetrics>(FALLBACK);

  useEffect(() => {
    const measure = () => {
      const el = ref.current;
      if (!el) return;
      const style = getComputedStyle(el);
      const next = {
        rowH: readPx(style, "--row-h", FALLBACK.rowH),
        headH: readPx(style, "--row-h-head", FALLBACK.headH),
      };
      setMetrics((prev) =>
        prev.rowH === next.rowH && prev.headH === next.headH ? prev : next,
      );
    };
    measure();
    window.addEventListener("resize", measure);
    // The density toggle writes an attribute on the app root; the cached row
    // height has to die with it or every row lands 6px out.
    const observer = new MutationObserver(measure);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-density", "data-theme"],
    });
    const body = document.body;
    observer.observe(body, {
      attributes: true,
      attributeFilter: ["data-density", "data-theme"],
    });
    return () => {
      window.removeEventListener("resize", measure);
      observer.disconnect();
    };
  }, [ref]);

  return metrics;
}

export interface VirtualWindow extends RowMetrics {
  /** First rendered row index, inclusive. */
  start: number;
  /** Last rendered row index, exclusive. */
  end: number;
  /** Height of the spacer row above `start`, in px. */
  padTop: number;
  /** Height of the spacer row below `end`, in px. */
  padBottom: number;
  /** Scroll viewport height, in px. 0 until the first measure. */
  viewport: number;
  scrollTop: number;
}

/**
 * Window `total` fixed-height rows against a scroll container.
 *
 * The returned `onScroll` must be wired to the container's `onScroll`; the
 * container's height must come from layout (a flex child with `min-height:
 * 0`), not from its contents, or `contain: strict` collapses it.
 */
export function useVirtualRows(
  scrollRef: React.RefObject<HTMLElement | null>,
  total: number,
  overscan: number = OVERSCAN,
): { win: VirtualWindow; onScroll: () => void; remeasure: () => void } {
  const metrics = useRowMetrics(scrollRef);
  const [viewport, setViewport] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    // Only commit real movement: a horizontal scroll or a rubber-band bounce
    // otherwise re-renders the whole window for nothing.
    setScrollTop((prev) => (prev === el.scrollTop ? prev : el.scrollTop));
  }, [scrollRef]);

  const remeasure = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setViewport((prev) => (prev === el.clientHeight ? prev : el.clientHeight));
    setScrollTop((prev) => (prev === el.scrollTop ? prev : el.scrollTop));
  }, [scrollRef]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    remeasure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => remeasure());
    observer.observe(el);
    return () => observer.disconnect();
  }, [scrollRef, remeasure]);

  const rowH = metrics.rowH;
  const first = Math.max(0, Math.floor(scrollTop / rowH) - overscan);
  // The sticky header covers the top of the viewport, so the visible run is
  // never longer than the viewport itself — overscan absorbs the difference.
  const span = Math.ceil((viewport || rowH * 20) / rowH) + overscan * 2 + 1;
  const start = Math.min(first, Math.max(0, total - 1));
  const end = Math.min(total, start + span);

  const win: VirtualWindow = {
    ...metrics,
    start: total === 0 ? 0 : start,
    end: total === 0 ? 0 : end,
    padTop: total === 0 ? 0 : start * rowH,
    padBottom: total === 0 ? 0 : Math.max(0, (total - end) * rowH),
    viewport,
    scrollTop,
  };

  return { win, onScroll, remeasure };
}

/**
 * Scroll a row index into view given fixed row heights. Written here rather
 * than with `scrollIntoView` because the target row usually is not in the DOM:
 * that is the entire point of the windowing.
 */
export function scrollIndexIntoView(
  el: HTMLElement | null,
  index: number,
  metrics: RowMetrics,
): void {
  if (!el) return;
  const top = index * metrics.rowH;
  const bottom = top + metrics.rowH;
  // The sticky header floats over the first `headH` px of the viewport.
  const visibleTop = el.scrollTop + metrics.headH;
  const visibleBottom = el.scrollTop + el.clientHeight;
  if (top < visibleTop) el.scrollTop = Math.max(0, top - metrics.headH);
  else if (bottom > visibleBottom) el.scrollTop = bottom - el.clientHeight;
}

/** Within this many px of the bottom still counts as "pinned to the bottom". */
export const BOTTOM_EPSILON = 4;

export function isPinnedToBottom(el: HTMLElement | null): boolean {
  if (!el) return true;
  return (
    el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_EPSILON
  );
}

/**
 * §9 gate 2. In dev, compare the token the virtualizer is doing arithmetic
 * with against a row the browser actually laid out, and say so loudly if they
 * disagree — a silent mismatch shows up as rows that drift further out of
 * place the further you scroll, which reads as "the app is buggy" rather than
 * "the row height token moved".
 */
export function useRowHeightAssertion(
  rowRef: React.RefObject<HTMLElement | null>,
  rowH: number,
  ready: boolean,
): void {
  const reported = useRef<number | null>(null);
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    if (!ready) return;
    const el = rowRef.current;
    if (!el) return;
    const measured = el.offsetHeight;
    if (measured === 0 || Math.abs(measured - rowH) < 0.5) return;
    if (reported.current === measured) return;
    reported.current = measured;
    console.error(
      `[kavka] --row-h says ${rowH}px but a rendered row measured ${measured}px. ` +
        "The virtualizer places every row from the token, so the two must agree — " +
        "see docs/DESIGN.md §9 gate 2.",
    );
  }, [rowRef, rowH, ready]);
}
