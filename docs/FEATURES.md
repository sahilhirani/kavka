# Cross-Platform Kafka Client — Full-App Feature Specification

**Date:** 2026-08-02 (revised same day: pivot to fully open source)
**Decisions locked:** Fully open source (AGPL-3.0 — no closed forks), every feature free, donation-funded (Buy Me a Coffee) · Desktop app only — no server component, no SSO/enterprise plane · Tauri + Rust (rust-rdkafka/librdkafka) · Full monitoring in scope · Public repo since the Phase 6 launch
**Positioning:** Fill the vacuum left by Conduktor Desktop's retirement (end 2025) and provectus/kafka-ui's abandonment. A modern, tiny, no-Docker, no-JVM desktop Kafka client for macOS + Windows that is completely free and open source — directly attacking Offset Explorer's personal-use-only license, dated UI, and bounded search, and answering the explicit "open source clone of Conduktor Desktop" community demand.

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
5. Orgs run **two tools**: a governed web/commercial tool for the platform team + a lightweight local tool for quick peeking. Kavka is the definitive second tool — the lightweight local one — and being free + open source removes every barrier to it spreading engineer-to-engineer.

### Competitive pricing reference (market context — Kavka itself is free)
- Conduktor Team: $1,200/seat/yr (per-seat, loudly resented)
- kpow: ~$4,500/cluster/yr (per-cluster, beloved — "Nothing is remotely close")
- Kadeck: $25–32/user/mo
- Lenses: from ~$4,000/yr
- Offset Explorer: $139/user + $46/yr maintenance

Every dollar on that list is a reason an engineer will try the free, open-source alternative first.

---

## Architecture at a glance

- **Desktop app:** Tauri 2.x shell, Rust core using rust-rdkafka (librdkafka) for the Kafka protocol; web-tech UI (React or Svelte). Target < 25 MB installers, instant startup, macOS Universal + Windows x64/ARM64. All cluster communication is local — credentials never leave the machine (OS keychain).
- **That's it.** No server component, no licensing plane, no account system, no telemetry. The one request the app makes on its own initiative is the daily update check against github.com (§8) — nothing identifying, nothing about the user's clusters, one switch that stops it, and nothing installed without an explicit click. The entire product is the desktop app; everything below ships in it, free.
- **Sustainability:** donation-funded — a Buy Me a Coffee button in the README, GitHub's Sponsor button via `.github/FUNDING.yml`, and a quiet "Support Kavka ☕" link in the app's About panel and command palette. Never a nag screen, never a feature gate.

---

## MUST-HAVE FEATURES

### 1. Connectivity & security — must match Offset Explorer's one strength, then beat it
- Multi-cluster saved connection profiles; **user-defined environment tagging** — name your own (`dev`, `QA`, `UAT`, `Production`, …), each with a colour token and a **protected** flag — with colour-coded UI chrome; instant cluster switcher
  - **Colour is identity; `protected` is the guardrail.** Marking an environment protected is what turns on the warm substrate, the type-to-confirm on destructive actions, the window-title suffix, the CLI's `--yes-prod` and the MCP server's `KAVKA_MCP_ALLOW_PROD` — for that environment whatever it is called. Ships with `dev`, `staging` and `prod` (protected), which is exactly what earlier versions had
- Auth matrix: PLAINTEXT, SSL/TLS, mTLS, SASL PLAIN, SCRAM-SHA-256/512, **AWS MSK IAM**, **OAUTHBEARER/OIDC to the broker** (top unresolved ask on every OSS tracker), Kerberos/GSSAPI, Confluent Cloud API keys, Azure Event Hubs connection strings
- **PEM certs/keys directly — no JKS/keystore conversion ever** (chronically upvoted pain point)
- Credentials in OS keychain (macOS Keychain / Windows Credential Manager), never plaintext on disk
- Connection profile export/import **excluding secrets** (shareable with teammates)
- Kafka 4.x / KRaft-native day one; KIP-848 next-gen consumer protocol; compatibility back to Kafka 2.x
- Verified against: Apache Kafka, Amazon MSK, Confluent Cloud/Platform, Redpanda, Aiven, Azure Event Hubs, Strimzi
- **Per-connection read-only mode** (hand prod to on-call/juniors safely — desktop counter-positioning vs web-UI-with-admin-creds fear)

### 2. Message browsing, search & produce — the crown jewel
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

### 3. Consumer groups
- All groups with state, members, assignment strategy; per-partition committed offset, end offset, **lag — live-refreshing**
- Lag visible directly in topic view (top Kafbat ergonomics ask)
- Offset reset: earliest / latest / specific offset / timestamp / shift-by-N — per group, per topic, or per partition; guarded when group is active
- **KIP-932 Share Groups view** (Kafka 4.x queues — single top-voted open ask on Kafbat, no tool has shipped it well)
- Static membership visibility; group deletion; member-level lag attribution

### 4. Topics & cluster operations
- Topic CRUD; config editing with **diff-from-broker-default highlighting**; batch operations across topics
- Partition detail: leader, replicas, ISR, offsets, size on disk; under-replicated partition surfacing
- Broker list with configs (read + edit dynamic configs), rack awareness, KRaft quorum/controller view
- **ACL management — full read AND write** (create/delete ACLs; the biggest free-tool gap everywhere)
- Client quota viewing and management
- **Partition reassignment + preferred leader election** (engineers still drop to CLI for these; CMAK's beloved feature, orphaned since 2022)

### 5. Schema Registry
- Subject/version browse, create, delete; **side-by-side version diff**; compatibility mode viewing/setting; compatibility check before registration
- Providers: Confluent SR, **Apicurio** (17👍 AKHQ ask), **AWS Glue Schema Registry**
- References/nested schema resolution; per-message schema traceability (see §2)

### 6. Kafka Connect
- Full CRUD on connectors; config validation against plugin's config-def before submit; plugin discovery
- Task status, restart (connector or task level), pause/resume; failure stack-trace surfacing
- Multiple Connect clusters per Kafka connection

### 7. Monitoring & alerting
- Point-in-time cluster health — broker status, URP count, controller, topic/partition counts, live lag
- **Throughput charts** (bytes/msgs in-out per topic/broker), **lag history** with retention, storage growth trends — via JMX/Prometheus endpoint scraping with graceful degradation when unreachable
- **Alert rules** (lag threshold, URP, offline partitions, throughput anomaly) → OS notifications + optional user-configured webhooks (Slack-compatible incoming webhooks, generic HTTP) fired directly from the app
- **Kafka Streams topology visualization** (only Confluent C3 and kpow have this)

### 8. Desktop UX fundamentals
- Modern UI; **dark mode** (a 5👍 open ask on Redpanda Console); command palette (⌘K); keyboard-first navigation
- < 25 MB installer, < 2 s cold start, low idle memory (the anti-JVM pitch)
- Multi-window/multi-tab; per-cluster workspaces
- **Update notices, never automatic installs** — the app asks github.com for the newest release at most once a day (on by default, one switch in Settings → Updates stops it) and states plainly what it found; **nothing is downloaded or installed until the user clicks Install**, and what is downloaded is verified against a signing public key compiled into the app before the installer is handed anything
  - **Two channels.** *Stable* — human-tagged `v*` releases. *Every build*, opt-in — the `v<version>-build.<n>` pre-release from the newest merge to `main`. While no stable release has ever been published, the stable channel says exactly that rather than reporting an error or "up to date"; an install carrying a build number is offered the stable release of its own base version when one appears
  - **This is the app's only outbound request of its own initiative.** It carries nothing identifying and nothing about the user's clusters — it asks GitHub the same question the public Releases page answers for anyone. Copies installed from a package manager (winget/Chocolatey/Homebrew) are updated by that manager. The README's *What Kavka sends* and the website say this in the same words; no surface may claim Kavka makes no network requests
- Local action log: every mutating action you performed, timestamped, exportable
- About panel with a quiet **"Support Kavka ☕" Buy Me a Coffee link** (also reachable from the command palette) — the app's only monetization surface, and it's a donation link, not a paywall

---

## POWER FEATURES (also free — later phases, see ROADMAP)

Formerly sketched as a paid "Desktop Pro" tier; with the open-source pivot they are simply the later-phase features of the one free app.

- **SQL over topics** — Lenses-style queryable streams for non-Kafka-experts ("engineers could just query topics" is why teams pay $4k+/yr elsewhere; here it's free)
- **Cross-cluster tooling:** copy/replay messages between clusters (with optional transform), **topic config diff between environments**, consumer-offset migration
- **DLQ replay workflows:** inspect dead-letter topics, edit, re-produce to source with provenance headers
- **AI assistant:** natural language → filter/SQL query generation; anomaly explanation; **MCP server** so Claude/Cursor can drive the app (2026's emerging differentiator — Kafbat/Conduktor/Lenses all advertise MCP now)
- Session data masking (regex/field-based redaction for screen-sharing/demos)
- Custom serde plugin API (WASM-based — safe, cross-platform, language-agnostic)

---

## NICE-TO-HAVE FEATURES (differentiators)

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
- **No server component, no SSO, no enterprise control plane** — dropped in the open-source pivot. SSO (OIDC/SAML/LDAP/SCIM), RBAC, central audit, governance/approval workflows, shared-profile sync, centrally-enforced masking policies, and the web console are all out of scope. The per-connection read-only mode and the local action log are the safety story; orgs that need governed access run a web tool alongside Kavka (the two-tool pattern the market already follows).
- **Not a Kafka wire proxy** (Conduktor Gateway's model): adds latency, an HA-critical hop, and operational fear
- **Not a hosted SaaS** — there is nothing to host; the product is the desktop app
- **No paid tiers, license keys, or feature gates — ever.** Every feature is free; the license is AGPL-3.0; monetization is a donation button.
- **No ZooKeeper-era support** below Kafka 2.x (CMAK died maintaining it)

## Open-source & sustainability model
Fully open source under **AGPL-3.0** — strong copyleft, chosen deliberately: everyone can use Kavka free of charge, personally or commercially, but forks and modified versions must stay open source under the same license, and the network clause stops anyone from wrapping `kavka-core` into a closed SaaS. **No closed-source forks** (the Grafana/Mattermost licensing model). For users this changes nothing — using an AGPL app at work is unrestricted; the obligations bind only redistributors. It converts Offset Explorer's entire resentful user base (personal-use-only license), the Conduktor-Desktop diaspora, and the explicit "open source clone of Conduktor Desktop" demand — with zero adoption friction and no reason for anyone to pick a competitor on price or license.

The repo went **public at the Phase 6 launch** ([github.com/sahilhirani/kavka](https://github.com/sahilhirani/kavka)); the license governs from that first public release.

Funding is by donation:
- **Buy Me a Coffee** button ([buymeacoffee.com/sahilhirani](https://buymeacoffee.com/sahilhirani)) in the README and on the docs site
- **GitHub Sponsor button** via `.github/FUNDING.yml` (points at the Buy Me a Coffee page)
- In-app: a quiet **"Support Kavka ☕"** link in the About panel and command palette — shown, never pushed. No nag screens, no telemetry-driven prompts.

Open source is also the distribution strategy: it removes procurement friction entirely, invites contributions (serdes, localization, auth providers), and makes engineer-to-engineer recommendation — the channel that made every incumbent — frictionless.

## Key sources
- r/apachekafka: "The best Kafka management tool" (Feb 2026), kafkalet launch (Mar 2026), free-tools thread (Dec 2025), "Open source clone of Conduktor Desktop" (May 2026)
- HN: kpow Show HN (158 pts), kafka-ui RCE thread (2024)
- GitHub trackers (live-queried Aug 2026): kafbat/kafka-ui, AKHQ, redpanda-data/console, provectus/kafka-ui top-voted issues
- Vendor docs/pricing: conduktor.io, factorhouse.io (kpow), lenses.io, kafkatool.com/offsetexplorer.com, kadeck.com, docs.redpanda.com
- Vendor-authored comparisons (bias flagged): Factor House review series, Conduktor kafka-ui guide, AutoMQ Top-12, AxonOps comparison
