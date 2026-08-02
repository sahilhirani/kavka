# Kavka

**A modern, fast, cross-platform Kafka client for macOS and Windows.**

*Kavka* (Czech: jackdaw) is the bird Franz Kafka's surname comes from. The app fills the gap left by Conduktor Desktop's retirement and Offset Explorer's stagnation: a tiny, no-Docker, no-JVM desktop client that is free for commercial use at its core, with paid premium and team-server tiers.

- **Free core:** full auth matrix (MSK IAM, OAuth-to-broker, PEM without keystores), unbounded streaming message search, Avro/Protobuf/JSON Schema via Schema Registry, consumer-group lag + offset reset, full ACL management, partition reassignment, Kafka Connect CRUD, read-only mode.
- **Desktop Pro (subscription):** SQL over topics, cross-cluster replay/diff, DLQ workflows, monitoring history + alerting, Streams topology viz, AI query generation + MCP server, custom WASM serdes.
- **Team Server (subscription):** SSO, RBAC, central audit, shared profiles/filters, approval workflows, data-masking policies, alert routing, read-mostly web console.

See [docs/FEATURES.md](docs/FEATURES.md) for the complete specification, [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for system design, and [docs/ROADMAP.md](docs/ROADMAP.md) for the phased build plan.

## Repository layout

```
apps/desktop/           Tauri 2 desktop app (React + TypeScript UI, Rust shell)
  src/                  UI (Vite + React)
  src-tauri/            Tauri Rust shell + IPC commands
crates/kavka-core/      Rust core: connections, admin ops, consume/search engine, serdes
docs/                   Product spec, architecture, roadmap
server/                 Team server (Phase 6 — placeholder)
```

## Development setup

Prerequisites:

1. **Rust** stable via rustup (`winget install Rustlang.Rustup`)
2. **Node.js ≥ 20** (installed) and npm
3. **CMake + Visual Studio Build Tools** (required later when the `kafka` feature flag is enabled — rust-rdkafka builds librdkafka via cmake)
4. Tauri 2 prerequisites: WebView2 (preinstalled on Win 11); on macOS, Xcode CLT

```sh
cd apps/desktop
npm install
npm run tauri dev     # runs the desktop app with hot reload
```

The `kavka-core` crate currently compiles without librdkafka (`kafka` feature off by default) so the scaffold builds on a fresh toolchain. Phase 0 of the roadmap flips the feature on.

App icons are generated from `assets/icon-source.png` (a placeholder "K" mark) — when a real logo exists, replace that file and re-run `npm run tauri icon assets/icon-source.png` from `apps/desktop/`.

## Status

Pre-development. Spec and scaffold complete; Phase 0 (foundation) not started. Domain: **kavka.io** (verified unregistered 2026-08-02 — register it!).
