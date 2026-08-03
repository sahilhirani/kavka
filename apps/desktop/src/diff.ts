/**
 * A line diff, in about eighty lines and with no dependency.
 *
 * docs/DESIGN.md §5.10: "One diff component serves three features — payload
 * diff, config diff-from-default, schema version diff." This is the arithmetic
 * half of it: pure, total, no React, no I/O, so the alignment can be checked by
 * calling it with two arrays and reading the rows back.
 *
 * The algorithm is the textbook LCS table walked forwards, which is O(n·m) in
 * both time and memory — fine for a schema (tens of lines) and deliberately
 * CAPPED rather than left to discover a 50 000-line payload at runtime. Above
 * the cap the caller shows both versions side by side and says why, which is
 * honest; a diff that freezes the window for four seconds is not.
 *
 * Rows are SIDE-BY-SIDE shaped: every row carries a left cell, a right cell,
 * or both. A run of removals immediately followed by a run of additions is
 * zipped into `change` rows so the two panes stay on the same line — without
 * that, a one-line edit pushes every following line out of alignment and the
 * eye has to do the diff the computer just did.
 */

export type DiffKind = "same" | "add" | "del" | "change";

export interface DiffRow {
  kind: DiffKind;
  /** The old text, or null when this row is a pure addition. */
  left: string | null;
  /** The new text, or null when this row is a pure removal. */
  right: string | null;
  /** 1-based line number in the old text, or null. */
  leftNo: number | null;
  /** 1-based line number in the new text, or null. */
  rightNo: number | null;
}

/**
 * The point past which Kavka stops diffing and says so. 600×600 is a 1.4 MB
 * table and about 360 000 comparisons — imperceptible. Ten times that is not.
 */
export const DIFF_LINE_CAP = 600;

/** Split for diffing: trailing newline is not a line, `\r\n` is one break. */
export function toLines(text: string): string[] {
  const normalized = text.replace(/\r\n?/g, "\n");
  const lines = normalized.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** True when either side is past the cap — the caller must not call diffLines. */
export function tooBigToDiff(a: string[], b: string[]): boolean {
  return a.length > DIFF_LINE_CAP || b.length > DIFF_LINE_CAP;
}

/** How many rows actually differ. Used for the one-line summary above a diff. */
export function countChanges(rows: DiffRow[]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const row of rows) {
    if (row.right !== null && row.kind !== "same") added += 1;
    if (row.left !== null && row.kind !== "same") removed += 1;
  }
  return { added, removed };
}

type Op = { kind: "same" | "add" | "del"; text: string };

export function diffLines(a: string[], b: string[]): DiffRow[] {
  const ops = script(a, b);

  const rows: DiffRow[] = [];
  let leftNo = 0;
  let rightNo = 0;
  // Pending runs. They are flushed when the run ends, not as they arrive,
  // because a removal only becomes half of a `change` once we know whether an
  // addition follows it.
  let dels: string[] = [];
  let adds: string[] = [];

  const flush = () => {
    const paired = Math.min(dels.length, adds.length);
    for (let i = 0; i < paired; i += 1) {
      leftNo += 1;
      rightNo += 1;
      rows.push({
        kind: "change",
        left: dels[i],
        right: adds[i],
        leftNo,
        rightNo,
      });
    }
    for (let i = paired; i < dels.length; i += 1) {
      leftNo += 1;
      rows.push({ kind: "del", left: dels[i], right: null, leftNo, rightNo: null });
    }
    for (let i = paired; i < adds.length; i += 1) {
      rightNo += 1;
      rows.push({ kind: "add", left: null, right: adds[i], leftNo: null, rightNo });
    }
    dels = [];
    adds = [];
  };

  for (const op of ops) {
    if (op.kind === "del") {
      dels.push(op.text);
      continue;
    }
    if (op.kind === "add") {
      adds.push(op.text);
      continue;
    }
    flush();
    leftNo += 1;
    rightNo += 1;
    rows.push({
      kind: "same",
      left: op.text,
      right: op.text,
      leftNo,
      rightNo,
    });
  }
  flush();
  return rows;
}

/** The LCS edit script: same / del / add, in order. */
function script(a: string[], b: string[]): Op[] {
  const n = a.length;
  const m = b.length;
  if (n === 0) return b.map((text) => ({ kind: "add" as const, text }));
  if (m === 0) return a.map((text) => ({ kind: "del" as const, text }));

  const width = m + 1;
  // table[i][j] = length of the LCS of a[i..] and b[j..]. Filled backwards so
  // the walk below can go forwards, which is what keeps the ops in order.
  const table = new Uint32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i * width + j] =
        a[i] === b[j]
          ? table[(i + 1) * width + (j + 1)] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + (j + 1)]);
    }
  }

  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      ops.push({ kind: "same", text: a[i] });
      i += 1;
      j += 1;
    } else if (table[(i + 1) * width + j] >= table[i * width + (j + 1)]) {
      // Removals lead, so a replaced line reads old-then-new — which is what
      // the `change` pairing above depends on.
      ops.push({ kind: "del", text: a[i] });
      i += 1;
    } else {
      ops.push({ kind: "add", text: b[j] });
      j += 1;
    }
  }
  while (i < n) {
    ops.push({ kind: "del", text: a[i] });
    i += 1;
  }
  while (j < m) {
    ops.push({ kind: "add", text: b[j] });
    j += 1;
  }
  return ops;
}
