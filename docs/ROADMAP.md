# Kavka Build Roadmap — Full App

Phases build the complete product from docs/FEATURES.md (this is the full-app plan, not an MVP cut). Each phase ends with all listed acceptance criteria green in CI on both macOS and Windows.

## Phase 0 — Foundation
- Rust toolchain + CMake in CI; enable the `kafka` cargo feature (rust-rdkafka with cmake-build, vendored OpenSSL); GitHub Actions matrix builds signed dev artifacts for macOS Universal + Windows x64/ARM64
- Connection engine: profiles CRUD, environment tags/colors, OS-keychain secret storage, secret-free profile export/import
- Full auth matrix: PLAINTEXT, SSL/mTLS (PEM direct), SASL PLAIN, SCRAM-SHA-256/512, OAUTHBEARER/OIDC, AWS MSK IAM, Kerberos; verified against containerized Kafka (KRaft), Redpanda, and live MSK/Confluent Cloud/Event Hubs
- Per-connection read-only mode enforced in core
- App shell: cluster switcher, command palette, dark mode, auto-update channel
- **Accept:** connect to all 5 managed-Kafka flavors incl. MSK IAM and OAuth; secrets provably absent from exported profiles; cold start < 2 s

## Phase 1 — Browse
- Topic list/detail (partitions, leaders, ISR, sizes, configs with diff-from-default), topic CRUD, batch ops, record deletion
- Message viewer: offset/timestamp/time-range seek, live tail, headers, configurable columns
- Serde pipeline: JSON, Avro, Protobuf, JSON Schema, XML, MessagePack, CBOR, hex; SR framing detection; per-message schema subject/version/id display
- Schema Registry read (Confluent, Apicurio, Glue); consumer groups: lag live view, all offset-reset modes, lag-in-topic-view
- **Accept:** decode golden-file corpus across all serdes; reset offsets safely on an active-group guard; browse a 1k-partition topic without UI jank

## Phase 2 — Search & Produce (crown jewel)
- Streaming unbounded search: parallel partition consumers, raw-byte prefilters, CEL push-down filters, progressive results, cancellation, cross-partition chronological sort
- Saved filters per topic; JSON-path column extraction; export CSV/JSON/NDJSON
- Produce: key/value/headers/partition, SR-serialized Avro/Proto/JSON Schema, produce-from-file, templated bulk data generator; same-topic replay
- **Accept:** ≥1M msgs/min scan on the 10M-message perf topic (CI regression gate); search never silently truncates

## Phase 3 — Ops
- ACL full read/write management; client quotas view/manage
- Partition reassignment (plan, throttle, monitor) + preferred leader election; broker dynamic config editing; KRaft quorum view
- Kafka Connect: multi-cluster, connector CRUD with config-def validation, task restart/pause/resume, failure traces
- Schema Registry write: register with compat check, side-by-side version diff, compatibility mode management
- **Accept:** complete a guided reassignment on a 3-broker cluster; create/validate/pause/resume a connector end-to-end

## Phase 4 — Monitoring & Alerting
- Lag history sampler → local embedded store (redb); lag charts with retention
- Metrics scraping (Prometheus/JMX-exporter endpoints): throughput per topic/broker, storage growth, URP; graceful degradation when unreachable
- Alert rules (lag threshold, URP, offline partitions, throughput anomaly) → OS notifications
- Kafka Streams topology visualization; KIP-932 Share Groups view
- **Accept:** 24 h lag history survives app restarts; alert fires within one sample interval; topology renders for a reference Streams app

## Phase 5 — Desktop Pro (premium) + entitlements
- Licensing: signed Ed25519 entitlement tokens, offline grace, upgrade flow
- SQL over topics (DataFusion-based engine over the consume API)
- Cross-cluster: message copy/replay with optional transform, topic config diff between environments, consumer-offset migration; DLQ inspect/edit/re-produce workflows
- AI: natural language → CEL/SQL generation; MCP server exposing read APIs (and gated write APIs) to Claude/Cursor
- Session data masking; WASM custom-serde plugin API + docs
- **Accept:** free tier fully functional with no license; every Pro surface cleanly gated; MCP server drives a topic browse from Claude Code

## Phase 6 — Team Server
- axum + Postgres control plane; Docker/Helm distribution; air-gapped licensing
- SSO (OIDC, SAML, LDAP/AD) + SCIM; RBAC policy bundles enforced by desktop clients; approval-required actions
- Shared profiles (vault-referenced secrets), shared filters/serdes; central audit ingestion (searchable, SIEM-forwardable); alert routing (Slack/PagerDuty/Teams/webhook/email)
- Governance workflows: topic/ACL/schema change requests + approvals; centrally-enforced masking policies
- Read-mostly web console reusing the React components
- **Accept:** desktop client in server mode cannot exceed its RBAC policy (verified by test matrix); full audit trail for a scripted multi-user session; SSO login on all three IdP types

## Phase 7 — Polish & Launch
- WCAG 2.2 AA pass; localization framework + first 5 languages; docs site at kavka.io; embedded single-node sandbox ("try without a cluster")
- Distribution: notarized dmg, signed NSIS/MSIX, Homebrew cask, winget, Chocolatey; crash reporting (opt-in); pricing/checkout integration
- Companion CLI sharing profiles (Phase 7 or fast-follow)
- **Accept:** clean install → connected → browsing in under 2 minutes on both OSes, measured with first-time users

## Deferred / fast-follow backlog
Event tracing across topics · tiered-storage visibility · ksqlDB panel · topic import/export (backup/restore) · Flatbuffers/Cap'n Proto serdes · JetBrains/VS Code companion · Kinesis/Pulsar adapters
