# Cross-Platform Kafka Client — Full-App Feature Specification

**Date:** 2026-08-02
**Decisions locked:** Freemium (free core + paid subscription premium) · Desktop app + team server component · Tauri + Rust (rust-rdkafka/librdkafka) · Full monitoring in scope
**Positioning:** Fill the vacuum left by Conduktor Desktop's retirement (end 2025) and provectus/kafka-ui's abandonment. A modern, tiny, no-Docker, no-JVM desktop Kafka client for macOS + Windows that is free for commercial use at the core — directly attacking Offset Explorer's personal-use-only license, dated UI, and bounded search.

---

## Market research summary

### Why now
- **Conduktor Desktop retired end of 2025**; its web replacement needs Docker + a server ("unreasonable setup burden just to peek at a topic" — r/apachekafka consensus). A "Open source clone of Conduktor Desktop" demand thread appeared May 2026.
- **provectus/kafka-ui (12k stars) abandoned** July 2024; community fork Kafbat UI is active but volunteer-run. A past RCE (CVE-2023-52251) that took ~6 months to patch makes teams wary of web UIs with admin creds on prod.
- **Offset Explorer** ($139/user commercial, free personal-only): dated Swing UI, bounded per-partition search (48s for 90k messages, silently misses matches), no ACL management, no monitoring, zero community presence.
- New native desktop entrants (kafkalet — 15 MB binary, no JVM — warmest r/apachekafka reception of 2026; KafkIO; Kafka King 1.3k stars) prove the demand signal.

### What wins users (demand-side signals)
1. **Message browse/search is the #1 job-to-be-done** by a wide margin. kpow's ~1M msgs/min streaming search and Redpanda Console's JS push-down filters are the praised benchmarks.
2. Correct **Avro/Protobuf decoding via Schema Registry** separates usable tools from toys.
3. **Consumer lag + offset reset** is table stakes in every thread.
4. Loudest complaints about incumbents: **Conduktor pricing** (~$80–150k/yr @ 100 seats), **Docker/server requirements**, **JVM bloat**, **clunky OSS UX**, **keystore-only TLS config**.
5. Orgs run **two tools**: a governed web/commercial tool for the platform team + a lightweight local tool for quick peeking. Our desktop+server model covers both sides.

### Competitive pricing reference
- Conduktor Team: $1,200/seat/yr (per-seat, loudly resented)
- kpow: ~$4,500/cluster/yr (per-cluster, beloved — "Nothing is remotely close")
- Kadeck: $25–32/user/mo
- Lenses: from ~$4,000/yr
- Offset Explorer: $139/user + $46/yr maintenance

---

## Architecture at a glance

- **Desktop app:** Tauri 2.x shell, Rust core using rust-rdkafka (librdkafka) for the Kafka protocol; web-tech UI (React or Svelte). Target < 25 MB installers, instant startup, macOS Universal + Windows x64/ARM64. All cluster communication is local — credentials never leave the machine (OS keychain).
- **Team server (premium):** self-hosted (Docker/Helm) control plane. It is NOT a Kafka proxy — desktop clients still talk to Kafka directly. The server provides identity (SSO), authorization (RBAC policies pushed to clients), shared config sync, central audit ingestion, alert routing, and a read-only web console. Stateless where possible; Postgres for state.
- **Licensing plane:** subscription entitlements checked by desktop app; offline/air-gapped grace licensing supported (air-gapped operation is a repeated enterprise requirement).

---

## MUST-HAVE FEATURES

### 1. Connectivity & security (free) — must match Offset Explorer's one strength, then beat it
- Multi-cluster saved connection profiles; environment tagging (prod/staging/dev) with color-coded UI chrome; instant cluster switcher
- Auth matrix: PLAINTEXT, SSL/TLS, mTLS, SASL PLAIN, SCRAM-SHA-256/512, **AWS MSK IAM**, **OAUTHBEARER/OIDC to the broker** (top unresolved ask on every OSS tracker), Kerberos/GSSAPI, Confluent Cloud API keys, Azure Event Hubs connection strings
- **PEM certs/keys directly — no JKS/keystore conversion ever** (chronically upvoted pain point)
- Credentials in OS keychain (macOS Keychain / Windows Credential Manager), never plaintext on disk
- Connection profile export/import **excluding secrets** (shareable with teammates)
- Kafka 4.x / KRaft-native day one; KIP-848 next-gen consumer protocol; compatibility back to Kafka 2.x
- Verified against: Apache Kafka, Amazon MSK, Confluent Cloud/Platform, Redpanda, Aiven, Azure Event Hubs, Strimzi
- **Per-connection read-only mode** (hand prod to on-call/juniors safely — desktop counter-positioning vs web-UI-with-admin-creds fear)

### 2. Message browsing, search & produce (free core) — the crown jewel
- **Streaming, unbounded search across partitions** — no "max messages per partition" pre-commit; progressive results; target ≥ 1M msgs/min scan rate (kpow benchmark)
- Seek by offset, timestamp, or **time range**; **cross-partition chronological sort**; live tail with pause/resume
- Serdes: JSON, Avro, Protobuf, JSON Schema, XML, UTF-8, hex/binary, MessagePack, CBOR — auto-detected, Schema-Registry-driven
- **Show which schema subject/version/ID decoded each message** (17👍 ask on provectus tracker)
- Programmable push-down filters (CEL or JS expressions evaluated before display); **saved/persistent filters per topic**
- Header display, header-based search/filtering, headers as optional list columns
- Key/value/timestamp/partition/offset columns, configurable; JSON path queries into values
- Export results to CSV / JSON / NDJSON; save individual messages to disk
- Produce: key + value + headers + explicit partition or key-hash; serialize via Schema Registry (Avro/Proto/JSON Schema); produce from file; **templated bulk data generator** (faker-style fields, N messages at interval)
- Record deletion (delete-records API); message replay onto the same topic

### 3. Consumer groups (free)
- All groups with state, members, assignment strategy; per-partition committed offset, end offset, **lag — live-refreshing**
- Lag visible directly in topic view (top Kafbat ergonomics ask)
- Offset reset: earliest / latest / specific offset / timestamp / shift-by-N — per group, per topic, or per partition; guarded when group is active
- **KIP-932 Share Groups view** (Kafka 4.x queues — single top-voted open ask on Kafbat, no tool has shipped it well)
- Static membership visibility; group deletion; member-level lag attribution

### 4. Topics & cluster operations (free)
- Topic CRUD; config editing with **diff-from-broker-default highlighting**; batch operations across topics
- Partition detail: leader, replicas, ISR, offsets, size on disk; under-replicated partition surfacing
- Broker list with configs (read + edit dynamic configs), rack awareness, KRaft quorum/controller view
- **ACL management — full read AND write** (create/delete ACLs; the biggest free-tool gap everywhere)
- Client quota viewing and management
- **Partition reassignment + preferred leader election** (engineers still drop to CLI for these; CMAK's beloved feature, orphaned since 2022)

### 5. Schema Registry (free)
- Subject/version browse, create, delete; **side-by-side version diff**; compatibility mode viewing/setting; compatibility check before registration
- Providers: Confluent SR, **Apicurio** (17👍 AKHQ ask), **AWS Glue Schema Registry**
- References/nested schema resolution; per-message schema traceability (see §2)

### 6. Kafka Connect (free)
- Full CRUD on connectors; config validation against plugin's config-def before submit; plugin discovery
- Task status, restart (connector or task level), pause/resume; failure stack-trace surfacing
- Multiple Connect clusters per Kafka connection

### 7. Monitoring & alerting (free basics / premium history+alerts)
- Free: point-in-time cluster health — broker status, URP count, controller, topic/partition counts, live lag
- Premium: **throughput charts** (bytes/msgs in-out per topic/broker), **lag history** with retention, storage growth trends — via JMX/Prometheus endpoint scraping with graceful degradation when unreachable
- Premium: **alert rules** (lag threshold, URP, offline partitions, throughput anomaly) → OS notifications; server routes to Slack/PagerDuty/webhook/email
- Premium: **Kafka Streams topology visualization** (only Confluent C3 and kpow have this)

### 8. Desktop UX fundamentals (free)
- Modern UI; **dark mode** (a 5👍 open ask on Redpanda Console); command palette (⌘K); keyboard-first navigation
- < 25 MB installer, < 2 s cold start, low idle memory (the anti-JVM pitch)
- Multi-window/multi-tab; per-cluster workspaces; auto-update channel
- Local action log: every mutating action you performed, timestamped, exportable

---

## PREMIUM FEATURES (subscription)

### Desktop Pro (per-user subscription)
- **SQL over topics** — Lenses-style queryable streams for non-Kafka-experts ("engineers could just query topics" is why teams pay $4k+/yr)
- **Cross-cluster tooling:** copy/replay messages between clusters (with optional transform), **topic config diff between environments**, consumer-offset migration
- **DLQ replay workflows:** inspect dead-letter topics, edit, re-produce to source with provenance headers
- Full monitoring history + alerting + Streams topology viz (per §7)
- **AI assistant:** natural language → filter/SQL query generation; anomaly explanation; **MCP server** so Claude/Cursor can drive the app (2026's emerging differentiator — Kafbat/Conduktor/Lenses all advertise MCP now)
- Session data masking (regex/field-based redaction for screen-sharing/demos)
- Custom serde plugin API (WASM-based — safe, cross-platform, language-agnostic)

### Team Server (per-seat or per-cluster subscription — recommend per-cluster to undercut Conduktor's resented per-seat model)
- **SSO:** OIDC, SAML, LDAP/AD; SCIM provisioning
- **RBAC:** role → cluster/topic-pattern/action policies enforced in desktop clients and web console; read-only roles; approval-required actions
- **Central audit log:** every action by every user across all clients, searchable, exportable, SIEM-forwardable
- Shared connection profiles (secrets held server-side or vault-referenced), shared saved filters/queries, shared serde plugins — pushed to all clients
- **Governance workflows:** topic/ACL/schema change requests with approvals (Topic-as-a-Service — Conduktor's NPS-80 feature; OSS answer Klaw is in patch-mode)
- **Web console** (read-mostly): browse topics/messages/lag from a browser for stakeholders without the desktop app
- Centrally-enforced data masking policies (PII compliance — hard blocker for regulated shops using OSS tools)
- Alert routing (Slack, PagerDuty, Teams, webhook, email); license/seat management; air-gapped license support

---

## NICE-TO-HAVE FEATURES (differentiators, any tier)

- **Companion CLI** sharing the same connection profiles/config as the desktop app (Offset Explorer ships one; enables scripting + CI)
- **Event tracing across topics** — follow a business key through a pipeline (Kouncil's unique WebSocket-tracing feature; no one else has it)
- **Tiered storage visibility** (remote vs local segment sizes/offsets) — latent, unexpressed demand; zero tools have it; cheap differentiation
- Topic import/export to files (full topic backup/restore incl. keys+headers+metadata)
- Embedded single-node Kafka sandbox ("try the app with zero setup"; also great for demos/tests)
- ksqlDB integration (streams/tables browser, query runner) — demand is declining with Flink's rise, hence nice-to-have not must-have
- Consumer group lag-clear ETA accounting for partition skew (Kafbat ask)
- WCAG 2.2 AA accessibility (only kpow does this — procurement checkbox in gov/enterprise)
- Localization (16+ languages — Offset Explorer does this; notable for APAC/EU adoption)
- Plugin/extension marketplace beyond serdes (custom panels, exporters)
- Flatbuffers / Cap'n Proto serdes (long-tail Redpanda Console asks)
- JetBrains/VS Code companion extension (Aiven and JetBrains validate the niche)
- Kinesis / Pulsar adapters (Kadeck differentiates with Kinesis) — far-future optionality

---

## Explicit non-goals
- **Not a Kafka wire proxy** (Conduktor Gateway's model): adds latency, an HA-critical hop, and operational fear; our enforcement lives in clients + server policy instead
- **Not a hosted SaaS** for v1 (Conduktor's SaaS attempt failed; self-hosted server matches enterprise trust posture)
- **No ZooKeeper-era support** below Kafka 2.x (CMAK died maintaining it)

## Freemium line rationale
Free tier must be genuinely better than Offset Explorer AND free for commercial use — that combination converts its entire resentful user base and the Conduktor-Desktop diaspora. Premium features are the ones with organizational (not individual) buyers: monitoring history, alerting, governance, SSO/RBAC/audit, cross-cluster ops, AI. Recommend per-cluster pricing for the server (kpow's beloved model, ~$4.5k/cluster/yr reference) and a modest per-user Desktop Pro (~$10–15/mo reference vs Kadeck's $25–32).

## Key sources
- r/apachekafka: "The best Kafka management tool" (Feb 2026), kafkalet launch (Mar 2026), free-tools thread (Dec 2025), "Open source clone of Conduktor Desktop" (May 2026)
- HN: kpow Show HN (158 pts), kafka-ui RCE thread (2024)
- GitHub trackers (live-queried Aug 2026): kafbat/kafka-ui, AKHQ, redpanda-data/console, provectus/kafka-ui top-voted issues
- Vendor docs/pricing: conduktor.io, factorhouse.io (kpow), lenses.io, kafkatool.com/offsetexplorer.com, kadeck.com, docs.redpanda.com
- Vendor-authored comparisons (bias flagged): Factor House review series, Conduktor kafka-ui guide, AutoMQ Top-12, AxonOps comparison
