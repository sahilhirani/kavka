import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  sandboxStart,
  sandboxStatus,
  sandboxStop,
  sandboxSubscribe,
  type DockerState,
  type SandboxStatus,
  type SandboxStep,
} from "./api";

/**
 * "TRY KAVKA WITHOUT A CLUSTER" — and the honest version of it.
 *
 * THE SENTENCE THIS COMPONENT EXISTS TO SAY. Kavka does not bundle a broker.
 * Apache Kafka is a JVM application, so bundling one means bundling a JRE —
 * which is precisely what the product's positioning is against, and what makes
 * it a 12 MB download instead of a 300 MB one. Every competitor that offers a
 * one-click local cluster is either shipping a JVM or shelling out to Docker
 * without telling you. Kavka shells out to Docker and says so on screen, which
 * is the difference between a limitation and a lie.
 *
 * SO THERE ARE EXACTLY TWO SCREENS, and the one without Docker is the one that
 * matters. With Docker: a button, a streaming checklist (docs/DESIGN.md §5.4),
 * and a connection called Playground at the end of it. Without: one paragraph
 * naming the trade — no button, no disabled button, no "coming soon". A
 * disabled control would need a `title` explaining a decision the user cannot
 * act on (§5.5), and a dead end costs more than the missing feature (§5.2).
 *
 * WHY IT IS SAFE ANYWHERE. It renders only in an empty state — the surface
 * that exists precisely when no cluster is open — and the commands behind it
 * take no `profile_id` and open no `ClusterConnection`. There is no path from
 * this panel to somebody's production broker, because the API it calls has
 * nothing to point at one with. Read-only is likewise not in play: that flag
 * governs what Kavka sends to *your* brokers, and the playground is not one of
 * them (see the section header in `src-tauri/src/lib.rs`).
 *
 * WHAT IT NEVER DOES. It never connects for you. `onReady` hands the new
 * profile id up and App decides — the same as any other connection, with the
 * same status dot and the same ladder. A panel that silently opened a cluster
 * would be teaching that connections happen without being asked for.
 */

interface PlaygroundProps {
  /**
   * The playground is ready and there is a connection for it. The parent
   * re-reads the profile list, selects and connects; this component does none
   * of those — it does not own the sidebar and must not pretend to.
   */
  onReady: (profileId: string) => void;
}

/**
 * The machine-specific half of the message. The honest half is in the JSX.
 *
 * `endpoint` belongs to `remote` alone and is `status.detail` — the Rust side
 * puts the resolved Docker endpoint there rather than a broker-style reply,
 * because naming the host IS the refusal (see `resolve_endpoint` in
 * `src-tauri/src/lib.rs`).
 */
function dockerTrouble(state: DockerState, endpoint: string | null): string {
  switch (state) {
    case "absent":
      // NOT "there's no Docker on this machine": Kavka only knows where it
      // looked, and on macOS that used to be the whole bug — a GUI app gets
      // launchd's `PATH`, not the user's, so Docker Desktop was invisible to a
      // plain `docker` lookup. `status.detail` carries the search itself
      // (`docker_search_detail` in `src-tauri/src/lib.rs`), and it is one
      // "Show details" away below.
      return "Kavka couldn't find a Docker command — the details say where it looked.";
    case "stopped":
      return "Docker is installed, but its engine isn't answering — it's usually not started yet.";
    case "remote":
      return `Docker is pointing at ${endpoint ?? "another machine"}, which isn't this computer.`;
    case "no_compose":
      return "This Docker doesn't have the compose plugin, and Kavka needs `docker compose`.";
    case "ready":
      return "";
  }
}

/** What one ladder step's state is called out loud (SC 4.1.3). */
function stateWord(state: SandboxStep["state"]): string {
  switch (state) {
    case "ok":
      return "done";
    case "fail":
      return "failed";
    case "skipped":
      return "skipped";
    case "running":
      return "working";
  }
}

export default function Playground({ onReady }: PlaygroundProps) {
  const [status, setStatus] = useState<SandboxStatus | null>(null);
  const [checkFailed, setCheckFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState<null | "start" | "stop">(null);
  const [steps, setSteps] = useState<SandboxStep[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [details, setDetails] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const next = await sandboxStatus();
      if (mounted.current) {
        setStatus(next);
        setCheckFailed(null);
      }
    } catch (err) {
      // Not a global banner: this panel is an offer, and an offer that can't be
      // made is this panel's own condition, not the app's (docs/DESIGN.md §7).
      if (mounted.current) setCheckFailed(errorMessage(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /**
   * Upsert by `id`, preserving first-seen order, so a step that reports six
   * times while Docker pulls an image stays one row and stays in place.
   */
  const onStep = useCallback((step: SandboxStep) => {
    setSteps((prev) => {
      const at = prev.findIndex((s) => s.id === step.id);
      if (at === -1) return [...prev, step];
      const next = prev.slice();
      next[at] = step;
      return next;
    });
  }, []);

  const run = useCallback(
    async (what: "start" | "stop") => {
      if (busy !== null) return;
      setBusy(what);
      setSteps([]);
      setFailure(null);
      setDetails(false);
      // Subscribed BEFORE the command, which is what makes the fixed event
      // name safe: the first step cannot be emitted until the invoke lands.
      const off = sandboxSubscribe(onStep, (message) => setFailure(message));
      try {
        if (what === "start") {
          onReady(await sandboxStart());
        } else {
          await sandboxStop();
        }
      } catch (err) {
        if (mounted.current) setFailure(errorMessage(err));
      } finally {
        off();
        if (mounted.current) setBusy(null);
        // Whatever happened, what the panel shows next is what is true now,
        // not what the command claimed. A start that failed halfway can still
        // have left the broker up.
        void refresh();
      }
    },
    [busy, onStep, onReady, refresh],
  );

  if (checkFailed !== null) {
    return (
      <div className="playground">
        <h2 className="playground-title">Or start a local playground</h2>
        <p className="empty-hint">
          Kavka couldn't check whether Docker is on this machine. It said:{" "}
          <code>{checkFailed}</code>
        </p>
        <div className="empty-actions">
          <button type="button" className="btn" onClick={() => void refresh()}>
            Try again
          </button>
        </div>
      </div>
    );
  }

  if (status === null) {
    return (
      <div className="playground">
        <h2 className="playground-title">Or start a local playground</h2>
        {/* A sentence, never a spinner (docs/DESIGN.md §7 rule 6). */}
        <p className="empty-hint">Looking for Docker on this machine…</p>
      </div>
    );
  }

  const ready = status.docker === "ready" && status.compose_file !== null;
  const running = ready && status.running;

  return (
    <div className="playground">
      <h2 className="playground-title">Or start a local playground</h2>

      {/* THE HONEST PARAGRAPH. It is above the button, not behind a
          disclosure, and it is the same text whether or not Docker is here:
          the reason Kavka has no built-in broker does not change based on what
          the user happens to have installed. */}
      <p className="empty-hint">
        Kavka doesn't bundle a broker — that would mean bundling a Java runtime,
        and a 300 MB download is the thing this app exists not to be. With
        Docker installed, this button starts one in about 30 seconds.
      </p>

      {status.docker !== "ready" && (
        <>
          <p className="empty-hint">
            {dockerTrouble(status.docker, status.detail)}{" "}
            {status.docker === "remote" ? (
              <>
                The playground starts containers —{" "}
                <strong>on this machine only</strong> — so Kavka won't create
                one on a host you'd then have to go and clean up, on a port it
                couldn't reach anyway. Switch back with{" "}
                <code>docker context use default</code>, or clear{" "}
                <code>DOCKER_HOST</code>, and check again.
              </>
            ) : (
              <>
                Install Docker Desktop — or any runtime with the{" "}
                <code>docker compose</code> command — and this offer turns into
                a button. Kavka never installs anything itself.
              </>
            )}
          </p>
          {/* Not for `remote`: its detail IS the endpoint, and the endpoint is
              already in the sentence above. A disclosure that repeats the
              sentence reads as a second, different problem. */}
          {status.detail !== null && status.docker !== "remote" && (
            <>
              <button
                type="button"
                className="support-link playground-details-toggle"
                aria-expanded={details}
                // Scoped to the open state, because an `aria-controls` naming
                // an element that is not in the DOM is a broken reference
                // (docs/A11Y-AUDIT.md A11Y-05).
                aria-controls={details ? "playground-docker-detail" : undefined}
                onClick={() => setDetails((open) => !open)}
              >
                {details ? "Hide details" : "Show details ▾"}
              </button>
              {details && (
                <pre className="banner-raw" id="playground-docker-detail">
                  {status.detail}
                </pre>
              )}
            </>
          )}
          <div className="empty-actions">
            <button type="button" className="btn" onClick={() => void refresh()}>
              Check again
            </button>
          </div>
        </>
      )}

      {status.docker === "ready" && status.compose_file === null && (
        <p className="empty-hint">
          Docker is ready, but this build of Kavka is missing its playground
          compose file. That's a packaging fault rather than anything on this
          machine — please report it.
        </p>
      )}

      {ready && (
        <>
          <p className="empty-hint">
            One broker, five topics and a few hundred records, on{" "}
            <code>{status.bootstrap}</code>. That port is deliberate: it can't
            collide with a Kafka already listening on 9092.
          </p>

          {/* THE LADDER, OUT LOUD (SC 4.1.3, docs/A11Y-AUDIT.md A11Y-37).
              The visible checklist is a list, and a list that rewrites itself
              announces nothing — so the state transitions get a live region of
              their own.

              It is a debounced mirror BY CONSTRUCTION rather than by timer:
              the sentence is built from the newest step's LABEL AND STATE and
              never from its note, so the once-a-second `47s · Pulling
              apache/kafka` ticks leave this text node identical — no mutation,
              no announcement — while `running → ok` changes it and is read
              out. That is the shape A11Y-36 defers to; here there is no second
              copy of a number to keep honest, so it is cheap enough to do now.

              Rendered whenever the panel is ready — before the first step can
              arrive — because a live region inserted at the same moment as its
              content is a live region screen readers do not announce. */}
          <p className="sr-only" role="status">
            {steps.length === 0
              ? ""
              : `${steps[steps.length - 1].label}: ${stateWord(
                  steps[steps.length - 1].state,
                )}`}
          </p>

          {/* The streaming checklist. Steps arrive at tertiary and resolve to
              ok/fail with a glyph — it localizes a failure before anybody has
              to read prose (docs/DESIGN.md §5.4). */}
          {steps.length > 0 && (
            <ol className="ladder playground-ladder">
              {steps.map((step) => (
                <li
                  key={step.id}
                  className={`ladder-step ladder-${
                    step.state === "ok"
                      ? "ok"
                      : step.state === "fail"
                        ? "fail"
                        : step.state === "skipped"
                          ? "skip"
                          : "running"
                  }`}
                >
                  <span className="ladder-step-glyph" aria-hidden="true">
                    {step.state === "ok"
                      ? "✓"
                      : step.state === "fail"
                        ? "✗"
                        : step.state === "skipped"
                          ? "—"
                          : "·"}
                  </span>
                  <span className="ladder-step-label">{step.label}</span>
                  {step.note !== null && (
                    <span className="ladder-step-note">{step.note}</span>
                  )}
                </li>
              ))}
            </ol>
          )}

          {failure !== null && (
            <p className="playground-failed" role="alert">
              {failure}
            </p>
          )}

          <div className="empty-actions">
            {running ? (
              <>
                <button
                  type="button"
                  className="btn"
                  disabled={busy !== null}
                  title={
                    busy !== null
                      ? "Kavka is talking to Docker"
                      : "Open the Playground connection"
                  }
                  onClick={() => {
                    if (status.profile_id !== null) onReady(status.profile_id);
                    else void run("start");
                  }}
                >
                  Open the playground
                </button>
                <button
                  type="button"
                  className="btn btn-danger btn-swap"
                  disabled={busy !== null}
                  // The same promise the start button makes: this one drives
                  // `docker compose down`, which is the same kind of wait, and
                  // a busy control that does not say so is a 4.1.2 gap
                  // (docs/A11Y-AUDIT.md A11Y-41).
                  aria-busy={busy === "stop"}
                  title={
                    busy !== null
                      ? "Kavka is talking to Docker"
                      : "Stop the playground's containers. Its data stays in a Docker volume."
                  }
                  onClick={() => void run("stop")}
                >
                  <span className="btn-swap-face">Stop the playground</span>
                  <span className="btn-swap-face btn-swap-busy">
                    <span className="spinner" aria-hidden="true" />
                    Stop the playground
                  </span>
                </button>
              </>
            ) : (
              <button
                type="button"
                className="btn btn-swap"
                disabled={busy !== null}
                aria-busy={busy === "start"}
                title={
                  busy !== null
                    ? "Kavka is talking to Docker"
                    : "Run a single-node Kafka in Docker and save a connection to it"
                }
                onClick={() => void run("start")}
              >
                {/* THE LABEL SURVIVES THE WAIT AND THE BUTTON DOES NOT RESIZE,
                    which is what the old `.btn-busy-slot` was for — except it
                    reserved a visibly empty 16px box on an idle button, which
                    reads as a missing icon. Both faces share one grid cell
                    instead, so the wider one fixes the width and the idle
                    button carries no void. See `.btn-swap` in
                    styles/jackdaw-shell.css. */}
                <span className="btn-swap-face">Start a local playground</span>
                <span className="btn-swap-face btn-swap-busy">
                  <span className="spinner" aria-hidden="true" />
                  Start a local playground
                </span>
              </button>
            )}
          </div>

          <p className="playground-footnote">
            It runs as its own Docker Compose project,{" "}
            <code>kavka-playground</code>. Stopping it leaves the data in a
            Docker volume so it comes back where you left it — Kavka never
            deletes that for you.
          </p>
        </>
      )}
    </div>
  );
}
