/**
 * THE ALERT TOAST'S SECOND BUTTON HAS TO LAND ON THE GROUP.
 *
 * The field report: a lag rule fires while the Monitoring screen is on, the
 * toast offers "View group demo-checkout", and pressing it opens the Consumer
 * groups LIST. The group is dropped somewhere between `goToAlertTarget` and
 * `ClusterView`'s adoption effect, and it is dropped in a way that reading
 * either side alone will not show you — which is the whole reason this file
 * exists rather than a second pass over the source.
 *
 * IT RENDERS THE REAL THING. `ClusterView` under `StrictMode` under a shell
 * that copies `App`'s `goTab` verbatim, with the real `alertNav` registry, the
 * real toast stack and the real `GroupsTab`. Only the IPC edge is doubled: the
 * Tauri `invoke`/`listen` pair becomes an in-memory backend, so an alert can be
 * pushed down the same channel the core pushes one down. A repro that stubbed
 * the hand-off would only prove the stub.
 *
 * WHAT IT CAUGHT, WRITTEN DOWN SO NOBODY HAS TO FIND IT TWICE. The adoption
 * effect used to read `pendingGroup.current` INSIDE its `setPlace` updater and
 * clear the ref on the next line. React does not promise to run an updater when
 * you hand it over: it runs it eagerly only while the component has no update
 * already in flight, and otherwise defers it to the render it schedules. The
 * toast's button navigates AND dismisses itself in one click, and the dismiss is
 * a `setToasts` on `ClusterView` itself — so on that one path, and only that
 * one, the updater ran a render later and read a ref that had already been
 * cleared. Not StrictMode (the second test below failed identically without it),
 * and not a lost commit: the effect ran once, with the right token, with the ref
 * still full. The three tests here are that finding, in order — the path that
 * broke, the same path with StrictMode off, and the path that used to work for
 * the wrong reason and must keep working for the right one.
 */

import { StrictMode, act, useCallback, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
// `vi.mock` is hoisted above every import, so the module graph these pull in
// binds to the doubles below rather than to Tauri.
import { goToAlertTarget } from "./alertNav";
import type {
  AlertEvent,
  AlertRule,
  ClusterOverview,
  ConnectionProfile,
  GroupDetail,
} from "./api";
import ClusterView, { type TabKey } from "./ClusterView";

// ---------------------------------------------------------------------------
// The IPC edge, doubled
// ---------------------------------------------------------------------------

const PROFILE_ID = "p-test";
const GROUP = "demo-checkout";
const RULE_ID = "rule-lag-1";

const LAG_RULE: AlertRule = {
  kind: "lag_threshold",
  id: RULE_ID,
  name: "checkout falling behind",
  group_id: GROUP,
  topic: null,
  threshold: 1000,
  for_ms: 60_000,
};

const GROUP_DETAIL: GroupDetail = {
  group_id: GROUP,
  state: "Stable",
  members: [],
  offsets: [],
};

/**
 * `vi.hoisted` because `vi.mock`'s factory is lifted above every import and may
 * not close over an ordinary module binding.
 */
const backend = vi.hoisted(() => {
  /**
   * A SET PER CHANNEL, AND UNLISTEN REMOVES ONE HANDLER.
   *
   * Tauri hands every `listen` its own id and `unlisten` drops exactly that
   * one. A double that keyed the channel to a single handler would have
   * StrictMode's discarded first subscription tear down the second one's
   * registration on the microtask after it was made — a bug in the double that
   * looks exactly like a bug in the app.
   */
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  return { listeners };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    switch (command) {
      case "alerts_list":
        return Promise.resolve([LAG_RULE]);
      case "group_detail":
        return Promise.resolve(GROUP_DETAIL);
      case "metrics_status":
        // The Monitoring screen the toast is read FROM. "Configured, nothing
        // scraped" is its quietest real state.
        return Promise.resolve({
          configured: false,
          reachable: false,
          last_scrape_ms: null,
          last_error: null,
          series_available: [],
        });
      default:
        // Every other read this workspace makes on mount is a list, and an
        // empty one is a legitimate answer — the screens render their own
        // "nothing here" copy for it. Nothing in this test depends on any of
        // them.
        return Promise.resolve([]);
    }
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    const set = backend.listeners.get(name) ?? new Set();
    set.add(handler);
    backend.listeners.set(name, set);
    return Promise.resolve(() => {
      set.delete(handler);
    });
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: () => Promise.resolve(null),
}));

// ---------------------------------------------------------------------------
// The shell, copied from App
// ---------------------------------------------------------------------------

const PROFILE: ConnectionProfile = {
  id: PROFILE_ID,
  name: "local",
  environment: "dev",
  bootstrap_servers: ["localhost:9092"],
  auth: { kind: "plaintext" },
  read_only: false,
};

const OVERVIEW: ClusterOverview = {
  cluster_id: "test-cluster",
  brokers: [{ id: 1, host: "localhost", port: 9092 }],
  topic_count: 0,
  partition_count: 0,
};

const noop = () => {};

/**
 * `App`'s half of the hand-off, verbatim: one `setSession` that moves the tab
 * and bumps the press counter, one `setScreen` beside it. Both are here because
 * both are in the batch the toast's button produces, and the batch is the thing
 * under test.
 */
function Shell({ initialTab }: { initialTab: TabKey }) {
  const [session, setSession] = useState<{ tab: TabKey; nav: number }>({
    tab: initialTab,
    nav: 0,
  });
  const [, setScreen] = useState<"cluster" | "settings">("cluster");
  const goTab = useCallback((next: TabKey) => {
    setSession((prev) => ({ ...prev, tab: next, nav: prev.nav + 1 }));
    setScreen("cluster");
  }, []);

  return (
    <ClusterView
      profile={PROFILE}
      overview={OVERVIEW}
      tab={session.tab}
      navNonce={session.nav}
      onFiringChange={noop}
      onTab={goTab}
      onRefresh={noop}
      onDisconnect={noop}
      onDangerChange={noop}
    />
  );
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

declare global {
  // React's own flag; jsdom does not set it and `act` insists on it.
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

let container: HTMLDivElement;
let root: Root;

/** Let the mount's promises (rule priming, the listener registration) land. */
async function settle(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

function fireAlert(): Promise<void> {
  const event: AlertEvent = {
    rule_id: RULE_ID,
    rule_name: LAG_RULE.name,
    fired_ms: 1,
    resolved_ms: null,
    detail: "12 400 behind on orders.v2",
  };
  const handlers = backend.listeners.get(`kavka://alerts/${PROFILE_ID}`);
  // Loudly, not silently: an unsubscribed channel is a broken harness, and a
  // broken harness that reports "no toast appeared" would read as a product bug.
  if (handlers === undefined || handlers.size === 0)
    throw new Error("nothing is listening on the alerts channel");
  return act(async () => {
    for (const handler of handlers) handler({ payload: event });
  });
}

function findButton(label: string): HTMLButtonElement {
  const match = [...container.querySelectorAll("button")].find(
    (button) => button.textContent?.trim() === label,
  );
  if (match === undefined) {
    const seen = [...container.querySelectorAll("button")]
      .map((button) => JSON.stringify(button.textContent?.trim()))
      .join(", ");
    throw new Error(`no button labelled "${label}" — on screen: ${seen}`);
  }
  return match;
}

function press(button: HTMLButtonElement): Promise<void> {
  return act(async () => {
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
}

/** What `ClusterView` persisted — its own `place`, which is what we are asking about. */
function placedGroup(): string | null {
  const raw = localStorage.getItem(`kavka.cluster.${PROFILE_ID}.view`);
  if (raw === null) return null;
  return (JSON.parse(raw) as { group: string | null }).group;
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  localStorage.clear();
  backend.listeners.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Mount, let the priming read land, fire the rule. The state the report starts from. */
async function alertFiresOnMonitoring(strict: boolean): Promise<void> {
  const shell = <Shell initialTab="monitoring" />;
  await act(async () => {
    root.render(strict ? <StrictMode>{shell}</StrictMode> : shell);
  });
  await settle();
  await fireAlert();
  await settle();
}

/**
 * The group's detail is on screen, not its list. Two readings of one fact: the
 * placement `ClusterView` wrote, and the back crumb only the detail draws.
 */
function expectGroupDetail(): void {
  expect(placedGroup()).toBe(GROUP);
  expect(
    [...container.querySelectorAll("button")].some(
      (button) => button.textContent?.trim() === "← Groups",
    ),
  ).toBe(true);
}

describe("an alert toast's View button", () => {
  it("opens the group the lag rule watches, not the list", async () => {
    await alertFiresOnMonitoring(true);

    await press(findButton(`View group ${GROUP}`));
    await settle();

    expectGroupDetail();
  });

  // The same press with StrictMode off. It is here to say what the failure was
  // NOT: a double-invocation artifact that only dev builds ever see. This test
  // failed exactly as loudly as the one above it.
  it("opens the group without StrictMode too", async () => {
    await alertFiresOnMonitoring(false);

    await press(findButton(`View group ${GROUP}`));
    await settle();

    expectGroupDetail();
  });

  // The same hand-off with nothing else in the batch — no toast to dismiss, so
  // React ran the updater eagerly and the old code passed here by luck. Kept so
  // a future change cannot trade one path for the other and look green.
  it("opens the group when the navigation is alone in the batch", async () => {
    await alertFiresOnMonitoring(true);

    await act(async () => {
      goToAlertTarget({ screen: "groups", group: GROUP });
    });
    await settle();

    expectGroupDetail();
  });
});
