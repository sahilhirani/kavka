import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

export interface ConnectionProfile {
  id: string;
  name: string;
  environment: Environment;
  bootstrap_servers: string[];
  auth: AuthConfig;
  read_only: boolean;
  schema_registry?: SchemaRegistryConfig | null;
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
