import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
} from "react";
import type { DlqMeta, MessageRecord } from "./api";
import { conventionWord } from "./dlq";
import { formatClock, groupDigits } from "./format";
import { Term } from "./Glossary";
import { previewText } from "./payload";
import {
  isPinnedToBottom,
  scrollIndexIntoView,
  useRowHeightAssertion,
  useVirtualRows,
} from "./virtual";

/**
 * THE VIRTUALIZED MESSAGE GRID.
 *
 * Extracted from MessagesView in Phase 2 because the search results table is
 * the same table: same columns, same ledger gutter, same keyboard contract,
 * same tombstone rendering. Two copies of a virtualized grid is two copies of
 * the `aria-rowindex` arithmetic, and the copy nobody is looking at is the one
 * that drifts.
 *
 * Two things are worth reading before changing anything here.
 *
 * 1. THE TABLE IS VIRTUALIZED, so it carries `role="grid"`, `aria-rowcount`
 *    (the TOTAL, including the header) and `aria-rowindex` on every row —
 *    docs/DESIGN.md §5.2 says all three land in the same change as the
 *    virtualizer and not before, because the rendered count no longer matches
 *    the real one. `role="grid"` is a promise of an interactive widget, so
 *    j/k, the arrows, Home/End and ⏎ actually walk and open rows.
 *
 * 2. FOLLOW MODE IS THE TAIL'S, AND IT LIVES HERE. The grid owns whether it is
 *    pinned to the bottom, because that is a fact about its scrollport, and it
 *    reports changes up so a live tail can count what arrived while the user
 *    was reading further back. A parent that tracked it separately would be
 *    reading a second, always slightly stale copy of one number.
 */

export function rowKey(record: MessageRecord): string {
  return `${record.partition}:${record.offset}`;
}

/**
 * `org.apache.kafka.connect.errors.DataException` → `DataException`.
 *
 * A fully-qualified Java class name is 60 characters of package and one word of
 * meaning, and the cell has room for the word. The full name is in the title
 * and in the inspector, so nothing is lost — only the part nobody reads is.
 */
export function shortClass(fqcn: string | null): string | null {
  if (fqcn === null) return null;
  const cut = fqcn.lastIndexOf(".");
  const short = cut >= 0 ? fqcn.slice(cut + 1) : fqcn;
  return short.length === 0 ? fqcn : short;
}

/** Everything the badge knows, for the hover. */
function dlqTitle(dlq: DlqMeta): string {
  const parts = [`Dead letter, ${conventionWord(dlq.convention)} convention.`];
  if (dlq.original_topic !== null) {
    const at =
      dlq.original_partition !== null && dlq.original_offset !== null
        ? ` at partition ${dlq.original_partition}, offset ${dlq.original_offset}`
        : "";
    parts.push(`It failed on ${dlq.original_topic}${at}.`);
  }
  if (dlq.exception_class !== null) parts.push(dlq.exception_class);
  if (dlq.exception_message !== null) parts.push(dlq.exception_message);
  return parts.join(" ");
}

/**
 * The DOM id `aria-activedescendant` points at.
 *
 * Keyed by the record's address rather than its row index: a live tail appends
 * rows and trims the head, so an index-keyed id would name a different record
 * from one frame to the next and the announced row would drift.
 */
function rowDomId(prefix: string, record: MessageRecord): string {
  return `${prefix}-row-${record.partition}-${record.offset}`;
}

export interface MessageGridHandle {
  /** A fetched range is read from its start. */
  scrollToTop(): void;
  /** Catch up with a stream, and re-pin to it. */
  scrollToNewest(): void;
  /** After a layout change the scrollport did not cause itself. */
  remeasure(): void;
}

interface MessageGridProps {
  records: MessageRecord[];
  /** Names the scrollport and the table's caption. */
  label: string;
  /** Namespaces row ids, so two grids on one screen can't collide. */
  idPrefix: string;
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
  /** Stay pinned to the newest row as records arrive (live tail, live search). */
  follow?: boolean;
  /** Fires only when the answer changes. */
  onPinnedChange?: (pinned: boolean) => void;
  /** The 2px accent hairline under the header — never a spinner over data. */
  loading?: boolean;
  /** Shown instead of rows when there are none. The caller owns the sentence. */
  empty?: React.ReactNode;
  /** Overlay children inside the wrap — the "N new messages" chip. */
  children?: React.ReactNode;
}

const MessageGrid = forwardRef<MessageGridHandle, MessageGridProps>(
  function MessageGrid(
    {
      records,
      label,
      idPrefix,
      selectedKey,
      onSelect,
      follow = false,
      onPinnedChange,
      loading = false,
      empty,
      children,
    },
    ref,
  ) {
    const scrollRef = useRef<HTMLDivElement | null>(null);
    const firstRowRef = useRef<HTMLTableRowElement | null>(null);
    const pinnedRef = useRef(true);
    const followRef = useRef(follow);
    followRef.current = follow;

    const { win, onScroll, remeasure } = useVirtualRows(
      scrollRef,
      records.length,
    );
    useRowHeightAssertion(firstRowRef, win.rowH, records.length > 0);

    /**
     * THE DLQ COLUMN APPEARS ONLY WHERE THERE IS ONE.
     *
     * A column of empty cells on every ordinary topic is a column that teaches
     * nothing and costs width on the payload preview, which is the loudest
     * thing in the table. `record.dlq` is populated by the core only when a
     * recognised convention matched (see api.ts), so the presence of the
     * column IS the answer to "is this a dead letter topic" — and the browser
     * says so in words when the answer is no but the topic's name suggests
     * otherwise (see MessagesView).
     */
    const hasDlq = useMemo(
      () => records.some((r) => r.dlq != null),
      [records],
    );
    const colCount = hasDlq ? 6 : 5;

    const setPinned = useCallback(
      (next: boolean) => {
        if (pinnedRef.current === next) return;
        pinnedRef.current = next;
        onPinnedChange?.(next);
      },
      [onPinnedChange],
    );

    const selectedIndex = useMemo(
      () =>
        selectedKey === null
          ? -1
          : records.findIndex((r) => rowKey(r) === selectedKey),
      [records, selectedKey],
    );
    const selected = selectedIndex >= 0 ? records[selectedIndex] : null;

    useImperativeHandle(
      ref,
      () => ({
        scrollToTop() {
          const el = scrollRef.current;
          if (!el) return;
          el.scrollTop = 0;
          setPinned(false);
          onScroll();
        },
        scrollToNewest() {
          const el = scrollRef.current;
          if (!el) return;
          el.scrollTop = el.scrollHeight;
          setPinned(true);
          remeasure();
        },
        remeasure,
      }),
      [onScroll, remeasure, setPinned],
    );

    // Pinned-to-bottom, and only while following: a fetched range must not
    // scroll itself away from the row the user is reading.
    useLayoutEffect(() => {
      if (records.length === 0) {
        // An emptied list is pinned by definition — the next arrival is the
        // newest thing there is.
        setPinned(true);
        return;
      }
      if (!followRef.current || !pinnedRef.current) return;
      const el = scrollRef.current;
      if (!el) return;
      el.scrollTop = el.scrollHeight;
      // Re-window in the same commit. The spacers always add up to the full
      // list height, so scrollHeight is already correct — but the RENDERED
      // slice is still the one from before the jump, and waiting for the
      // scroll event to arrive would paint one frame of blank rows.
      onScroll();
    }, [records, onScroll, setPinned]);

    const handleScroll = useCallback(() => {
      onScroll();
      setPinned(isPinnedToBottom(scrollRef.current));
    }, [onScroll, setPinned]);

    // ── Keyboard: the promise `role="grid"` makes ─────────────────────────

    const moveTo = useCallback(
      (index: number) => {
        const clamped = Math.max(0, Math.min(records.length - 1, index));
        const record = records[clamped];
        if (!record) return;
        onSelect(rowKey(record));
        scrollIndexIntoView(scrollRef.current, clamped, win);
        // Re-window in this same commit rather than waiting for the scroll
        // event to come back around. `aria-activedescendant` may only name a
        // row that is actually in the DOM, and the row we just scrolled to is
        // outside the rendered slice until the window catches up.
        onScroll();
        // Walking rows means the user is reading, not following the stream.
        setPinned(isPinnedToBottom(scrollRef.current));
      },
      [records, win, onScroll, onSelect, setPinned],
    );

    const onKeyDown = useCallback(
      (e: React.KeyboardEvent<HTMLDivElement>) => {
        if (records.length === 0) return;
        const cur = selectedIndex;
        switch (e.key) {
          case "ArrowDown":
          case "j":
            e.preventDefault();
            moveTo(cur < 0 ? win.start : cur + 1);
            break;
          case "ArrowUp":
          case "k":
            e.preventDefault();
            moveTo(cur < 0 ? win.start : cur - 1);
            break;
          case "Home":
            e.preventDefault();
            moveTo(0);
            break;
          case "End":
            e.preventDefault();
            moveTo(records.length - 1);
            break;
          case "Enter":
            e.preventDefault();
            if (cur < 0) moveTo(win.start);
            break;
          case "Escape":
            if (selectedKey !== null) {
              e.preventDefault();
              onSelect(null);
            }
            break;
          default:
            break;
        }
      },
      [records.length, selectedIndex, selectedKey, moveTo, win.start, onSelect],
    );

    const visible = records.slice(win.start, win.end);
    /**
     * The row `aria-activedescendant` names, or nothing.
     *
     * Keyboard navigation is invisible to assistive technology without it:
     * focus never leaves the scrollport, so a screen reader has no way to know
     * which row j/k just moved to. `moveTo` scrolls the active row into view
     * and re-windows in the same commit, so on the keyboard path the id is
     * always rendered. It is dropped when the user scrolls the selected row
     * out of the window with the mouse — pointing at an element that is not in
     * the DOM is worse than pointing at nothing, and the selection is still
     * announced by `aria-selected` when the row comes back.
     */
    const activeDescendant =
      selected !== null && selectedIndex >= win.start && selectedIndex < win.end
        ? rowDomId(idPrefix, selected)
        : undefined;

    return (
      <div className="table-wrap messages-table-wrap">
        {loading && <div className="table-loading" role="presentation" />}

        {/* THE VIRTUALIZED GRID. Two spacer rows carry the height of
            everything not rendered, so the scrollbar, the keyboard
            navigation and aria-rowindex all describe the same list. */}
        <div
          className="messages-scroll"
          ref={scrollRef}
          // The grid's focus target. role="group" so the label is actually
          // exposed — aria-label on a generic element is not guaranteed to
          // reach assistive technology.
          role="group"
          aria-label={label}
          tabIndex={0}
          aria-activedescendant={activeDescendant}
          onScroll={handleScroll}
          onKeyDown={onKeyDown}
        >
          <table
            className="data-table messages-table"
            role="grid"
            aria-rowcount={records.length + 1}
          >
            <caption className="sr-only">{label}</caption>
            <colgroup>
              <col className="mcol-offset" />
              <col className="mcol-part" />
              <col className="mcol-ts" />
              <col className="mcol-key" />
              <col className="mcol-value" />
              {hasDlq && <col className="mcol-dlq" />}
            </colgroup>
            <thead>
              <tr aria-rowindex={1}>
                <th scope="col" className="ledger-gutter">
                  <Term name="offset">Offset</Term>
                </th>
                <th scope="col" className="col-num">
                  Part.
                </th>
                <th scope="col">Time</th>
                <th scope="col">Key</th>
                <th scope="col">Value</th>
                {hasDlq && <th scope="col">Dead letter</th>}
              </tr>
            </thead>
            <tbody>
              {win.padTop > 0 && (
                <tr aria-hidden="true" className="row-pad">
                  <td
                    colSpan={colCount}
                    style={{ height: win.padTop, padding: 0 }}
                  />
                </tr>
              )}
              {visible.map((record, i) => {
                const index = win.start + i;
                const key = rowKey(record);
                const tombstone = record.value === null;
                return (
                  <tr
                    key={key}
                    // The id is what `aria-activedescendant` points at; the
                    // rowindex is the row's place in the WHOLE list, not in
                    // the rendered slice.
                    id={rowDomId(idPrefix, record)}
                    ref={i === 0 ? firstRowRef : undefined}
                    aria-rowindex={index + 2}
                    aria-selected={key === selectedKey}
                    className={[
                      key === selectedKey ? "row-selected" : "",
                      tombstone ? "row-tombstone" : "",
                    ]
                      .filter(Boolean)
                      .join(" ")}
                    onClick={() => onSelect(key)}
                  >
                    <td className="ledger-gutter">
                      {tombstone && (
                        <span className="tomb-tick" aria-hidden="true">
                          •
                        </span>
                      )}
                      {groupDigits(record.offset)}
                    </td>
                    <td className="col-num cell-num">{record.partition}</td>
                    <td className="cell-mono">
                      {record.timestamp_ms === null ? (
                        <span
                          className="absent"
                          title="This message carries no timestamp."
                        >
                          ∅
                        </span>
                      ) : (
                        formatClock(record.timestamp_ms)
                      )}
                    </td>
                    <td className="cell-mono cell-preview">
                      {record.key === null ? (
                        <span
                          className="absent"
                          title="No key — Kafka spread this message across partitions."
                        >
                          ∅
                        </span>
                      ) : (
                        previewText(record.key)
                      )}
                    </td>
                    <td className="cell-mono cell-preview">
                      {tombstone ? (
                        <>
                          <span className="absent">∅</span>
                          <span className="cell-tag"> tombstone</span>
                        </>
                      ) : (
                        <>
                          {previewText(record.value)}
                          {/* A masking rule rewrote this row on its way here.
                              The tag is per ROW because a session can hold
                              both: rows fetched before a rule was switched on
                              are verbatim, and the status-bar chip alone
                              cannot tell those two apart. Same device as the
                              tombstone tag — a word, never a colour. */}
                          {record.masked === true && (
                            <span
                              className="cell-tag"
                              title="A masking rule replaced part of this record before it reached this window. Copies and exports carry the replacement."
                            >
                              {" "}
                              masked
                            </span>
                          )}
                        </>
                      )}
                    </td>
                    {hasDlq && (
                      <td className="cell-preview">
                        {record.dlq == null ? (
                          <span
                            className="absent"
                            title="No dead-letter headers Kavka recognises on this record."
                          >
                            ∅
                          </span>
                        ) : (
                          // Law 2: the badge carries a word, and the word is
                          // the exception when the framework named one — a
                          // coloured pill saying "DLQ" would be a colour with
                          // a label, not information.
                          <span
                            className="dlq-badge"
                            title={dlqTitle(record.dlq)}
                          >
                            {shortClass(record.dlq.exception_class) ??
                              "dead letter"}
                          </span>
                        )}
                      </td>
                    )}
                  </tr>
                );
              })}
              {win.padBottom > 0 && (
                <tr aria-hidden="true" className="row-pad">
                  <td
                    colSpan={colCount}
                    style={{ height: win.padBottom, padding: 0 }}
                  />
                </tr>
              )}
            </tbody>
          </table>

          {records.length === 0 && empty != null && (
            <div className="messages-empty">{empty}</div>
          )}
        </div>

        {children}
      </div>
    );
  },
);

export default MessageGrid;
