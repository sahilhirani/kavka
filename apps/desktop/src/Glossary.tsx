/**
 * Teaching without documentation.
 *
 * Every Kafka term that must appear as a label gets a dotted underline and a
 * popover on hover *and* focus. One sentence plus one concrete example.
 *
 * HARD CAP: 15 terms, one registry, one gloss per term per view. Without the
 * cap, each phase adds its own and the app becomes a tooltip farm. If a term
 * needs more than a sentence, the label is wrong — rewrite the label instead
 * of growing the gloss.
 *
 * See docs/DESIGN.md § "Teaching without documentation".
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export type GlossaryKey =
  | "offset"
  | "partition"
  | "lag"
  | "consumer-group"
  | "broker"
  | "bootstrap-server"
  | "replication-factor"
  | "internal-topic"
  | "retention"
  | "live-tail"
  | "tombstone"
  | "under-replicated"
  | "isr";

export const GLOSSARY: Record<GlossaryKey, string> = {
  offset:
    "A message's position in its partition, counting from 0. Offset 512 is the 513th message that partition ever received.",
  partition:
    "A topic is split into partitions so several consumers can read it at once. Order is guaranteed inside a partition, not across them.",
  lag: "How many messages a consumer group still has to read. Lag 0 means it's caught up. Lag that keeps rising means the application can't keep up.",
  "consumer-group":
    "A set of application instances sharing the work of reading a topic. Each partition goes to exactly one member.",
  broker: "One Kafka server. A cluster is several brokers sharing the work.",
  "bootstrap-server":
    "Any single broker in your cluster, written as host:port. Kavka connects to it once and discovers every other broker from there.",
  "replication-factor":
    "How many brokers keep a copy of each partition. 3 means the data survives losing two brokers.",
  "internal-topic":
    "A topic Kafka uses for its own bookkeeping, like __consumer_offsets. Usually safe to ignore.",
  retention:
    "How long Kafka keeps messages before deleting them. Older messages are gone, not hidden.",
  "live-tail":
    "Keeps the list scrolled to the newest message as it arrives. Pause to read without the view moving.",
  tombstone:
    'A record with a null value. On a compacted topic it means "this key is deleted."',
  "under-replicated":
    "A partition that doesn't have all its copies right now. Usually a broker is down or catching up.",
  isr: "The replicas currently in sync with the partition leader. Fewer than the replication factor means the partition is at risk.",
};

interface TermProps {
  /** Registry key — the gloss is looked up, never passed in at the call site. */
  name: GlossaryKey;
  /** Visible label. Defaults to the term's own wording at the call site. */
  children: React.ReactNode;
}

/** Popover box, and the gap it keeps from the term. Mirrors styles.css. */
const POP_W = 260;
const POP_GAP = 6;
/** Rough height cap used only to decide above-vs-below before paint. */
const POP_H = 96;
const EDGE = 8;

/**
 * A glossed Kafka term. Keyboard-reachable (tabIndex 0) and described by the
 * popover via aria-describedby, so the gloss reaches a screen reader without
 * ever becoming part of the term's accessible name.
 *
 * THE POPOVER IS PORTALLED TO <body> AND POSITIONED `fixed`.
 * As a positioned child of the term it was clipped by whatever happened to be
 * between them, and two of its three real call sites are inside a sticky
 * `<th>` inside `.table-scroll { overflow: auto }` — so the gloss was cropped
 * to a 28px header strip exactly where a novice needs it most. A portal has no
 * clipping ancestor, and `fixed` + getBoundingClientRect means the same code
 * works in a table header, a form label and a panel title without any of them
 * knowing about it.
 *
 * The node stays MOUNTED and merely hidden. An `aria-describedby` target that
 * only exists while the mouse is over the term is a description a screen
 * reader never resolves; hidden nodes are included in the accessible
 * description when they are referenced directly, so this is both correct and
 * invisible.
 */
export function Term({ name, children }: TermProps) {
  const id = `gloss-${name}`;
  const anchor = useRef<HTMLSpanElement | null>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number }>({
    top: -9999,
    left: -9999,
  });
  /**
   * SC 1.4.13 HOVERABLE: the pointer has to be able to reach the gloss
   * without it disappearing. It could not — `.term-pop` was
   * `pointer-events: none` and sits 6px away from the term, so moving
   * towards it left the term, fired `mouseleave` and closed the thing you
   * were moving towards. That is the one manoeuvre a magnifier user makes
   * constantly, because at 300% the gloss and its term are never both on
   * screen.
   *
   * The grace timer is what bridges the gap: `mouseleave` on the term fires
   * BEFORE `mouseenter` on the popover, so a close has to be schedulable and
   * cancellable rather than immediate. Focus and Esc still close outright —
   * neither of them is crossing a gap.
   */
  const closeTimer = useRef<number | null>(null);

  const place = useCallback(() => {
    const el = anchor.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // Prefer above; flip below when the term sits near the top of the window
    // (which is exactly where a sticky table header puts it).
    const above = r.top - POP_GAP - POP_H >= EDGE;
    const top = above ? r.top - POP_GAP - POP_H : r.bottom + POP_GAP;
    const left = Math.min(
      Math.max(EDGE, r.left),
      Math.max(EDGE, window.innerWidth - POP_W - EDGE),
    );
    setPos({ top, left });
  }, []);

  const cancelClose = useCallback(() => {
    if (closeTimer.current !== null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  }, []);

  const show = useCallback(() => {
    cancelClose();
    place();
    setOpen(true);
  }, [cancelClose, place]);

  /** Immediately — for blur and Esc, which never cross the gap. */
  const hide = useCallback(() => {
    cancelClose();
    setOpen(false);
  }, [cancelClose]);

  /** After the grace period — for the pointer, which does. */
  const hideSoon = useCallback(() => {
    cancelClose();
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null;
      setOpen(false);
    }, 120);
  }, [cancelClose]);

  // A pending close must not outlive the term it belongs to.
  useEffect(() => cancelClose, [cancelClose]);

  // A popover anchored to a viewport coordinate has to close (or move) when
  // that coordinate stops meaning anything. Capture phase, so a scroll inside
  // .table-scroll counts too.
  useEffect(() => {
    if (!open) return;
    const onScroll = () => place();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, place]);

  return (
    <span className="term-wrap">
      <span
        ref={anchor}
        className="term"
        tabIndex={0}
        aria-describedby={id}
        onMouseEnter={show}
        onMouseLeave={hideSoon}
        onFocus={show}
        onBlur={hide}
      >
        {children}
      </span>
      {createPortal(
        <span
          className={`term-pop${open ? " term-pop-open" : ""}`}
          id={id}
          role="tooltip"
          style={{ top: pos.top, left: pos.left }}
          // The other half of Hoverable: entering the gloss cancels the
          // close the term scheduled on the way out, and leaving it
          // schedules one again. It only takes pointer events while it is
          // open (see `.term-pop-open` in styles.css), so a hidden gloss can
          // never swallow a click on the row underneath it.
          onMouseEnter={cancelClose}
          onMouseLeave={hideSoon}
        >
          {GLOSSARY[name]}
        </span>,
        document.body,
      )}
    </span>
  );
}
