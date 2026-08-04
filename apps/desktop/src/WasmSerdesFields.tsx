import { useCallback, useEffect, useRef, useState } from "react";
import {
  WASM_ABI_VERSION,
  WASM_FUEL_MS_APPROX,
  WASM_FUEL_PER_RECORD,
  WASM_MEMORY_CAP_BYTES,
  errorMessage,
  wasmSerdesDelete,
  wasmSerdesList,
  wasmSerdesSave,
  type WasmSerdeConfig,
} from "./api";
import { formatBytes } from "./format";

/**
 * CUSTOM DECODERS — a WebAssembly module Kavka runs over a payload before its
 * own ladder.
 *
 * This is the one place in the connection form where a field is not part of the
 * profile. Plugins are stored per connection through their own commands
 * (`wasm_serdes_list/save/delete`), so they are written AS YOU EDIT THEM rather
 * than when the form's Save button is pressed — and the section says so, in the
 * one sentence a user needs to not lose work.
 *
 * CONTRACT FRICTION — flagged, not resolved. The Phase 5b contract puts this
 * section in the profile editor and gives the plugin list its own commands, and
 * those two facts pull in opposite directions: everything else on this form is
 * a draft until Save, and this is not. Folding the writes into the form's
 * submit would need the editor to own a fourth kind of pending change and would
 * still leave a new, unsaved connection with nowhere to put them. Immediate
 * writes plus a stated rule is the honest version of that trade.
 *
 * THREE THINGS THE SECTION HAS TO SAY, because they are what makes a plugin
 * predictable rather than magic:
 *
 *  1. A MATCHING PLUGIN RUNS FIRST, before JSON, Avro, MessagePack and the
 *     rest. That is the point — a proprietary framing that happens to look like
 *     UTF-8 would otherwise be decoded as text and never reach the plugin.
 *  2. A PLUGIN THAT FAILS FALLS THROUGH TO THE LADDER. The record still
 *     decodes; the error is recorded once for the session rather than per
 *     record, because ten thousand identical failures is not a log, it is a
 *     denial of service on the person reading it.
 *  3. THE SANDBOX IS REAL AND HAS NUMBERS. 10 MB of memory, a 100 ms fuel cap
 *     per record, and no WASI imports at all: a decoder computes, it does not
 *     open files or sockets. Quoted from the constants in api.ts so the
 *     sentence and the runtime cannot drift.
 */

/** One row while it is being edited. `saved` is the name it is stored under. */
interface PluginRow {
  /** Stable across reorders so React never reuses one row's input for another. */
  key: number;
  name: string;
  path: string;
  /** Globs, as typed: comma or newline separated. */
  topics: string;
  /** The name this row is persisted under, or null if it never has been. */
  saved: string | null;
}

let nextKey = 1;

function parseGlobs(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function rowFrom(config: WasmSerdeConfig): PluginRow {
  return {
    key: nextKey++,
    name: config.name,
    path: config.path,
    topics: config.applies_to_topics.join(", "),
    saved: config.name,
  };
}

interface WasmSerdesFieldsProps {
  /** null while the connection has never been saved — see the note below. */
  profileId: string | null;
}

export default function WasmSerdesFields({ profileId }: WasmSerdesFieldsProps) {
  const [rows, setRows] = useState<PluginRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const seq = useRef(0);

  useEffect(() => {
    if (profileId === null) {
      setLoaded(true);
      return;
    }
    const mine = ++seq.current;
    wasmSerdesList(profileId)
      .then((list) => {
        if (seq.current !== mine) return;
        setRows(list.map(rowFrom));
        setLoaded(true);
      })
      .catch((err: unknown) => {
        if (seq.current !== mine) return;
        setError(errorMessage(err));
        setLoaded(true);
      });
    return () => {
      seq.current += 1;
    };
  }, [profileId]);

  const edit = useCallback((index: number, partial: Partial<PluginRow>) => {
    setRows((prev) =>
      prev.map((row, i) => (i === index ? { ...row, ...partial } : row)),
    );
  }, []);

  /**
   * Write a row, and clean up after a rename.
   *
   * The name IS the identity on the wire, so a rename is a save under the new
   * name followed by a delete of the old one — IN THAT ORDER. Delete-first
   * reads more naturally and is wrong: a save that fails after it would leave
   * the connection with neither entry, and the user would have lost a plugin
   * by editing its name. This way the worst case is two entries for one file,
   * which is visible in the list and fixable with Remove.
   */
  const commit = useCallback(
    async (index: number) => {
      if (profileId === null) return;
      const row = rows[index];
      if (row === undefined) return;
      const name = row.name.trim();
      const path = row.path.trim();
      // An incomplete row is a row being typed, not an error. Nothing is
      // written until it names a plugin and a file.
      if (name.length === 0 || path.length === 0) return;
      const config: WasmSerdeConfig = {
        name,
        path,
        applies_to_topics: parseGlobs(row.topics),
      };
      try {
        await wasmSerdesSave(profileId, config);
        if (row.saved !== null && row.saved !== name)
          await wasmSerdesDelete(profileId, row.saved);
        setError(null);
        edit(index, { saved: name });
      } catch (err) {
        setError(errorMessage(err));
      }
    },
    [profileId, rows, edit],
  );

  const remove = useCallback(
    async (index: number) => {
      const row = rows[index];
      if (row === undefined) return;
      if (profileId !== null && row.saved !== null) {
        try {
          await wasmSerdesDelete(profileId, row.saved);
          setError(null);
        } catch (err) {
          setError(errorMessage(err));
          return;
        }
      }
      setRows((prev) => prev.filter((_, i) => i !== index));
    },
    [profileId, rows],
  );

  const add = useCallback(() => {
    setRows((prev) => [
      ...prev,
      { key: nextKey++, name: "", path: "", topics: "", saved: null },
    ]);
  }, []);

  return (
    <fieldset className="fieldset">
      <legend className="eyebrow">Custom decoders (optional)</legend>

      <span className="field-hint">
        Kavka reads JSON, Avro, MessagePack, CBOR and plain text on its own. A
        WebAssembly plugin is how it reads anything else — a proprietary framing,
        a compressed envelope, a protobuf you have the schema for. A plugin that
        matches the topic runs <em>before</em> the built-in ladder, and if it
        fails the record still decodes the ordinary way; the error is shown once
        for the session rather than once per record.
      </span>

      {profileId === null ? (
        <p className="table-note">
          Save this connection first. Plugins are stored beside it, under its
          id, so there is nowhere to put one until it exists.
        </p>
      ) : (
        <>
          {/* This section writes as you type, so its failures arrive on a
              blur rather than on a button press — nothing moves focus, and
              without a live region the message is silent (SC 4.1.3). */}
          {error !== null && (
            <span className="field-error" role="alert">
              {error}
            </span>
          )}

          {loaded && rows.length === 0 && (
            <p className="table-note">
              No plugins on this connection. The example decoder — Rust source
              and the build command — is in{" "}
              <code>docs/examples/wasm-serde/</code> in the repository; it is
              source rather than a shipped binary on purpose, because a{" "}
              <code>.wasm</code> you did not build is code you did not choose to
              run.
            </p>
          )}

          {rows.map((row, index) => (
            <div className="connect-cluster" key={row.key}>
              <div className="connect-cluster-head">
                <span className="eyebrow">
                  {row.name.trim().length > 0
                    ? row.name
                    : `Decoder ${index + 1}`}
                </span>
                {/* One Remove per row, so the accessible name says which
                    row — the visible word stays "Remove" (SC 2.4.6). */}
                <button
                  type="button"
                  className="btn btn-ghost"
                  aria-label={`Remove ${
                    row.name.trim().length > 0
                      ? row.name
                      : `decoder ${index + 1}`
                  }`}
                  title="Remove this decoder from the connection"
                  onClick={() => void remove(index)}
                >
                  Remove
                </button>
              </div>

              <div className="field">
                <label className="field-label" htmlFor={`pe-wasm-${index}-name`}>
                  Name
                </label>
                <input
                  id={`pe-wasm-${index}-name`}
                  type="text"
                  value={row.name}
                  placeholder="orders-envelope"
                  autoComplete="off"
                  spellCheck={false}
                  aria-describedby={`pe-wasm-${index}-name-hint`}
                  onChange={(e) => edit(index, { name: e.target.value })}
                  onBlur={() => void commit(index)}
                />
                <span className="field-hint" id={`pe-wasm-${index}-name-hint`}>
                  What the payload inspector shows as the decoder. Renaming it
                  keeps the plugin — Kavka moves it under the new name.
                </span>
              </div>

              <div className="field">
                <label className="field-label" htmlFor={`pe-wasm-${index}-path`}>
                  Module file
                </label>
                <input
                  id={`pe-wasm-${index}-path`}
                  type="text"
                  className="input-mono"
                  value={row.path}
                  placeholder="/Users/you/decoders/orders.wasm"
                  autoComplete="off"
                  spellCheck={false}
                  aria-describedby={`pe-wasm-${index}-path-hint`}
                  onChange={(e) => edit(index, { path: e.target.value })}
                  onBlur={() => void commit(index)}
                />
                <span className="field-hint" id={`pe-wasm-${index}-path-hint`}>
                  A <code>.wasm</code> file on this machine. The path is stored,
                  never the module — so a connection you export carries the
                  address of your plugin and not the code.
                </span>
              </div>

              <div className="field">
                <label
                  className="field-label"
                  htmlFor={`pe-wasm-${index}-topics`}
                >
                  Topics it decodes
                </label>
                <input
                  id={`pe-wasm-${index}-topics`}
                  type="text"
                  className="input-mono"
                  value={row.topics}
                  placeholder="orders.*, payments.v2"
                  autoComplete="off"
                  spellCheck={false}
                  aria-describedby={`pe-wasm-${index}-topics-hint`}
                  onChange={(e) => edit(index, { topics: e.target.value })}
                  onBlur={() => void commit(index)}
                />
                <span className="field-hint" id={`pe-wasm-${index}-topics-hint`}>
                  Globs, comma separated — <code>orders.*</code> matches every
                  topic starting with <code>orders.</code>. Leave it empty and
                  the plugin is configured but matches nothing, which is a
                  perfectly good half-finished state and not the same as "every
                  topic".
                </span>
              </div>
            </div>
          ))}

          <button type="button" className="btn" onClick={add}>
            Add a decoder
          </button>

          {/* THE BUDGET IS FUEL, AND THE SENTENCE SAYS SO. Quoting a
              millisecond ceiling would name a limit nothing enforces: the
              engine counts WebAssembly operations, which is what makes the
              cap the same on a fast laptop and a slow build agent. The
              milliseconds are a measured approximation and are written as
              one. */}
          <span className="field-hint">
            Decoders are saved as you type them, not with the Save button below
            — they are stored beside this connection rather than inside it.
            Kavka runs each one with ABI v{WASM_ABI_VERSION}:{" "}
            <code>kavka_decode</code>, <code>kavka_alloc</code> and{" "}
            <code>kavka_free</code>, at most{" "}
            {formatBytes(WASM_MEMORY_CAP_BYTES)} of memory and{" "}
            {WASM_FUEL_PER_RECORD.toLocaleString()} WebAssembly operations per
            record — a fixed amount of work rather than a stopwatch, which
            measured out at roughly {WASM_FUEL_MS_APPROX} — and no host imports
            at all: a decoder computes, it cannot open a file or a socket. The
            path has to be absolute and on this machine; a network path is
            refused, because a plugin is code Kavka runs. The ABI and a worked
            example are in <code>docs/examples/wasm-serde/</code>.
          </span>
        </>
      )}
    </fieldset>
  );
}
