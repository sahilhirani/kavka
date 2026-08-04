import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

/**
 * A DISCLOSURE POPOVER — the Term gloss's bigger sibling.
 *
 * `Term` teaches one word in one sentence and opens on hover; this opens on a
 * click and holds a small block of examples (the CEL cheatsheet, the template
 * placeholders). Everything else is deliberately identical, because §5.10's
 * bug is the same bug: it is PORTALLED to <body> and positioned `fixed` from
 * the trigger's `getBoundingClientRect()`, so no clipping ancestor — a sticky
 * header, a scroll well, a modal with `overflow-y: auto` — can crop the one
 * component whose entire job is teaching.
 *
 * It re-places on scroll (capture phase, so scrolling a table counts) and on
 * resize, closes on `Esc` and on a click outside, and clamps to the viewport.
 *
 * Unlike the gloss it is CONDITIONALLY RENDERED, and that is correct here: it
 * is a disclosure wired with `aria-expanded`/`aria-controls`, not a
 * description. `aria-controls` may name nothing while it is closed; an
 * `aria-describedby` target that comes and goes is a description a screen
 * reader never resolves, which is why the gloss stays mounted and this does
 * not.
 */

const POP_GAP = 6;
const EDGE = 8;
/** Matches `.help-pop`'s width in styles.css. */
const POP_W = 380;
/** Rough height cap, used only to choose above-vs-below before paint. */
const POP_H = 260;

interface HelpPopoverProps {
  /** The trigger's label. `ƒx` is a glyph, so this is never only a glyph. */
  label: React.ReactNode;
  /** Accessible name when `label` is not prose on its own. */
  buttonTitle: string;
  className?: string;
  /** The popover's own heading, so the disclosure has an accessible name. */
  title: string;
  children: React.ReactNode;
}

export default function HelpPopover({
  label,
  buttonTitle,
  className,
  title,
  children,
}: HelpPopoverProps) {
  const id = useId();
  const anchor = useRef<HTMLButtonElement | null>(null);
  const pop = useRef<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number }>({
    top: -9999,
    left: -9999,
  });

  const place = useCallback(() => {
    const el = anchor.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const below = r.bottom + POP_GAP + POP_H <= window.innerHeight - EDGE;
    const top = below ? r.bottom + POP_GAP : Math.max(EDGE, r.top - POP_GAP - POP_H);
    const left = Math.min(
      Math.max(EDGE, r.left),
      Math.max(EDGE, window.innerWidth - POP_W - EDGE),
    );
    setPos({ top, left });
  }, []);

  useEffect(() => {
    if (!open) return;
    place();
    const onScroll = () => place();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Stop here: a modal underneath also treats Esc as "close", and one key
      // press must never do two things.
      e.stopPropagation();
      setOpen(false);
      anchor.current?.focus();
    };
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (target === null) return;
      if (anchor.current?.contains(target)) return;
      if (pop.current?.contains(target)) return;
      setOpen(false);
    };
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onDown, true);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onDown, true);
    };
  }, [open, place]);

  return (
    <>
      <button
        type="button"
        ref={anchor}
        className={className ?? "btn btn-ghost help-trigger"}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        title={buttonTitle}
        onClick={() => setOpen((prev) => !prev)}
      >
        {label}
      </button>
      {open &&
        createPortal(
          <div
            ref={pop}
            id={id}
            className="help-pop"
            role="dialog"
            aria-label={title}
            style={{ top: pos.top, left: pos.left }}
          >
            <p className="help-pop-title">{title}</p>
            {children}
          </div>,
          document.body,
        )}
    </>
  );
}
