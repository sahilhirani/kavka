import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import type { JsonValue, SqlColumn } from "./api";
import { groupDigits } from "./format";
import {
  scrollIndexIntoView,
  useRowHeightAssertion,
  useVirtualRows,
} from "./virtual";

/**
 * THE VIRTUALIZED GRID FOR A SCHEMA NOBODY KNEW IN ADVANCE.
 *
 * MessageGrid's sibling, and deliberately not MessageGrid itself. The two share
 * everything that is hard — `virtual.ts`'s windowing, the `--row-h` assertion,
 * the `role="grid"` keyboard contract, the ledger gutter, the `∅` rendering —
 * and differ in the one thing neither can share: MessageGrid's five columns ARE
 * the message contract, and its gutter carries the record's offset because
 * every row has one. A SQL result set has whatever columns the query asked for
 * and may not contain an offset at all, so folding the two together would give
 * MessageGrid a mode in which none of its columns exist. Two components, one
 * virtualizer.
 *
 * Three things are worth reading before changing anything here.
 *
 * 1. THE GUTTER CARRIES THE ROW ORDINAL. §2's table assigns each view's gutter
 *    "the row's address in Kafka's own vocabulary", and a result row has no
 *    such address — it is the nth row of an answer. So this is the inspector's
 *    line-number treatment at table scale, and the ordinal is the row's place
 *    in the SORTED order the user is looking at, because that is the row they
 *    would count to.
 *
 * 2. SORTING IS CLIENT-SIDE AND SAYS SO. The rows on screen are the rows the
 *    query returned, capped or not; sorting reorders those and never asks the
 *    engine for more. A capped result sorted descending shows the largest of
 *    WHAT WAS READ, which is not the largest in the topic — the view above this
 *    one carries that sentence, and this component's job is to not quietly
 *    contradict it by looking authoritative.
 *
 * 3. NULL SORTS LAST IN BOTH DIRECTIONS. A null is an absent value, not a small
 *    one: putting it first on a descending sort would rank "we don't know"
 *    above every real answer. It is `∅` in the cell for the same reason.
 */

/** Which column, and which way. `null` = the engine's own row order. */
export interface SortState {
  index: number;
  dir: "asc" | "desc";
}

export interface ResultGridHandle {
  scrollToTop(): void;
  remeasure(): void;
}

interface ResultGridProps {
  columns: SqlColumn[];
  rows: JsonValue[][];
  /** Names the scrollport and the table's caption. */
  label: string;
  /** Namespaces row ids so two grids on one screen can't collide. */
  idPrefix: string;
  /** The 2px accent hairline under the header — never a spinner over data. */
  loading?: boolean;
  /** Shown instead of rows when there are none. The caller owns the sentence. */
  empty?: React.ReactNode;
}

/** DataFusion's numeric type names, lower-cased for the prefix test. */
const NUMERIC = [
  "int",
  "uint",
  "float",
  "double",
  "decimal",
  "bigint",
  "smallint",
  "tinyint",
  "numeric",
];

/**
 * Is this column a quantity rather than a literal?
 *
 * §4's law splits on where a value came from, and a result set is the one place
 * the UI cannot tell: `offset` selected straight through is an address you
 * could paste into a seek, and `count(*)` beside it is a number Kavka's engine
 * worked out. The declared type is the only signal available, so numbers read
 * as quantities — sans, tabular, grouped — and text reads as a literal. The
 * cost is that a bare `offset` column renders as a quantity; the alternative
 * was to special-case column NAMES, which would be wrong the moment a query
 * aliases one.
 */
export function isNumericType(dataType: string): boolean {
  const t = dataType.toLowerCase();
  return NUMERIC.some((n) => t.startsWith(n));
}

/** What goes in a cell, as text. Exported so the CSV path can agree with it. */
export function cellText(value: JsonValue): string {
  if (value === null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number")
    return Number.isInteger(value) ? groupDigits(value) : String(value);
  if (typeof value === "boolean") return value ? "true" : "false";
  // An array or object can only reach a cell through a JSON function that
  // returned structure; it is shown as compact JSON rather than [object].
  return JSON.stringify(value);
}

/** Nulls last, whatever the direction. See rule 3 at the top. */
function compare(a: JsonValue, b: JsonValue, numeric: boolean): number {
  const aNull = a === null || a === undefined;
  const bNull = b === null || b === undefined;
  if (aNull && bNull) return 0;
  if (aNull) return 1;
  if (bNull) return -1;
  if (numeric) {
    const an = typeof a === "number" ? a : Number(a);
    const bn = typeof b === "number" ? b : Number(b);
    if (Number.isFinite(an) && Number.isFinite(bn)) return an - bn;
  }
  return String(a).localeCompare(String(b));
}

const ResultGrid = forwardRef<ResultGridHandle, ResultGridProps>(
  function ResultGrid(
    { columns, rows, label, idPrefix, loading = false, empty },
    ref,
  ) {
    const scrollRef = useRef<HTMLDivElement | null>(null);
    const firstRowRef = useRef<HTMLTableRowElement | null>(null);
    const [sort, setSort] = useState<SortState | null>(null);
    const [selected, setSelected] = useState<number>(-1);

    const sorted = useMemo(() => {
      if (sort === null) return rows;
      const numeric = isNumericType(columns[sort.index]?.data_type ?? "");
      // A copy: the caller's array is the engine's answer and the order it
      // arrived in is a fact about the query, not about this table.
      const next = rows.slice();
      next.sort((ra, rb) => {
        const d = compare(ra[sort.index] ?? null, rb[sort.index] ?? null, numeric);
        return sort.dir === "asc" ? d : -d;
      });
      return next;
    }, [rows, sort, columns]);

    const { win, onScroll, remeasure } = useVirtualRows(
      scrollRef,
      sorted.length,
    );
    useRowHeightAssertion(firstRowRef, win.rowH, sorted.length > 0);

    useImperativeHandle(
      ref,
      () => ({
        scrollToTop() {
          const el = scrollRef.current;
          if (!el) return;
          el.scrollTop = 0;
          onScroll();
        },
        remeasure,
      }),
      [onScroll, remeasure],
    );

    const toggleSort = useCallback((index: number) => {
      setSelected(-1);
      setSort((prev) => {
        if (prev === null || prev.index !== index)
          return { index, dir: "asc" };
        if (prev.dir === "asc") return { index, dir: "desc" };
        // Third click returns the engine's own order, which is a real answer
        // for a query that carried its own ORDER BY.
        return null;
      });
    }, []);

    const moveTo = useCallback(
      (index: number) => {
        const clamped = Math.max(0, Math.min(sorted.length - 1, index));
        if (sorted.length === 0) return;
        setSelected(clamped);
        scrollIndexIntoView(scrollRef.current, clamped, win);
        // Re-window in the same commit: `aria-activedescendant` may only name a
        // row that is actually in the DOM.
        onScroll();
      },
      [sorted.length, win, onScroll],
    );

    const onKeyDown = useCallback(
      (e: React.KeyboardEvent<HTMLDivElement>) => {
        if (sorted.length === 0) return;
        switch (e.key) {
          case "ArrowDown":
          case "j":
            e.preventDefault();
            moveTo(selected < 0 ? win.start : selected + 1);
            break;
          case "ArrowUp":
          case "k":
            e.preventDefault();
            moveTo(selected < 0 ? win.start : selected - 1);
            break;
          case "Home":
            e.preventDefault();
            moveTo(0);
            break;
          case "End":
            e.preventDefault();
            moveTo(sorted.length - 1);
            break;
          case "Escape":
            if (selected >= 0) {
              e.preventDefault();
              setSelected(-1);
            }
            break;
          default:
            break;
        }
      },
      [sorted.length, selected, moveTo, win.start],
    );

    const span = columns.length + 1;
    const visible = sorted.slice(win.start, win.end);
    const activeDescendant =
      selected >= win.start && selected < win.end
        ? `${idPrefix}-row-${selected}`
        : undefined;

    return (
      <div className="table-wrap messages-table-wrap">
        {loading && <div className="table-loading" role="presentation" />}

        <div
          className="messages-scroll"
          ref={scrollRef}
          role="group"
          aria-label={label}
          tabIndex={0}
          aria-activedescendant={activeDescendant}
          onScroll={onScroll}
          onKeyDown={onKeyDown}
        >
          <table
            className="data-table result-table"
            role="grid"
            aria-rowcount={sorted.length + 1}
            aria-colcount={span}
          >
            <caption className="sr-only">{label}</caption>
            <thead>
              <tr aria-rowindex={1}>
                <th scope="col" className="ledger-gutter">
                  <span className="sr-only">Row</span>
                </th>
                {columns.map((col, i) => {
                  const active = sort?.index === i;
                  const dir = active ? sort.dir : null;
                  return (
                    <th
                      key={`${col.name}:${i}`}
                      scope="col"
                      className={isNumericType(col.data_type) ? "col-num" : ""}
                      aria-sort={
                        dir === "asc"
                          ? "ascending"
                          : dir === "desc"
                            ? "descending"
                            : "none"
                      }
                    >
                      {/* The caret's slot is reserved whether or not this is
                          the sorted column (§5.2), so sorting never reflows
                          the header. */}
                      <button
                        type="button"
                        className="th-sort"
                        title={`${col.name} · ${col.data_type} — sort ${
                          dir === "asc" ? "descending" : "by this column"
                        }. Sorting reorders the rows already returned; it does not ask the engine for more.`}
                        onClick={() => toggleSort(i)}
                      >
                        <span className="th-sort-name">{col.name}</span>
                        <span className="sort-caret" aria-hidden="true">
                          {dir === "asc" ? "▲" : dir === "desc" ? "▼" : ""}
                        </span>
                      </button>
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {win.padTop > 0 && (
                <tr aria-hidden="true" className="row-pad">
                  <td colSpan={span} style={{ height: win.padTop, padding: 0 }} />
                </tr>
              )}
              {visible.map((row, i) => {
                const index = win.start + i;
                return (
                  <tr
                    key={index}
                    id={`${idPrefix}-row-${index}`}
                    ref={i === 0 ? firstRowRef : undefined}
                    aria-rowindex={index + 2}
                    aria-selected={index === selected}
                    className={index === selected ? "row-selected" : ""}
                    onClick={() => setSelected(index)}
                  >
                    <td className="ledger-gutter">{groupDigits(index + 1)}</td>
                    {columns.map((col, c) => {
                      const value = row[c] ?? null;
                      const numeric = isNumericType(col.data_type);
                      return (
                        <td
                          key={`${col.name}:${c}`}
                          className={
                            numeric
                              ? "col-num cell-num"
                              : "cell-mono cell-preview"
                          }
                        >
                          {value === null ? (
                            <span
                              className="absent"
                              title={`${col.name} is null for this row.`}
                            >
                              ∅
                            </span>
                          ) : (
                            cellText(value)
                          )}
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
              {win.padBottom > 0 && (
                <tr aria-hidden="true" className="row-pad">
                  <td
                    colSpan={span}
                    style={{ height: win.padBottom, padding: 0 }}
                  />
                </tr>
              )}
            </tbody>
          </table>

          {sorted.length === 0 && empty != null && (
            <div className="messages-empty">{empty}</div>
          )}
        </div>
      </div>
    );
  },
);

export default ResultGrid;
