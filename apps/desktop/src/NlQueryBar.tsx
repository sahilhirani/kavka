import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  nlGrammar,
  nlToQuery,
  type NlMode,
  type NlTranslation,
} from "./api";
import HelpPopover from "./HelpPopover";
import { hasSchemaHint, schemaHint } from "./nl";

/**
 * PLAIN ENGLISH → THE EDITOR. Never → the cluster.
 *
 * This is the most easily over-sold control in the product, so three rules run
 * it and all three are honesty rules.
 *
 * 1. IT IS A GRAMMAR, NOT AN LLM, AND IT SAYS SO ON SCREEN. `nl_to_query` is a
 *    pure function in the core over a curated pattern list: no network, no
 *    model, no memory between calls, and the same words always produce the same
 *    query. The note under the bar states that in one sentence and points at
 *    the MCP server, which is the road to real AI assistance — because the
 *    honest answer to "can it understand anything?" is "no, and here is the
 *    thing that can".
 *
 * 2. IT FILLS THE EDITOR AND NEVER RUNS ANYTHING. Translate writes the query
 *    into the CEL box or the SQL box and stops. The user reads it and presses
 *    the button they were always going to press — which is the whole safety
 *    model of a translator that can misread a sentence.
 *
 * 3. WHAT IT DIDN'T UNDERSTAND IS ON SCREEN, NOT IN A LOG. `unrecognized`
 *    renders as chips, one per phrase the grammar dropped, and a `low`
 *    confidence paints the block in the caution style. A translator that
 *    silently ignores half a sentence and answers confidently is exactly how
 *    someone ends up trusting a filter that matches the wrong records.
 */

interface NlQueryBarProps {
  mode: NlMode;
  /** For the schema hint: which topic's field names the grammar may use. */
  profileId: string;
  topic: string;
  /** Unique per host view, so two bars on one screen never share an id. */
  idPrefix: string;
  /** Fill the editor with the translated query. Never runs it. */
  onFill: (query: string) => void;
  /** Set while the host is busy — a translation nobody can use yet. */
  disabled?: boolean;
  disabledReason?: string;
}

const PLACEHOLDER: Record<NlMode, string> = {
  cel: "orders over 100 that failed in the last hour",
  sql: "count failed orders by partition in the last hour",
};

const EDITOR_WORD: Record<NlMode, string> = {
  cel: "the CEL box",
  sql: "the query editor",
};

export default function NlQueryBar({
  mode,
  profileId,
  topic,
  idPrefix,
  onFill,
  disabled = false,
  disabledReason,
}: NlQueryBarProps) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<NlTranslation | null>(null);
  /** A condition of THIS control, so it renders under it (§5.3). */
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const seq = useRef(0);
  /**
   * The grammar's own text, read from the core.
   *
   * `null` means "this build doesn't publish it", which is a state and not an
   * error: the command is not in the fixed Phase 5b contract (see `nlGrammar`
   * in api.ts), so a rejection leaves the popover with the summary below and
   * says where the full list lives. Nothing here is allowed to raise about it.
   */
  const [grammar, setGrammar] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    nlGrammar()
      .then((text) => {
        if (!cancelled) setGrammar(text);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const translate = useCallback(() => {
    const input = text.trim();
    if (input.length === 0) {
      setError(
        `Describe what you are looking for — “${PLACEHOLDER[mode]}”, say — and Kavka writes it into ${EDITOR_WORD[mode]}.`,
      );
      inputRef.current?.focus();
      return;
    }
    const mine = ++seq.current;
    setError(null);
    setBusy(true);
    nlToQuery(input, mode, schemaHint(profileId, topic))
      .then((next) => {
        if (seq.current !== mine) return;
        setResult(next);
        // Fills, never runs. The user still presses Search or Run.
        onFill(next.query);
      })
      .catch((err: unknown) => {
        if (seq.current !== mine) return;
        setResult(null);
        setError(errorMessage(err));
      })
      .finally(() => {
        if (seq.current === mine) setBusy(false);
      });
  }, [text, mode, profileId, topic, onFill]);

  const knowsFields = hasSchemaHint(profileId, topic);
  const errorId = `${idPrefix}-nl-error`;
  const hintId = `${idPrefix}-nl-hint`;

  return (
    <div className="nlbar">
      <div className="nlbar-row">
        <label className="seekbar-label" htmlFor={`${idPrefix}-nl`}>
          Plain English
        </label>
        <input
          id={`${idPrefix}-nl`}
          ref={inputRef}
          type="text"
          className={`nlbar-input${error !== null ? " input-invalid" : ""}`}
          value={text}
          placeholder={PLACEHOLDER[mode]}
          autoComplete="off"
          spellCheck={false}
          disabled={disabled}
          title={disabled ? disabledReason : undefined}
          aria-invalid={error !== null ? true : undefined}
          aria-describedby={
            error !== null ? `${errorId} ${hintId}` : hintId
          }
          onChange={(e) => {
            setText(e.target.value);
            // Editing a field clears its own message; nothing ever adds one
            // mid-keystroke (§5.3).
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            // ⏎ translates. It deliberately does NOT reach the host's "run on
            // Enter" handler: the one thing this control must never do is
            // start a scan the user has not read the query for.
            e.preventDefault();
            e.stopPropagation();
            if (!busy && !disabled) translate();
          }}
        />
        <button
          type="button"
          className="btn btn-swap"
          disabled={busy || disabled}
          aria-busy={busy || undefined}
          title={
            disabled
              ? disabledReason
              : busy
                ? "Kavka is writing the query"
                : `Write this into ${EDITOR_WORD[mode]}. It never runs on its own.`
          }
          onClick={translate}
        >
          <span className="btn-swap-face">
            Translate
          </span>
          <span className="btn-swap-face btn-swap-busy">
            <span className="spinner" aria-hidden="true" />
            Translate
          </span>
        </button>
        <HelpPopover
          className="btn btn-ghost"
          label="What it understands"
          buttonTitle="Every sentence this grammar accepts"
          title="Plain English → query"
        >
          {grammar === null ? (
            <p className="help-pop-lead">
              Kavka recognises field comparisons (
              <code>status is failed</code>, <code>orderId over 100</code>),
              times (<code>last hour</code>, <code>since yesterday</code>), the
              record's own address (<code>key is A-102</code>,{" "}
              <code>partition 3</code>, <code>offset over 5000</code>),{" "}
              <code>and</code>, <code>or</code> and negation. Anything else is
              listed back to you untouched. This build doesn't publish the full
              list; it lives with the translator in the app's core.
            </p>
          ) : (
            // Verbatim from the core, which keeps the text honest against the
            // implementation with a test. Rendered rather than paraphrased for
            // exactly that reason.
            <pre className="nlbar-grammar">{grammar}</pre>
          )}
        </HelpPopover>
      </div>

      {error !== null && (
        <span className="field-error" id={errorId}>
          {error}
        </span>
      )}

      {result !== null && (
        <div
          className={`nlbar-result${
            result.confidence === "low" ? " nlbar-result-low" : ""
          }`}
          role="status"
        >
          {/* Law 2: the caution state is a word and a glyph, never the border
              colour on its own. */}
          {result.confidence === "low" && (
            <p className="nlbar-caution">
              <span aria-hidden="true">! </span>
              Read this one before you run it — Kavka matched only part of what
              you wrote.
            </p>
          )}
          <p className="nlbar-explanation">{result.explanation}</p>
          {result.unrecognized.length > 0 && (
            <p className="nlbar-unknown">
              <span className="nlbar-unknown-label">Not understood:</span>
              {result.unrecognized.map((phrase, i) => (
                <span className="nlbar-chip" key={`${phrase}:${i}`}>
                  {phrase}
                </span>
              ))}
              <span className="nlbar-unknown-tail">
                {" "}
                — {result.unrecognized.length === 1 ? "it is" : "they are"} not
                in the query at all, so the results will be wider than the
                sentence suggests.
              </span>
            </p>
          )}
        </div>
      )}

      <p className="nlbar-note" id={hintId}>
        A fixed grammar, not an AI: Kavka matches a curated list of phrases —
        field comparisons, times, keys, partitions, offsets, <code>and</code>/
        <code>or</code> and negation — and writes the result into{" "}
        {EDITOR_WORD[mode]} for you to check. Nothing is sent anywhere and the
        same words always produce the same query. For questions this can't
        express, connect your AI assistant to Kavka's MCP server — the address
        is in About.
        {!knowsFields && (
          <>
            {" "}
            Kavka hasn't read any messages from <code>{topic}</code> in this
            window yet, so it doesn't know the payload's field names — browse or
            search once and field comparisons start working.
          </>
        )}
      </p>
    </div>
  );
}
