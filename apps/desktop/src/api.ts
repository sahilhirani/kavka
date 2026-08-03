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

export function secretSet(entry: string, value: string): Promise<void> {
  return invoke<void>("secret_set", { entry, value });
}

export function secretDelete(entry: string): Promise<void> {
  return invoke<void>("secret_delete", { entry });
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
