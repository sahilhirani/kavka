import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";

// ---------------------------------------------------------------------------
// Types (IPC contract — must match the Rust side exactly)
//
// Rust serialises structs with snake_case fields and enums tagged { "kind": … }
// with snake_case variant names. Every type below mirrors that verbatim; the
// only camelCase in this file is the ARGUMENT keys of invoke(), which Tauri
// converts to snake_case Rust parameters for us.
// ---------------------------------------------------------------------------

export type Environment = "dev" | "staging" | "prod";

export interface SecretRef {
  entry: string;
}

export type ScramMechanism = "SCRAM-SHA-256" | "SCRAM-SHA-512";

export type AuthConfig =
  | { kind: "plaintext" }
  | {
      kind: "tls";
      ca_pem_path?: string | null;
      client_cert_pem_path?: string | null;
      client_key?: SecretRef | null;
    }
  | { kind: "sasl_plain"; username: string; password: SecretRef; tls: boolean }
  | {
      kind: "sasl_scram";
      mechanism: ScramMechanism;
      username: string;
      password: SecretRef;
      tls: boolean;
    }
  | { kind: "aws_msk_iam"; region: string; profile?: string | null }
  | {
      kind: "oauth_bearer";
      token_endpoint: string;
      client_id: string;
      client_secret: SecretRef;
    }
  | { kind: "kerberos"; service_name: string; principal: string };

export type AuthKind = AuthConfig["kind"];

/**
 * Where Kavka looks up schemas for this cluster. Optional and serde-defaulted
 * to None on the Rust side, so a profile written before Phase 1 deserialises
 * without the key — which is why this is `?` here and not `| null` alone.
 * The password is a keychain reference like every other secret: never a value.
 */
export interface SchemaRegistryConfig {
  url: string;
  username: string | null;
  password: SecretRef | null;
}

/**
 * One Kafka Connect worker group's REST endpoint, as the profile stores it.
 *
 * A list rather than one entry, because a cluster routinely has several
 * Connect groups and every `connect_*` call has to name the one it goes to.
 * The password is a keychain reference like every other secret: never a value.
 * Its entry name is `{profileId}/connect_password/{cluster}` — see
 * ProfileEditor, which is the only thing that mints one.
 */
export interface ConnectClusterConfig {
  name: string;
  url: string;
  username: string | null;
  password: SecretRef | null;
}

/**
 * Where Kavka scrapes broker metrics for this cluster (Phase 4).
 *
 * Kafka's brokers do not serve their own metrics over the Kafka protocol —
 * they publish JMX, and everyone puts a Prometheus exporter in front of it. So
 * this is a plain HTTP address, independent of the brokers, of the registry and
 * of Connect: a cluster can want mTLS on 9093 and answer metrics on an
 * unauthenticated 7071.
 *
 * The password is a keychain reference like every other secret: never a value.
 * Its entry name is `{profileId}/metrics_password` — see ProfileEditor, which
 * is the only thing that mints one, and `kavka_core::secrets::SECRET_SUFFIXES`,
 * which is what purges it.
 */
export interface MetricsEndpointConfig {
  url: string;
  username: string | null;
  password: SecretRef | null;
}

export interface ConnectionProfile {
  id: string;
  name: string;
  environment: Environment;
  bootstrap_servers: string[];
  auth: AuthConfig;
  read_only: boolean;
  schema_registry?: SchemaRegistryConfig | null;
  /**
   * Absent on every profile written before Phase 3a — the Rust side serde-
   * defaults it to an empty Vec, so this is `?` here rather than `| null`
   * alone, and every reader treats missing and empty as the same thing.
   */
  connect_clusters?: ConnectClusterConfig[];
  /**
   * Absent on every profile written before Phase 4 — serde-defaulted to None
   * on the Rust side, so `?` here says the same thing the struct does. Null and
   * missing both mean "this cluster has no metrics endpoint", and every reader
   * treats them identically.
   */
  metrics_endpoint?: MetricsEndpointConfig | null;
  /**
   * How often the lag sampler takes a reading while this connection is up, in
   * milliseconds. Absent means "the core's default" (SAMPLER_DEFAULT_MS), which
   * is why this is `?` and not a number with a default baked in here: two
   * defaults in two languages drift, and the one that matters is the sampler's.
   *
   * CONTRACT FRICTION — flagged, not resolved. The Phase 4 IPC contract names
   * exactly one new profile field (`metrics_endpoint`) and gives the sampler no
   * command of its own, but `sampler_status` reports an `interval_ms` the user
   * has to be able to change somewhere. The profile is the only place that
   * survives a restart, so the interval lives here; the core has to grow the
   * field with a serde default, or every profile on disk fails to parse.
   */
  sampler_interval_ms?: number | null;
}

export interface BrokerInfo {
  id: number;
  host: string;
  port: number;
}

export interface ClusterOverview {
  cluster_id: string | null;
  brokers: BrokerInfo[];
  topic_count: number;
  partition_count: number;
}

export interface TopicInfo {
  name: string;
  partitions: number;
  replication_factor: number;
  internal: boolean;
}

/** What to do when an imported profile's id already exists in the store. */
export type ImportStrategy = "skip" | "replace";

export interface ImportReport {
  imported: number;
  skipped: number;
  replaced: number;
}

// ---------------------------------------------------------------------------
// Phase 1 — topics, groups, messages
// ---------------------------------------------------------------------------

export interface PartitionDetail {
  partition: number;
  leader: number;
  replicas: number[];
  isr: number[];
  earliest_offset: number;
  latest_offset: number;
}

export interface ConfigEntry {
  name: string;
  value: string | null;
  is_default: boolean;
  is_read_only: boolean;
  is_sensitive: boolean;
  source: string;
}

export interface TopicDetail {
  name: string;
  internal: boolean;
  partitions: PartitionDetail[];
  configs: ConfigEntry[];
}

/** A config row on its way to `topic_create`. */
export interface TopicConfigInput {
  name: string;
  value: string;
}

export interface GroupInfo {
  group_id: string;
  state: string;
  protocol_type: string;
  member_count: number;
}

export interface TopicPartition {
  topic: string;
  partition: number;
}

export interface GroupMember {
  member_id: string;
  client_id: string;
  client_host: string;
  assignments: TopicPartition[];
}

export interface GroupOffset {
  topic: string;
  partition: number;
  /** null = this group has never committed for this partition. */
  committed: number | null;
  end_offset: number;
  /** null whenever `committed` is null — there is nothing to subtract from. */
  lag: number | null;
}

export interface GroupDetail {
  group_id: string;
  state: string;
  members: GroupMember[];
  offsets: GroupOffset[];
}

export type ResetTarget =
  | { kind: "earliest" }
  | { kind: "latest" }
  | { kind: "offset"; offset: number }
  | { kind: "timestamp_ms"; timestamp_ms: number }
  | { kind: "shift_by"; shift_by: number };

export interface OffsetResetSpec {
  group_id: string;
  topic: string;
  /** null = every partition the group has an offset for. */
  partitions: number[] | null;
  target: ResetTarget;
  /**
   * Override the active-group guard. Only ever set after the broker has
   * already refused once — see ResetOffsetsModal.
   */
  force: boolean;
}

export interface SchemaMeta {
  schema_id: number;
  subject: string | null;
  version: number | null;
}

/** Anything `serde_json::Value` can hold. */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

export type PayloadEncoding =
  | "json"
  | "utf8"
  | "avro"
  | "msgpack"
  | "cbor"
  | "hex";

export interface DecodedPayload {
  /** One of PayloadEncoding, but the core may add more — keep it open. */
  encoding: string;
  text: string;
  json: JsonValue | null;
  raw_len: number;
  /** True when the core cut the value at `max_value_bytes` for display. */
  truncated: boolean;
  schema: SchemaMeta | null;
}

export interface HeaderEntry {
  key: string;
  value: string | null;
  is_text: boolean;
}

export interface MessageRecord {
  partition: number;
  offset: number;
  timestamp_ms: number | null;
  key: DecodedPayload | null;
  /** null = tombstone (a record with no value, not a record we failed to read). */
  value: DecodedPayload | null;
  headers: HeaderEntry[];
  /**
   * Dead-letter metadata, when this record's headers matched a convention
   * Kavka knows (Phase 5a). See `DlqMeta` at the bottom of this file.
   *
   * ADDITIVE AND ABSENT BY DEFAULT — `?` rather than `| null` alone, for the
   * same reason `connect_clusters` is: the Rust side serde-defaults it to
   * None, so a record decoded by an older core (or a record whose headers
   * matched nothing) simply has no key here. Missing and null mean the same
   * thing to every reader: this is not a recognised dead letter. A record that
   * IS one always carries the object, so `record.dlq != null` is the whole
   * test and no view has to sniff header names for itself.
   */
  dlq?: DlqMeta | null;
}

export type SeekSpec =
  | { kind: "earliest" }
  | { kind: "latest"; last_n: number }
  | { kind: "offset"; partition: number; offset: number }
  | { kind: "timestamp"; timestamp_ms: number };

export interface FetchSpec {
  topic: string;
  seek: SeekSpec;
  /** null = every partition. */
  partitions: number[] | null;
  /** The core caps this at 2000 however large we ask. */
  max_messages: number;
  /** Display truncation only; null uses the core default (256 KB). */
  max_value_bytes: number | null;
}

/** The core's own ceiling on a single fetch. Mirrored so the UI can say so. */
export const MAX_FETCH_MESSAGES = 2000;

/** The core's default display truncation, in bytes. */
export const DEFAULT_MAX_VALUE_BYTES = 262144;

/**
 * One batch from a live tail. `ended: true` arrives exactly once, with an
 * empty `records`, when the session dies — the counter is still meaningful on
 * that payload, so it is read before the session is torn down.
 */
export interface TailPayload {
  records: MessageRecord[];
  /** Cumulative, not per-batch: messages the session had to drop to keep up. */
  dropped: number;
  ended?: boolean;
}

/** What `tailListen` hands back. Idempotent — calling it twice is harmless. */
export type TailUnlisten = UnlistenFn;

// ---------------------------------------------------------------------------
// UI-level connection state (not part of the IPC contract)
// ---------------------------------------------------------------------------

export type ConnStatus = "disconnected" | "connecting" | "connected";

export interface ConnState {
  status: ConnStatus;
  overview?: ClusterOverview;
  error?: string;
}

// ---------------------------------------------------------------------------
// Typed command wrappers (commands reject with a plain string message)
// ---------------------------------------------------------------------------

export function profilesList(): Promise<ConnectionProfile[]> {
  return invoke<ConnectionProfile[]>("profiles_list");
}

export function profilesSave(profile: ConnectionProfile): Promise<void> {
  return invoke<void>("profiles_save", { profile });
}

export function profilesDelete(profileId: string): Promise<void> {
  return invoke<void>("profiles_delete", { profileId });
}

/** Returns the export envelope as JSON. Secret-free: profiles carry refs. */
export function profilesExport(): Promise<string> {
  return invoke<string>("profiles_export");
}

export function profilesImport(
  json: string,
  strategy: ImportStrategy,
): Promise<ImportReport> {
  return invoke<ImportReport>("profiles_import", { json, strategy });
}

export function secretSet(entry: string, value: string): Promise<void> {
  return invoke<void>("secret_set", { entry, value });
}

export function secretDelete(entry: string): Promise<void> {
  return invoke<void>("secret_delete", { entry });
}

/**
 * Whether the keychain holds this entry. Presence only — the value stays on
 * the Rust side. A profile carries a SecretRef, never a value, so this is the
 * only honest way to say "leave it blank to keep the stored one".
 */
export function secretExists(entry: string): Promise<boolean> {
  return invoke<boolean>("secret_exists", { entry });
}

export function clusterConnect(profileId: string): Promise<ClusterOverview> {
  return invoke<ClusterOverview>("cluster_connect", { profileId });
}

export function clusterDisconnect(profileId: string): Promise<void> {
  return invoke<void>("cluster_disconnect", { profileId });
}

export function topicsList(profileId: string): Promise<TopicInfo[]> {
  return invoke<TopicInfo[]>("topics_list", { profileId });
}

export function coreVersion(): Promise<string> {
  return invoke<string>("core_version");
}

// ── Topics ─────────────────────────────────────────────────────────────────

export function topicDetail(
  profileId: string,
  topic: string,
): Promise<TopicDetail> {
  return invoke<TopicDetail>("topic_detail", { profileId, topic });
}

/** Mutating: the core rejects this on a read-only connection. */
export function topicCreate(
  profileId: string,
  name: string,
  partitions: number,
  replicationFactor: number,
  configs: TopicConfigInput[],
): Promise<void> {
  return invoke<void>("topic_create", {
    profileId,
    name,
    partitions,
    replicationFactor,
    configs,
  });
}

/** Mutating: the core rejects this on a read-only connection. */
export function topicDelete(profileId: string, topic: string): Promise<void> {
  return invoke<void>("topic_delete", { profileId, topic });
}

// ── Consumer groups ────────────────────────────────────────────────────────

export function groupsList(profileId: string): Promise<GroupInfo[]> {
  return invoke<GroupInfo[]>("groups_list", { profileId });
}

export function groupDetail(
  profileId: string,
  groupId: string,
): Promise<GroupDetail> {
  return invoke<GroupDetail>("group_detail", { profileId, groupId });
}

/**
 * Mutating, and guarded twice: the core refuses unless the group is Empty or
 * `spec.force` is set, and refuses outright on a read-only connection.
 * Resolves with the offsets as they stand AFTER the reset.
 */
export function offsetsReset(
  profileId: string,
  spec: OffsetResetSpec,
): Promise<GroupOffset[]> {
  return invoke<GroupOffset[]>("offsets_reset", { profileId, spec });
}

// ── Messages ───────────────────────────────────────────────────────────────

export function messagesFetch(
  profileId: string,
  spec: FetchSpec,
): Promise<MessageRecord[]> {
  return invoke<MessageRecord[]>("messages_fetch", { profileId, spec });
}

/** Resolves with the tail id the batches will be addressed to. */
export function tailStart(
  profileId: string,
  topic: string,
  partitions: number[] | null,
): Promise<string> {
  return invoke<string>("tail_start", { profileId, topic, partitions });
}

/** Idempotent on the Rust side — stopping a dead session is not an error. */
export function tailStop(tailId: string): Promise<void> {
  return invoke<void>("tail_stop", { tailId });
}

/** The event name a tail session's batches arrive on. */
export function tailEventName(tailId: string): string {
  return `kavka://tail/${tailId}`;
}

/**
 * Subscribe to one tail session's batches.
 *
 * The event is emitted to ALL windows, so the tail id in the name is what
 * keeps two browsers of the same topic apart. Resolves with the unlisten
 * function — call it, always: an orphaned listener holds the callback (and
 * whatever it closes over) alive for the life of the window.
 *
 * Prefer `tailSubscribe` from a component: registering a listener is async, so
 * an unmount that happens before it resolves has nothing to cancel yet.
 */
export function tailListen(
  tailId: string,
  cb: (payload: TailPayload) => void,
): Promise<TailUnlisten> {
  return listen<TailPayload>(tailEventName(tailId), (event) => cb(event.payload));
}

/**
 * `tailListen` with the unmount race closed: returns a synchronous cancel that
 * works whether or not the listener has finished registering. `onError` fires
 * if the subscription itself fails — a tail whose batches never arrive must
 * not look like a quiet topic.
 */
export function tailSubscribe(
  tailId: string,
  cb: (payload: TailPayload) => void,
  onError?: (message: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: TailUnlisten | null = null;
  void tailListen(tailId, (payload) => {
    if (!cancelled) cb(payload);
  })
    .then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    })
    .catch((err: unknown) => {
      if (!cancelled) onError?.(errorMessage(err));
    });
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

/** Normalize a rejected invoke value (string per contract, but be defensive). */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

// ---------------------------------------------------------------------------
// Phase 2 — search, produce, export
//
// Same contract shape as Phase 1: snake_case struct fields, enums tagged
// { "kind": … }, and camelCase ONLY in invoke()'s argument keys.
//
// Two sessions live here — search and bulk produce — and both follow the tail
// session's pattern exactly: `*_start` resolves with an id, events are
// addressed by that id, `*_stop` is idempotent, and the subscribe helper hands
// back a SYNCHRONOUS cancel so an unmount that lands before the listener
// finishes registering still has something to call.
// ---------------------------------------------------------------------------

/**
 * What a message has to match. Both null matches everything; both set is an
 * AND — the substring is a raw-byte prefilter the core runs BEFORE it decodes
 * anything, and the CEL program runs after, against the decoded record.
 */
export interface SearchQuery {
  substring: string | null;
  cel: string | null;
}

export interface SearchSpec {
  topic: string;
  seek: SeekSpec;
  /** null = every partition. */
  partitions: number[] | null;
  query: SearchQuery;
  /**
   * The RESULT buffer, not a scan limit. The core caps it at
   * MAX_SEARCH_BUFFERED; matching never stops there, and `matched` keeps
   * climbing after the buffer is full — which is the whole reason the UI can
   * say "the first 10 000 of N" honestly instead of "10 000".
   */
  max_buffered: number;
  /** Display truncation only; null uses the core default (256 KB). */
  max_value_bytes: number | null;
}

/** The core's own ceiling on a search's result buffer. Mirrored so the UI can say so. */
export const MAX_SEARCH_BUFFERED = 10000;

export interface PartitionProgress {
  partition: number;
  current_offset: number;
  end_offset: number;
}

export interface SearchProgress {
  scanned: number;
  matched: number;
  /** How many of `matched` are actually on this side of the IPC. */
  buffered: number;
  /**
   * Records the CEL expression could not be evaluated against — a field the
   * record doesn't have, or an answer that wasn't true/false. Not matched, and
   * NOT a failed search: a topic holding two shapes of message is the normal
   * case. Shown, though, because "0 matches" and "0 matches, 12 000 records
   * this expression couldn't read" are different answers.
   */
  unevaluated: number;
  per_partition: PartitionProgress[];
  /**
   * Partitions that finished because nothing more arrived, rather than because
   * they reached the end offset captured when the search started. Transactional
   * markers are the usual explanation; a broker that stopped answering is the
   * other one, which is why this is surfaced rather than assumed away.
   */
  assumed_complete: number[];
  msgs_per_sec: number;
  done: boolean;
  /** Set on the final progress when the search died rather than finished. */
  error: string | null;
  /** The first evaluation failure, verbatim — one example for `unevaluated`. */
  filter_error: string | null;
}

/** One batch of matches. Only emitted while `buffered < max_buffered`. */
export interface SearchResultsPayload {
  records: MessageRecord[];
}

export interface SearchHandlers {
  onResults: (payload: SearchResultsPayload) => void;
  onProgress: (progress: SearchProgress) => void;
}

/** Resolves with the search id its results and progress are addressed to. */
export function searchStart(
  profileId: string,
  spec: SearchSpec,
): Promise<string> {
  return invoke<string>("search_start", { profileId, spec });
}

/** Idempotent on the Rust side — stopping a finished search is not an error. */
export function searchStop(searchId: string): Promise<void> {
  return invoke<void>("search_stop", { searchId });
}

/**
 * THE OTHER HALF OF THE SUBSCRIBE HANDSHAKE. Call it the moment the listeners
 * for a session are registered.
 *
 * A session's events are addressed to an id that does not exist until
 * `*_start` resolves, so there is a window in which the Rust side could emit
 * into nothing — and a lost `done` is a progress bar that never finishes. The
 * shell holds every session's first event until this arrives (or for three
 * seconds, whichever comes first), which turns "probably subscribed by now"
 * into "subscribed".
 *
 * Idempotent, and an unknown id is not an error: a session that already started
 * emitting has nothing left to release.
 */
export function sessionReady(sessionId: string): Promise<void> {
  return invoke<void>("session_ready", { sessionId });
}

export function searchResultsEventName(searchId: string): string {
  return `kavka://search/${searchId}/results`;
}

export function searchProgressEventName(searchId: string): string {
  return `kavka://search/${searchId}/progress`;
}

/**
 * Subscribe to one search's two channels and get ONE unlisten back.
 *
 * Both events go to every window, so the id in the name is what keeps two
 * searches of the same topic apart. The returned function drops both
 * listeners; a half-unsubscribed search is a callback (and everything it
 * closes over) alive for the life of the window.
 */
export async function searchListen(
  searchId: string,
  handlers: SearchHandlers,
): Promise<UnlistenFn> {
  const [offResults, offProgress] = await Promise.all([
    listen<SearchResultsPayload>(searchResultsEventName(searchId), (event) =>
      handlers.onResults(event.payload),
    ),
    listen<SearchProgress>(searchProgressEventName(searchId), (event) =>
      handlers.onProgress(event.payload),
    ),
  ]);
  return () => {
    offResults();
    offProgress();
  };
}

/**
 * `searchListen` with the unmount race closed — see `tailSubscribe`, which
 * this mirrors deliberately. `onError` fires if the subscription itself fails,
 * because a search whose results never arrive must not read as "no matches".
 *
 * It also closes the OTHER race, in the other direction: `sessionReady` is
 * called here, the instant both listeners are registered, so the shell can stop
 * holding the session's events. It lives in this helper rather than at the call
 * site precisely because a caller who forgets it gets a search that looks
 * subscribed and silently loses its first three seconds.
 */
export function searchSubscribe(
  searchId: string,
  handlers: SearchHandlers,
  onError?: (message: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | null = null;
  void searchListen(searchId, {
    onResults: (payload) => {
      if (!cancelled) handlers.onResults(payload);
    },
    onProgress: (progress) => {
      if (!cancelled) handlers.onProgress(progress);
    },
  })
    .then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      // Failing to confirm is not worth surfacing: the shell's own timeout
      // starts the session anyway, which is exactly what this call would have
      // done sooner.
      void sessionReady(searchId).catch(() => undefined);
    })
    .catch((err: unknown) => {
      if (!cancelled) onError?.(errorMessage(err));
    });
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

// ── Produce ────────────────────────────────────────────────────────────────

/**
 * What to put in the value. `avro` encodes `json` against the latest schema
 * registered for `subject` in the profile's Schema Registry — so it is only
 * offered when the connection actually has one configured.
 */
export type ProduceValueSpec =
  | { kind: "text"; text: string }
  | { kind: "json"; json: JsonValue }
  | { kind: "avro"; subject: string; json: JsonValue };

export interface ProduceHeaderInput {
  key: string;
  value: string;
}

export interface ProduceRecordSpec {
  key: string | null;
  /** null = a tombstone: a record with no value at all, not an empty one. */
  value: ProduceValueSpec | null;
  headers: ProduceHeaderInput[];
  /** null = let Kafka choose (by key hash, or round-robin without a key). */
  partition: number | null;
}

/** Where the broker actually put it. */
export interface ProduceAck {
  partition: number;
  offset: number;
}

/** Mutating: the core rejects this on a read-only connection. */
export function produceSend(
  profileId: string,
  topic: string,
  record: ProduceRecordSpec,
): Promise<ProduceAck> {
  return invoke<ProduceAck>("produce_send", { profileId, topic, record });
}

export interface BulkSpec {
  /** The core caps this at MAX_BULK_COUNT. */
  count: number;
  interval_ms: number;
  key_template: string | null;
  value_template: string;
  partition: number | null;
}

/** The core's own ceiling on one bulk run. Mirrored so the UI can say so. */
export const MAX_BULK_COUNT = 100000;

export interface BulkPayload {
  sent: number;
  failed: number;
  done: boolean;
  error: string | null;
}

/** Mutating. Resolves with the bulk id its progress is addressed to. */
export function produceBulk(
  profileId: string,
  topic: string,
  spec: BulkSpec,
): Promise<string> {
  return invoke<string>("produce_bulk", { profileId, topic, spec });
}

/** Idempotent on the Rust side. */
export function bulkStop(bulkId: string): Promise<void> {
  return invoke<void>("bulk_stop", { bulkId });
}

export function bulkEventName(bulkId: string): string {
  return `kavka://bulk/${bulkId}`;
}

export function bulkListen(
  bulkId: string,
  cb: (payload: BulkPayload) => void,
): Promise<UnlistenFn> {
  return listen<BulkPayload>(bulkEventName(bulkId), (event) =>
    cb(event.payload),
  );
}

/**
 * `bulkListen` with the unmount race closed, and the subscribe handshake
 * confirmed the moment the listener is up — see `searchSubscribe`. It matters
 * more here: a short run at interval 0 can be finished before the panel has
 * subscribed, and a lost `done` leaves the panel counting forever.
 */
export function bulkSubscribe(
  bulkId: string,
  cb: (payload: BulkPayload) => void,
  onError?: (message: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | null = null;
  void bulkListen(bulkId, (payload) => {
    if (!cancelled) cb(payload);
  })
    .then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      void sessionReady(bulkId).catch(() => undefined);
    })
    .catch((err: unknown) => {
      if (!cancelled) onError?.(errorMessage(err));
    });
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

// ── Export ─────────────────────────────────────────────────────────────────

export type ExportFormat = "csv" | "json" | "ndjson";

/**
 * The shell writes the file; the UI only supplies a path the user picked in
 * the OS save dialog. Records go over IPC as they are — what you exported is
 * exactly what was on screen, cap and all.
 */
export function exportRecords(
  path: string,
  format: ExportFormat,
  records: MessageRecord[],
): Promise<void> {
  return invoke<void>("export_records", { path, format, records });
}

/**
 * The same write, for a table that is NOT a list of messages — a SQL result
 * set, whose columns are whatever the query asked for.
 *
 * A SECOND COMMAND RATHER THAN A WIDER `export_records`, because a result row
 * is not a record: `export_records` takes `MessageRecord[]` and knows its six
 * columns by name, while a result set's schema is whatever the query asked
 * for. The webview has no filesystem capability of its own (only the save
 * dialog), so serialising here and writing there is not an option either —
 * the shell writes both, and both go through the same RFC 4180 quoting.
 *
 * `columns` carries the header row so the shell never has to guess names from
 * the first row's keys: a result set with zero rows still has a schema, and a
 * CSV of it is a header line rather than an empty file. `rows` are positional
 * against that list, exactly as they arrive on `kavka://sql/{id}/rows`.
 */
export function exportRows(
  path: string,
  format: ExportFormat,
  columns: SqlColumn[],
  rows: JsonValue[][],
): Promise<void> {
  return invoke<void>("export_rows", { path, format, columns, rows });
}

export interface SaveDialogFilter {
  name: string;
  /** Extensions WITHOUT the dot, as the plugin wants them. */
  extensions: string[];
}

/**
 * The OS save dialog, as one wrapper so no component imports the plugin.
 *
 * Resolves with null when the user cancels — which is not an error and must
 * not raise anything. It rejects only if the dialog itself could not be shown
 * (the plugin missing from the Rust side, or its capability not granted).
 */
export function saveDialog(options: {
  title?: string;
  defaultPath?: string;
  filters?: SaveDialogFilter[];
}): Promise<string | null> {
  return save(options);
}

// ---------------------------------------------------------------------------
// Phase 3a — ACLs, broker configs, Kafka Connect, Schema Registry writes
//
// Same contract shape as every phase before it: snake_case struct fields, and
// camelCase ONLY in invoke()'s argument keys.
//
// EVERY MUTATING COMMAND HERE GOES THROUGH A CORE FUNCTION THAT CALLS
// ensure_writable FIRST. The disabled-with-a-reason controls in the UI are a
// courtesy so nobody clicks into a refusal; they are never the enforcement,
// and a read-only connection refuses these even if a caller forgets.
// ---------------------------------------------------------------------------

// ── ACLs ───────────────────────────────────────────────────────────────────

/**
 * The four resource types Kavka's editor can WRITE.
 *
 * Kafka itself has more (delegation tokens, users), and a cluster can hold
 * bindings for them, so every field on the wire type below is a plain `string`
 * and every label lookup falls back to the raw value. A rule Kavka can't name
 * is still a rule the user has to be able to see — the same reason internal
 * topics are dimmed rather than hidden.
 */
export type AclResourceType =
  | "topic"
  | "group"
  | "cluster"
  | "transactional_id";

/** `literal` matches one name; `prefixed` matches every name starting with it. */
export type AclPatternType = "literal" | "prefixed";

export type AclPermission = "allow" | "deny";

/** Kafka's operation names, lowercased. Order is the order the picker shows. */
export const ACL_OPERATIONS = [
  "read",
  "write",
  "create",
  "delete",
  "alter",
  "describe",
  "describe_configs",
  "alter_configs",
  "cluster_action",
  "idempotent_write",
  "all",
] as const;

export type AclOperation = (typeof ACL_OPERATIONS)[number];

/**
 * One access rule, exactly as Kafka stores it. Seven fields, all present:
 * this is a binding, not a filter, so nothing here is "any".
 */
export interface AclBinding {
  /** One of AclResourceType, but the cluster may hold others — keep it open. */
  resource_type: string;
  /** `kafka-cluster` for a cluster-scoped rule; Kafka's own literal. */
  resource_name: string;
  pattern_type: string;
  /** `User:alice` — the prefix is part of what Kafka stores. */
  principal: string;
  /** `*` means any host. */
  host: string;
  operation: string;
  permission: string;
}

/** What to narrow the list to. Every field null lists everything. */
export interface AclFilter {
  resource_type: string | null;
  resource_name: string | null;
  principal: string | null;
}

export function aclsList(
  profileId: string,
  filter: AclFilter,
): Promise<AclBinding[]> {
  return invoke<AclBinding[]>("acls_list", { profileId, filter });
}

/** Mutating: the core rejects this on a read-only connection. */
export function aclsCreate(
  profileId: string,
  bindings: AclBinding[],
): Promise<void> {
  return invoke<void>("acls_create", { profileId, bindings });
}

/**
 * Mutating. Resolves with the bindings that were ACTUALLY removed, which is
 * how the UI can tell "gone" from "was already gone" — an empty array means
 * the rule had been deleted elsewhere and this click did nothing.
 */
export function aclsDelete(
  profileId: string,
  filter: AclBinding,
): Promise<AclBinding[]> {
  return invoke<AclBinding[]>("acls_delete", { profileId, filter });
}

// ── Broker configuration ───────────────────────────────────────────────────

/** Reuses Phase 1's ConfigEntry verbatim — a broker setting is a setting. */
export function brokerConfigs(
  profileId: string,
  brokerId: number,
): Promise<ConfigEntry[]> {
  return invoke<ConfigEntry[]>("broker_configs", { profileId, brokerId });
}

/**
 * Mutating, and INCREMENTAL: only this key is touched, and `null` deletes the
 * dynamic override so the broker falls back to whatever it computes as the
 * default. Passing `null` is "revert", never "set to empty".
 */
export function brokerConfigSet(
  profileId: string,
  brokerId: number,
  name: string,
  value: string | null,
): Promise<void> {
  return invoke<void>("broker_config_set", {
    profileId,
    brokerId,
    name,
    value,
  });
}

// ── Kafka Connect ──────────────────────────────────────────────────────────

/** One task of one connector. `trace` is the worker's raw stack trace. */
export interface TaskStatus {
  id: number;
  /** RUNNING · PAUSED · FAILED · UNASSIGNED · RESTARTING, as the worker says. */
  state: string;
  worker_id: string;
  trace: string | null;
}

export interface ConnectorSummary {
  name: string;
  connector_state: string;
  worker_id: string;
  /** `source` (into Kafka) or `sink` (out of Kafka). */
  connector_type: string;
  tasks: TaskStatus[];
}

export function connectList(
  profileId: string,
  cluster: string,
): Promise<ConnectorSummary[]> {
  return invoke<ConnectorSummary[]>("connect_list", { profileId, cluster });
}

/** The connector's flattened config, exactly as the worker holds it. */
export function connectConfig(
  profileId: string,
  cluster: string,
  name: string,
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("connect_config", {
    profileId,
    cluster,
    name,
  });
}

/** One field of a validation response. `errors` is empty when it is fine. */
export interface ConfigValidationEntry {
  name: string;
  value: string | null;
  errors: string[];
  required: boolean;
  documentation: string | null;
}

/**
 * What the WORKER thinks of a config, which is not the same as what it will
 * think in a minute: validation runs against the plugin's config-def on the
 * worker that answered, so it is a snapshot and the UI says so before it lets
 * anyone act on a clean result.
 */
export interface ConfigValidation {
  error_count: number;
  configs: ConfigValidationEntry[];
}

export function connectValidate(
  profileId: string,
  cluster: string,
  connectorClass: string,
  config: Record<string, string>,
): Promise<ConfigValidation> {
  return invoke<ConfigValidation>("connect_validate", {
    profileId,
    cluster,
    connectorClass,
    config,
  });
}

/** Mutating. PUT config — creates the connector, or updates it in place. */
export function connectApply(
  profileId: string,
  cluster: string,
  name: string,
  config: Record<string, string>,
): Promise<void> {
  return invoke<void>("connect_apply", { profileId, cluster, name, config });
}

/** Mutating: the core rejects this on a read-only connection. */
export function connectDelete(
  profileId: string,
  cluster: string,
  name: string,
): Promise<void> {
  return invoke<void>("connect_delete", { profileId, cluster, name });
}

/**
 * Mutating. `task === null` restarts the connector itself; `includeTasks`
 * carries its tasks with it. A single task id restarts exactly that task.
 */
export function connectRestart(
  profileId: string,
  cluster: string,
  name: string,
  task: number | null,
  includeTasks: boolean,
): Promise<void> {
  return invoke<void>("connect_restart", {
    profileId,
    cluster,
    name,
    task,
    includeTasks,
  });
}

/** Mutating: the core rejects this on a read-only connection. */
export function connectPause(
  profileId: string,
  cluster: string,
  name: string,
): Promise<void> {
  return invoke<void>("connect_pause", { profileId, cluster, name });
}

/** Mutating: the core rejects this on a read-only connection. */
export function connectResume(
  profileId: string,
  cluster: string,
  name: string,
): Promise<void> {
  return invoke<void>("connect_resume", { profileId, cluster, name });
}

// ── Schema Registry — versions, compatibility, register ────────────────────

/** The registry's own wire spelling. `AVRO` is the default when it omits it. */
export type SchemaType = "AVRO" | "JSON" | "PROTOBUF";

export interface SubjectVersion {
  subject: string;
  version: number;
  schema_id: number;
  /** One of SchemaType, but a registry may serve others — keep it open. */
  schema_type: string;
  /** The schema text itself, verbatim. */
  schema: string;
}

/** The registry's verdict, and its own words for why. */
export interface CompatResult {
  compatible: boolean;
  /** Empty on a pass. On a failure these are SR's messages, not Kavka's. */
  messages: string[];
}

export interface RegisteredSchema {
  schema_id: number;
}

/** The seven levels a Confluent-compatible registry accepts. */
export const COMPAT_LEVELS = [
  "BACKWARD",
  "BACKWARD_TRANSITIVE",
  "FORWARD",
  "FORWARD_TRANSITIVE",
  "FULL",
  "FULL_TRANSITIVE",
  "NONE",
] as const;

export type CompatLevel = (typeof COMPAT_LEVELS)[number];

export function srSubjectVersions(
  profileId: string,
  subject: string,
): Promise<SubjectVersion[]> {
  return invoke<SubjectVersion[]>("sr_subject_versions", {
    profileId,
    subject,
  });
}

export function srCheckCompat(
  profileId: string,
  subject: string,
  schema: string,
  schemaType: string,
): Promise<CompatResult> {
  return invoke<CompatResult>("sr_check_compat", {
    profileId,
    subject,
    schema,
    schemaType,
  });
}

/**
 * Mutating. The core checks compatibility itself before it registers, and
 * surfaces the registry's own refusal verbatim — so a UI that skipped the
 * check button still cannot sneak an incompatible schema past the level.
 */
export function srRegister(
  profileId: string,
  subject: string,
  schema: string,
  schemaType: string,
): Promise<RegisteredSchema> {
  return invoke<RegisteredSchema>("sr_register", {
    profileId,
    subject,
    schema,
    schemaType,
  });
}

/**
 * The compatibility level in force, and whether the subject merely inherits it.
 *
 * `inherited` is the difference between "this subject is set to BACKWARD" and
 * "this subject follows a registry default that happens to be BACKWARD" — two
 * different facts, with two different consequences for setting a level here.
 * A bare level string cannot express it, so this is an object.
 */
export interface CompatLevelInForce {
  level: string;
  /** true = the subject has no setting of its own and follows the global one. */
  inherited: boolean;
}

/**
 * `subject: null` reads the registry-wide default, which comes back with
 * `inherited: false` — it inherits from nothing.
 */
export function srGetCompat(
  profileId: string,
  subject: string | null,
): Promise<CompatLevelInForce> {
  return invoke<CompatLevelInForce>("sr_get_compat", { profileId, subject });
}

/** Mutating. `subject: null` sets the registry-wide default. */
export function srSetCompat(
  profileId: string,
  subject: string | null,
  level: string,
): Promise<void> {
  return invoke<void>("sr_set_compat", { profileId, subject, level });
}

// ---------------------------------------------------------------------------
// Phase 3b — the KRaft quorum, leader election, replica moves, quotas
//
// Same contract shape as every phase before it: snake_case struct fields, and
// camelCase ONLY in invoke()'s argument keys.
//
// These five answer over hand-rolled Kafka protocol frames rather than through
// librdkafka's admin surface (docs/ARCHITECTURE.md D2 — ApiKeys 43, 45, 46,
// 48/49 and 55). That is deliberately invisible from here: a command is a
// command, the UI never learns which transport answered it, and a broker error
// arrives already mapped into the same classified vocabulary as everything
// else — so `classifyError` is still the only thing that reads a raw string.
//
// EVERY MUTATING COMMAND HERE — elect_leaders, reassign_alter, reassign_cancel,
// quotas_alter — GOES THROUGH ensure_writable IN THE CORE, BEFORE ANY NETWORK.
// The disabled-with-a-reason controls in the UI are a courtesy so nobody clicks
// into a refusal; they are never the enforcement.
// ---------------------------------------------------------------------------

// ── The quorum (ApiKey 55, DescribeQuorum — controller-bound) ───────────────

/**
 * One replica of the metadata log, as the quorum leader sees it.
 *
 * Both ages are how long ago, in milliseconds, measured by the leader — not
 * timestamps, so no clock on this machine is involved. `null` means the leader
 * did not say, which is a different fact from "a long time ago" and is
 * rendered as `∅` rather than as a large number.
 */
export interface ReplicaState {
  replica_id: number;
  log_end_offset: number;
  /** How long since the leader last heard from this replica at all. */
  last_fetch_age_ms: number | null;
  /** How long since this replica was last level with the leader. */
  last_caught_up_age_ms: number | null;
}

/**
 * The KRaft quorum. `voters` elect the leader among themselves; `observers`
 * copy the metadata log without ever voting — brokers that aren't controllers.
 *
 * A cluster that still runs on ZooKeeper has no quorum to describe and the
 * core says so; see QuorumPanel, which turns that refusal into a sentence
 * rather than an error.
 */
export interface QuorumInfo {
  leader_id: number;
  leader_epoch: number;
  high_watermark: number;
  voters: ReplicaState[];
  observers: ReplicaState[];
}

export function quorumDescribe(profileId: string): Promise<QuorumInfo> {
  return invoke<QuorumInfo>("quorum_describe", { profileId });
}

// ── Leader election + replica moves (ApiKeys 43, 45, 46) ───────────────────

/**
 * What happened to ONE partition inside a batch request.
 *
 * `error: null` means it worked. A non-null error is per-partition and never
 * fails the whole call: half a reassignment being accepted is a real state the
 * cluster can be in, and hiding it behind a single rejected promise would lose
 * exactly the half the user needs to see.
 *
 * WHAT `error: null` DOES NOT SAY. The core folds Kafka's "the end state you
 * asked for already holds" codes — ELECTION_NOT_NEEDED for a partition already
 * led by its preferred replica, NO_REASSIGNMENT_IN_PROGRESS for a cancel with
 * nothing to cancel — into `error: null`, because they are successes and
 * rendering them as failures would light a healthy cluster up entirely red.
 * The cost is that `error: null` cannot distinguish "this changed" from "this
 * was already so", and nothing in the string recovers it: there is no string.
 *
 * So a caller that needs the difference takes a SNAPSHOT before it asks and
 * classifies against that — which partitions were off their preferred leader,
 * which were listed as moving — rather than reading the result text. See
 * `splitResults` in PartitionOps, whose `wasNoOp` argument is exactly that
 * snapshot, and its two callers.
 */
export interface PartitionResult {
  topic: string;
  partition: number;
  error: string | null;
}

/**
 * Mutating. Moves each partition's leadership to the first broker in its
 * replica list — the "preferred" replica — if that broker is in sync.
 *
 * `topic: null` means every eligible partition on the cluster; a topic with
 * `partitions: null` means every partition of that topic.
 */
export function electLeaders(
  profileId: string,
  topic: string | null,
  partitions: number[] | null,
): Promise<PartitionResult[]> {
  return invoke<PartitionResult[]>("elect_leaders", {
    profileId,
    topic,
    partitions,
  });
}

/** Where one partition's replicas should end up. Order matters: `replicas[0]`
    is the preferred leader, which is what a later election will move to. */
export interface ReassignmentSpec {
  topic: string;
  partition: number;
  replicas: number[];
}

/**
 * Mutating. Asks the cluster to move these partitions onto these brokers.
 *
 * It RESOLVES when the cluster has accepted the plan, not when the data has
 * moved — copying happens in the background and is watched through
 * `reassignList`. Nothing about the acceptance says how long that will take.
 */
export function reassignAlter(
  profileId: string,
  specs: ReassignmentSpec[],
): Promise<PartitionResult[]> {
  return invoke<PartitionResult[]>("reassign_alter", { profileId, specs });
}

/**
 * Mutating. Stops moves that are still in flight and puts each partition back
 * on the replicas it had. Whatever the new brokers had already copied is
 * discarded, which is why this asks first.
 */
export function reassignCancel(
  profileId: string,
  parts: TopicPartition[],
): Promise<PartitionResult[]> {
  return invoke<PartitionResult[]>("reassign_cancel", { profileId, parts });
}

/**
 * One partition mid-move. `replicas` is the union Kafka is holding right now:
 * `adding` are copying data in, `removing` will drop it once the copy is in
 * sync. A partition finishes by leaving this list entirely.
 */
export interface ReassignmentState {
  topic: string;
  partition: number;
  replicas: number[];
  adding: number[];
  removing: number[];
}

/**
 * `topic: null` lists every move in flight on the cluster.
 *
 * A NAMED topic is resolved to its partitions first — Kafka's request has no
 * "all partitions of this topic" shorthand, and an empty partition list asks
 * about none of them — so naming a topic that has been deleted REJECTS with
 * that name in the message rather than resolving to an empty list. Callers that
 * poll (ReassignMonitor) treat that like any other failure: they stop asking
 * and say so, which is the right answer for a topic that is no longer there.
 */
export function reassignList(
  profileId: string,
  topic: string | null,
): Promise<ReassignmentState[]> {
  return invoke<ReassignmentState[]>("reassign_list", { profileId, topic });
}

// ── Quotas (ApiKeys 48/49, Describe/AlterClientQuotas) ─────────────────────

/** The three entity types Kafka quotas can be attached to. */
export type QuotaEntityType = "user" | "client-id" | "ip";

/**
 * One half of a quota's address. `name: null` is Kafka's `<default>` entity —
 * the fallback that applies to everything of that type without a quota of its
 * own. It is a real, settable entity, not a missing value, so it is rendered
 * as a labelled default rather than as `∅`.
 */
export interface QuotaEntityPart {
  /** One of QuotaEntityType, but a broker may report others — keep it open. */
  entity_type: string;
  name: string | null;
}

/** One quota key and the number in force for it. Kafka's own units. */
export interface QuotaValue {
  key: string;
  value: number;
}

/**
 * Everything set on one entity. `entity` is a LIST because Kafka addresses a
 * quota by a combination — `(user alice, client-id svc-orders)` is a different
 * entity from `user alice`, with its own values.
 */
export interface QuotaEntity {
  entity: QuotaEntityPart[];
  values: QuotaValue[];
}

/** One change. `value: null` removes the key rather than setting it to zero —
    and zero is a real quota meaning "throttled to a standstill". */
export interface QuotaOp {
  key: string;
  value: number | null;
}

export function quotasList(profileId: string): Promise<QuotaEntity[]> {
  return invoke<QuotaEntity[]>("quotas_list", { profileId });
}

/** Mutating, and INCREMENTAL: keys not named in `ops` are left alone. */
export function quotasAlter(
  profileId: string,
  entity: QuotaEntityPart[],
  ops: QuotaOp[],
): Promise<void> {
  return invoke<void>("quotas_alter", { profileId, entity, ops });
}

// ---------------------------------------------------------------------------
// Phase 4 — lag history, broker metrics, alerts, share groups, Streams
//
// Same contract shape as every phase before it: snake_case struct fields, enums
// tagged { "kind": … }, and camelCase ONLY in invoke()'s argument keys.
//
// ONE THING IS GENUINELY NEW HERE, and it is worth naming: everything below is
// about time, and time is the one axis Kafka itself does not keep for us. A
// broker will tell you the lag right now; it will not tell you what the lag was
// an hour ago. So the history is Kavka's own — sampled while a connection is
// up, written to a local redb file, pruned at 7 days — and every screen that
// reads it has to say so, because a gap in a chart is a gap in Kavka's
// attendance record, not an outage on the cluster.
//
// NOTHING HERE MUTATES THE CLUSTER. Alert rules, channels, the sampler and the
// metrics scrape are all local observation, which is why they are the one
// surface a read-only connection is NOT blocked from — see AlertsTab, which
// says so on screen rather than leaving the divergence implicit.
// ---------------------------------------------------------------------------

// ── Lag history ────────────────────────────────────────────────────────────

/**
 * One partition's lag at one moment, as the sampler recorded it.
 *
 * `committed` and `lag` are null together and for the same reason as
 * `GroupOffset`: a group that has never committed for this partition has
 * nothing to subtract from. A null lag is NOT zero and must never be plotted
 * as zero — it is a gap in the line.
 */
export interface LagSample {
  ts_ms: number;
  group_id: string;
  topic: string;
  partition: number;
  committed: number | null;
  end_offset: number;
  lag: number | null;
}

/**
 * Lag history for one group, between two timestamps.
 *
 * `topic: null` is every topic the group has samples for. The core downsamples
 * to at most `maxPoints` PER PARTITION, and the point it keeps for each bucket
 * is the one with the HIGHEST lag — not the mean, not the last. That is the
 * whole reason this is safe to draw: a spike that lasted a single sample
 * survives every zoom level, where an average would erase it exactly when it
 * matters. Every chart drawn from this says so in its axis note.
 */
export function historyQuery(
  profileId: string,
  groupId: string,
  topic: string | null,
  fromMs: number,
  toMs: number,
  maxPoints: number,
): Promise<LagSample[]> {
  return invoke<LagSample[]>("history_query", {
    profileId,
    groupId,
    topic,
    fromMs,
    toMs,
    maxPoints,
  });
}

/**
 * Which groups this profile's store actually holds history for, and the window
 * each one covers.
 *
 * The two timestamps are what make an honest empty state possible: a group with
 * no samples in the last hour is a different fact from a group Kavka has never
 * seen, and a picker that cannot tell them apart offers a range that can only
 * come back empty.
 */
export interface HistoryGroup {
  group_id: string;
  first_ts_ms: number;
  last_ts_ms: number;
}

export function historyGroups(profileId: string): Promise<HistoryGroup[]> {
  return invoke<HistoryGroup[]>("history_groups", { profileId });
}

/**
 * What the sampler is doing right now for this profile.
 *
 * `last_error` is the sampler's own last failure, kept rather than thrown: the
 * sampler runs unattended, so an error nobody was watching for still has to be
 * findable afterwards. `running: false` on a connected profile is a fact the
 * Monitoring tab states plainly — a chart that simply stops is indistinguishable
 * from a cluster that went quiet.
 */
export interface SamplerStatus {
  running: boolean;
  interval_ms: number;
  last_sample_ms: number | null;
  last_error: string | null;
}

export function samplerStatus(profileId: string): Promise<SamplerStatus> {
  return invoke<SamplerStatus>("sampler_status", { profileId });
}

/** The core's floor and default for the sampler interval. Mirrored so the UI
    can refuse a value the core would refuse, in the form rather than after it. */
export const SAMPLER_MIN_MS = 5_000;
export const SAMPLER_DEFAULT_MS = 15_000;

/** How long the local store keeps history before pruning. Mirrored so the
    charts can say what "no data before this" actually means. */
export const HISTORY_RETENTION_DAYS = 7;

// ── Broker metrics (Prometheus / jmx_exporter scrape) ──────────────────────

/** One reading of one series. `value` is whatever the exporter published,
    already in the series' own unit — bytes, messages, partitions. */
export interface MetricPoint {
  ts_ms: number;
  value: number;
}

/**
 * THE FIXED SERIES VOCABULARY.
 *
 * Six names, aggregated cluster-level in v1. An exporter publishes hundreds of
 * metrics under names that differ between jmx_exporter configs, Redpanda and
 * Confluent — so the core maps them onto these six, and the UI never learns a
 * scrape's own spelling. A series the scrape did not expose is absent from
 * `metrics_status.series_available`, which is a different fact from a series
 * whose value is zero, and the two are rendered differently.
 */
export const METRIC_SERIES = [
  "bytes_in_per_sec",
  "bytes_out_per_sec",
  "messages_in_per_sec",
  "under_replicated_partitions",
  "offline_partitions",
  "log_size_bytes",
] as const;

export type MetricSeries = (typeof METRIC_SERIES)[number];

/**
 * The per-topic variant of a series, when the scrape exposes one:
 * `bytes_in_per_sec:topic:orders.v2`.
 *
 * The topic goes last and is not escaped — a Kafka topic name cannot contain a
 * colon, so nothing this scheme builds can be ambiguous, and `splitSeries`
 * recovers the two halves by taking the FIRST two segments rather than by
 * splitting the whole string.
 */
export function topicSeries(series: MetricSeries, topic: string): string {
  return `${series}:topic:${topic}`;
}

/** The inverse. `topic` is null for a cluster-level series. */
export function splitSeries(name: string): { base: string; topic: string | null } {
  const marker = ":topic:";
  const at = name.indexOf(marker);
  if (at < 0) return { base: name, topic: null };
  return { base: name.slice(0, at), topic: name.slice(at + marker.length) };
}

/**
 * One series over a window, downsampled to at most `maxPoints`.
 *
 * Unlike lag, a throughput bucket keeps its own natural summary rather than a
 * maximum — the core decides, and the UI does not pretend to know which. What
 * the UI DOES say is the bucket width, so nobody reads a 24-hour chart as if it
 * were a per-second one.
 */
export function metricsQuery(
  profileId: string,
  series: string,
  fromMs: number,
  toMs: number,
  maxPoints: number,
): Promise<MetricPoint[]> {
  return invoke<MetricPoint[]>("metrics_query", {
    profileId,
    series,
    fromMs,
    toMs,
    maxPoints,
  });
}

/**
 * Whether this profile has a metrics endpoint at all, whether it answered, and
 * what it published.
 *
 * FOUR STATES, NOT TWO. `configured: false` is "you never told Kavka where to
 * look" and gets a teaching screen; `configured: true, reachable: false` is a
 * connection problem and gets the endpoint's own error; reachable with an empty
 * `series_available` is an exporter that answered with nothing Kavka
 * recognised, which is a config problem on the exporter and not on the cluster.
 * Collapsing any of those into "no data" is how a user spends an afternoon
 * debugging the wrong machine.
 */
export interface MetricsStatus {
  configured: boolean;
  reachable: boolean;
  last_scrape_ms: number | null;
  last_error: string | null;
  series_available: string[];
}

export function metricsStatus(profileId: string): Promise<MetricsStatus> {
  return invoke<MetricsStatus>("metrics_status", { profileId });
}

// ── Alerts ─────────────────────────────────────────────────────────────────

/**
 * One rule Kavka watches for, tagged by kind like every other enum on the wire.
 *
 * `for_ms` is a DWELL, not a delay: the condition has to hold continuously for
 * that long before the rule fires. It is what separates "the lag crossed 10 000
 * for one sample during a rebalance" from "this application has been falling
 * behind for five minutes", and it is why `offline_partitions` has none — a
 * partition with no leader is not a condition anyone wants smoothed.
 */
export type AlertRule =
  | {
      kind: "lag_threshold";
      id: string;
      name: string;
      group_id: string;
      /** null = any topic this group reads. */
      topic: string | null;
      threshold: number;
      for_ms: number;
    }
  | { kind: "under_replicated"; id: string; name: string; for_ms: number }
  | { kind: "offline_partitions"; id: string; name: string }
  | {
      kind: "throughput_floor";
      id: string;
      name: string;
      /** One of METRIC_SERIES, or a per-topic variant. */
      series: string;
      below: number;
      for_ms: number;
    };

export type AlertKind = AlertRule["kind"];

/**
 * One firing, and its resolution if it has had one.
 *
 * `resolved_ms: null` means still firing — not "we lost track". The same event
 * arrives twice on the event channel, once on fire and once with `resolved_ms`
 * set, so a UI that only listens for fires shows conditions that cleared hours
 * ago as if they were live.
 */
export interface AlertEvent {
  rule_id: string;
  rule_name: string;
  fired_ms: number;
  resolved_ms: number | null;
  /** The numbers that tripped it, in Kavka's words. Shown verbatim. */
  detail: string;
}

export function alertsList(profileId: string): Promise<AlertRule[]> {
  return invoke<AlertRule[]>("alerts_list", { profileId });
}

/** Upsert by `rule.id`. Local only — never touches the cluster. */
export function alertsSave(profileId: string, rule: AlertRule): Promise<void> {
  return invoke<void>("alerts_save", { profileId, rule });
}

export function alertsDelete(profileId: string, ruleId: string): Promise<void> {
  return invoke<void>("alerts_delete", { profileId, ruleId });
}

/** Newest first. `limit` is a ceiling, not a promise of that many. */
export function alertsHistory(
  profileId: string,
  limit: number,
): Promise<AlertEvent[]> {
  return invoke<AlertEvent[]>("alerts_history", { profileId, limit });
}

/**
 * Where a firing goes. Stored per profile, because "notify me about prod" and
 * "don't notify me about my laptop's dev cluster" is the normal want.
 *
 * `webhook_is_slack` picks the BODY SHAPE, not the destination: Slack wants
 * `{"text": …}` and a generic endpoint wants Kavka's own JSON, and posting the
 * wrong one to the right URL fails silently at the far end — which is exactly
 * what the Test button exists to catch before an incident does.
 */
export interface AlertChannels {
  os_notification: boolean;
  webhook_url: string | null;
  webhook_is_slack: boolean;
}

export function alertsChannelsGet(profileId: string): Promise<AlertChannels> {
  return invoke<AlertChannels>("alerts_channels_get", { profileId });
}

export function alertsChannelsSet(
  profileId: string,
  channels: AlertChannels,
): Promise<void> {
  return invoke<void>("alerts_channels_set", { profileId, channels });
}

/**
 * Send one test firing through the configured channels.
 *
 * NOT IN THE PHASE 4 CONTRACT — flagged, not smuggled. The UI is specified to
 * carry a Test button beside the webhook, and there is no honest way to build
 * one on this side: a `fetch` from the webview would bypass the Rust side that
 * actually posts the alert, would be blocked by the app's CSP, and would prove
 * nothing about the request Kavka will really send. So the button calls this,
 * and until the core grows it the button surfaces the rejection like any other
 * failure rather than pretending the test passed.
 */
export function alertsChannelsTest(profileId: string): Promise<void> {
  return invoke<void>("alerts_channels_test", { profileId });
}

/** The event name a profile's alert firings and resolutions arrive on. */
export function alertsEventName(profileId: string): string {
  return `kavka://alerts/${profileId}`;
}

/**
 * Subscribe to one profile's alerts. Fires AND resolutions arrive here — the
 * same payload shape both times, with `resolved_ms` set on the second.
 *
 * The event is emitted to all windows, so the profile id in the name is what
 * keeps two clusters' alerts apart. Resolves with the unlisten function; call
 * it, always. Prefer `alertsSubscribe` from a component.
 */
export function alertsListen(
  profileId: string,
  cb: (event: AlertEvent) => void,
): Promise<UnlistenFn> {
  return listen<AlertEvent>(alertsEventName(profileId), (event) =>
    cb(event.payload),
  );
}

/**
 * `alertsListen` with the unmount race closed — see `tailSubscribe`, which this
 * mirrors deliberately. Returns a SYNCHRONOUS cancel that works whether or not
 * the listener has finished registering.
 *
 * No `sessionReady` handshake here, and that is the one difference worth
 * stating: alerts are not a session. There is no id that has to exist before
 * the first event can be addressed, and no first three seconds to lose — the
 * channel is named after the profile, which existed long before this listener
 * did. A missed alert is a real cost, so the subscription is set up when the
 * cluster workspace mounts rather than when the Alerts tab is opened.
 */
export function alertsSubscribe(
  profileId: string,
  cb: (event: AlertEvent) => void,
  onError?: (message: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | null = null;
  void alertsListen(profileId, (event) => {
    if (!cancelled) cb(event);
  })
    .then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    })
    .catch((err: unknown) => {
      if (!cancelled) onError?.(errorMessage(err));
    });
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

// ── Share groups (KIP-932) ─────────────────────────────────────────────────

/**
 * A share group, which is Kafka's queue-shaped consumer: members take
 * individual records rather than whole partitions, so two members can read the
 * same partition at once and each record is acknowledged on its own.
 *
 * A broker that does not have the feature refuses the call by name rather than
 * answering with an empty list, and the core passes that refusal through — see
 * ShareGroupsPanel, which turns it into a sentence about the broker's version
 * instead of a protocol error about an unknown API key.
 */
export interface ShareGroupInfo {
  group_id: string;
  state: string;
  member_count: number;
}

export function shareGroupsList(profileId: string): Promise<ShareGroupInfo[]> {
  return invoke<ShareGroupInfo[]>("share_groups_list", { profileId });
}

/**
 * One share group's members and where each partition's delivery starts.
 *
 * `start_offset` is NOT a committed offset and must not be labelled as one: a
 * share group acknowledges records individually, so what the broker keeps is
 * the point before which everything has been acknowledged. `null` means the
 * broker did not report one for that partition, which is a gap in the answer
 * rather than a zero.
 */
export interface ShareGroupMember {
  member_id: string;
  client_id: string;
  assignments: TopicPartition[];
}

export interface ShareGroupOffset {
  topic: string;
  partition: number;
  start_offset: number | null;
}

export interface ShareGroupDetail {
  group_id: string;
  state: string;
  members: ShareGroupMember[];
  offsets: ShareGroupOffset[];
}

export function shareGroupDetail(
  profileId: string,
  groupId: string,
): Promise<ShareGroupDetail> {
  return invoke<ShareGroupDetail>("share_group_detail", { profileId, groupId });
}

// ── Kafka Streams topology (INFERRED) ──────────────────────────────────────

/**
 * One box in the topology picture.
 *
 * `kind` is the only thing that makes a node readable — a repartition topic and
 * a source topic are both "a topic" to Kafka and completely different things to
 * whoever is debugging the application — so it drives shape and a word, never
 * colour alone.
 */
export interface TopologyNode {
  id: string;
  /** source_topic · sub_topology · repartition · changelog · sink_topic. */
  kind: string;
  label: string;
  /** The Kafka topics this node stands for. Literals, rendered mono. */
  topics: string[];
}

export interface TopologyEdge {
  from: string;
  to: string;
}

/**
 * What Kavka could work out about a Streams application's shape.
 *
 * `inferred` is literally `true` and is not negotiable, because the whole
 * object is a guess: Kafka does not expose a Streams topology anywhere. The
 * core reconstructs it from the group's subscriptions plus the internal-topic
 * naming convention (`<app-id>-…-repartition`, `<app-id>-…-changelog`), which
 * gets the boxes right and cannot get the processors inside them right.
 *
 * `caveats` names exactly what the inference cannot know, in Kavka's words, and
 * the view renders every one of them as an always-visible note. A topology
 * picture that looks authoritative and is a guess is worse than no picture.
 */
export interface StreamsTopology {
  app_id: string;
  nodes: TopologyNode[];
  edges: TopologyEdge[];
  inferred: true;
  caveats: string[];
}

export function streamsTopology(
  profileId: string,
  groupId: string,
): Promise<StreamsTopology> {
  return invoke<StreamsTopology>("streams_topology", { profileId, groupId });
}

// ---------------------------------------------------------------------------
// Phase 5a — SQL over topics, cross-cluster copy, config diff, offset
// migration, DLQ metadata
//
// Same contract shape as every phase before it: snake_case struct fields, enums
// tagged { "kind": … }, and camelCase ONLY in invoke()'s argument keys.
//
// TWO THINGS ARE GENUINELY NEW HERE, and both are worth naming.
//
// 1. A COMMAND CAN NOW NAME TWO CLUSTERS. Copy, config diff and offset
//    migration all take a SECOND profile id, and the second one is the one that
//    gets written to. Every guardrail in docs/DESIGN.md §6 is written about
//    "the cluster on screen", and on these three screens the cluster on screen
//    is the SOURCE while the danger is at the DESTINATION — so the UI reads the
//    destination profile's own environment and read-only flag, never the
//    workspace's. A prod destination gets the prod treatment even when you
//    started from dev.
//
// 2. SQL AND COPY ARE SESSIONS, exactly like search and bulk produce: `*_start`
//    resolves with an id, events are addressed by that id, `*_stop` is
//    idempotent, and the subscribe helper calls `session_ready` the moment the
//    listeners are registered so the shell can release the first event. There
//    is no new machinery here at all — see `searchSubscribe`, which these
//    mirror deliberately.
// ---------------------------------------------------------------------------

// ── SQL over a topic (DataFusion) ──────────────────────────────────────────

/**
 * One query, and the slice of the topic it is allowed to read.
 *
 * `scan_cap` and `max_rows` are TWO DIFFERENT CEILINGS and neither is the
 * other: the first is how many records Kavka will read off the topic, the
 * second is how many result rows it will hand back. A query that scans a
 * million records and returns four rows hits neither; `SELECT *` on a busy
 * topic hits both. Whichever bit, `SqlProgress.capped` says so — see there.
 */
export interface SqlSpec {
  topic: string;
  /** The SQL text, against the `messages` virtual table. */
  query: string;
  seek: SeekSpec;
  /** null = every partition. */
  partitions: number[] | null;
  /** Records READ. The core caps this at SQL_SCAN_CAP however large we ask. */
  scan_cap: number;
  /** Result ROWS. The core caps this at SQL_MAX_ROWS. */
  max_rows: number;
}

/** One column of a result set: the name the query gave it, and its type. */
export interface SqlColumn {
  name: string;
  /**
   * SQL SPELLING, not Arrow's — `BIGINT`, `VARCHAR`, `DOUBLE`, `TIMESTAMP`.
   * The core maps Arrow's own names (`Int64`, `Utf8`, …) through
   * `kavka_core::sql::sql_type_name` before this crosses IPC, because the
   * person reading a column header wrote SQL and has never seen `Utf8`.
   * Rendered verbatim.
   */
  data_type: string;
}

/**
 * How far the query has got, and whether the answer is the whole answer.
 *
 * `capped` IS THE HONESTY BIT, and it is the same rule as search's buffer
 * (§5.7, and this phase's "never silently truncates" gate): it is true when
 * either ceiling truncated the result, and the view that ignores it shows a
 * partial answer to an aggregate query as if it were the answer. A `COUNT(*)`
 * over a capped scan is not a count of the topic — it is a count of what was
 * read, and the difference is the whole reason this flag exists.
 */
export interface SqlProgress {
  /** Records read off the topic so far. */
  scanned: number;
  /** Result rows produced so far. */
  produced_rows: number;
  done: boolean;
  /** Set on the final progress when the query died rather than finished. */
  error: string | null;
  /** True when `scan_cap` or `max_rows` truncated the answer. NEVER silent. */
  capped: boolean;
}

/** The schema event: the columns, once, before any rows. */
export interface SqlSchemaPayload {
  columns: SqlColumn[];
}

/** One batch of result rows, positional against `SqlSchemaPayload.columns`. */
export interface SqlRowsPayload {
  rows: JsonValue[][];
}

/** The core's own ceiling on records SCANNED by one query. */
export const SQL_SCAN_CAP = 100000;

/** The core's own ceiling on result ROWS from one query. */
export const SQL_MAX_ROWS = 10000;

/**
 * THE VIRTUAL TABLE, mirrored from the core so one file states the schema and
 * the UI never invents a column.
 *
 * Every query runs against a table called `messages` with exactly these
 * columns. `value_json` is the payload as a **compact JSON string**, not a
 * parsed structure — and this build has no JSON functions to hand it to (see
 * SQL_SURFACE), so it is matched as text: compact is what makes
 * `'%"status":"failed"%'` a stable pattern, because there is no space after the
 * colon to guess at. `value_text` is the same payload rendered for reading.
 */
export const SQL_TABLE_NAME = "messages";

export const SQL_TABLE_COLUMNS: ReadonlyArray<
  SqlColumn & { nullable: boolean; what: string }
> = [
  {
    name: "partition",
    data_type: "BIGINT",
    nullable: false,
    what: "Which partition the record came from.",
  },
  {
    name: "offset",
    data_type: "BIGINT",
    nullable: false,
    what: "The record's offset within that partition.",
  },
  {
    name: "timestamp_ms",
    data_type: "BIGINT",
    nullable: true,
    what: "Epoch milliseconds, or NULL when the record carries no timestamp.",
  },
  {
    name: "key_text",
    data_type: "VARCHAR",
    nullable: true,
    what: "The decoded key, or NULL when the record has no key.",
  },
  {
    name: "value_text",
    data_type: "VARCHAR",
    nullable: true,
    what: "The decoded value as text — NULL for a tombstone.",
  },
  {
    name: "value_json",
    data_type: "VARCHAR",
    nullable: true,
    what: "The value as compact JSON text when it decoded to JSON, else NULL — matched with LIKE or regexp_like, not with JSON functions.",
  },
  {
    name: "headers_json",
    data_type: "VARCHAR",
    nullable: false,
    what: "Every header as a JSON object, encoded as a string.",
  },
];

/**
 * WHAT THE ENGINE ACTUALLY SUPPORTS, in the engine's own terms.
 *
 * Rendered verbatim under the editor. It is a constant rather than prose in a
 * component because it mirrors the core's documented surface: when the core's
 * DataFusion version grows a feature, this list is the one place that changes
 * and every screen quoting it changes with it. A cheatsheet that drifts from
 * the engine teaches expressions that fail.
 *
 * CONTRACT FRICTION — flagged, not resolved. The core owns the canonical text
 * (`kavka_core::sql::sql_surface()`, kept honest against the functions the
 * build actually registers by a test), and the Phase 5a IPC contract has no
 * command that returns it — so the canonical answer cannot be reached from
 * here and this list is a second copy of it. The moment the shell exposes that
 * function as a command, this constant should become a call to it and the text
 * should be rendered verbatim; until then the two are kept in step by hand,
 * which is exactly the arrangement the core's own comment warns about.
 */
export const SQL_SURFACE: readonly string[] = [
  "SELECT with WHERE, GROUP BY, HAVING, ORDER BY and LIMIT.",
  "The usual aggregates — count, sum, min, max, avg — and CASE expressions.",
  "String functions on the text columns: lower, upper, substr, length, position, and LIKE.",
  "No JSON functions — DataFusion ships none at all, so `value_json` is text and you match it as text: WHERE value_json LIKE '%\"status\":\"failed\"%', or regexp_like(value_json, '\"amount\":[0-9]{4,}'). It is COMPACT — no space after `:` or `,` — which is what makes those patterns stable.",
  "One table, always called `messages`. There is nothing to join to — a query only ever sees the records this scan read.",
  "No INSERT, UPDATE, DELETE or CREATE. SQL here reads a topic; producing is the Produce panel's job.",
];

/** Resolves with the sql id its schema, rows and progress are addressed to. */
export function sqlStart(profileId: string, spec: SqlSpec): Promise<string> {
  return invoke<string>("sql_start", { profileId, spec });
}

/** Idempotent on the Rust side — stopping a finished query is not an error. */
export function sqlStop(sqlId: string): Promise<void> {
  return invoke<void>("sql_stop", { sqlId });
}

export function sqlSchemaEventName(sqlId: string): string {
  return `kavka://sql/${sqlId}/schema`;
}

export function sqlRowsEventName(sqlId: string): string {
  return `kavka://sql/${sqlId}/rows`;
}

export function sqlProgressEventName(sqlId: string): string {
  return `kavka://sql/${sqlId}/progress`;
}

export interface SqlHandlers {
  onSchema: (payload: SqlSchemaPayload) => void;
  onRows: (payload: SqlRowsPayload) => void;
  onProgress: (progress: SqlProgress) => void;
}

/**
 * Subscribe to one query's three channels and get ONE unlisten back.
 *
 * Three rather than search's two, and the extra one has to be registered
 * FIRST-CLASS rather than folded into the rows event: the columns arrive once,
 * before any row, and a grid that learned its schema from the first batch would
 * render nothing at all for a query that matched nothing — which is exactly
 * when the column list is the only answer there is.
 */
export async function sqlListen(
  sqlId: string,
  handlers: SqlHandlers,
): Promise<UnlistenFn> {
  const [offSchema, offRows, offProgress] = await Promise.all([
    listen<SqlSchemaPayload>(sqlSchemaEventName(sqlId), (event) =>
      handlers.onSchema(event.payload),
    ),
    listen<SqlRowsPayload>(sqlRowsEventName(sqlId), (event) =>
      handlers.onRows(event.payload),
    ),
    listen<SqlProgress>(sqlProgressEventName(sqlId), (event) =>
      handlers.onProgress(event.payload),
    ),
  ]);
  return () => {
    offSchema();
    offRows();
    offProgress();
  };
}

/**
 * `sqlListen` with the unmount race closed and the subscribe handshake
 * confirmed — see `searchSubscribe`, which this mirrors deliberately.
 *
 * The handshake matters more here than anywhere else so far: the SCHEMA event
 * is emitted once and never again, so a query whose first event is lost has a
 * result set with no column names for the rest of its life.
 */
export function sqlSubscribe(
  sqlId: string,
  handlers: SqlHandlers,
  onError?: (message: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | null = null;
  void sqlListen(sqlId, {
    onSchema: (payload) => {
      if (!cancelled) handlers.onSchema(payload);
    },
    onRows: (payload) => {
      if (!cancelled) handlers.onRows(payload);
    },
    onProgress: (progress) => {
      if (!cancelled) handlers.onProgress(progress);
    },
  })
    .then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      void sessionReady(sqlId).catch(() => undefined);
    })
    .catch((err: unknown) => {
      if (!cancelled) onError?.(errorMessage(err));
    });
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

// ── Cross-cluster copy / replay ────────────────────────────────────────────

/**
 * One copy: from a topic on the connection this command is sent to, into a
 * topic on `dest_profile_id`.
 *
 * `dest_profile_id` may be the SAME profile — replaying a topic into another
 * topic on one cluster is the same operation and takes the same path. What it
 * may never be is implicit: the destination is always named, because "copy this
 * somewhere" with the somewhere inferred is how a replay lands on prod.
 */
export interface CopySpec {
  source_topic: string;
  dest_profile_id: string;
  dest_topic: string;
  seek: SeekSpec;
  /** null = every partition of the source. */
  partitions: number[] | null;
  /** null = copy everything in the range. Same shape search uses. */
  filter: SearchQuery | null;
  /** null = no ceiling beyond the range itself. */
  max_messages: number | null;
  /** null = as fast as the destination will take them. */
  rate_per_sec: number | null;
  /**
   * Write each record to the partition it came from, rather than letting the
   * destination's partitioner choose. Only honest when the destination has at
   * least as many partitions as the source — see CopyWizard, which says so.
   */
  preserve_partition: boolean;
  /** Add the `kavka.replay.source.*` headers to every copied record. */
  provenance_headers: boolean;
}

/**
 * What a copy would do, without doing any of it.
 *
 * `would_copy_estimate` is WATERMARK ARITHMETIC — end minus start across the
 * partitions in scope — so it is exact for an unfiltered copy and an UPPER
 * BOUND when `filter` is set: nothing can tell how many records match without
 * reading them, which is the copy itself. Every renderer of this number says
 * which of the two it is looking at.
 */
export interface CopyEstimate {
  would_copy_estimate: number;
  /**
   * CONTRACT FRICTION — flagged, not resolved, and consumed defensively.
   *
   * The Phase 5a contract fixes this payload as `{ would_copy_estimate,
   * per_partition }`; the core's `CopyEstimate` carries a third field saying
   * whether the number is a scan size rather than a match count — which is the
   * same honesty the UI was inferring for itself from "is a filter set". The
   * core's answer is better than the inference (it also reads high on a
   * transactional topic, where commit markers occupy offsets no consumer ever
   * receives), so the wizard prefers it and falls back to the inference when
   * the field is absent. Optional here rather than required for exactly that
   * reason: a build that answers with the contract's two fields must not make
   * the dry run unreadable.
   */
  estimate_only?: boolean;
  per_partition: PartitionProgress[];
}

/**
 * The `kavka.replay.source.*` headers a provenance-tagged copy carries.
 *
 * The timestamp is the only conditional one: a record written before KIP-32 has
 * none, and the core writes no header rather than inventing a value. Every
 * screen quoting this list says so.
 */
export const COPY_PROVENANCE_HEADERS: readonly string[] = [
  "kavka.replay.source.cluster",
  "kavka.replay.source.topic",
  "kavka.replay.source.partition",
  "kavka.replay.source.offset",
  "kavka.replay.source.ts",
];

/**
 * How the copy is going. `scanned` counts records READ from the source and
 * `copied` counts records the destination ACCEPTED — they differ whenever a
 * filter is set, and `failed` is the third number because a copy that lost
 * eleven records to the destination and says nothing is worse than one that
 * stops.
 */
export interface CopyProgress {
  copied: number;
  scanned: number;
  failed: number;
  done: boolean;
  error: string | null;
  /**
   * Partitions the copy finished because NOTHING MORE ARRIVED FROM THE SOURCE,
   * rather than because they reached the end offset captured when the copy
   * started. Ascending, and empty on a copy that read every window out.
   *
   * The same field, the same word and the same reason as
   * `SearchProgress.assumed_complete`: transactional markers take offsets a
   * consumer never receives, so the watermark cannot always be reached — and a
   * broker that stopped answering looks identical from here. A copy that moved
   * 900 of 1 000 records and reported `done` with nothing else to say would be
   * a silently truncated result wearing a success message, so the core names
   * the partitions and the panel repeats them.
   *
   * Time the copy spends waiting on the DESTINATION — the rate limit's pacing
   * gap, the in-flight drain — never counts towards that deadline, so a paced
   * or a slow copy takes longer and still reads everything.
   */
  assumed_complete: number[];
}

/** Read-only: nothing is written, and the destination is never contacted. */
export function copyDryRun(
  profileId: string,
  spec: CopySpec,
): Promise<CopyEstimate> {
  return invoke<CopyEstimate>("copy_dry_run", { profileId, spec });
}

/**
 * Mutating — AT THE DESTINATION. The core's `ensure_writable` runs against
 * `spec.dest_profile_id`, not against the profile this command is addressed
 * to, so a read-only destination refuses a copy started from a writable
 * source. Resolves with the copy id its progress is addressed to.
 */
export function copyStart(profileId: string, spec: CopySpec): Promise<string> {
  return invoke<string>("copy_start", { profileId, spec });
}

/** Idempotent on the Rust side. Records already accepted stay written. */
export function copyStop(copyId: string): Promise<void> {
  return invoke<void>("copy_stop", { copyId });
}

export function copyEventName(copyId: string): string {
  return `kavka://copy/${copyId}`;
}

export function copyListen(
  copyId: string,
  cb: (payload: CopyProgress) => void,
): Promise<UnlistenFn> {
  return listen<CopyProgress>(copyEventName(copyId), (event) =>
    cb(event.payload),
  );
}

/**
 * `copyListen` with the unmount race closed and the handshake confirmed — see
 * `bulkSubscribe`, which this mirrors for the same reason: a short copy can be
 * finished before the panel has subscribed, and a lost `done` leaves a progress
 * panel counting forever over a write that has already happened.
 */
export function copySubscribe(
  copyId: string,
  cb: (payload: CopyProgress) => void,
  onError?: (message: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | null = null;
  void copyListen(copyId, (payload) => {
    if (!cancelled) cb(payload);
  })
    .then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      void sessionReady(copyId).catch(() => undefined);
    })
    .catch((err: unknown) => {
      if (!cancelled) onError?.(errorMessage(err));
    });
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

// ── Config diff between two topics ─────────────────────────────────────────

/**
 * One setting, as both topics hold it.
 *
 * `a_is_default` / `b_is_default` are the difference between "staging sets
 * retention.ms to 7 days" and "staging inherits 7 days from its brokers" —
 * two facts with two different consequences for copying one to the other, and
 * DESIGN's config table already renders the second one muted. `differs` is the
 * core's own verdict rather than a string comparison here, so one rule decides
 * it for both sides.
 */
export interface ConfigDiffRow {
  name: string;
  /** null = this topic has no value for the setting at all. */
  a: string | null;
  b: string | null;
  a_is_default: boolean;
  b_is_default: boolean;
  differs: boolean;
}

/**
 * Compare one topic's configuration with another's, across two connections.
 *
 * V1 IS TOPICS ONLY. The wire type accepts `null` for either topic — a
 * broker-level diff — and the core rejects it by name; this wrapper does not
 * offer the shape at all, so the refusal can only ever be reached by a caller
 * bypassing it. Broker configs already have their own screen (BrokersTab).
 */
export function configDiff(
  profileIdA: string,
  topicA: string,
  profileIdB: string,
  topicB: string,
): Promise<ConfigDiffRow[]> {
  return invoke<ConfigDiffRow[]>("config_diff", {
    profileIdA,
    topicA,
    profileIdB,
    topicB,
  });
}

// ── Consumer-offset migration ──────────────────────────────────────────────

/**
 * How one partition's destination offset was arrived at.
 *
 *  - `timestamp`  — the source's committed offset had a timestamp, and the
 *                   destination's broker answered with the offset that time
 *                   maps to there. The only method that survives two clusters
 *                   whose offsets were never in step, which is all of them.
 *  - `earliest`   — no timestamp was available, so the destination starts at
 *                   the beginning of its own partition. Honest, and usually a
 *                   lot of re-processing.
 *  - `latest`     — the group was already caught up here (its offset is at or
 *                   past the source partition's end, or nothing readable
 *                   remains there), so it starts at the END of the destination
 *                   partition and waits for new records. A distinct fact from
 *                   all three others: reporting it as `timestamp` would claim a
 *                   lookup that never happened, and as `none` would claim there
 *                   was nothing to migrate when the group was in fact finished.
 *  - `none`       — nothing to migrate: the source group never committed here.
 */
export type OffsetMigrationMethod =
  | "timestamp"
  | "earliest"
  | "latest"
  | "none";

export interface OffsetMigrationRow {
  partition: number;
  /** null = the source group has never committed for this partition. */
  source_committed: number | null;
  /** The timestamp of the record at `source_committed`, if there was one. */
  source_ts_ms: number | null;
  /** Where the destination group would be put. null with `method: "none"`. */
  dest_offset: number | null;
  /** One of OffsetMigrationMethod — kept open in case the core adds one. */
  method: string;
}

/**
 * Read-only. Works out where each of the source group's partitions would land
 * on the destination, and says how it worked each one out.
 *
 * NOTHING IS WRITTEN and the plan is a snapshot: both clusters keep moving, so
 * a plan read an hour ago describes an hour ago. The wizard re-plans rather
 * than caching one.
 */
export function offsetsMigratePlan(
  profileId: string,
  groupId: string,
  topic: string,
  destProfileId: string,
  destGroupId: string,
  destTopic: string,
): Promise<OffsetMigrationRow[]> {
  return invoke<OffsetMigrationRow[]>("offsets_migrate_plan", {
    profileId,
    groupId,
    topic,
    destProfileId,
    destGroupId,
    destTopic,
  });
}

/**
 * Mutating, AT THE DESTINATION, and guarded with the same vocabulary as
 * `offsets_reset`: the core refuses unless the destination group is Empty, and
 * refuses outright on a read-only destination connection.
 *
 * There is no `force` here on purpose — see OffsetMigrateModal, which states
 * the Empty requirement before the button rather than after the refusal.
 * Resolves with the destination group's offsets as they stand AFTER the write.
 */
export function offsetsMigrateApply(
  destProfileId: string,
  destGroupId: string,
  destTopic: string,
  plan: OffsetMigrationRow[],
): Promise<GroupOffset[]> {
  return invoke<GroupOffset[]>("offsets_migrate_apply", {
    destProfileId,
    destGroupId,
    destTopic,
    // CONTRACT FRICTION — the contract names this argument "plan-rows" and the
    // core function's parameter is `plan`, so `plan` is the guess with two
    // votes. It is the one argument key in this file that has not been read
    // back off a registered command, because the shell has not registered one
    // yet: an invoke argument key must match the Rust parameter name exactly,
    // and a mismatch fails at the bridge with a message about a missing field
    // rather than anywhere near this line.
    plan,
  });
}

// ── Dead letter queues ─────────────────────────────────────────────────────

/**
 * Which convention Kavka recognised in a record's headers.
 *
 * `none` never reaches the UI — a record whose headers match nothing carries no
 * `dlq` at all — but the variant exists because the core's own function is
 * total, and a UI that pattern-matches on the string must not be surprised by
 * it.
 */
export type DlqConvention = "connect" | "spring" | "none";

/**
 * Where a dead-lettered record came from and what killed it, as the framework
 * that wrote it recorded.
 *
 * EVERY FIELD IS OPTIONAL BECAUSE EVERY FIELD IS OPTIONAL IN THE FRAMEWORKS.
 * Kafka Connect writes the exception headers only when the failure carried one,
 * and Spring's DLT publisher can be configured to leave any of them out. A
 * record with an original topic and nothing else is a normal, useful dead
 * letter — so the inspector renders what is there and says nothing about what
 * is not.
 */
export interface DlqMeta {
  /** One of DlqConvention — kept open in case the core learns another. */
  convention: string;
  original_topic: string | null;
  original_partition: number | null;
  original_offset: number | null;
  exception_class: string | null;
  exception_message: string | null;
  stacktrace: string | null;
}
