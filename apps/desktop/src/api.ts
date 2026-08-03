import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Types (IPC contract — must match the Rust side exactly)
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

export interface ConnectionProfile {
  id: string;
  name: string;
  environment: Environment;
  bootstrap_servers: string[];
  auth: AuthConfig;
  read_only: boolean;
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

/** Normalize a rejected invoke value (string per contract, but be defensive). */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}
