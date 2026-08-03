# Kavka

**A modern, fast, fully open-source Kafka client for macOS and Windows.**

*Kavka* (Czech: jackdaw) is the bird Franz Kafka's surname comes from. The app fills the gap left by Conduktor Desktop's retirement and Offset Explorer's stagnation: a tiny, no-Docker, no-JVM desktop client that is **free for everyone — personal and commercial use alike — with every feature included**. No tiers, no license keys, no server component. If Kavka saves you time, you can [buy us a coffee](#supporting-kavka).

Everything ships in the one free app:

- **Connectivity:** full auth matrix (MSK IAM, OAuth-to-broker, Kerberos, PEM certs without keystore conversion), OS-keychain secret storage, per-connection read-only mode.
- **Browse & search:** unbounded streaming message search (≥1M msgs/min target), Avro/Protobuf/JSON Schema via Schema Registry, CEL push-down filters, cross-partition chronological sort.
- **Operations:** consumer-group lag + offset reset, full ACL management, partition reassignment, client quotas, Kafka Connect CRUD, Schema Registry management.
- **Monitoring:** lag history, throughput charts, alert rules with OS notifications and webhooks, Kafka Streams topology visualization.
- **Power tools:** SQL over topics, cross-cluster replay/diff, DLQ workflows, AI query generation + MCP server, custom WASM serdes.

See [docs/FEATURES.md](docs/FEATURES.md) for the complete specification, [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for system design, and [docs/ROADMAP.md](docs/ROADMAP.md) for the phased build plan.

## Repository layout

```
apps/desktop/           Tauri 2 desktop app (React + TypeScript UI, Rust shell)
  src/                  UI (Vite + React)
  src-tauri/            Tauri Rust shell + IPC commands
crates/kavka-core/      Rust core: connections, admin ops, consume/search engine, serdes
docs/                   Product spec, architecture, roadmap
```

## Installing

Prebuilt installers (Windows `.exe`/`.msi`, macOS `.dmg` for Apple Silicon and Intel) are attached to every [GitHub Release](https://github.com/sahilhirani/kavka/releases) — built automatically when a version tag (`v*`) is pushed. Download, install, done. Until the first release, build from source below.

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

### Local Kafka cluster for development & testing

```sh
docker compose -f dev/docker-compose.yml up -d --wait   # single-node KRaft Kafka on localhost:9092
```

This creates five dev topics (`orders`, `payments`, `customers`, `inventory`, `dead-letter`)
and seeds `orders` with keyed JSON messages. Data persists in a named volume;
`down -v` resets it. Integration smoke test against it:

```sh
KAVKA_IT=1 cargo test -p kavka-core --features kafka --test local_cluster
```

### Cargo features

- `kafka` (enabled by the desktop app): compiles librdkafka via `cmake-build` — needs CMake + MSVC Build Tools (`winget install Kitware.CMake`)
- `kafka-ssl`: adds TLS/SASL_SSL via vendored OpenSSL — additionally needs Strawberry Perl + NASM on Windows

App icons are generated from `assets/icon-source.png` (a placeholder "K" mark) — when a real logo exists, replace that file and re-run `npm run tauri icon assets/icon-source.png` from `apps/desktop/`.

## Supporting Kavka

Kavka is free and open source, and always will be — no paid tiers, no feature gates. Development is funded by donations:

☕ **[Buy Me a Coffee](https://buymeacoffee.com/sahilhirani)**

The app itself will carry a small, unobtrusive "Support Kavka ☕" link in its About panel and command palette — never a nag screen.

## License

[AGPL-3.0](LICENSE). Free to use anywhere — personally or commercially. Copyleft: if you distribute a modified Kavka, or offer one as a network service, you must publish your source under the same license. **No closed-source forks.**

## Status

Pre-development. Spec and scaffold complete; Phase 0 (foundation) not started. The repo stays private until the Phase 6 launch. Domain: **kavka.io** (verified unregistered 2026-08-02 — register it!).
