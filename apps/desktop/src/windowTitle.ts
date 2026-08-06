import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

/**
 * THE WINDOW TITLE — the third place the environment has to appear.
 *
 * DESIGN.md §6 lists "adds the environment to the window title" as one of the
 * protected-environment guardrails, and the connection form promises out loud
 * that the name you type "shows in the window title, the cluster switcher and
 * every alert Kavka sends you". Until now neither happened: the native bar
 * read the literal from `tauri.conf.json` — "Kavka" — on every cluster, so the
 * one piece of chrome that survives being minimised, alt-tabbed past or
 * screenshotted into a ticket said nothing about which cluster it was.
 *
 * THE SHAPE IS `Kavka · <connection> · <ENVIRONMENT>`, which is the mockup's
 * own in-window titlebar breadcrumb (`Kavka · local · DEV`). A real window with
 * native decorations cannot reproduce that drawing's layout, but its CONTENT is
 * the load-bearing half and it is three lines of code.
 *
 * THE ENVIRONMENT IS UPPERCASED, NOT TRANSLATED. It is the same string as the
 * chip, the forced-colors wire label and the CLI's refusal (§6, §10): a
 * guardrail that reads differently in different locales is two signals where
 * the design specifies one. "Kavka" is not translated either — it is the
 * product's name.
 *
 * IT NEEDS A PERMISSION. `core:window:default` grants the READ side only
 * (`allow-title`); setting one is `core:window:allow-set-title`, which is now
 * in `src-tauri/capabilities/default.json`. That grant reaches the binary at
 * BUILD time, so a dev shell built before it landed will reject the call — see
 * the catch below, which is also what makes this file safe outside Tauri.
 */

/** What the bar says with nothing selected, and the floor for everything else. */
const APP_NAME = "Kavka";

/**
 * Push a title at the window. Never throws and never reports: a title that did
 * not change is a cosmetic loss, and an error banner about the WINDOW FRAME
 * over whatever the user actually opened the app to do would be worse than the
 * thing it is complaining about. The failure modes are all environmental —
 * no Tauri (a browser-served dev build), or an older shell whose capability
 * set predates the grant above.
 */
export function setWindowTitle(title: string): void {
  try {
    void getCurrentWindow()
      .setTitle(title)
      .catch(() => undefined);
  } catch {
    /* not running inside a Tauri window */
  }
}

/**
 * Build the title for a selection. Exported for the test that will one day
 * assert the prod string, and because the assembly is the part worth reading:
 * absent parts collapse rather than leaving a dangling separator, so an
 * unnamed environment gives `Kavka · local` and not `Kavka · local · `.
 */
export function windowTitleFor(
  connection: string | null,
  environment: string | null,
): string {
  const parts = [APP_NAME];
  if (connection !== null && connection !== "") parts.push(connection);
  if (environment !== null && environment !== "") {
    parts.push(environment.toUpperCase());
  }
  return parts.join(" · ");
}

/**
 * Keep the window title pointed at the selected cluster.
 *
 * Called once, from the shell. It does NOT carry the connection STATE — a
 * title that flickers between "connecting…" and "connected" is a title people
 * stop reading, and the state has three homes on screen already (the cluster
 * card, the status bar, the switcher row). What belongs here is identity,
 * which is exactly what survives the window being minimised.
 */
export function useWindowTitle(
  connection: string | null,
  environment: string | null,
): void {
  useEffect(() => {
    setWindowTitle(windowTitleFor(connection, environment));
  }, [connection, environment]);
}
