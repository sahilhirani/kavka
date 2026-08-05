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
```

The desktop app is the entire system. There is no server component, no account
system, and no licensing plane — Kavka talks only to the Kafka clusters (and
Schema Registry / Connect / metrics endpoints) you configure, plus the
auto-update manifest.

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
Built-in decoders: JSON, Avro (apache-avro), Protobuf (prost + dynamic descriptors from SR), JSON Schema, XML, MessagePack, CBOR, UTF-8, hex. Custom serdes (Phase 5) are WASM modules (wasmtime) with a byte-in/JSON-out ABI — sandboxed, cross-platform, language-agnostic. SR clients: Confluent, Apicurio, AWS Glue behind one trait.

### D5 — Credentials & profiles
Profiles are JSON documents in the app data dir; **secrets never live in them** — they hold keychain references. Secrets go to macOS Keychain / Windows Credential Manager via the `keyring` crate. Profile export produces a secret-free document by construction. Read-only mode is enforced in `kavka-core` (mutating APIs check the connection's mode), not in the UI.

### D6 — Monitoring (Phase 4)
Lag history needs no broker cooperation (computed from committed vs end offsets, sampled by a background task into a local embedded store — redb). Throughput/broker metrics scrape JMX-exporter/Prometheus endpoints when configured, degrading gracefully when unreachable. Alert rules evaluate locally → OS notifications, plus optional user-configured webhooks (Slack-compatible incoming webhooks, generic HTTP) fired directly from the app.

### D7 — Fully open source, desktop-only, donation-funded
AGPL-3.0 across the whole repo; every feature free for personal and commercial use. Strong copyleft is deliberate — forks and derivatives (including network services built on `kavka-core`) must publish their source: no closed forks. Consequences for the architecture:
- **No server component.** SSO, RBAC, central audit, shared-profile sync, governance workflows, and the web console (the old "Team Server" plan) are out of scope. The safety story is per-connection read-only mode (enforced in `kavka-core`, see D5) plus the local action log.
- **No licensing/entitlement machinery.** No license tokens, no feature gates, no account system — code that never has to exist.
- **No telemetry, and exactly one request nobody asked for.** Everything else the app contacts is an endpoint the user configured — brokers, schema registries, Connect, their own webhooks. The one exception is the update check: **github.com, at most once a day, on by default, with one switch in Settings → Updates that stops it.** It asks GitHub the same question the public Releases page answers for anybody; it carries nothing that identifies the user and nothing about their clusters; and **nothing is downloaded or installed until the user clicks Install**, at which point the download is verified against a signing public key compiled into the app before the installer is handed anything. Crash reporting and telemetry remain strictly opt-in (a repeated praise point for KafkIO — "no telemetry"). No surface — app, README, docs or site — may claim Kavka makes no network requests; see `docs/FEATURES.md` §8 and `apps/desktop/src-tauri/src/update.rs`.
- **Donations, not subscriptions:** Buy Me a Coffee via README badge, `.github/FUNDING.yml` (GitHub Sponsor button), and a quiet "Support Kavka ☕" link in the app's About panel and command palette.

## Testing strategy
- Unit: serde pipeline golden tests; CEL filter semantics; read-only-mode enforcement.
- Integration: testcontainers with single-binary Redpanda + Apache Kafka (KRaft) matrices; SR (Confluent + Apicurio) containers; auth matrix against SASL-configured containers.
- E2E: Playwright driving the built app via tauri-driver on Windows + macOS CI runners.
- Perf: search throughput benchmark topic (10M messages) as a CI regression gate.

## Distribution
Tauri bundler → signed `.dmg`/notarized (macOS, Universal) and NSIS `.exe`/MSIX (Windows, x64 + ARM64); auto-update via tauri-updater (signed manifests); Homebrew cask, winget, Chocolatey. CI: GitHub Actions matrix (macos-14, windows-2022).
