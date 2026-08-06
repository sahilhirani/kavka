import { useCallback, useEffect, useRef, useState } from "react";
import { useI18n } from "./i18n";

/**
 * TOASTS — docs/DESIGN.md §5.8's three-surface rule.
 *
 *   Toast  = something you did, finished.
 *   Banner = a condition you are in.
 *   Modal  = something irreversible needs your consent.
 *
 * AN ERROR THE USER MUST ACT ON IS NEVER A TOAST. Phase 2 raises exactly three
 * kinds: "Exported 412 messages", "Sent to partition 3 · offset 8 412", and the
 * read-only refusal — which is `role="alert"` and never auto-dismisses, because
 * nothing was written and the user has to decide what to do about that.
 *
 * Copy is the past tense of the button that caused it (§5.8), so a toast is
 * always readable as the answer to the click that produced it.
 */

export type ToastKind = "ok" | "warn" | "danger" | "info";

/**
 * The one thing a toast can offer besides going away.
 *
 * A FIRING ALERT IS THE CASE THIS EXISTS FOR. "checkout falling behind" tells
 * you what happened and leaves you to find it; the second button takes you to
 * the thing the rule is watching — the group's own detail for a lag rule, the
 * Alerts screen for a cluster-wide one. See `alertNav.ts`, which decides the
 * label and the destination together so the two can never disagree.
 *
 * It sits BEFORE Dismiss in the DOM, which is both the reading order and the
 * tab order: the action you might take, then the one that ends the toast.
 */
export interface ToastAction {
  label: string;
  run: () => void;
  /**
   * Carry the accent. For an action that is the point of the toast rather than
   * a convenience — a firing alert's "View group orders-service". Off by
   * default, because a toast full of primary buttons has none.
   */
  primary?: boolean;
}

export interface ToastSpec {
  kind: ToastKind;
  /** Line 1 — the past tense of the button. */
  title: string;
  /** Line 2 — the detail worth keeping: a path, an address, a count. */
  detail?: string;
  /** Mono-renders `detail`: a file path and an offset are literals. */
  mono?: boolean;
  action?: ToastAction;
}

interface Toast extends ToastSpec {
  id: number;
}

/** Success auto-dismisses at 5s; errors and warnings never do. */
const DISMISS_MS = 5000;

/** Beyond this the stack shows "+N more" rather than papering the corner. */
const MAX_VISIBLE = 3;

let nextToastId = 1;

export interface Toaster {
  toasts: Toast[];
  push: (spec: ToastSpec) => void;
  dismiss: (id: number) => void;
}

export function useToasts(): Toaster {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const push = useCallback((spec: ToastSpec) => {
    setToasts((prev) => [...prev, { ...spec, id: nextToastId++ }]);
  }, []);
  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);
  return { toasts, push, dismiss };
}

export function ToastStack({ toasts, dismiss }: Toaster) {
  if (toasts.length === 0) return null;
  const visible = toasts.slice(0, MAX_VISIBLE);
  const hidden = toasts.length - visible.length;
  return (
    <div className="toast-stack">
      {visible.map((toast) => (
        <ToastRow key={toast.id} toast={toast} onDismiss={dismiss} />
      ))}
      {hidden > 0 && (
        <p className="toast-more" role="status">
          +{hidden} more
        </p>
      )}
    </div>
  );
}

function ToastRow({
  toast,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: (id: number) => void;
}) {
  const { t } = useI18n();
  const transient = toast.kind === "ok" || toast.kind === "info";
  const [paused, setPaused] = useState(false);
  const timer = useRef<number | null>(null);

  // Pause on hover AND focus: a toast that vanishes while someone is reading
  // it — or tabbing to its action — is a toast that was never shown.
  useEffect(() => {
    if (!transient || paused) return;
    timer.current = window.setTimeout(() => onDismiss(toast.id), DISMISS_MS);
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
    };
  }, [transient, paused, toast.id, onDismiss]);

  return (
    <div
      className={`toast toast-${toast.kind}`}
      // A success is announced politely; anything that needs a decision
      // interrupts.
      role={toast.kind === "danger" ? "alert" : "status"}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <p className="toast-title">{toast.title}</p>
      {toast.detail !== undefined && (
        <p className={`toast-detail${toast.mono ? " toast-detail-mono" : ""}`}>
          {toast.detail}
        </p>
      )}
      <div className="toast-actions">
        {toast.action && (
          <button
            type="button"
            className={`btn ${toast.action.primary === true ? "btn-primary" : "btn-ghost"}`}
            onClick={() => {
              toast.action?.run();
              // Acting on a toast is also an answer to it. Leaving it up after
              // the user has gone where it pointed would mean the corner of
              // the screen still asking about something they are now looking
              // at.
              onDismiss(toast.id);
            }}
          >
            {toast.action.label}
          </button>
        )}
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => onDismiss(toast.id)}
        >
          {t("common.dismiss")}
        </button>
      </div>
      {/* The 1px progress hairline — only where there is progress to show.
          It pauses with the timer, so the two never disagree. */}
      {transient && (
        <span
          className={`toast-progress${paused ? " toast-progress-paused" : ""}`}
          aria-hidden="true"
        />
      )}
    </div>
  );
}
