/**
 * TWO SECURITY ANSWERS THE EDITOR GIVES, PINNED AT THE PLACE THE USER MEETS
 * THEM.
 *
 * Both are one character wide in the source and neither is visible from any
 * other test in this repository:
 *
 *   1. The OIDC token endpoint is checked with `isHttpsUrl`, not `isHttpUrl`.
 *      Kavka posts this profile's client secret to that URL in a form body on
 *      every connect, so `http` is a long-lived replayable credential on the
 *      wire. `kavka-core` refuses it too (`auth/oidc.rs`, and its own tests) —
 *      this is the half that answers while someone is still typing. The two
 *      helpers differ by exactly one letter and share the same shape, so a
 *      well-meaning tidy-up that folded them back together would compile,
 *      pass everything else, and quietly restore the hole.
 *
 *   2. The TLS hint is mechanism-aware. With the box unticked SASL/PLAIN puts
 *      the password itself on the wire and SCRAM puts a proof of it that can
 *      still be captured and attacked offline — a real difference the app used
 *      to be silent about, now a three-way branch in JSX that nothing held in
 *      place.
 *
 * IT RENDERS THE REAL EDITOR, like `alertNav.test.tsx` renders the real
 * ClusterView: a draft profile (`profile={null}`) touches no keychain and no
 * broker, so the only thing doubled is the Tauri IPC edge. The refusals are
 * read where a user reads them — the message under the control — rather than
 * out of a helper, because it is the wiring that was unpinned, not the
 * predicate.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import en from "./i18n/catalogs/en";
import ProfileEditor from "./ProfileEditor";

// `vi.mock` is hoisted above every import, so `./api`'s module graph binds to
// these rather than to Tauri. A draft profile asks the backend nothing; these
// exist so importing the module does not blow up.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.resolve(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: () => Promise.resolve(null),
}));

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

declare global {
  // React's own flag; jsdom does not set it and `act` insists on it.
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const noop = () => {};

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Let the mount's effects (the keychain questions, which a draft skips) land. */
function settle(): Promise<void> {
  return act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function mountDraft(): Promise<void> {
  await act(async () => {
    root.render(
      <ProfileEditor
        profile={null}
        connStatus="disconnected"
        onSaved={noop}
        onConnect={noop}
        onDeleted={noop}
        onCancelNew={noop}
        onError={noop}
        onProfilesChanged={noop}
      />,
    );
  });
  await settle();
}

function byId<T extends HTMLElement>(id: string): T {
  const found = container.querySelector<T>(`#${id}`);
  if (found === null) throw new Error(`no #${id} on screen`);
  return found;
}

/**
 * React keeps its own record of a controlled node's value, so a plain
 * assignment is invisible to it. Going through the prototype setter is what
 * makes the change one React sees.
 */
function type(
  element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement,
  value: string,
): Promise<void> {
  const prototype =
    element instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : element instanceof HTMLSelectElement
        ? HTMLSelectElement.prototype
        : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
  if (setter === undefined) throw new Error("no value setter on the prototype");
  // A <select> reports through `change`; text controls report through `input`.
  const eventName = element instanceof HTMLSelectElement ? "change" : "input";
  return act(async () => {
    setter.call(element, value);
    element.dispatchEvent(new Event(eventName, { bubbles: true }));
  });
}

function click(element: HTMLElement): Promise<void> {
  return act(async () => {
    element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
}

function pressSave(): Promise<void> {
  const button = [...container.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === en["editor.saveConnection"],
  );
  if (button === undefined) throw new Error("no Save connection button");
  return click(button);
}

/** The message under a control, or null when that control has nothing to say. */
function messageUnder(field: string): string | null {
  return container.querySelector(`#pe-${field}-error`)?.textContent ?? null;
}

/**
 * A draft filled in far enough to reach the OAuth block of `validate`, with
 * the client id deliberately LEFT EMPTY: it is the next thing checked after
 * the token endpoint, so a complaint about it is proof the URL got through.
 */
async function draftWithTokenEndpoint(endpoint: string): Promise<void> {
  await mountDraft();
  await type(byId<HTMLInputElement>("pe-name"), "orders");
  await type(byId<HTMLTextAreaElement>("pe-bootstrap"), "localhost:9092");
  await type(byId<HTMLSelectElement>("pe-auth-kind"), "oauth_bearer");
  await type(byId<HTMLInputElement>("pe-token-endpoint"), endpoint);
}

// ---------------------------------------------------------------------------

describe("the OIDC token endpoint", () => {
  it("refuses http and every other scheme, naming the field", async () => {
    for (const endpoint of [
      "http://login.example.com/oauth2/token",
      "HTTP://login.example.com/oauth2/token",
      // `https` in the query string is not the scheme.
      "http://login.example.com/token?next=https://login.example.com",
      "ftp://login.example.com/token",
      "file:///tmp/token",
      // No scheme at all, which is what a paste of a hostname looks like.
      "login.example.com/oauth2/token",
    ]) {
      await draftWithTokenEndpoint(endpoint);
      await pressSave();

      expect(messageUnder("tokenEndpoint"), endpoint).toBe(
        en["editor.err.tokenEndpointUrl"],
      );
      // The message is the one that already told the user the answer.
      expect(en["editor.err.tokenEndpointUrl"]).toContain("https://");
      // Nothing further was reached, so nothing further complained.
      expect(messageUnder("clientId"), endpoint).toBeNull();

      await act(() => root.unmount());
      root = createRoot(container);
    }
  });

  it("accepts https and moves on to the next field", async () => {
    await draftWithTokenEndpoint("https://login.example.com/oauth2/token");
    await pressSave();

    expect(messageUnder("tokenEndpoint")).toBeNull();
    // The client id is what `validate` checks immediately after the URL, so
    // this is the reading that says the URL passed rather than that the form
    // stopped caring.
    expect(messageUnder("clientId")).toBe(en["editor.err.clientId"]);
  });
});

describe("the TLS hint", () => {
  /** The SASL block, where the TLS box and its hint live. */
  async function draftWithSasl(kind: "sasl_plain" | "sasl_scram"): Promise<void> {
    await mountDraft();
    await type(byId<HTMLSelectElement>("pe-auth-kind"), kind);
  }

  const hint = () => byId("pe-tls-hint").textContent;

  it("names what SASL/PLAIN puts on the wire when TLS is off", async () => {
    await draftWithSasl("sasl_plain");
    // A new connection starts with the box unticked, which is the state the
    // sentence is about.
    expect(byId<HTMLInputElement>("pe-tls").checked).toBe(false);
    expect(hint()).toBe(en["editor.tls.hintPlainCleartext"]);
  });

  it("names the different consequence for SCRAM when TLS is off", async () => {
    await draftWithSasl("sasl_scram");
    expect(byId<HTMLInputElement>("pe-tls").checked).toBe(false);
    expect(hint()).toBe(en["editor.tls.hintScramCleartext"]);
  });

  it("goes back to the connectivity hint once TLS is on, for either mechanism", async () => {
    await draftWithSasl("sasl_plain");
    await click(byId("pe-tls"));
    expect(byId<HTMLInputElement>("pe-tls").checked).toBe(true);
    expect(hint()).toBe(en["editor.tls.hint"]);

    // The mechanism stops mattering the moment the traffic is encrypted.
    await type(byId<HTMLSelectElement>("pe-auth-kind"), "sasl_scram");
    expect(hint()).toBe(en["editor.tls.hint"]);
  });

  /** Three branches are only three branches if they say three different things. */
  it("says three different things", () => {
    const sentences = new Set([
      en["editor.tls.hint"],
      en["editor.tls.hintPlainCleartext"],
      en["editor.tls.hintScramCleartext"],
    ]);
    expect(sentences.size).toBe(3);
  });
});
