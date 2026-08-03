import { useCallback, useEffect, useRef, useState } from "react";
import {
  MCP_ALLOW_PROD_ENV,
  MCP_ALLOW_WRITES_ENV,
  MCP_READ_TOOLS,
  MCP_UNMASKED_ENV,
  MCP_WRITE_TOOLS,
  errorMessage,
  mcpInfo,
  type McpInfo,
} from "./api";
import { copyText } from "./clipboard";

/**
 * THE MCP SERVER, as the About dialog explains it.
 *
 * Kavka ships a second binary that speaks the Model Context Protocol over
 * stdio and reads the SAME profiles.json and the SAME OS keychain this window
 * does. Point Claude Code or Cursor at it and an assistant can look at your
 * clusters with exactly the connections you already trust.
 *
 * FOUR THINGS THIS SECTION HAS TO GET RIGHT, and they are all the same thing —
 * nobody should be surprised by what an AI can do with their brokers:
 *
 * 1. THE PATH IS THE SHELL'S ANSWER, not one this window assembles. A dev
 *    build, an installed .app and a Windows install directory put the binary in
 *    three different places, and a snippet the user has to hand-edit is a
 *    snippet that teaches nothing.
 * 2. THE SNIPPETS ARE RENDERED VERBATIM AND ARE COPYABLE. They are literals
 *    that go into someone else's config file, so they are mono, selectable and
 *    one click from the clipboard — §4's rule, at document scale.
 * 3. THE WRITE GATING IS EXPLAINED IN PLAIN LANGUAGE AND NAMES THE VARIABLES.
 *    Out of the box the server can only read. Producing and resetting offsets
 *    need an environment variable set where the CLIENT launches the server —
 *    which means this window cannot turn them on, and must never look like it
 *    can. There is no toggle here on purpose.
 * 4. WHAT IS DELIBERATELY ABSENT IS STATED. No delete, no create, no ACL or
 *    config changes over MCP in v1. Saying "there is no tool for that" is the
 *    difference between a considered boundary and a missing feature.
 * 5. MASKING TRAVELS TO IT, AND THE SECTION SAYS SO. The rules written in the
 *    Masking tab are stored beside profiles.json, which is the same file the
 *    server reads — so they apply to what an assistant is handed, by default.
 *    That is the surprising direction (a person expects a second program to
 *    ignore them), so it is stated here rather than left to be discovered from
 *    a transcript full of bullets.
 */

interface McpSectionProps {
  /** Open the repository — the one allowlisted external link this can use. */
  onOpenRepo: () => void;
}

export default function McpSection({ onOpenRepo }: McpSectionProps) {
  const [info, setInfo] = useState<McpInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    mcpInfo()
      .then((next) => {
        if (!cancelled) setInfo(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(
    () => () => {
      if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    },
    [],
  );

  const copy = useCallback(async (what: string, text: string) => {
    const ok = await copyText(text);
    setCopied(ok ? `${what} copied` : "Kavka couldn't reach the clipboard");
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => setCopied(null), 2400);
  }, []);

  return (
    <section className="mcp-section" aria-labelledby="about-mcp-title">
      <h3 className="eyebrow" id="about-mcp-title">
        MCP server
      </h3>

      <p className="dialog-note">
        Kavka ships a separate program that lets an AI assistant read these
        clusters — the same connections, the same keychain, no second set of
        credentials. It speaks the Model Context Protocol over stdio, so a
        client starts it and talks to it directly; nothing listens on a port and
        nothing leaves this machine unless your assistant sends it.
      </p>

      {error !== null && (
        <p className="dialog-note" role="status">
          Kavka couldn't work out where the MCP server is on this machine. It
          said: <code>{error}</code>
        </p>
      )}

      {info === null && error === null && (
        <p className="dialog-note">Finding the MCP server…</p>
      )}

      {info !== null && (
        <>
          <div className="mcp-field">
            <span className="mcp-label">Binary</span>
            <code className="mcp-path">{info.binary_path}</code>
            <button
              type="button"
              className="btn btn-ghost"
              title="Copy the server's path"
              onClick={() => void copy("Path", info.binary_path)}
            >
              Copy
            </button>
          </div>

          <McpSnippet
            title="Claude Code"
            lead="Run this once, in the project you want it available in."
            snippet={info.snippet_claude}
            onCopy={() => void copy("Claude Code command", info.snippet_claude)}
          />

          <McpSnippet
            title="Cursor"
            lead={
              <>
                Put this in <code>.cursor/mcp.json</code> in the project, or in{" "}
                <code>~/.cursor/mcp.json</code> for every project.
              </>
            }
            snippet={info.snippet_cursor}
            onCopy={() => void copy("Cursor config", info.snippet_cursor)}
          />

          {/* role="status": a copy is something you did, finished. */}
          <span className="inspector-copied" role="status">
            {copied ?? ""}
          </span>
        </>
      )}

      <h4 className="mcp-subhead">What it can do</h4>
      <p className="dialog-note">
        Out of the box the server can only <strong>read</strong>: profiles,
        cluster overviews, topics, messages, bounded searches and SQL scans,
        consumer groups. Every scan has a ceiling — at most 500 messages a
        fetch, and search and SQL stop at their own caps — so an assistant can
        never quietly pull a whole topic through your terminal.
      </p>
      <ul className="mcp-tools">
        {MCP_READ_TOOLS.map((tool) => (
          <li className="mcp-tool" key={tool.name}>
            <code>{tool.name}</code>
            <span className="mcp-tool-what">{tool.what}</span>
          </li>
        ))}
      </ul>

      <h4 className="mcp-subhead">Masking applies to it</h4>
      <p className="dialog-note">
        The rules in this connection's <strong>Masking</strong> tab are stored
        beside <code>profiles.json</code> — the same folder the server reads —
        so they apply to what an assistant is handed, not just to what is on
        your screen. A record it returns is redacted the same way, and every
        answer a rule rewrote carries a note saying the values are replacements
        rather than data. That is the direction most people expect to be wrong,
        and it is the direction that matters: an assistant's context is logged,
        replayed and sent to somebody else's server.
      </p>
      <p className="dialog-note">
        Two things it does <em>not</em> do. Masking never touches a write — a
        record it produces carries the bytes it was given, because redacting on
        the way out would corrupt a topic while hiding nothing. And a query's
        counts are still counts of the real records: the scan reads the cluster,
        the redaction happens on the answer. To read raw payloads, whoever
        launches the server starts it with <code>{MCP_UNMASKED_ENV}=1</code>;
        like the two variables below, there is no switch for it in this window.
      </p>

      <h4 className="mcp-subhead">What it can't do until you say so</h4>
      <p className="dialog-note">
        Two tools write, and both are switched off unless the server is{" "}
        <em>started</em> with the environment variable set — by the client, in
        the config above. There is no switch for it in this window, and that is
        deliberate: the app you are looking at should not be able to widen what
        another program is allowed to do.
      </p>
      <ul className="mcp-tools">
        {MCP_WRITE_TOOLS.map((tool) => (
          <li className="mcp-tool" key={tool.name}>
            <code>{tool.name}</code>
            <span className="mcp-tool-what">{tool.what}</span>
          </li>
        ))}
      </ul>
      <dl className="mcp-gates">
        <div className="mcp-gate">
          <dt>
            <code>{MCP_ALLOW_WRITES_ENV}=1</code>
          </dt>
          <dd>
            Lets the two write tools exist at all. Without it every call to them
            is refused, and the refusal names this variable.
          </dd>
        </div>
        <div className="mcp-gate">
          <dt>
            <code>{MCP_ALLOW_PROD_ENV}=1</code>
          </dt>
          <dd>
            Needed <em>as well</em> before a write touches a connection whose
            environment is prod. A read-only connection refuses writes whatever
            these are set to — that flag is yours and the server honours it.
          </dd>
        </div>
      </dl>

      <p className="dialog-note">
        There is no delete, no create, no ACL and no config change over MCP in
        this version. Those are one sentence away from an incident and the blast
        radius isn't worth it; the app in front of you does them, with the
        confirmations. The server says the same thing in its own description, so
        an assistant knows not to look for them.
      </p>

      <p className="dialog-note">
        The example WebAssembly decoder — source and build instructions — is in{" "}
        <code>docs/examples/wasm-serde/</code> in the repository, along with the
        ABI these plugins implement.{" "}
        <button
          type="button"
          className="support-link mcp-repo-link"
          onClick={onOpenRepo}
        >
          Open the repository
        </button>
      </p>
    </section>
  );
}

function McpSnippet({
  title,
  lead,
  snippet,
  onCopy,
}: {
  title: string;
  lead: React.ReactNode;
  snippet: string;
  onCopy: () => void;
}) {
  return (
    <div className="mcp-snippet">
      <div className="mcp-snippet-head">
        <span className="mcp-label">{title}</span>
        <button
          type="button"
          className="btn btn-ghost"
          title={`Copy the ${title} configuration`}
          onClick={onCopy}
        >
          Copy
        </button>
      </div>
      <p className="dialog-note">{lead}</p>
      {/* Verbatim, selectable, and never re-wrapped: it goes into someone
          else's config file exactly as it is. */}
      <pre className="banner-raw mcp-code">{snippet}</pre>
    </div>
  );
}
