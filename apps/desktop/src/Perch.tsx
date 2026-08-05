import type { ReactNode } from "react";
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
 * THERE IS NO DISMISS CONTROL, and the mockup's "Hide" button is deliberately
 * not here: a verdict the user can switch off is a verdict the app stops
 * being accountable for. If a Perch is noise on some screen, that screen's
 * verdict is wrong — fix the sentence, not the visibility.
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

  return (
    // A region, not a status: this is standing content that happens to
    // change, and role="status" would make a screen reader announce every
    // screen's verdict on arrival, over the heading the user came for.
    <section
      className={`perch perch-${effective}`}
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
        {note !== undefined && note !== null && note !== "" && (
          <p className="perch-caveat">{note}</p>
        )}
      </div>
      {actions !== undefined && <div className="perch-actions">{actions}</div>}
    </section>
  );
}
