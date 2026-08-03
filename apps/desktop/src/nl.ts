import type { MessageRecord, NlSchemaHint } from "./api";

/**
 * THE SCHEMA HINT — top-level JSON field names Kavka has actually SEEN.
 *
 * `nl_to_query` is a grammar, and the one thing a grammar cannot do is guess
 * what a payload's fields are called. "orderId over 100" only becomes
 * `value.orderId > 100` if something can say that `orderId` is a field on this
 * topic; without that, the translator can only report it as a phrase it could
 * not place, which is honest and useless.
 *
 * So the hint is OBSERVED, never invented. Every view that receives records
 * hands them here, this module keeps the top-level keys per topic for the life
 * of the window, and the plain-English bar passes them down. That means:
 *
 *  - A topic nobody has looked at yet has NO hint, and the bar says so rather
 *    than pretending. Browsing or searching once fills it in.
 *  - The keys are the ones in the messages on screen. A field that only appears
 *    on records outside the range you read is not offered, because Kavka has
 *    genuinely never seen it.
 *  - Nothing is persisted. A field list cached across restarts would go stale
 *    exactly when a producer changes shape, which is the moment it matters.
 *
 * Pure and total: no React, no I/O, so the whole thing can be exercised by
 * calling it with records.
 */

/** How many field names one topic keeps. Beyond this, the earliest stay. */
const MAX_FIELDS = 64;

/** How many records one call reads. A tail hands over thousands; the shape of
    a topic is legible from the first few, and this runs on every batch. */
const SCAN_RECORDS = 50;

const byTopic = new Map<string, string[]>();

function key(profileId: string, topic: string): string {
  // The topic goes last and is not escaped: a Kafka topic name cannot contain
  // a newline, so nothing this scheme builds is ambiguous.
  return `${profileId}\n${topic}`;
}

/**
 * Learn the top-level field names in a batch of decoded records.
 *
 * Only objects contribute: an array payload has no field names, and a string
 * one has nothing to offer a field comparison at all.
 */
export function noteJsonFields(
  profileId: string,
  topic: string,
  records: readonly MessageRecord[],
): void {
  if (records.length === 0) return;
  const k = key(profileId, topic);
  const known = byTopic.get(k) ?? [];
  const seen = new Set(known);
  let added = false;
  for (const record of records.slice(0, SCAN_RECORDS)) {
    const json = record.value?.json;
    if (json === null || json === undefined) continue;
    if (Array.isArray(json) || typeof json !== "object") continue;
    for (const field of Object.keys(json)) {
      if (seen.has(field) || seen.size >= MAX_FIELDS) continue;
      seen.add(field);
      known.push(field);
      added = true;
    }
  }
  if (added || !byTopic.has(k)) byTopic.set(k, known);
}

/** What the translator may assume about this topic. Empty is a real answer. */
export function schemaHint(profileId: string, topic: string): NlSchemaHint {
  return { json_fields: byTopic.get(key(profileId, topic)) ?? [] };
}

/** Has anything been learned about this topic yet? Drives the bar's hint. */
export function hasSchemaHint(profileId: string, topic: string): boolean {
  return (byTopic.get(key(profileId, topic)) ?? []).length > 0;
}
