import { useEffect, useRef, useState, type ReactNode } from "react";
import { useAppearance, type PerchMode } from "./appearance";
import { classifyError, type ErrorContext } from "./errors";
import { useI18n, type MessageKey } from "./i18n";

/**
 * THE PERCH — Jackdaw's answer-first banner.
 *
 * Every screen opens with one line of plain English derived from LIVE state:
 * what am I looking at, and what can Kafka actually tell me here. It is the
 * direction's signature element and it is where the honesty culture lives, so
 * the rules below are not stylistic.
 *
 * THE FOUR HONESTY RULES (docs/DESIGN.md §7 — non-negotiable)
 *
 * 1. WHILE LOADING, SAY SO. `loading` outranks everything: no tone, no
 *    verdict, no caveat. A banner that renders "Everything looks healthy"
 *    from an empty response is worse than one that renders nothing.
 * 2. ON ERROR, SPEAK THROUGH `errors.ts`. A raw string is never the verdict.
 *    The classification happens HERE rather than at each call site, so one
 *    screen cannot accidentally put a librdkafka sentence in the banner that
 *    the whole app is judged by.
 * 3. NEVER INVENT CERTAINTY. `tone="unknown"` exists precisely so a screen
 *    that cannot answer has somewhere to sit. Reach for it.
 * 4. NEVER BE CHEERFUL ABOUT DATA YOU DON'T HAVE. If the verdict rests on a
 *    partial page, a sampled window or numbers fetched a while ago, pass
 *    `caveat` — it renders beside the verdict, never behind a disclosure,
 *    because a qualification one click away is a qualification that gets
 *    quoted without it.
 *
 * LAW 2 SURVIVES. The tone paints a 4px edge, and the kicker spells the same
 * state in words. The edge is decoration; the sentence is the signal.
 *
 * THE HIDE CONTROL IS BACK, AND SO IS THE RULE IT LOOKED LIKE IT BROKE.
 *
 * This component used to refuse the mockup's "Hide" pill outright — a verdict
 * the user can switch off is a verdict the app stops being accountable for.
 * That argument is sound about a VERDICT and wrong about a NOTE, and this one
 * component is both: it absorbed the mockup's teaching note and its separate
 * `.verdict` card. So the refusal is replaced by a distinction, not dropped.
 *
 *   · What can be hidden is the standing note — the sentence that is the same
 *     on the fortieth visit as it was on the first.
 *   · What can NEVER be hidden is a screen that is still reading, or a screen
 *     whose read FAILED. Both force the full form back, in every mode, on
 *     every screen, and they take the Hide control away while they hold it.
 *     "Do not tell me the cluster is fine" has never meant "do not tell me the
 *     numbers are missing", and a preference that could mean the second one
 *     would be a preference for being lied to by omission.
 *
 * Two controls express that. `appearance.perch` (full / line / hidden) is the
 * durable answer, set in Settings; the pill on the Perch itself is the local
 * one, for this screen and this session, exactly as the mockup drew it —
 * press Hide and the note is replaced by a small "Show the note for this
 * screen" button, which is the mockup's own restore affordance and the reason
 * hiding here is never a dead end. The local answer outranks the preference
 * while it is set, and the forced states outrank both.
 *
 * A PREFERENCE OF "hidden" RENDERS NOTHING AT ALL — not even the restore
 * button. That asymmetry is the point: a user who pressed Hide one minute ago
 * needs the way back on screen, and a user who turned Perches off in Settings
 * last month does not need a button on all ten screens reminding them.
 *
 * ON A FAILED READ THIS AND `ErrorBanner` BOTH SAY `classifyError`'s TITLE, and
 * that repetition is deliberate. They are not the same object: the Perch
 * answers "what can this screen tell you" and the answer is nothing, while the
 * banner is the error itself — the unconditional raw-text escape hatch its own
 * comment insists on, and the Dismiss the Perch may not have. Collapsing them
 * costs one of those two things. If it is ever collapsed anyway, the honest
 * shape is a raw-details slot HERE, not a banner deleted there.
 */

export type PerchTone = "ok" | "watch" | "problem" | "unknown";

const STATE_KEY: Record<PerchTone, MessageKey> = {
  ok: "perch.state.ok",
  watch: "perch.state.watch",
  problem: "perch.state.problem",
  unknown: "perch.state.unknown",
};

export interface PerchProps {
  /**
   * Which screen this is, in the user's words — "Cluster home", "Topics".
   * Translated by the caller, because only the caller knows the key.
   */
  screen: string;
  /**
   * The verdict's shape. Ignored while `loading`, and forced to "problem"
   * when `error` is set: a screen cannot claim health it could not measure.
   */
  tone: PerchTone;
  /** The one-line verdict. Derived from live state, never from a constant. */
  children: ReactNode;
  /** Rule 1. True whenever the numbers behind `children` are still in flight. */
  loading?: boolean;
  /**
   * Rule 2. The RAW error string from the core; classified here. `null` and
   * `undefined` both mean "no error", so a caller can pass its state directly.
   */
  error?: string | null;
  /** Rule 4. What the verdict does not cover. */
  caveat?: ReactNode;
  /** Passed to `classifyError` so its wording can be cluster-aware. */
  errorContext?: ErrorContext;
  /** At most one or two. The Perch is a sentence, not a toolbar. */
  actions?: ReactNode;
}

/** The bird, sitting on its perch. Decorative — the kicker carries the state. */
function Bird() {
  return (
    <svg
      className="perch-bird"
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <path
        d="M20.5 5.4a4.6 4.6 0 0 0-4.6 4.6c0 1.3-.6 2.5-1.7 3.2L5.5 19h8.7c4.9 0 9-3.8 9.4-8.7l.1-1.2 3.8-1.7-3.6-1.1-.8-2.4-2.6 1.5z"
        fill="currentColor"
      />
      <circle cx="22.6" cy="8.3" r="1" fill="var(--perch-bg)" />
      <path
        d="M17.6 19v3.6M13.9 19v3.6"
        stroke="currentColor"
        strokeWidth="1.5"
        fill="none"
        strokeLinecap="round"
      />
      <path
        d="M4 23.6h24"
        stroke="currentColor"
        strokeWidth="1.5"
        opacity=".45"
        strokeLinecap="round"
      />
    </svg>
  );
}

export default function Perch({
  screen,
  tone,
  children,
  loading = false,
  error = null,
  caveat,
  errorContext,
  actions,
}: PerchProps) {
  const { t } = useI18n();
  const { appearance } = useAppearance();
  /**
   * This screen's own answer, for this session. `null` means "follow the
   * preference", which is what it goes back to when the user presses Show —
   * so a screen that was hidden by hand and a screen that was never touched
   * end up in the same state, and neither remembers a decision the user
   * expressed once about one screen.
   */
  const [local, setLocal] = useState<PerchMode | null>(null);

  /**
   * HIDE AND SHOW HAND FOCUS TO EACH OTHER.
   *
   * They are two different elements in two different branches: pressing Hide
   * unmounts the whole <section> and renders `.perch-restore` in its place, and
   * pressing Show does the reverse. React moves nothing, so focus falls to
   * <body> and the next Tab restarts at the top of a ten-screen app — whose
   * first stop is the rail, several hundred pixels and one whole navigation
   * away from the sentence you were reading. The Hide control was put back
   * because the owner asked for it to be reachable by keyboard; reachable and
   * then dropping you at the top of the document is half a control.
   *
   * `toggled` is what keeps this from stealing focus on MOUNT. Every screen
   * renders a Perch, so an effect that focused on every `local` change would
   * yank focus to the Hide pill on arrival at every one of them. It fires only
   * on a press, which is the only time the user's focus was on the button that
   * just stopped existing.
   */
  const hideRef = useRef<HTMLButtonElement>(null);
  const showRef = useRef<HTMLButtonElement>(null);
  const toggled = useRef(false);

  useEffect(() => {
    if (!toggled.current) return;
    toggled.current = false;
    // Exactly one of the two is mounted for any given `local`. If neither is —
    // the preference changed to `hidden` under the press, or the screen started
    // reading and `forced` took the pill away — this does nothing rather than
    // throwing focus somewhere arbitrary.
    (showRef.current ?? hideRef.current)?.focus();
  }, [local]);

  // Rule 1 outranks rule 2 outranks the caller. Resolved in one place so no
  // screen can get the precedence wrong.
  let effective: PerchTone;
  let stateWord: string;
  let verdict: ReactNode;
  let note: ReactNode = caveat;

  if (loading) {
    effective = "unknown";
    stateWord = t("perch.state.checking");
    verdict = t("perch.checking");
    // Whatever the caller wanted to qualify is about numbers that do not
    // exist yet. Dropping it is the honest move.
    note = undefined;
  } else if (error !== null && error !== "") {
    const classified = classifyError(error, errorContext);
    effective = "problem";
    stateWord = t("perch.state.problem");
    verdict = classified.title;
    // `detail` is the next click. It is empty only when there genuinely
    // isn't one, and an empty caveat renders nothing.
    note = classified.detail === "" ? undefined : classified.detail;
  } else {
    effective = tone;
    stateWord = t(STATE_KEY[tone]);
    verdict = children;
  }

  /**
   * THE VISIBILITY PRECEDENCE, in one expression and in this order:
   *
   *   1. loading or error  → full, and no Hide control. Non-negotiable.
   *   2. this screen's own Hide/Show, if the user pressed one.
   *   3. the stored preference.
   *
   * `forced` is deliberately the same condition the tone precedence above
   * uses. There is exactly one definition of "this screen cannot speak for
   * itself yet", and both halves read it.
   */
  const forced = loading || (error !== null && error !== "");
  const mode: PerchMode = forced ? "full" : (local ?? appearance.perch);

  if (mode === "hidden") {
    // Hidden by preference: nothing. Hidden by hand: the way back.
    if (local === null) return null;
    return (
      <button
        ref={showRef}
        type="button"
        className="btn btn-sm perch-restore"
        onClick={() => {
          toggled.current = true;
          setLocal(null);
        }}
      >
        {t("perch.show")}
      </button>
    );
  }

  const oneLine = mode === "line";

  return (
    // A region, not a status: this is standing content that happens to
    // change, and role="status" would make a screen reader announce every
    // screen's verdict on arrival, over the heading the user came for.
    <section
      className={`perch perch-${effective}${oneLine ? " perch-oneline" : ""}`}
      aria-label={t("perch.label", { screen })}
    >
      <Bird />
      <div className="perch-body">
        {/* Law 2: this is the WORD for the edge colour, and it names the
            screen so the verdict is never a floating claim. */}
        <p className="perch-kicker">
          {t("perch.kicker", { screen, state: stateWord })}
        </p>
        <p className="perch-verdict">{verdict}</p>
        {/* The caveat is the one thing one-line mode drops, and the only
            thing it is allowed to drop — which is why the control beside it
            reads "Show the whole note" rather than "Expand". Nothing is
            silently missing: the sentence that says so is one click away and
            the click is on screen. A caveat that came from `classifyError`
            never gets here, because an error is forced back to full. */}
        {!oneLine && note !== undefined && note !== null && note !== "" && (
          <p className="perch-caveat">{note}</p>
        )}
      </div>
      {/* Actions survive one-line mode. A deep link the verdict offered is
          the user's next click, not decoration on the note. */}
      {actions !== undefined && <div className="perch-actions">{actions}</div>}
      {/* No Hide while the screen is loading or broken — pressing it would
          have to do nothing, and a control that does nothing is worse than
          the one that is not there. */}
      {!forced && (
        <button
          ref={hideRef}
          type="button"
          className="perch-hide"
          onClick={() => {
            toggled.current = true;
            setLocal(oneLine ? "full" : "hidden");
          }}
        >
          {t(oneLine ? "perch.more" : "perch.hide")}
        </button>
      )}
    </section>
  );
}
