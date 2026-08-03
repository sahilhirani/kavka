/**
 * localStorage, defensively.
 *
 * Storage can be disabled or full, and both throw on access rather than
 * returning null. Nothing Kavka persists here is worth an exception in a
 * render path — every value is a convenience (which tab you were on, which
 * topic you were reading), so every helper degrades to "no preference".
 */

export function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function lsSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* best-effort */
  }
}

export function lsRemove(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    /* best-effort */
  }
}

// ---------------------------------------------------------------------------
// Saved search filters — per profile, per topic
// ---------------------------------------------------------------------------

/**
 * A named query the user parked above the search bar.
 *
 * Deliberately just the QUERY, not the seek scope: "orders that failed" is a
 * question about content and stays true tomorrow, while "the last 500 messages
 * from 14:02" is a place, and a chip that silently restored yesterday's time
 * window would answer a different question from the one it is named after.
 */
export interface SavedFilter {
  /** Stable across renames so React keys and deletes never cross rows. */
  id: string;
  name: string;
  /** Raw-byte substring prefilter. Empty = not part of this filter. */
  substring: string;
  /** CEL program. Empty = not part of this filter. */
  cel: string;
}

/**
 * MIGRATION NOTE — the `.v1.` segment is the schema version, and it is in the
 * KEY on purpose.
 *
 * When the saved-filter shape changes (a seek scope, a column set, a colour),
 * bump this to `v2` and write a converter that reads the v1 key and writes the
 * v2 one. Do NOT reshape v1 records in place: two Kavka windows can be open on
 * the same machine, and an older build that still reads `v1` must keep finding
 * exactly what it wrote. Anything unreadable degrades to "no saved filters" —
 * `readFilters` below never throws, because a corrupt preference must not take
 * a view down.
 */
const FILTERS_VERSION = "v1";

function filtersKey(profileId: string, topic: string): string {
  // The topic goes last and is not escaped: localStorage keys are opaque
  // strings and a topic name cannot contain a newline, so no separator this
  // scheme uses can collide.
  return `kavka.filters.${FILTERS_VERSION}.${profileId}.${topic}`;
}

function isFilter(value: unknown): value is SavedFilter {
  if (typeof value !== "object" || value === null) return false;
  const f = value as Partial<Record<keyof SavedFilter, unknown>>;
  return (
    typeof f.id === "string" &&
    typeof f.name === "string" &&
    typeof f.substring === "string" &&
    typeof f.cel === "string"
  );
}

/** Every saved filter for one topic on one connection. Never throws. */
export function readFilters(profileId: string, topic: string): SavedFilter[] {
  const raw = lsGet(filtersKey(profileId, topic));
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isFilter);
  } catch {
    return [];
  }
}

export function writeFilters(
  profileId: string,
  topic: string,
  filters: SavedFilter[],
): void {
  if (filters.length === 0) {
    lsRemove(filtersKey(profileId, topic));
    return;
  }
  lsSet(filtersKey(profileId, topic), JSON.stringify(filters));
}
