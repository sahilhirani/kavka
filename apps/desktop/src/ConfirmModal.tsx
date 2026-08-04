import { useCallback, useRef, useState } from "react";
import Overlay from "./Overlay";

/**
 * The destructive confirmation from docs/DESIGN.md §5.8 / §7.
 *
 * Three rules it exists to enforce, so no call site has to remember them:
 *
 *  - The confirm button restates the verb (`Delete topic`), never "OK".
 *  - Type-to-confirm is ENVIRONMENT-gated, not action-gated: a PROTECTED
 *    environment always asks, an unprotected one never does (§6 layer 4).
 *    Friction where the stakes are, nowhere else.
 *  - Initial focus is Cancel, `Esc` cancels, and `⏎` does NOT confirm — a
 *    destructive modal must not be dismissable by the key a user was already
 *    pressing.
 *
 * The blast radius is the caller's `body`, because only the caller knows it
 * ("every message in it — about 4.2M records").
 */
export interface ConfirmModalProps {
  title: string;
  /** What is about to happen, and how much of it. One or two sentences. */
  body: React.ReactNode;
  /** The verb, restated. `Delete topic`, `Reset offsets`. Never "OK". */
  confirmLabel: string;
  /**
   * `destructive` (the default) paints the 2px `--danger-fill` wire across the
   * top edge and fills the confirm button red. `plain` is for the consents that
   * are NOT destructive — pausing a connector, applying a config — where the
   * red wire would be crying wolf: §5.5's guardrail rule says a filled red
   * button exists one click from a destructive action, so a modal that isn't
   * one must not wear it. Everything else (focus on Cancel, Esc, the restated
   * verb, type-to-confirm when protected) is identical, because the friction is what
   * the environment asked for, not what the tone is.
   */
  tone?: "destructive" | "plain";
  /**
   * When non-null the user must type this string exactly. Pass the topic name
   * when the connection's environment is PROTECTED, null everywhere else — the
   * gate is the environment, not the verb.
   */
  typeToConfirm?: string | null;
  /** Overrides the default "Type `x` to confirm" line when there is more to say. */
  typePrompt?: React.ReactNode;
  /** An extra warning above the actions — the active-group case, say. */
  extra?: React.ReactNode;
  busy?: boolean;
  busyLabel?: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export default function ConfirmModal({
  title,
  body,
  confirmLabel,
  tone = "destructive",
  typeToConfirm = null,
  typePrompt,
  extra,
  busy = false,
  busyLabel,
  onCancel,
  onConfirm,
}: ConfirmModalProps) {
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const [typed, setTyped] = useState("");

  const needsTyping = typeToConfirm !== null && typeToConfirm.length > 0;
  // Exactly, not case-insensitively: the point of the gate is that the user
  // reads the name they are about to destroy and reproduces it.
  const matches = !needsTyping || typed === typeToConfirm;

  const confirm = useCallback(() => {
    if (!matches || busy) return;
    onConfirm();
  }, [matches, busy, onConfirm]);

  const reason = busy
    ? (busyLabel ?? "Kavka is working on it")
    : !matches
      ? `Type ${typeToConfirm} exactly to confirm this`
      : undefined;

  return (
    <Overlay
      surfaceClass={`modal${tone === "destructive" ? " modal-destructive" : ""}`}
      labelledBy="confirm-title"
      initialFocus={cancelRef}
      onClose={onCancel}
    >
      <h2 className="modal-title" id="confirm-title">
        {title}
      </h2>
      <div className="modal-body">{body}</div>

      {needsTyping && (
        <div className="field confirm-type">
          <label className="field-label" htmlFor="confirm-type-input">
            {typePrompt ?? (
              <>
                Type <code>{typeToConfirm}</code> to confirm
              </>
            )}
          </label>
          <input
            id="confirm-type-input"
            type="text"
            className="input-mono"
            value={typed}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setTyped(e.target.value)}
          />
        </div>
      )}

      {extra}

      <div className="modal-actions">
        <button
          type="button"
          className="btn"
          ref={cancelRef}
          onClick={onCancel}
          disabled={busy}
          title={busy ? busyLabel : undefined}
        >
          Cancel
        </button>
        {/* A filled red button only ever exists one click from the action —
            and only when the action is actually destructive. */}
        <button
          type="button"
          className={`btn ${
            tone === "destructive" ? "btn-danger-confirm" : "btn-primary"
          }`}
          disabled={!matches || busy}
          aria-busy={busy || undefined}
          title={reason}
          onClick={confirm}
        >
          <span className="btn-busy-slot" aria-hidden="true">
            {busy ? <span className="spinner" /> : null}
          </span>
          {confirmLabel}
        </button>
      </div>
    </Overlay>
  );
}
