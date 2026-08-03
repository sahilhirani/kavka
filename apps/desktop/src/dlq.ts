/**
 * DEAD LETTERS — the UI's half, which is deliberately small.
 *
 * PARSING IS THE CORE'S JOB. `dlq_inspect` is a pure core function surfaced
 * through consume and the serde pipeline, and a record arrives here with
 * `dlq` already filled in or absent. Nothing in this file reads a header to
 * decide whether something is a dead letter — `record.dlq != null` is the
 * whole test, in every view, so the UI and the core can never disagree about
 * what counts.
 *
 * What IS the UI's job is the way back: re-producing a record to the topic it
 * originally failed on. That means stripping the framework's own bookkeeping
 * headers (they describe a failure that is about to be retried, and carrying
 * them into the replay would make the next failure's headers ambiguous) and
 * adding Kavka's own provenance, so the replayed record says where it came
 * from. Both are pure string work, and both live here rather than in a
 * component: they are the kind of rule that gets quietly forked the second
 * a second call site needs it.
 *
 * THE TWO CONVENTIONS, and the exact names, verified against the frameworks:
 *
 *  - Kafka Connect (`connect-runtime`, DeadLetterQueueReporter) writes
 *    `__connect.errors.topic`, `.partition`, `.offset`,
 *    `.exception.class.name`, `.exception.message`, `.exception.stacktrace`,
 *    plus `.connector.name`, `.task.id`, `.stage`, `.class.name` and
 *    `.timestamp` — every one of them under the same `__connect.errors.`
 *    prefix.
 *  - Spring for Apache Kafka (`KafkaHeaders`, the `DLT_*` constants) writes
 *    `kafka_dlt-original-topic`, `-original-partition`, `-original-offset`,
 *    `-original-timestamp`, `-original-timestamp-type`,
 *    `-original-consumer-group`, `-exception-fqcn`, `-exception-cause-fqcn`,
 *    `-exception-message`, `-exception-stacktrace` and the `-key-exception-*`
 *    trio — every one under `kafka_dlt-`.
 *
 * Stripping is BY PREFIX, not by that list. Both frameworks add headers
 * between releases, and a prefix rule keeps working when they do; an
 * enumeration silently starts letting the new ones through, which is how a
 * replayed record ends up carrying a stale stack trace nobody notices.
 */

import type { HeaderEntry, MessageRecord, ProduceHeaderInput } from "./api";

/** The header namespaces the two recognised conventions own. */
export const DLQ_HEADER_PREFIXES: readonly string[] = [
  "__connect.errors.",
  "kafka_dlt-",
];

/** The provenance Kavka adds to a record it replays out of a dead letter. */
export const DLQ_REPLAY_HEADERS: readonly string[] = [
  "kavka.dlq.replayed.from.cluster",
  "kavka.dlq.replayed.from.topic",
  "kavka.dlq.replayed.from.partition",
  "kavka.dlq.replayed.from.offset",
  "kavka.dlq.replayed.at",
];

/** True when a header belongs to a framework's dead-letter bookkeeping. */
export function isDlqHeader(name: string): boolean {
  return DLQ_HEADER_PREFIXES.some((prefix) => name.startsWith(prefix));
}

/** The sentence a read-only connection blocks a replay with. */
export const REPLAY_READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/**
 * A MASKED RECORD CANNOT BE REPLAYED, and this is why.
 *
 * Masking runs in the core, on the decoded record, before it crosses IPC — so
 * a masked payload in this window is `•••` and the original bytes are not here
 * to send. A replay of it would produce the REPLACEMENT to the topic the record
 * originally failed on: a real record, on a real topic, carrying Kavka's
 * redaction where the customer's data was, with `kavka.dlq.replayed.from.*`
 * headers claiming it came from somewhere. That is the one masking failure that
 * writes.
 *
 * So the action is disabled with the reason, rather than hidden or silently
 * degraded — DESIGN §5.8: a control that cannot act says what would make it
 * able to. The way out is the user's own rule, and the sentence names it.
 */
export const REPLAY_MASKED_WHY =
  "Re-producing is blocked: this record was masked on its way here — turn the rule off in the Masking settings and fetch again to re-produce the real bytes.";

/**
 * Why this record cannot be sent back to its original topic, or `undefined`
 * when it can.
 *
 * Read-only first: it is a property of the connection rather than of the
 * record, so it is the answer for every row on the screen and naming the
 * masking rule to somebody whose real obstacle is read-only would send them to
 * the wrong settings page.
 */
export function replayBlockedWhy(
  readOnly: boolean,
  record: MessageRecord,
): string | undefined {
  if (readOnly) return REPLAY_READ_ONLY_WHY;
  if (record.masked === true) return REPLAY_MASKED_WHY;
  return undefined;
}

/** How Kavka names a convention in prose. Unknown values pass through. */
export function conventionWord(convention: string): string {
  switch (convention) {
    case "connect":
      return "Kafka Connect";
    case "spring":
      return "Spring for Apache Kafka";
    case "none":
      return "no recognised convention";
    default:
      return convention;
  }
}

/**
 * Does this topic's NAME suggest it holds dead letters?
 *
 * Used for one thing only: teaching. A topic called `orders.dlq` where no
 * record carries recognised headers is a question the user is about to ask, so
 * the browser answers it unprompted. It is never used to decide whether
 * something IS a dead letter — that is `record.dlq`, and a naming convention is
 * not evidence about a record.
 */
export function looksLikeDlqTopic(topic: string): boolean {
  const t = topic.toLowerCase();
  return (
    t.includes("dlq") ||
    t.includes("dead") ||
    t.endsWith(".dlt") ||
    t.endsWith("-dlt")
  );
}

/**
 * The headers a replay carries: everything the original record had EXCEPT the
 * framework's dead-letter bookkeeping, plus Kavka's own provenance.
 *
 * TWO KINDS OF HEADER ARE DROPPED BESIDES THE BOOKKEEPING, and both because
 * produce cannot express them:
 *
 *  - A header with NO VALUE at all. `∅` in the inspector means the header
 *    carried no bytes; `ProduceHeaderInput` has only a string, so inventing
 *    `""` would change the record.
 *  - A BINARY header. `is_text: false` means `value` is Kavka's own hex
 *    RENDERING of bytes that were not text — `"00 ff 2a"` — and produce would
 *    send those eleven ASCII characters as the header's value. That is not the
 *    header, and it is not a near miss either: the replayed record would carry
 *    a different signature, a different big-endian integer, a different
 *    protobuf fragment, and nothing downstream would flag it. Dropping it loses
 *    a header; keeping it FABRICATES one, and that is the worse of the two.
 *
 * Both are counted into the sentence the inspector shows before the button —
 * see `droppedHeaderCount` and `droppedBinaryCount` — because a replay that
 * quietly loses a header the user could see on screen is the same lie either
 * way.
 */
export function replayHeaders(
  record: MessageRecord,
  clusterName: string,
  sourceTopic: string,
  now = Date.now(),
): ProduceHeaderInput[] {
  const kept = record.headers
    .filter(
      (h: HeaderEntry) => !isDlqHeader(h.key) && h.value !== null && h.is_text,
    )
    .map((h) => ({ key: h.key, value: h.value as string }));
  return kept.concat([
    { key: "kavka.dlq.replayed.from.cluster", value: clusterName },
    { key: "kavka.dlq.replayed.from.topic", value: sourceTopic },
    { key: "kavka.dlq.replayed.from.partition", value: String(record.partition) },
    { key: "kavka.dlq.replayed.from.offset", value: String(record.offset) },
    { key: "kavka.dlq.replayed.at", value: new Date(now).toISOString() },
  ]);
}

/**
 * How many of a record's headers the replay drops as FRAMEWORK BOOKKEEPING,
 * for the sentence.
 */
export function droppedHeaderCount(record: MessageRecord): number {
  return record.headers.filter((h) => isDlqHeader(h.key)).length;
}

/**
 * How many of a record's own headers the replay drops because they are BINARY
 * and produce writes text.
 *
 * Counted separately from the bookkeeping because they are a different fact
 * with a different remedy: the bookkeeping is dropped on purpose and is not
 * missed, while a binary header is a header the user can see in the inspector
 * that the replayed record will not have. Framework headers are excluded so no
 * header is counted twice — a binary `kafka_dlt-original-offset` is dropped by
 * the prefix rule first.
 */
export function droppedBinaryCount(record: MessageRecord): number {
  return record.headers.filter(
    (h) => !isDlqHeader(h.key) && h.value !== null && !h.is_text,
  ).length;
}
