/**
 * The prod de-collision signal, as one hook.
 *
 * docs/DESIGN.md §5.8: `data-alert="danger"` must be set for ANY danger on
 * screen — including a banner rendered inside a view, not just the global one.
 * Miss the inline case and a prod cluster shows a coral rule behind a coral
 * banner, which is the exact composition the damper exists to prevent (§9
 * gate 4).
 *
 * The attribute lives on `.app`, so every view that can raise a danger banner
 * reports up rather than reaching for the root itself.
 *
 * IT REPORTS A SOURCE, NOT A BOOLEAN. Two components on the same screen can
 * each have a banner — the topic list and the message browser inside it — and
 * child effects run before parent effects, so a plain boolean lets the outer
 * component's "no danger here" land last and switch the guardrail back off
 * while an error is still on screen. The collector counts sources instead, so
 * the attribute is on exactly while at least one of them says so.
 *
 * The cleanup is the other load-bearing half: a component that unmounts with
 * an error up must not leave the whole app dampened.
 */

import { useEffect, useId } from "react";

export type DangerReport = (source: string, danger: boolean) => void;

export function useDangerSignal(active: boolean, report: DangerReport): void {
  const source = useId();
  useEffect(() => {
    report(source, active);
    return () => report(source, false);
  }, [source, active, report]);
}
