# Kavka Architecture

## System overview

```
┌────────────────────────── Desktop app (Tauri 2) ──────────────────────────┐
│  React/TS UI (webview)  ⇄  Tauri IPC (commands + event channels)          │
│                              ⇄  kavka-core (Rust)                          │
│                                   ├─ connection manager (profiles, auth)   │
│                                   ├─ admin ops (topics, ACLs, quotas,      │
│                                   │   reassignment, groups, configs)       │
│                                   ├─ consume/search engine (streaming)     │
│                                   ├─ serde pipeline (+ SR clients)         │
│                                   ├─ metrics collector (Phase 4)           │
│                                   └─ rdkafka (librdkafka) → Kafka clusters │
└────────────────────────────────────────────────────────────────────────────┘
            │  HTTPS (only when Team Server is configured)
┌───────────▼── Team Server (Phase 6, self-hosted, axum + Postgres) ────────┐
│  identity (OIDC/SAML/LDAP) · RBAC policy distribution · profile sync      │
│  central audit ingestion · alert routing · read-mostly web console        │
│  (control plane only — NEVER a Kafka wire proxy; clients talk to Kafka    │
│   directly)                                                                │
└────────────────────────────────────────────────────────────────────────────┘
```

## Key decisions

### D1 — Tauri 2 + Rust core, React UI
Small installers (<25 MB target), instant startup, native feel — the explicit market demand ("no JVM, no Docker, 15 MB binary" was the most-upvoted aspect of competing launches). UI in React/TypeScript for velocity; all Kafka work in Rust.

### D2 — rust-rdkafka (librdkafka) behind the `kafka` cargo feature
librdkafka is the most complete non-Java client (SASL/SCRAM/OAUTHBEARER/mTLS, KIP-848 as of 2.x). The feature flag exists only so the scaffold compiles before CMake/Build Tools are set up; it becomes default-on in Phase 0. Risks and mitigations:
- Windows builds need CMake + MSVC; use `cmake-build` + vendored OpenSSL. CI produces all artifacts so contributors rarely build librdkafka locally.
- Gaps in librdkafka (e.g. future KIPs like Share Groups/KIP-932 consumer APIs) are filled by implementing the specific Kafka protocol frames directly in Rust (`kavka-core::protocol`) over the existing authenticated connection — admin-style RPCs are simple request/response and don't need the full client machinery.
- MSK IAM: implemented as an OAUTHBEARER token provider (SigV4-presigned token, same mechanism the aws-msk-iam-sasl-signer libraries use).

### D3 — Streaming search engine (the crown jewel)
- One consumer per partition (bounded parallelism), seeked by offset/timestamp; raw batches decoded and filtered **in Rust** before anything crosses IPC.
- Filters compile to CEL programs (cel-interpreter crate) evaluated post-deserialization; fast-path prefilters (substring/regex on raw bytes) run pre-deserialization to skip messages cheaply.
- Results stream to the UI over a Tauri event channel with backpressure (bounded ring buffer + UI-driven pull for scrollback); progressive counts; cooperative cancellation tokens everywhere.
- No fetch-limit pre-commit: search runs until the end offsets captured at start (or live-tails past them if requested). Target ≥1M msgs/min on a dev laptop.

### D4 — Serde pipeline
`bytes → (optional SR framing detect: magic byte 0x0 + schema id) → decoder → canonical JSON value + metadata (schema subject/version/id) → UI`.
Built-in decoders: JSON, Avro (apache-avro), Protobuf (prost + dynamic descriptors from SR), JSON Schema, XML, MessagePack, CBOR, UTF-8, hex. Custom serdes (Pro) are WASM modules (wasmtime) with a byte-in/JSON-out ABI — sandboxed, cross-platform, language-agnostic. SR clients: Confluent, Apicurio, AWS Glue behind one trait.

### D5 — Credentials & profiles
Profiles are JSON documents in the app data dir; **secrets never live in them** — they hold keychain references. Secrets go to macOS Keychain / Windows Credential Manager via the `keyring` crate. Profile export produces a secret-free document by construction. Read-only mode is enforced in `kavka-core` (mutating APIs check the connection's mode), not in the UI.

### D6 — Monitoring (Phase 4)
Lag history needs no broker cooperation (computed from committed vs end offsets, sampled by a background task into a local embedded store — redb). Throughput/broker metrics scrape JMX-exporter/Prometheus endpoints when configured, degrading gracefully when unreachable. Alert rules evaluate locally → OS notifications; when a Team Server is attached, the server evaluates centrally and routes to Slack/PagerDuty/webhooks.

### D7 — Team Server is a control plane, not a data plane
No Kafka proxying (Conduktor Gateway's latency/HA-hop model is a known adoption fear). The server: authenticates users (OIDC/SAML/LDAP), issues short-lived policy bundles (RBAC rules per cluster/topic-pattern/action) that clients enforce in `kavka-core`, syncs shared profiles/filters/serdes, ingests audit events, routes alerts, and serves a read-mostly web console (reusing the React components against a server-side consume API for browser users). Stack: Rust axum + Postgres; stateless except Postgres; Docker/Helm distribution; air-gapped licensing supported.

### D8 — Licensing/entitlements
Signed license tokens (Ed25519) checked in `kavka-core`; offline grace period for air-gapped use. Free features never phone home; telemetry is opt-in only (a repeated praise point for KafkIO — "no telemetry").

## Testing strategy
- Unit: serde pipeline golden tests; CEL filter semantics; policy enforcement.
- Integration: testcontainers with single-binary Redpanda + Apache Kafka (KRaft) matrices; SR (Confluent + Apicurio) containers; auth matrix against SASL-configured containers.
- E2E: Playwright driving the built app via tauri-driver on Windows + macOS CI runners.
- Perf: search throughput benchmark topic (10M messages) as a CI regression gate.

## Distribution
Tauri bundler → signed `.dmg`/notarized (macOS, Universal) and NSIS `.exe`/MSIX (Windows, x64 + ARM64); auto-update via tauri-updater (signed manifests); Homebrew cask, winget, Chocolatey. CI: GitHub Actions matrix (macos-14, windows-2022).
