import { useSyncExternalStore } from "react";
import { maskingList, type MaskRule, type MessageRecord } from "./api";

/**
 * WHAT THIS WINDOW KNOWS ABOUT MASKING, and why it is a store rather than props.
 *
 * Masking is applied in the shell, on the decoded record, BEFORE it crosses IPC
 * — so by the time any component sees a payload the decision has already been
 * made and the raw bytes are not in the webview at all. Nothing here can mask
 * or unmask anything; what this module holds is the two facts every surface has
 * to be able to state honestly:
 *
 *  - HOW MANY RULES ARE IN FORCE on a connection, so the status bar can say
 *    "Masking on — 3 rules" without every view fetching it for itself.
 *  - WHETHER THIS SESSION HAS ACTUALLY SEEN MASKED DATA, which is a different
 *    fact: a rule can be on and match nothing in the range you are reading, and
 *    telling someone their payload was rewritten when it wasn't is the same
 *    class of lie as the reverse.
 *
 * It is a module store and not a React context because its readers are three
 * levels apart and never in the same subtree: the app status bar, the two
 * message views' status lines, and the settings panel that writes to it. A
 * context would mean threading a provider through the shell for a chip.
 *
 * Everything degrades to "no masking": a failed read leaves the count at zero
 * and says nothing, because a chip that appears because a fetch failed is worse
 * than no chip. The PANEL is where a failure to read the rules is surfaced —
 * that is the screen with somewhere to put it.
 */

export interface MaskingState {
  /** Rules with `enabled: true`, as of the last read. */
  enabled: number;
  /** Rules stored on this connection at all, enabled or not. */
  total: number;
  /** True once a record the core actually masked has arrived in this window. */
  sawMasked: boolean;
  /** False until the rules have been read once — "unknown", not "none". */
  known: boolean;
}

const UNKNOWN: MaskingState = {
  enabled: 0,
  total: 0,
  sawMasked: false,
  known: false,
};

/**
 * One frozen state per profile. Frozen because `useSyncExternalStore` compares
 * snapshots by identity: returning a fresh object per read would re-render
 * every subscriber on every unrelated store change, forever.
 */
const states = new Map<string, MaskingState>();
const listeners = new Set<() => void>();
/** Profiles whose rules have been asked for, so a chip never storms the shell. */
const inFlight = new Set<string>();

function emit(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function read(profileId: string | null): MaskingState {
  if (profileId === null) return UNKNOWN;
  return states.get(profileId) ?? UNKNOWN;
}

/** Replace a profile's state, but only when something actually changed. */
function put(profileId: string, next: MaskingState): void {
  const prev = read(profileId);
  if (
    prev.enabled === next.enabled &&
    prev.total === next.total &&
    prev.sawMasked === next.sawMasked &&
    prev.known === next.known
  )
    return;
  states.set(profileId, Object.freeze(next));
  emit();
}

/** Every rule this connection holds, as the panel just read them. */
export function noteMaskRules(profileId: string, rules: MaskRule[]): void {
  const prev = read(profileId);
  put(profileId, {
    enabled: rules.filter((rule) => rule.enabled).length,
    total: rules.length,
    // A session that has seen masked data has seen it. Turning the last rule
    // off does not un-mask the rows already on screen, and the export note has
    // to keep saying so for as long as they are there.
    sawMasked: prev.sawMasked,
    known: true,
  });
}

/**
 * Record that a batch of records arrived, and whether any of them were masked.
 *
 * Called by every view that receives records. It is deliberately one-way: the
 * flag latches on for the session, because the rows it is about stay on screen
 * (and stay exportable) after the batch that set it.
 */
export function noteMaskedRecords(
  profileId: string,
  records: readonly MessageRecord[],
): void {
  if (!hasMasked(records)) return;
  const prev = read(profileId);
  if (prev.sawMasked) return;
  put(profileId, { ...prev, sawMasked: true });
}

/** Did the core mask anything in this batch? The whole test is the flag. */
export function hasMasked(records: readonly MessageRecord[]): boolean {
  return records.some((record) => record.masked === true);
}

/**
 * Read this connection's rules once, so a chip can exist before anybody opens
 * the Masking panel.
 *
 * Deduplicated per profile and per window: the status bar mounts once, the two
 * message views mount and unmount constantly, and none of them should be able
 * to turn a chip into a poll. `force` is what the panel passes after a write —
 * the one caller that knows the answer has changed.
 */
export function ensureMaskRules(profileId: string, force = false): void {
  if (!force && (states.get(profileId)?.known === true || inFlight.has(profileId)))
    return;
  inFlight.add(profileId);
  maskingList(profileId)
    .then((rules) => noteMaskRules(profileId, rules))
    .catch(() => {
      // Silent BY DESIGN. This is the read behind a chip; the panel does its
      // own read and has a banner to put a failure in. A chip that appeared
      // because a command is missing would be a claim about someone's data.
      const prev = read(profileId);
      if (!prev.known) put(profileId, { ...prev, known: true });
    })
    .finally(() => {
      inFlight.delete(profileId);
    });
}

/** Forget a connection's state — it was deleted, or its rules were purged. */
export function forgetMasking(profileId: string): void {
  if (!states.delete(profileId)) return;
  emit();
}

/** Subscribe to one connection's masking state. `null` is always "unknown". */
export function useMasking(profileId: string | null): MaskingState {
  return useSyncExternalStore(
    subscribe,
    () => read(profileId),
    () => UNKNOWN,
  );
}

/**
 * The chip's words. One sentence, always with a number in it — never a bare
 * coloured mark, and never "on" with nothing to say how much.
 *
 * THIS STRING IS ALSO IN RUST, AND CHANGING IT IS A TWO-FILE EDIT.
 * `kavka_core::masking::masking_status` writes the same sentence for callers on
 * that side, and `the_notice_and_the_status_line_are_fixed` in that module pins
 * it character for character — so an edit here that is not made there produces
 * two spellings of one piece of session state, and an edit THERE that is not
 * made here fails that test rather than this UI.
 *
 * It is duplicated rather than fetched over IPC on purpose: this renders on
 * every store change, in a chip that must never flicker or await, and one short
 * sentence pinned by a test on the other side is cheaper than a command, its
 * loading state and its failure state. The count in it is the only variable,
 * and it comes from `masking_list`, which the UI already reads.
 */
export function maskingChipLabel(state: MaskingState): string {
  return `Masking on — ${state.enabled} ${state.enabled === 1 ? "rule" : "rules"}`;
}

/** The chip's title: what it means, and where the switch is. */
export function maskingChipTitle(state: MaskingState): string {
  const seen = state.sawMasked
    ? "Records on screen have been rewritten by it. "
    : "Nothing on screen has matched a rule yet. ";
  return (
    `${state.enabled} masking ${state.enabled === 1 ? "rule is" : "rules are"} in force on this connection. ` +
    `${seen}Kavka masks in the app's core, before the records reach this window, so exports and copies carry the masked text too. ` +
    `Turn a rule off in the Masking tab to see the real values.`
  );
}
