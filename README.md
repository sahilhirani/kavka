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
    playground/         the single-node Kafka the app can start for you
crates/kavka-core/      Rust core: connections, admin ops, consume/search engine, serdes
crates/kavka-mcp/       MCP server — the same profiles, over stdio, for AI clients
crates/kavka-cli/       `kavka` — the same profiles, on a terminal
docs/                   Product spec, architecture, roadmap, design system
docs-site/              The website. Hand-rolled HTML/CSS, no build system
packaging/              winget / Chocolatey / Homebrew manifest templates
```

## Installing

Prebuilt installers are attached to every [GitHub Release](https://github.com/sahilhirani/kavka/releases). Every merge to `main` publishes an automated build (tagged `v<version>-build.<n>`, marked pre-release) so the newest Kavka is always downloadable; stable versions carry a plain `v*` tag pushed by a human. Download, install, done.

| Platform | Asset | Requires |
|---|---|---|
| macOS, Apple Silicon | `Kavka_<version>_aarch64.dmg` | macOS 12 Monterey or newer |
| macOS, Intel | `Kavka_<version>_x64.dmg` | macOS 12 Monterey or newer |
| Windows, installer | `Kavka_<version>_x64-setup.exe` | Windows 10 1809 or newer, x64 |
| Windows, for fleets | `Kavka_<version>_x64_en-US.msi` | as above; installs per-machine |

Every release also carries `SHA256SUMS.txt`, so you can check what you downloaded against what CI built.

**The binaries are not code-signed yet.** On macOS, right-click the app and choose *Open* the first time; on Windows, SmartScreen shows a warning and you need *More info → Run anyway*. That is what an unsigned build from an independent developer looks like — signing certificates cost money and identity verification, and both are on the list. Saying so here is better than letting the OS say it first.

### Package managers

Manifests are written and rendered against each release ([`packaging/`](packaging/)), but none of them is submitted yet — each ecosystem has a blocker that only a human can clear, listed in [`packaging/README.md`](packaging/README.md).

| | Command, once it lands | Blocked on |
|---|---|---|
| Homebrew | `brew install --cask kavka` | Apple notarization; homebrew-cask's notability threshold |
| winget | `winget install SahilHirani.Kavka` | Windows code-signing certificate |
| Chocolatey | `choco install kavka` (elevated — it installs the MSI per-machine) | a chocolatey.org account, and first-package moderation |

### Try it without a cluster

Kavka's first-run screen offers to start a single-node Kafka in Docker on `localhost:19092`, seed it, and save a connection called *Playground* — about thirty seconds, one click. **Kavka does not bundle a broker**: Apache Kafka is a JVM application, so bundling one means shipping a Java runtime, which is exactly what this app exists not to be. With Docker installed the button works; without it, the screen says so in one sentence rather than offering a button that cannot.

The compose file it runs ships inside the app ([`apps/desktop/src-tauri/playground/docker-compose.yml`](apps/desktop/src-tauri/playground/docker-compose.yml)) and is deliberately **not** `dev/docker-compose.yml`: the dev cluster is a test fixture with an authorizer and a feature upgrade in it, and it binds 9092, which is the port whatever Kafka you already run is on.

### What Kavka sends

Nothing. **There is no telemetry endpoint in the app** — not a disabled one, not one behind a flag. There is no analytics, no update ping, no crash reporter phoning home, and no account.

The one adjacent feature is **opt-in diagnostics**, off by default, in the About panel. Turned on, it writes rotating text files (5 files, 512 KB each, 2.5 MB total) into the app's data directory: Rust panics, uncaught errors in the window, and one line per launch with the version and OS. It never logs payloads, keys, headers or anything read out of your keychain. There is an *Open logs folder* button and a *Delete them* button beside it, and the file is plain text so you can read it before you attach it to an issue.

## The companion CLI

`kavka` is the same product without the window: it reads the connections the desktop app saved on this machine — the same `profiles.json`, the same OS keychain, the same read-only flag and the same display-masking rules — so there is nothing to configure. Point it at a saved connection and it sees exactly what the app sees.

```sh
cargo build -p kavka-cli --release    # target/release/kavka
```

```sh
kavka profiles list
kavka -p orders topics list
kavka -p orders topics detail orders
kavka -p orders fetch orders --last 20
kavka -p orders search orders "failed" --earliest
kavka -p orders search orders --cel 'value.amount > 100' --since 2h
kavka -p orders sql orders "select count(*) from messages" --earliest
kavka -p orders groups detail checkout-service
kavka -p orders produce dead-letter --key A-102 --json '{"retry":1}'
```

Seek flags mirror the app's own — `--earliest`, `--last N`, `--partition P --offset N`, `--since 2h|<epoch ms>` — and every read is bounded, with the bound reported rather than applied silently. Every numeric flag states its range in `--help` and clap enforces it, so a number past a ceiling is refused with the bound named rather than quietly clamped.

A produce payload can stay off the command line: **`--value-file <path>`** reads it from a file and `--value-file -` from standard input, because an argv is visible to every other account on the machine while the command runs and lands in the shell history afterwards — `--value`'s own help says so.

**stdout is the answer; stderr is everything Kavka has to say about it.** A terminal gets a table, a pipe gets NDJSON, and nothing has to be passed for either:

```sh
kavka -p orders fetch orders --last 100 | jq -r '.value.orderId'
kavka -p orders search orders "timeout" --earliest > hits.ndjson
```

`--output table|ndjson|json` overrides the detection; `json` is one document carrying the rows *and* the caps, progress and masking state they came with. Progress, cap notices, the masking notice and every error go to stderr, so a redirect is always clean.

Errors come from the same library the app's banners use (docs/DESIGN.md §7): a plain title, the fix, and the broker's own reply under `--details`.

| Exit | Meaning |
|---|---|
| 0 | Answered. **An empty result is still an answer** — a search with no matches exits 0, deliberately unlike `grep`. |
| 1 | The cluster, the keychain or the profile file failed. |
| 2 | The command line was wrong. |
| 3 | A guardrail refused it — read-only, or `prod` without `--yes-prod`. Nothing was sent. |
| 4 | No such connection on this machine. |

The write surface is one command, `produce`, behind two gates: a connection marked read-only refuses and no flag lifts it, and a connection tagged `prod` needs `--yes-prod` on the command line. Every command on a prod connection also prints `! PROD · <name> · <bootstrap>` to stderr first — in words, so it survives a pipe, a log and colour-blindness. There is deliberately no `delete-topic`, no config editing and no offset reset: those belong behind the app's confirmations, where the blast radius can be stated before the click.

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

Feature-complete through Phase 5 (see [docs/ROADMAP.md](docs/ROADMAP.md)); Phase 6 polish is in progress. The repo stays private until the Phase 6 launch.

Distribution is prepared but not executed — everything below needs a human, money or an account, and none of it can be done from this repository:

- **Domain** — `kavka.io` was verified unregistered on 2026-08-02. Register it, point it at GitHub Pages, and add a `CNAME` to `docs-site/`.
- **macOS** — Apple Developer Program membership, a Developer ID Application certificate, and `notarytool` credentials in the release workflow. Until then the `.dmg` is unsigned and Gatekeeper says the app is "damaged", which is the wrong message for the truth.
- **Windows** — an OV or EV code-signing certificate. Until then SmartScreen warns on every first run.
- **Package managers** — a fork and a pull request each for winget-pkgs and homebrew-cask, a chocolatey.org account and API key. Manifests are rendered onto every release; see [`packaging/README.md`](packaging/README.md).
- **The site** — [`docs-site/`](docs-site/) is written and has three `TODO` screenshot placeholders in it.
