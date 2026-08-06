import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  COMPAT_LEVELS,
  errorMessage,
  srCheckCompat,
  srGetCompat,
  srRegister,
  srSetCompat,
  srSubjectVersions,
  type CompatResult,
  type ConnectionProfile,
  type SubjectVersion,
} from "./api";
import ConfirmModal from "./ConfirmModal";
import { useDangerSignal, type DangerReport } from "./danger";
import { useIsProtected } from "./environments";
import { countChanges, diffLines, toLines, tooBigToDiff, type DiffRow } from "./diff";
import { useI18n } from "./i18n";
import Perch from "./Perch";
import { ErrorBanner } from "./ProfileEditor";
import { ToastStack, useToasts } from "./Toast";

/**
 * SCHEMAS, FOR ONE TOPIC.
 *
 * The subject picker starts where the topic-name strategy says it should —
 * `<topic>-value` and `<topic>-key` — because that is where 95% of clusters put
 * them, and a free-text box is right there for the other 5%. Guessing the
 * subject and saying so is better than an empty field that makes the user go
 * and read Confluent's naming doc.
 *
 * Two things here are deliberately not what an SR UI usually does:
 *
 *  - The version diff is a real line diff (see diff.ts), computed after the
 *    schema is pretty-printed. A registry hands back Avro as one enormous JSON
 *    line, and diffing two of those tells you only that they differ.
 *  - Register is GATED on a compatibility check but not WALLED by it. The
 *    registry itself enforces the level (the core re-checks before it writes,
 *    whatever this screen did), so the button's job is to make sure nobody
 *    registers an incompatible schema by accident — not to pretend it is
 *    impossible.
 */

const READ_ONLY_WHY =
  "This connection is read-only. Turn that off in the connection's settings to produce or edit.";

/** One plain sentence per level. The registry's own word stays on screen. */
const LEVEL_WORDS: Record<string, string> = {
  BACKWARD:
    "A new schema can read data written with the version before it. Consumers upgrade first. This is the registry's default, and what most teams want.",
  BACKWARD_TRANSITIVE:
    "A new schema can read data written with EVERY earlier version, not just the last one.",
  FORWARD:
    "The version before it can read data written with the new schema. Producers upgrade first.",
  FORWARD_TRANSITIVE:
    "Every earlier version can read data written with the new schema.",
  FULL: "Both directions, against the version before it.",
  FULL_TRANSITIVE: "Both directions, against every earlier version.",
  NONE: "Nothing is checked. The registry accepts whatever it is sent.",
};

function levelWords(level: string): string {
  return LEVEL_WORDS[level.toUpperCase()] ?? `The registry reports ${level}.`;
}

/** SR says 404 for "no such subject" and for "no subject-level config". */
function looksMissing(raw: string): boolean {
  const text = raw.toLowerCase();
  return (
    text.includes("40401") ||
    text.includes("40403") ||
    text.includes("40408") ||
    text.includes("subject not found") ||
    text.includes("not configured") ||
    text.includes("404")
  );
}

/**
 * Pretty-print so the diff has lines to work with. A registry stores Avro and
 * JSON Schema as one line of JSON; Protobuf arrives as source text and is left
 * exactly as it is.
 */
function prettySchema(schema: string, schemaType: string): string {
  if (schemaType.toUpperCase() === "PROTOBUF") return schema;
  try {
    return JSON.stringify(JSON.parse(schema), null, 2);
  } catch {
    // Not JSON after all: show what the registry actually sent.
    return schema;
  }
}

interface SchemasPanelProps {
  profile: ConnectionProfile;
  topic: string;
  onBack: () => void;
  onDanger: DangerReport;
  /** Opens this connection's settings, which means disconnecting first. */
  onEditConnection: () => void;
}

export default function SchemasPanel({
  profile,
  topic,
  onBack,
  onDanger,
  onEditConnection,
}: SchemasPanelProps) {
  const [subject, setSubject] = useState(`${topic}-value`);
  const [subjectDraft, setSubjectDraft] = useState(`${topic}-value`);
  const [versions, setVersions] = useState<SubjectVersion[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [missing, setMissing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Compatibility: the registry-wide default, and this subject's own setting
  // when it has one — null when it has none, which the IPC now says out loud
  // (`inherited`) rather than leaving to be guessed from a level string. A
  // subject with no setting of its own is not an error to show.
  const [globalLevel, setGlobalLevel] = useState<string | null>(null);
  const [subjectLevel, setSubjectLevel] = useState<string | null>(null);
  /**
   * The fourth state nobody remembers to have: the subject's own setting could
   * not be read at all. "No setting of its own" and "we don't know" are
   * different claims, and only one of them justifies telling the user which
   * level is in force.
   */
  const [subjectLevelUnknown, setSubjectLevelUnknown] = useState(false);
  const [levelDraft, setLevelDraft] = useState<string>("");
  const [levelPending, setLevelPending] = useState<string | null>(null);
  const [levelBusy, setLevelBusy] = useState(false);

  // The two versions being compared.
  const [leftV, setLeftV] = useState<number | null>(null);
  const [rightV, setRightV] = useState<number | null>(null);

  // The register flow.
  const [draft, setDraft] = useState("");
  const [draftType, setDraftType] = useState("AVRO");
  const [check, setCheck] = useState<CompatResult | null>(null);
  const [checkedText, setCheckedText] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [registering, setRegistering] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const toaster = useToasts();
  const push = toaster.push;
  const seq = useRef(0);
  // Only the panel's foot is translated here — see the Perch below, and
  // docs/I18N.md §1 for where the boundary runs through this screen.
  const { tx } = useI18n();

  useDangerSignal(error !== null, onDanger);

  const isProtected = useIsProtected(profile.environment);
  const readOnly = profile.read_only;
  const hasRegistry = (profile.schema_registry?.url ?? "").length > 0;

  const load = useCallback(
    async (which: string) => {
      const mine = ++seq.current;
      setLoading(true);
      setMissing(false);
      try {
        const list = await srSubjectVersions(profile.id, which);
        if (seq.current !== mine) return;
        const sorted = [...list].sort((a, b) => a.version - b.version);
        setVersions(sorted);
        const newest = sorted[sorted.length - 1];
        setRightV(newest?.version ?? null);
        setLeftV(sorted[sorted.length - 2]?.version ?? newest?.version ?? null);
        if (newest !== undefined) {
          setDraft(prettySchema(newest.schema, newest.schema_type));
          setDraftType(newest.schema_type.toUpperCase() || "AVRO");
        } else {
          setDraft("");
        }
        setCheck(null);
        setCheckedText(null);
      } catch (err) {
        if (seq.current !== mine) return;
        const raw = errorMessage(err);
        if (looksMissing(raw)) {
          // Not an error: a subject nobody has registered to yet is the normal
          // state of a topic that has never had a schema.
          setVersions([]);
          setMissing(true);
          setDraft("");
          setCheck(null);
          setCheckedText(null);
        } else {
          setVersions(null);
          setError(raw);
        }
      } finally {
        if (seq.current === mine) setLoading(false);
      }
    },
    [profile.id],
  );

  const loadLevels = useCallback(
    async (which: string) => {
      try {
        const global = await srGetCompat(profile.id, null);
        setGlobalLevel(global.level);
        setLevelDraft((prev) => (prev.length > 0 ? prev : global.level));
      } catch {
        // A registry that won't say is not an error worth a banner here — the
        // subject's own level is what the register button leans on.
        setGlobalLevel(null);
      }
      try {
        // `inherited` is what tells "set to BACKWARD" from "follows a global
        // that happens to be BACKWARD". The core answers 404 on the subject's
        // own config by fetching the global and marking it, so the level below
        // is always the one actually in force — only its PROVENANCE varies.
        const inForce = await srGetCompat(profile.id, which);
        setSubjectLevelUnknown(false);
        setLevelDraft(inForce.level);
        if (inForce.inherited) {
          setSubjectLevel(null);
          // The core read the global default to answer this, so it is known
          // here even if the direct read of it a moment ago failed.
          setGlobalLevel(inForce.level);
        } else {
          setSubjectLevel(inForce.level);
        }
      } catch (err) {
        setSubjectLevel(null);
        setSubjectLevelUnknown(!looksMissing(errorMessage(err)));
      }
    },
    [profile.id],
  );

  useEffect(() => {
    if (!hasRegistry) return;
    void load(subject);
    void loadLevels(subject);
    return () => {
      seq.current += 1;
    };
  }, [hasRegistry, subject, load, loadLevels]);

  /**
   * The level actually in force for this subject, and where it came from.
   * `inherited` follows the IPC's own answer: null subject level with nothing
   * unread means the registry said this subject has no setting of its own.
   */
  const effectiveLevel = subjectLevel ?? globalLevel;
  const inherited =
    subjectLevel === null && !subjectLevelUnknown && globalLevel !== null;

  const left = versions?.find((v) => v.version === leftV) ?? null;
  const right = versions?.find((v) => v.version === rightV) ?? null;

  const diff = useMemo<{ rows: DiffRow[]; capped: boolean } | null>(() => {
    if (left === null || right === null) return null;
    const a = toLines(prettySchema(left.schema, left.schema_type));
    const b = toLines(prettySchema(right.schema, right.schema_type));
    if (tooBigToDiff(a, b)) return { rows: [], capped: true };
    return { rows: diffLines(a, b), capped: false };
  }, [left, right]);

  const changes = diff === null || diff.capped ? null : countChanges(diff.rows);
  const stale = checkedText !== null && checkedText !== draft;
  const compatible = check !== null && check.compatible && !stale;

  const runCheck = useCallback(async () => {
    if (draft.trim().length === 0) return;
    setChecking(true);
    setError(null);
    try {
      const result = await srCheckCompat(profile.id, subject, draft, draftType);
      setCheck(result);
      setCheckedText(draft);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setChecking(false);
    }
  }, [draft, draftType, profile.id, subject]);

  const register = useCallback(async () => {
    setRegistering(true);
    try {
      const result = await srRegister(profile.id, subject, draft, draftType);
      setConfirming(false);
      push({
        kind: "ok",
        title: `Registered a schema for ${subject}`,
        detail: `Schema id ${result.schema_id}`,
        mono: true,
      });
      await load(subject);
    } catch (err) {
      setConfirming(false);
      setError(errorMessage(err));
    } finally {
      setRegistering(false);
    }
  }, [profile.id, subject, draft, draftType, push, load]);

  const applyLevel = useCallback(async () => {
    if (levelPending === null) return;
    setLevelBusy(true);
    try {
      await srSetCompat(profile.id, subject, levelPending);
      setSubjectLevel(levelPending);
      setLevelPending(null);
      push({
        kind: "ok",
        title: `${subject} now checks compatibility as ${levelPending}`,
        detail: levelWords(levelPending),
      });
    } catch (err) {
      setLevelPending(null);
      setError(errorMessage(err));
    } finally {
      setLevelBusy(false);
    }
  }, [levelPending, profile.id, subject, push]);

  // ── No registry on this connection ─────────────────────────────────────

  if (!hasRegistry) {
    return (
      <>
        <SchemasPerch
          registry={false}
          loading={false}
          error={null}
          missing={false}
          subject={subject}
          versions={null}
          level={null}
          levelUnknown={false}
        />

        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              <button
                type="button"
                className="btn btn-ghost crumb-btn"
                onClick={onBack}
              >
                ← {topic}
              </button>
              <span className="topic-name">Schemas</span>
            </h2>
          </div>
          <p className="table-note">
            This connection has no Schema Registry, so there is nothing to read
            schemas from. A registry is a separate service with its own address —
            Confluent, Apicurio and Redpanda all speak the same API.
          </p>
          <p className="table-note">
            Add its address under <em>Schema Registry</em> in this connection's
            settings, usually something like <code>http://localhost:8081</code>.
          </p>
          <div className="empty-actions">
            <button type="button" className="btn" onClick={onEditConnection}>
              Open connection settings
            </button>
          </div>
          <p className="table-note">
            Opening the settings disconnects this cluster — Kavka reconnects when
            you press Connect again.
          </p>
        </section>
      </>
    );
  }

  return (
    <>
      <SchemasPerch
        registry
        loading={loading && versions === null}
        error={error}
        missing={missing}
        subject={subject}
        versions={versions}
        level={effectiveLevel}
        levelUnknown={subjectLevelUnknown}
      />

      {error !== null && (
        <ErrorBanner raw={error} onDismiss={() => setError(null)} />
      )}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <button
              type="button"
              className="btn btn-ghost crumb-btn"
              onClick={onBack}
            >
              ← {topic}
            </button>
            <span className="topic-name">{subject}</span>
            {versions !== null && versions.length > 0 && (
              <span className="panel-count">
                {versions.length} version{versions.length === 1 ? "" : "s"}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn"
              disabled={loading}
              aria-busy={loading || undefined}
              title={loading ? "Kavka is already asking the registry" : undefined}
              onClick={() => void load(subject)}
            >
              Refresh
            </button>
          </div>
        </div>

        {/* The subject picker. Topic-name strategy is the default, and it is
            named rather than assumed silently. */}
        <form
          className="seekbar"
          onSubmit={(e) => {
            e.preventDefault();
            setSubject(subjectDraft.trim());
          }}
        >
          <div className="seekbar-field seekbar-field-wide">
            <label className="seekbar-label" htmlFor="sr-subject">
              Subject
            </label>
            <input
              id="sr-subject"
              type="text"
              className="input-mono"
              value={subjectDraft}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setSubjectDraft(e.target.value)}
            />
          </div>
          <div className="seekbar-field seekbar-actions">
            <button type="submit" className="btn">
              Load
            </button>
          </div>
          <div className="seekbar-field seekbar-actions">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                setSubjectDraft(`${topic}-value`);
                setSubject(`${topic}-value`);
              }}
            >
              {topic}-value
            </button>
          </div>
          <div className="seekbar-field seekbar-actions">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                setSubjectDraft(`${topic}-key`);
                setSubject(`${topic}-key`);
              }}
            >
              {topic}-key
            </button>
          </div>
        </form>

        <p className="table-note">
          Most clusters name subjects after the topic —{" "}
          <code>{topic}-value</code> for the message body and{" "}
          <code>{topic}-key</code> for the key. If yours uses a record-name
          strategy instead, type the subject in above.
        </p>

        {readOnly && <p className="readonly-note">{READ_ONLY_WHY}</p>}

        {loading && versions === null ? (
          <p className="table-note">Asking the registry about {subject}…</p>
        ) : missing ? (
          <p className="table-note">
            The registry has no subject called <code>{subject}</code> yet.
            Registering a schema below creates it — and whatever you register
            first becomes version 1, with nothing to be compatible with.
          </p>
        ) : versions === null ? (
          <p className="table-note">
            Kavka couldn't read this subject. The registry may be unreachable, or
            it may want credentials this connection doesn't carry — both are in
            the connection's settings.
          </p>
        ) : (
          <div className="table-wrap">
            {/* The gutter carries the version — this row's address in the
                registry's own vocabulary. */}
            <table className="data-table">
              <caption className="sr-only">Versions of {subject}</caption>
              <thead>
                <tr>
                  <th scope="col" className="ledger-gutter">
                    Ver.
                  </th>
                  <th scope="col" className="col-num">
                    Schema id
                  </th>
                  <th scope="col">Type</th>
                  <th scope="col">First line</th>
                  <th scope="col" className="col-affordance">
                    <span className="sr-only">Compare</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {versions.map((v) => (
                  <tr
                    key={v.version}
                    className={`row-click${
                      v.version === rightV || v.version === leftV
                        ? " row-selected"
                        : ""
                    }`}
                    onClick={() => {
                      setLeftV(rightV);
                      setRightV(v.version);
                    }}
                    title="Compare this version with the one on the right"
                  >
                    <td className="ledger-gutter">{v.version}</td>
                    <td className="col-num cell-mono cell-mono-num">
                      {v.schema_id}
                    </td>
                    <td className="cell-tag">{v.schema_type || "AVRO"}</td>
                    <td className="cell-mono config-value">
                      <span title={v.schema}>
                        {prettySchema(v.schema, v.schema_type)
                          .split("\n")
                          .find((line) => line.trim().length > 0) ?? ""}
                      </span>
                    </td>
                    {/* The row's keyboard path, and the only element here
                        with a role that says it does something (SC 4.1.2). */}
                    <td className="col-affordance">
                      <button
                        type="button"
                        className="row-affordance"
                        aria-label={`Compare version ${v.version}`}
                        // The row is still clickable for the pointer;
                        // without this the handler runs twice per click.
                        onClick={(e) => {
                          e.stopPropagation();
                          setLeftV(rightV);
                          setRightV(v.version);
                        }}
                      >
                        Compare →
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {/* THE PANEL'S FOOT — the mockup's honesty idiom, and here it guards
            against the most expensive wrong conclusion this screen can lead
            someone to: an empty or short list looks like proof that nothing is
            using a schema, and it is not. The subject name is a CONVENTION.
            Kavka asks for the one the box above holds; a producer configured
            with a record-name strategy registers somewhere else entirely, and
            no API asks a topic which subjects its writers chose. */}
        <p className="panel-foot">
          {tx("schemas.versions.foot", {
            subject: <code>{subject}</code>,
            topic: <code>{topic}</code>,
          })}
        </p>
      </section>

      {/* ── Compatibility mode ──────────────────────────────────────────── */}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Compatibility
            <span className="panel-count">
              {effectiveLevel === null
                ? "the registry didn't say"
                : subjectLevelUnknown
                  ? `the registry-wide default is ${effectiveLevel}`
                  : inherited
                    ? `${effectiveLevel} · from the registry default`
                    : `${effectiveLevel} · set on this subject`}
            </span>
          </h2>
        </div>

        <p className="table-note">
          {effectiveLevel === null
            ? "Kavka couldn't read the compatibility level. The registry enforces one whatever this screen shows, so a register can still be refused."
            : levelWords(effectiveLevel)}
        </p>

        {subjectLevelUnknown && (
          <p className="table-note">
            Kavka couldn't read whether <code>{subject}</code> has a setting of
            its own, so the level above may not be the one in force here. The
            registry checks with its own answer either way.
          </p>
        )}

        {inherited && globalLevel !== null && (
          <p className="table-note">
            <code>{subject}</code> has no setting of its own, so it follows the
            registry-wide default (<code>{globalLevel}</code>). Setting one here
            affects this subject only.
          </p>
        )}

        <div className="seekbar">
          <div className="seekbar-field">
            <label className="seekbar-label" htmlFor="sr-level">
              Level for this subject
            </label>
            <select
              id="sr-level"
              value={levelDraft}
              onChange={(e) => setLevelDraft(e.target.value)}
            >
              {/* Every disabled control says why. No dead ends. */}
              <option
                value=""
                disabled
                title="Kavka can't clear a subject's own setting yet — that needs the registry's DELETE /config/{subject}, which isn't in this version."
              >
                Use the registry default (not yet)
              </option>
              {COMPAT_LEVELS.map((level) => (
                <option key={level} value={level}>
                  {level}
                </option>
              ))}
            </select>
          </div>
          <div className="seekbar-field seekbar-actions">
            <button
              type="button"
              className={`btn ${isProtected ? "btn-danger" : ""}`}
              disabled={
                readOnly ||
                levelDraft.length === 0 ||
                levelDraft === subjectLevel
              }
              title={
                readOnly
                  ? READ_ONLY_WHY
                  : levelDraft === subjectLevel
                    ? "This is already the level set on this subject"
                    : "Change how the registry checks new versions of this subject"
              }
              onClick={() => setLevelPending(levelDraft)}
            >
              Change level
            </button>
          </div>
        </div>

        <p className="table-note">{levelWords(levelDraft || "BACKWARD")}</p>
      </section>

      {/* ── Version diff ────────────────────────────────────────────────── */}

      {versions !== null && versions.length > 0 && (
        <section className="panel">
          <div className="panel-head">
            <h2 className="panel-title">
              Compare versions
              {changes !== null && (
                <span className="panel-count">
                  {changes.added === 0 && changes.removed === 0
                    ? "identical"
                    : `${changes.added} added · ${changes.removed} removed`}
                </span>
              )}
            </h2>
          </div>

          <div className="seekbar">
            <div className="seekbar-field">
              <label className="seekbar-label" htmlFor="sr-left">
                Older
              </label>
              <select
                id="sr-left"
                value={leftV ?? ""}
                onChange={(e) => setLeftV(Number.parseInt(e.target.value, 10))}
              >
                {versions.map((v) => (
                  <option key={v.version} value={v.version}>
                    v{v.version} · id {v.schema_id}
                  </option>
                ))}
              </select>
            </div>
            <div className="seekbar-field">
              <label className="seekbar-label" htmlFor="sr-right">
                Newer
              </label>
              <select
                id="sr-right"
                value={rightV ?? ""}
                onChange={(e) => setRightV(Number.parseInt(e.target.value, 10))}
              >
                {versions.map((v) => (
                  <option key={v.version} value={v.version}>
                    v{v.version} · id {v.schema_id}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {diff === null ? (
            <p className="table-note">
              Pick two versions to see what changed between them.
            </p>
          ) : diff.capped ? (
            <p className="table-note">
              These schemas are too long to diff in the window without stalling
              it. Open them one at a time instead.
            </p>
          ) : (
            <>
              <p className="table-note">
                <code>−</code> is what v{leftV} had and v{rightV} doesn't;{" "}
                <code>+</code> is what v{rightV} added. A field that changed
                shows on both sides, on the same line.
              </p>
              <div className="diff-cols">
                <div className="diff-pane">
                  <p className="diff-pane-head">v{leftV}</p>
                  <div className="diff-view">
                    {diff.rows.map((row, i) => (
                      <div
                        className={`code-line${
                          row.left !== null && row.kind !== "same"
                            ? " diff-del"
                            : ""
                        }`}
                        key={i}
                      >
                        <span className="code-gutter" aria-hidden="true">
                          {row.left !== null && row.kind !== "same" ? "−" : ""}
                        </span>
                        <span className="code-text">{row.left ?? ""}</span>
                      </div>
                    ))}
                  </div>
                </div>
                <div className="diff-pane">
                  <p className="diff-pane-head">v{rightV}</p>
                  <div className="diff-view">
                    {diff.rows.map((row, i) => (
                      <div
                        className={`code-line${
                          row.right !== null && row.kind !== "same"
                            ? " diff-add"
                            : ""
                        }`}
                        key={i}
                      >
                        <span className="code-gutter" aria-hidden="true">
                          {row.right !== null && row.kind !== "same" ? "+" : ""}
                        </span>
                        <span className="code-text">{row.right ?? ""}</span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </>
          )}
        </section>
      )}

      {/* ── Register a new version ──────────────────────────────────────── */}

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            Register a version
            {check !== null && !stale && (
              <span className="panel-count">
                {check.compatible
                  ? "the registry says this is compatible"
                  : "the registry says this is NOT compatible"}
              </span>
            )}
          </h2>
          <div className="panel-tools">
            <button
              type="button"
              className="btn btn-swap"
              disabled={checking || draft.trim().length === 0}
              aria-busy={checking || undefined}
              title={
                checking
                  ? "The registry is checking this schema"
                  : draft.trim().length === 0
                    ? "There is no schema to check yet"
                    : "Ask the registry whether this passes the level above"
              }
              onClick={() => void runCheck()}
            >
              <span className="btn-swap-face">
                Check compatibility
              </span>
              <span className="btn-swap-face btn-swap-busy">
                <span className="spinner" aria-hidden="true" />
                Check compatibility
              </span>
            </button>
            <button
              type="button"
              className={`btn ${
                isProtected || !compatible ? "btn-danger" : "btn-primary"
              } btn-swap`}
              disabled={readOnly || draft.trim().length === 0 || registering}
              // This button was the one in the sweep with no `aria-busy` at
              // all: it spun without ever saying so to a screen reader, and
              // `.btn-swap` keys its face swap on exactly this attribute.
              aria-busy={registering || undefined}
              title={
                readOnly
                  ? READ_ONLY_WHY
                  : registering
                    ? "Kavka is sending the schema to the registry"
                    : draft.trim().length === 0
                      ? "There is no schema to register yet"
                      : compatible && !isProtected
                      ? "Write this schema to the registry"
                      : "Kavka will ask you to confirm first"
              }
              onClick={() => {
                if (compatible && !isProtected) void register();
                else setConfirming(true);
              }}
            >
              <span className="btn-swap-face">Register</span>
              <span className="btn-swap-face btn-swap-busy">
                <span className="spinner" aria-hidden="true" />
                Register
              </span>
            </button>
          </div>
        </div>

        <div className="field">
          <div className="field-label-row">
            <label className="field-label" htmlFor="sr-draft">
              Schema
            </label>
            <span className="field-hint">
              {versions !== null && versions.length > 0
                ? `Started from v${versions[versions.length - 1].version}.`
                : "Nothing registered yet — this becomes version 1."}
            </span>
          </div>
          <textarea
            id="sr-draft"
            className="io-json"
            rows={16}
            value={draft}
            spellCheck={false}
            placeholder={'{\n  "type": "record",\n  "name": "Order",\n  "fields": []\n}'}
            onChange={(e) => setDraft(e.target.value)}
          />
          <span className="field-hint">
            The registry stores the text as it is sent, so formatting is
            preserved. Adding a field with a default is the change that stays
            backward compatible; removing one, or adding one without a default,
            is the change that doesn't.
          </span>
        </div>

        <div className="field">
          <label className="field-label" htmlFor="sr-type">
            Schema type
          </label>
          <select
            id="sr-type"
            value={draftType}
            onChange={(e) => {
              setDraftType(e.target.value);
              setCheck(null);
              setCheckedText(null);
            }}
          >
            <option value="AVRO">Avro</option>
            <option value="JSON">JSON Schema</option>
            <option value="PROTOBUF">Protobuf</option>
          </select>
          <span className="field-hint">
            It has to match what the subject already holds — a registry won't
            take an Avro version of a Protobuf subject.
          </span>
        </div>

        {check !== null && (
          <div
            className={`banner ${
              stale ? "banner-warn" : check.compatible ? "banner-info" : "banner-warn"
            }`}
            role="status"
          >
            <span className="banner-glyph" aria-hidden="true">
              {stale ? "!" : check.compatible ? "✓" : "!"}
            </span>
            <div className="banner-body">
              <p className="banner-title">
                {stale
                  ? "The schema has changed since this check."
                  : check.compatible
                    ? `The registry accepts this as the next version of ${subject}.`
                    : `The registry refuses this as the next version of ${subject}.`}
              </p>
              <p className="banner-detail">
                {stale
                  ? "Check it again before registering — the answer below is about the version you checked, not the one in the box."
                  : check.compatible
                    ? `Checked against ${effectiveLevel ?? "the level in force"}. Registering is the next step.`
                    : "The registry's own words:"}
              </p>
              {!stale && check.messages.length > 0 && (
                <ul className="sr-messages">
                  {check.messages.map((message, i) => (
                    <li key={i}>{message}</li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        )}
      </section>

      {confirming && (
        <ConfirmModal
          tone="plain"
          title={
            isProtected
              ? `Register a schema for ${subject} on ${profile.name}?`
              : `Register this schema for ${subject}?`
          }
          body={
            <>
              {compatible ? (
                <>
                  The registry has already said this is compatible with{" "}
                  {effectiveLevel ?? "the level in force"}. It becomes the newest
                  version, and producers that ask for the latest schema get it
                  from the moment it lands.
                </>
              ) : check === null ? (
                <>
                  This schema hasn't been checked. The registry enforces{" "}
                  {effectiveLevel ?? "its level"} itself and will refuse it if it
                  doesn't pass — Kavka checks again on the way through — so the
                  worst case is a refusal, not a broken subject.
                </>
              ) : stale ? (
                <>
                  The last check was for a different version of this schema, so
                  it says nothing about what is in the box now. The registry
                  checks again on the way through.
                </>
              ) : (
                <>
                  The registry said this schema is NOT compatible with{" "}
                  {effectiveLevel ?? "the level in force"} and will refuse it
                  unless that level is changed. Registering anyway is how a
                  subject ends up with a version its consumers can't read.
                </>
              )}
            </>
          }
          confirmLabel="Register schema"
          typeToConfirm={isProtected ? subject : null}
          busy={registering}
          busyLabel="Kavka is sending the schema to the registry"
          onCancel={() => setConfirming(false)}
          onConfirm={() => void register()}
        />
      )}

      {levelPending !== null && (
        <ConfirmModal
          tone={isProtected ? "destructive" : "plain"}
          title={`Set ${subject} to ${levelPending}?`}
          body={
            <>
              <p className="reset-preview">{levelWords(levelPending)}</p>
              {levelPending === "NONE" ? (
                <>
                  With NONE the registry stops checking anything for this
                  subject. Every consumer that reads it is on its own.
                </>
              ) : (
                <>
                  Every version registered from now on is checked this way. The
                  versions already in the subject are not re-checked, so an
                  existing history that wouldn't pass stays exactly as it is.
                </>
              )}
              {inherited && (
                <>
                  {" "}
                  This also stops <code>{subject}</code> following the registry
                  default.
                </>
              )}
            </>
          }
          confirmLabel="Change level"
          typeToConfirm={isProtected ? subject : null}
          busy={levelBusy}
          busyLabel="Kavka is talking to the registry"
          onCancel={() => setLevelPending(null)}
          onConfirm={() => void applyLevel()}
        />
      )}

      <ToastStack {...toaster} />
    </>
  );
}

/**
 * THE SCHEMA SCREEN'S VERDICT.
 *
 * The interesting honesty here is the FOURTH state: "this subject has no
 * compatibility setting of its own" and "Kavka could not read this subject's
 * setting" are different claims, and only the first justifies telling anyone
 * which level is in force. `levelUnknown` is what keeps the verdict from
 * naming a level it inferred rather than read.
 */
function SchemasPerch({
  registry,
  loading,
  error,
  missing,
  subject,
  versions,
  level,
  levelUnknown,
}: {
  /** Whether this connection has a Schema Registry address at all. */
  registry: boolean;
  loading: boolean;
  error: string | null;
  /** The registry answered, and it has no such subject. */
  missing: boolean;
  subject: string;
  versions: SubjectVersion[] | null;
  /** The level actually in force, subject-level or inherited. */
  level: string | null;
  levelUnknown: boolean;
}) {
  const { t } = useI18n();
  const shared = { screen: t("perch.screen.schemas"), error };

  if (!registry) {
    return (
      <Perch
        {...shared}
        tone="unknown"
        caveat={t("perch.schemas.noRegistry.next")}
      >
        {t("perch.schemas.noRegistry")}
      </Perch>
    );
  }

  if (versions === null || loading) {
    return (
      <Perch
        {...shared}
        tone="unknown"
        loading={loading}
        caveat={missing ? t("perch.schemas.missing.next") : undefined}
      >
        {t("perch.schemas.missing", { subject })}
      </Perch>
    );
  }

  return (
    <Perch
      {...shared}
      tone="ok"
      caveat={levelUnknown ? t("perch.schemas.levelUnknown") : undefined}
    >
      {t("perch.schemas.versions", { count: versions.length })}
      {!levelUnknown && level !== null
        ? ` ${t("perch.schemas.level", { level })}`
        : ""}
    </Perch>
  );
}
