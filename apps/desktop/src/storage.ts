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
