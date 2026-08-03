import { useEffect, useRef } from "react";

/**
 * The overlay primitive — scrim, focus trap, Esc, focus restoration.
 *
 * Three surfaces need exactly this behaviour (the command palette and the two
 * dialogs), and DESIGN.md §5.8 states it once for all of them: focus trap,
 * `Esc` always cancels, and the user is put back where they were. Written once
 * here rather than three times, because a focus trap that is subtly different
 * in one of three places is the kind of bug nobody finds by looking.
 *
 * It is NOT portalled to <body>. The palette "inherits the warm substrate" on
 * prod (§5.9) and a danger banner dampens the env rule (§5.8) — both are
 * `[data-env]` / `[data-alert]` cascades from `.app`, so an overlay that
 * escaped that subtree would quietly lose the prod guardrail. `position:
 * fixed` on `.scrim` already takes it out of the layout; nothing between here
 * and `.app` creates a containing block.
 */

/**
 * Everything a user can Tab to. `[tabindex="-1"]` is deliberately excluded:
 * it means "focusable by script, not by Tab", which is exactly what the
 * wrap-around below must not land on.
 */
const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "textarea:not([disabled])",
  "select:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(", ");

interface OverlayProps {
  /** Class for the raised surface: `modal`, `modal modal-wide`, `palette`. */
  surfaceClass: string;
  /** Accessible name, when there is no visible title element to point at. */
  label?: string;
  /** id of the visible title element. Preferred over `label`. */
  labelledBy?: string;
  /**
   * What to focus when the overlay opens. Falls back to the first focusable
   * control, so an overlay that forgets to pass one is still keyboard-usable.
   */
  initialFocus?: React.RefObject<HTMLElement | null>;
  onClose: () => void;
  children: React.ReactNode;
}

export default function Overlay({
  surfaceClass,
  label,
  labelledBy,
  initialFocus,
  onClose,
  children,
}: OverlayProps) {
  const surface = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    // Whatever had focus when we opened. Captured on mount rather than passed
    // in, so no caller can forget to do it.
    const invoker = document.activeElement as HTMLElement | null;
    const target =
      initialFocus?.current ??
      surface.current?.querySelector<HTMLElement>(FOCUSABLE) ??
      null;
    target?.focus();
    return () => {
      // Put the user back — but only if nothing else has claimed focus. The
      // command an overlay runs on its way out often focuses something on
      // purpose (⌘K → "Add connection" → the autoFocus name field), and this
      // cleanup runs after that: restoring unconditionally yanks the caret
      // back to the invoker and the user types into nothing.
      const active = document.activeElement;
      if (active !== null && active !== document.body) return;
      // `isConnected` guards the case where the invoker was itself removed
      // while we were open (a deleted profile row, say) — focus then stays on
      // the body, which is still better than throwing.
      if (invoker?.isConnected) invoker.focus();
    };
  }, [initialFocus]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      // Stop here. The profile editor underneath also treats Esc as "undo
      // edits", and one key press must never do two things.
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const nodes = Array.from(
      surface.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [],
    );
    if (nodes.length === 0) {
      e.preventDefault();
      return;
    }
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    const active = document.activeElement;
    if (e.shiftKey ? active === first : active === last) {
      e.preventDefault();
      (e.shiftKey ? last : first).focus();
    }
  };

  return (
    <div
      className="scrim"
      // mousedown, not click: a click that STARTED inside the surface and
      // ended on the scrim (a drag across a text selection) must not close.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={surface}
        className={surfaceClass}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        aria-labelledby={labelledBy}
        onKeyDown={handleKeyDown}
      >
        {children}
      </div>
    </div>
  );
}
