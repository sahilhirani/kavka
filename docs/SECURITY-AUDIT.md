# Kavka — security sweep

**Date:** 2026-08-05. **Branch:** `chore/node-security-sweep`. **Version at the
time:** 0.1.0, pre-1.0, no stable release yet.

**Why now.** `release.yml` publishes signed installers on every merge to `main`,
and existing installs pick them up through the in-app updater. A mistake in the
supply chain here does not stay in the repository — it reaches machines. This
sweep ran before 1.0.0 for that reason, and the release pipeline got as much
attention as the product code.

**What this document is.** The honest record: what was examined, what was
deliberately *not* examined, every confirmed finding with its severity and
whether it was fixed on this branch or left open, the one finding that was
refuted, and the tool versions behind the numbers. A finding being listed here
and not fixed is a decision, not an oversight — the reasoning is with it.

**Method.** Five parallel audits, each with its own scope: Rust supply chain,
JS/npm supply chain and CI, the Tauri desktop shell, the wire protocol / MCP /
sandbox internals, and repository-and-CI configuration. Every finding was then
put through a separate verification pass whose job was to *refute* it — check
the evidence first-hand, look for a worse consequence the auditor missed, and
challenge the proposed fix. Several findings were corrected by that pass; one
was refuted outright and is recorded below. Fixes were applied only where
verification approved them as correct, minimal, and safe.

**Result.** 38 findings (43 raw, less five that two auditors found
independently). **19 fixed on this branch, 18 recorded and left open with a
reason, 1 refuted.**

No **high**-severity finding is left open. Of the four **medium**s this
document originally left open, three have since been closed outside this
branch — RS-01 (`--locked`, PR #32 with a follow-up shell fix), CI-02 (the
signing key now lives in the `release` environment) and CI-03 (secret scanning
and push protection are on). One remains: B2 (CEL evaluation has no budget). Two of those four
are repository settings that no commit can change (§4); the other two are open
because the proposed fixes were verified **wrong** — applying them as written
would have broken CI in one case and the macOS Intel release leg in the other.
The corrected fixes are recorded with each.

---

## 1. Scope

### Examined

**Rust dependency supply chain.** The root `Cargo.lock` (752 dependencies)
through `cargo-audit` and `cargo-deny` (advisories, bans, licenses, sources),
resolved with `all-features = true` so the gated tiers (`kafka`, `kafka-ssl`,
`msk-iam`, `wasm-serdes`) were all in the graph rather than just the default
one. The standalone `docs/examples/wasm-serde` crate — a separate workspace with
its own lockfile — audited separately: clean. All five workspace manifests read
in full. Absence of `.cargo/config.toml`, `rust-toolchain.toml`,
`[patch]`/`[source]`/`replace-with`, and non-crates.io `source =` lines in the
lockfile confirmed by direct search. Per-target reachability of every advisory
crate checked with `cargo tree --target` for all three shipped targets.

**JS/npm supply chain.** `npm audit` (production and dev separated) and
`npm audit signatures`. Programmatic forensics over all 207 `package-lock.json`
entries: resolved host, scheme, integrity algorithm, git/file/http detection,
missing-`resolved` detection, and an enumeration of every package with an
install script (two: esbuild and one transitive). Registry `repository.url` and
`dist-tags.latest` checked for all 13 direct dependencies as a typosquat check.
`.npmrc` search across the repo. Frontend remote-resource loading in source, in
`index.html`, in the vite config, and in the built `dist/` bundle.

**CI/CD and repository configuration.** All three workflow files read line by
line; all 41 `${{ }}` expressions in `.github/` enumerated and classified by
sink; action refs resolved live against the GitHub API and annotated tags
dereferenced to commits. Live repository settings pulled with authenticated
`gh api`: visibility, security-and-analysis, Actions permissions, branch
protection, rulesets, tag protection, private vulnerability reporting,
collaborators, deploy keys, environments, and secrets.

**Secrets in history.** All 273 tracked files, then all 48 commits reachable
from every ref, then a blob-level sweep of all 771 objects from
`git cat-file --batch-all-objects` — which covers unreachable and dangling
objects that a `git log -p` sweep would miss. Patterns: PEM private-key blocks
of every type, the minisign/rsign secret-key header, GitHub token prefixes, AWS
key ids, Slack and SendGrid and OpenAI and Google key shapes, and
credential-literal assignments. Every filename ever added (276) checked against
sensitive extensions; all three files ever deleted inspected. **Nothing was
found.**

**Desktop shell.** `update.rs` in full including tests, `tauri.conf.json`, the
capabilities ACL expanded against the app's own generated ACL manifest rather
than from memory, the updater/diagnostics/danger/window-title frontend modules,
and the vendored `tauri-plugin-updater` and `tauri-plugin-opener` sources for
the guarantees the app relies on. `lib.rs` is 7,400 lines with 102
`#[tauri::command]`s: every command signature was enumerated and every command
touching a shell, a path, a URL, a secret or the updater was read in full. The
remaining ~90 are Kafka pass-throughs into `kavka-core`.

**Protocol, MCP and sandbox internals.** `protocol/tls.rs`, `protocol/sasl.rs`,
`protocol/scram.rs`, and the MCP write gate read in full. Targeted reads of
`protocol/conn.rs` (connect, frame bounds, ApiVersions decode), `protocol/wire.rs`
(decoder length handling), `wasm_serde.rs` (constants, load, instantiate, run,
path validation), the MCP server's dispatch, both write tools, masking,
tool-result shaping and `initialize` text, `search.rs` (CEL filter and scan
loop), `sql.rs` (options, session, execute, caps), `connection.rs`, `auth/oidc.rs`,
`masking.rs`, and the profile store's export/import paths.

### Not examined, and why

- **Runtime behaviour of any workflow.** Every CI finding comes from reading
  YAML and from the GitHub API, not from observing a run. No workflow was
  dispatched or re-run. Where that matters — the `--locked` passthrough in
  RS-01 — the finding said so and stayed open until the passthrough-free
  fix landed in PR #32; its first real release run then exposed a Windows
  shell bug in the preflight, fixed the same day. Both are recorded in the
  RS-01 entry below.
- **The vendored C sources of librdkafka and OpenSSL.** Version numbers were
  compared against crates.io and against upstream release notes; not one byte of
  the vendored trees was inspected or diffed.
- **Linux and macOS builds.** Everything was built and run from Windows. The
  per-target dependency claims come from cargo's own resolver, which is
  authoritative for dependency graphs, but were not cross-checked against a real
  macOS or Linux build.
- **No dedicated secret-scanning binary was run.** `gitleaks` and `trufflehog`
  are not installed on this machine and their output was **not** simulated. The
  history sweep above is hand-rolled pattern matching, which is weaker than
  either tool on high-entropy secrets in unknown formats. Closing that gap is
  exactly what finding CI-03 asks for.
- **No dynamic testing of the desktop app.** The app was not launched, the
  updater was not driven against a fixture server, and no exploit was executed
  against a live install. The desktop findings are derived from source.
- **No live exploitation of CI.** Compromising a third-party action repository
  was neither possible nor attempted.
- **Not every transitive package's source.** npm install-script exposure was
  assessed by enumerating which packages have scripts at all and reading
  esbuild's `install.js` — not by auditing all 147 installed packages.
- **Per-API protocol decoders.** `share.rs`, `quotas.rs`, `reassign.rs`,
  `elect.rs`, `meta.rs` and `quorum.rs` — roughly 120 KB of decode paths — were
  confirmed to route through the same framing and the same write gate, but their
  individual decoders were not audited for wire-level bugs.
- **The advisory database is a snapshot.** A clean advisory result expires the
  moment somebody publishes a new one. That is the entire argument for the
  scheduled gate added by RS-02.

---

## 2. Fixed on this branch

Nineteen findings. Each was verified first-hand, and each fix was checked
against the full gate: `cargo fmt --all --check`, clippy at every tier CI runs,
`cargo test` for all four crates including the integration suites against a live
KRaft cluster, and `npx tsc --noEmit && npm test && npm run build` for the
frontend. All green.

| id | severity | what | where |
| --- | --- | --- | --- |
| RS-06 / CI-01 / JS-01 | **high** | Every GitHub Action was on a mutable ref — including `dtolnay/rust-toolchain@stable`, a *branch*, in the job that holds the updater signing key. All pinned to full commit SHAs. | all four workflows |
| A1 / B4 | **high** | Nothing bound a profile's keychain reference to that profile, so an imported profile could name another connection's secret and have it posted to an attacker's URL. | `crates/kavka-core/src/profiles.rs` |
| RS-02 | medium | Nothing in CI objected to a new advisory, a yanked crate, a license conflict or a git dependency. | `.github/workflows/supply-chain.yml`, `deny.toml` |
| JS-02 / CI-08 | medium | `actions/checkout` left the `contents: write` token in `.git/config` for `npm ci`'s install scripts and every `build.rs` to read. | all four workflows |
| JS-05 / CI-04 | medium | No Dependabot or Renovate configuration had ever existed. | `.github/dependabot.yml` |
| B1 | medium | MCP display masking failed **open**: an unreadable rules file returned raw payloads with no indicator, while `initialize` promised the rules were honoured. | `crates/kavka-mcp/src/server.rs` |
| B3 | medium | DataFusion ran with an unbounded memory pool; a permitted self-join of the one registered table reached 7.3 GB of working set from 20,000 rows. | `crates/kavka-core/src/sql.rs` |
| B5 | medium | The OIDC token endpoint was accepted over plain `http`, putting the client secret — longer-lived than the token it buys — on the wire in the clear. | `crates/kavka-core/src/auth/oidc.rs`, `ProfileEditor.tsx` |
| A3 | low | The diagnostics panel promised the log holds no credentials; the writer did no redaction of any kind. | `apps/desktop/src-tauri/src/lib.rs` |
| B6 | low | A pre-authentication ApiVersions reply could drive a multi-gigabyte allocation from a declared array count nothing bounded. | `crates/kavka-core/src/protocol/conn.rs` |
| B7 | low | A serde plugin was read and compiled with no size ceiling, on the thread opening the connection. | `crates/kavka-core/src/wasm_serde.rs` |
| B8 | low | `tool_error` returned broker-supplied text as a bare, undelimited string — the one un-fenced channel from cluster data into a model's context. | `crates/kavka-mcp/src/server.rs` |
| B9 | low | The TLS hint sold encryption as a connectivity fix and never said that SASL/PLAIN with TLS off puts the password on the wire in cleartext. | `ProfileEditor.tsx` + all six i18n catalogs |
| JS-03 | low | `actions/setup-node@v4` runs on the node20 Actions runtime GitHub is retiring. Moved to v7. | `ci.yml`, `release.yml` |
| CI-06 | low | `${{ github.ref_name }}` was interpolated into four `run:` bodies; git permits `$(…)` in a tag name and the trigger filter is `v*`. | `release.yml` |
| CI-07 | low | `ci.yml` declared no `permissions:` block, inheriting whatever the repo default happened to be. | `ci.yml` |
| RS-08 | info | No crate set `publish = false`, so the dependency gate treated first-party AGPL code as publishable. | root `Cargo.toml` + four members |
| A6 | info | A comment claimed the notification plugin is granted `notification:default`; the capability file deliberately says the opposite. | `apps/desktop/src-tauri/src/lib.rs` |
| CI-09 | info | CI and release pinned Node 22; the target LTS and the local toolchain are 24. | `ci.yml`, `release.yml` |

### Notes on three of them

**A1/B4 was found twice, with two different proposed fixes.** Both auditors
described the same vulnerability. The fix that landed is A1's — refuse, at the
store door, any profile whose `SecretRef` names an entry not prefixed with that
profile's own id — because it covers both write paths (`upsert` and `import`)
with one check and needs no signature changes. B4's alternative (rewrite entry
names during import) was not applied: it depends on exhaustively enumerating
every secret-bearing field, and an enumeration that silently misses one looks
complete while still leaking.

The check is strict on the **id only, never the suffix**. The suffixes in the
wild are not the closed set `secrets::SECRET_SUFFIXES` lists — a Schema Registry
password is stored as `schema_registry_password`, and each Connect cluster's
suffix contains the cluster name — so a suffix-strict check would have rejected
profiles that real installs already have on disk. Validation is on write only;
existing files load exactly as before.

One test fixture had to change with it. `every_auth_variant` built profiles with
ids `p0`…`p6` while hardcoding their secret entries as the literal
`p/password`, `p/client_key` and `p/client_secret`. Harmless while nothing
checked — those fixtures never touch a keychain — but not a shape any real
install writes, since the editor names every entry `{profile_id}/{suffix}`. The
fixture now derives its entries from the owning profile's id, and one assertion
that pinned the old literal was updated with it.

**A3's redaction is a backstop, not a proof.** It removes `Authorization:`
headers, the librdkafka `*.password=` config keys, bare `password=`/`secret:`
assignments, and long high-entropy runs that look like base64. It is hand-rolled
rather than regex-based for two reasons: it runs inside the panic hook, where it
must not panic, and a regex engine would be a new dependency in the shipped
binary.

**It now covers a third caller, by construction rather than by promise.** The
shell installs a `tracing` subscriber that routes its own WARN and ERROR events
into the same log — the change that made "a webhook that refuses is written to
Kavka's log" true, having been false since that sentence shipped. It reaches the
file through `diagnostics_write`, the one function that both checks the user's
opt-in and redacts, so there is still exactly one writer and one redaction pass;
routing them past it would have opened a second, unsanitised path into a file
the About panel makes promises about. The subscriber's filter is level and
target only — WARN and ERROR, from this crate's targets — so no dependency's
logging is published into a user's file, and it stays a discard while the
toggle is off. The base64 rule is deliberately narrow — 40+ characters, mixing case and
digits — so that it cannot eat the backtraces the log exists to capture. It will
not catch every encoding of every secret. Three tests hold both halves: nothing
credential-shaped reaches the file, ordinary diagnostics survive intact, and a
credential sitting on the truncation boundary is still redacted (redaction runs
before truncation, so half a password cannot escape by no longer matching).

**RS-02's gate starts green and stays meaningful.** `deny.toml` seeds sixteen
accepted advisories, every one of them an unmaintained-crate notice with no fix
available, split into two clearly-commented blocks: five that genuinely ship
(the `unic-*` family, reached through Tauri's `urlpattern`) and eleven that are
Linux-only and reach nothing this project builds today. Verified locally with
cargo-deny 0.20.2 against this tree: `advisories ok, bans ok, licenses ok,
sources ok`. Any *new* advisory turns it red. The CI job deliberately does not
pin a cargo-deny version — 0.18.4 hard-fails to parse the current advisory
database, and a gate that breaks on new data trains people to ignore it.

---

## 3. Confirmed, not fixed here

Fifteen findings in the code and the dependency tree. Each is real and verified;
none is fixed on this branch, and the reason is stated with it. Three further
open findings are repository settings and are in §4.

### Dependencies

**RS-01 — no `--locked` in any cargo invocation (medium). CLOSED by PR #32
plus a same-day follow-up.** The ten resolving invocations carry `--locked`,
`release.yml` runs a fail-closed `cargo metadata --locked` preflight before
tauri-action, and `cargo deny` audits under `--locked` too. History, kept
because process lessons rot fastest: the first merged preflight omitted
`shell: bash`, so the Windows leg ran it under pwsh, read the /dev/null
redirect as a literal path, and failed every release unconditionally —
fail-closed harder than intended, caught by independent re-verification
before any tag was pushed. The original finding text follows unchanged.

**Original finding:** A manifest change
without a regenerated lockfile causes CI *and* the release build to silently
re-resolve dependencies, including transitive ones nobody reviewed, and the
result is signed and auto-installed. Verified: no `--locked` or `--frozen`
anywhere; `cargo metadata --locked` passes today, so this is a missing guard
rail rather than present drift. **Not fixed because the proposed fix is wrong as
written** — `cargo fmt` has its own argument parser and rejects `--locked`
(reproduced: exit 2), so applying it to the Format step would turn every CI run
red. The corrected fix is `--locked` on the ten dependency-resolving invocations
only, plus a `cargo metadata --locked` preflight in `release.yml` that fails
closed regardless of whether the `-- --locked` passthrough into `tauri-action`
works. That passthrough has never been observed in a real run and fails *open*
if it does not work, which is precisely the kind of thing this branch should not
guess at.

**RS-03 — five unmaintained crates that genuinely ship (low).** The `unic-*`
family, reached through `urlpattern` ← `tauri-utils` ← `tauri`, on Windows and
macOS. There is nothing to fix: `urlpattern` 0.6.0 already dropped `unic`, but
`tauri-utils` requires `^0.3` and no other 0.3.x exists, so neither
`cargo update` nor a `[patch]` can reach it — only an upstream Tauri change can.
Static Unicode tables parsing developer-authored URL patterns. Accepted
knowingly and recorded in `deny.toml`; tracked upstream as
`rustsec/advisory-db#2414`.

**RS-04 — twelve advisories that reach nothing (info).** The GTK3 stack and
`proc-macro-error`, present only on Linux, which `release.yml` does not build.
`cargo tree -i` finds none of them on any of the three shipped targets. Recorded
so the raw "17 warnings" figure is not misread as 17 shipped problems, and
flagged in `deny.toml` with an explicit re-evaluate-before-adding-Linux note.

**RS-05 — the Rust toolchain is unpinned (low).** Every workflow uses
`dtolnay/rust-toolchain@stable` and there is no `rust-toolchain.toml`, so the
compiler that produces signed release binaries changes without any commit.
**Not fixed because the proposed fix breaks the macOS Intel release leg.** A
toolchain file pinning `channel` and `components` but not `targets` would leave
the pinned toolchain without `x86_64-apple-darwin`'s standard library — the
action installs cross-targets into the *stable* toolchain, and rustup does not
auto-add targets that appear only as a `cargo --target` flag. The corrected fix
must also list `targets`. Worth doing deliberately, not as a footnote to a
sweep.

**RS-07 — the rdkafka wrapper is two minor versions behind (low).** 0.37.0
locked, 0.39.0 latest. No advisory has ever been filed against either crate. The
security-relevant half — the librdkafka C library — is current *for this build
configuration*: the CVEs listed against librdkafka 2.14.2 are all in bundled
third-party dependencies that this build does not consume (it links OpenSSL
3.6.3 via `openssl-src`, zlib 1.3.2 via `libz-sys`, and no `curl-sys` at all).
For a 0.x crate, 0.37 → 0.39 is two breaking changes across APIs used throughout
`kavka-core`; it needs its own branch and a run against a real cluster, not a
line in a security sweep.

**JS-04 — `tauri-action` is on the v0 line while v1.0.0 has shipped (low).**
Not taken. v1's release notes change `latest.json`'s URLs (the exact URL the
shipped updater fetches), the updater artifact filenames, and how the action
treats an existing non-draft release. That is a behaviour change on the release
path and belongs on a throwaway tag first. Noted while verifying: the workflow
already passes `uploadUpdaterJson`, which is a **v1 input name** — on the pinned
v0 the input is `includeUpdaterJson`, so that line is currently inert and the
manifest is uploaded only because the v0 default is `true`. Harmless today,
worth knowing before anyone "fixes" it by flipping it to `false`.

**JS-06 — esbuild's postinstall can fetch a binary over raw https (info,
substantially refuted).** The descriptive half is accurate: `install.js` does
contain an https download-and-extract path, and it is latent under `npm ci`. The
security claim that it "bypasses integrity verification" is **false** — the
auditor's evidence quote stopped one line short of
`binaryIntegrityCheck(pkg, subpath, bytes)`, which verifies the downloaded bytes'
sha256 against `esbuild.binaryHashes` pinned in esbuild's own `package.json`.
Since the esbuild package is itself sha512-pinned in the project lockfile, the
chain is unbroken; and in the one threat model where those hashes are
attacker-controlled, the postinstall is already arbitrary attacker code and the
download adds nothing. **Downgraded low → info** and kept only as the true
observation underneath it: a build-time install script performs network egress
in a job that also held a write token — which is what JS-02 fixed.

**JS-08 — four dev-dependency majors available (info).** vite 7→8, vitest 3→4,
typescript 5→7, `@vitejs/plugin-react` 5→6. `npm audit` is clean at 0
vulnerabilities and no advisory exists against any locked version. All four are
dev dependencies. They are maintenance, they are majors, and they want
individually-gated PRs.

### Product code

**B2 — CEL evaluation has no step, fuel or time budget (medium).** One
expression can pin a search worker for an unbounded time per record, and the
answer still reports `stopped_because: "deadline"` as if the timeout had worked.
Real and verified. Two of the auditor's claims were **corrected downward** in
verification and are recorded here so nobody re-inflates them: the NLQ path
*cannot* auto-generate the pathological expression (its emitter produces only
flat predicates and cannot emit comprehensions), and the "110-hour scan" figure
is unreachable while the deadline is active — post-deadline overrun is one or
two in-flight records per worker. **Not fixed** because the proposed AST-walk
heuristic is not a budget: it leaves regex catastrophic backtracking, single-level
`map`/`filter` over large lists, and unbounded string growth, and its
feasibility against cel-rust's public API is uncertain. The separable half — not
labelling an overrun as `"deadline"` — is sound and should be done on its own.

**A2 — the updater's test-only environment hooks ship in release builds (low).**
`KAVKA_UPDATE_ENDPOINT` and `KAVKA_UPDATE_API` are documented as test-only, are
compiled into every build, and no test in the repository uses them — so the
stated purpose is unrealised while the surface is live. Severity **downgraded
from medium**: the impact is bounded to a forced *downgrade* to a genuinely
signed older Kavka, because signature verification is unconditional and the
endpoints are https-forced, and the precondition is an attacker who already has
persistent control of the user's environment variables. **Not fixed** because
the proposal bundles the correct half (gate the hooks behind
`debug_assertions`) with a version-comparator change that is both ineffective
against the attack it targets and a regression against the module's tested
cross-version behaviour. The gating half alone is right and should land
separately.

**A4 — `core:default` grants more than the webview uses (low).** Menu, tray,
image and event-emit commands the frontend never calls, in a capability file
whose own first line says nothing is granted "in case". No exploit path exists
today: no `dangerouslySetInnerHTML`, no `eval`, no CSP script exception, and
devtools are off in release. **Not fixed** because Tauri ACL under-grants fail
only at *runtime*, the proposed replacement permission list is self-admittedly
approximate, and validating it requires exercising the save dialog, the About
links, the update banner and the window title in a running app — which this
sweep did not do.

**A5 — export commands write any path the webview names (low).** The comment
calling the path "trusted" describes a convention, not an enforced check; Tauri
does not bind a command argument to a prior save-dialog result. This is a
post-compromise primitive and no first stage exists in the app today. **Not
fixed:** the structurally correct option (move the picker into Rust) is a real
refactor of dialog filters, extension-driven format selection and cancel
handling, and the alternative is a comment edit that changes nothing.

**A7 — the alert webhook URL is the one credential outside the keychain (low).**
Stored in plaintext in `alerts.json`, and posted to over plain `http` if the
user supplies an `http` URL — which is a deliberately supported case for
internal receivers. The module's own comment admits it. Blast radius is a Slack
incoming webhook: post-only, one channel. **Not fixed:** moving it to the
keychain changes the IPC contract mirrored in TypeScript, needs
keep-the-stored-secret UI semantics, and requires migrating existing
`alerts.json` files.

**B10 — an unknown environment name resolves unprotected (low).** A profile
tagged with an environment this machine defines nothing for never needs
`KAVKA_MCP_ALLOW_PROD`. This is deliberate, tested, and disclosed in three
places including the MCP `initialize` text. Recorded because it *composes* with
A1/B4: a legitimately shared production profile tagged with a name the importing
machine does not define is writable under `ALLOW_WRITES=1` without
`ALLOW_PROD`. Failing closed on every unknown name would break the first-run
experience; the smallest honest change, if it is ever revisited, is a fourth
policy bit.

**B11 — the SCRAM client nonce has a modulo bias, and it does not matter
(info).** Recorded so a future reader knows it was checked rather than skipped.
24 bytes from a CSPRNG over an 88-character alphabet: 256 mod 88 leaves 80
characters at 3/256 and 8 at 2/256, giving 153.96 bits of min-entropy against a
nominal 155.03. The nonce's job — freshness and uniqueness, bound into the
`AuthMessage` — is untouched by a one-bit aggregate loss. The load-bearing
controls (the server nonce must extend the client's; the server signature is
compared in constant time) are present and tested. The only real defect is a
stale doc comment saying "~143 bits", which understates the truth. Not worth a
diff on its own.

---

## 4. Owner actions — settings, not commits

These cannot be fixed by anything in this repository. They need somebody with
admin on the repository, and they are the highest-value items left open.

1. **Create a `release` GitHub Environment and move
   `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` into
   it**, with a deployment branch/tag restriction (`main` + `v*`), then delete
   the repository-level copies and add `environment: release` to the build job.
   Today both secrets are repository-level with no environment gating, so a
   workflow file added on *any* branch can read the key that signs every
   auto-update. Free on public repositories. **(CI-02, medium.)** Sequencing
   matters: adding the `environment:` line before the environment holds the
   secrets is harmless, but deleting the repository-level secrets before that is
   done breaks the next release outright.
2. **Enable secret scanning and push protection** (plus non-provider patterns
   and validity checks). All four are free on public repositories, none changes
   CI behaviour, and enabling them runs a retroactive scan of the existing
   history against an independent detector rather than only the hand-rolled
   patterns this sweep used. The history is clean today; nothing stops or alerts
   on the next commit that is not. **(CI-03, medium.)**
3. **Enable Dependabot alerts and security updates.** Both are currently
   disabled. `.github/dependabot.yml` on this branch opens *version* update PRs;
   it cannot switch alerts on. **(JS-05 / CI-04, medium.)**
4. **Enable private vulnerability reporting.** `SECURITY.md` now points at
   `security/advisories/new`, and that link does not work until this is turned
   on. Every commit email in the history is a `users.noreply.github.com`
   address, so until then there is genuinely nowhere private to report.
   **(CI-05, low.)**
5. **Add a ruleset on `main`** (require a pull request, block force-push and
   deletion) **and a tag ruleset for `v*`.** On a solo repository the value is
   the force-push/deletion block and the checkpoint, not an approval count. The
   tag ruleset also closes CI-06's injection sink at the source.

---

## 5. Refuted

**JS-07 — "the pre-paint inline script survives into the bundle while the CSP
forbids inline script".** Both factual halves are true: the inline theme script
in `index.html` does reach `dist/index.html`, and `tauri.conf.json` declares a
CSP with no `script-src`. The conclusion does not follow. Verified against the
exact pinned versions: `tauri-codegen` 2.6.3 computes a sha256 hash of every
non-empty inline script at build time — active because a CSP is configured and
`dangerousDisableAssetCspModification` is not set anywhere — and `tauri` 2.11.5
merges those hashes into the CSP served for every HTML asset, creating the
`script-src` directive if it is absent. The delivered policy therefore
whitelists that exact block by hash. No blocked script, no cold-start flash, and
no weakening: a synthesised `script-src` of `'self'` plus one hash is stricter
than falling back to `default-src`. The auditor flagged this as unconfirmed
rather than asserting it, which was the right call. Proven at source level for
the pinned versions, not by running a packaged binary.

---

## 6. Tools

| tool | version | notes |
| --- | --- | --- |
| rustc / cargo | 1.97.1 (8bab26f4f 2026-07-14) | unpinned in CI — see RS-05 |
| cargo-audit | 0.22.2 | **not installed on this machine.** Its JSON output was produced by the supply-chain auditor and inspected here; the tool was not re-run. |
| cargo-deny | 0.20.2 | run first-hand against this tree with the committed `deny.toml`: `advisories ok, bans ok, licenses ok, sources ok` |
| RustSec advisory DB | snapshot 2026-08-04, 1,189 advisories | 0 vulnerabilities, 17 warnings (16 unmaintained, 1 unsound) |
| npm | 11.12.1 on Node 24.15.0 | `npm audit`: 0 vulnerabilities |
| gitleaks / trufflehog | **not installed** | output was not simulated; see §1 |

**Numbers worth keeping straight.** `cargo audit` reports 17 warnings. Zero are
vulnerabilities. Five reach shipped binaries and have no available fix (RS-03).
Twelve reach nothing this project builds (RS-04). Sixteen of the seventeen are
what `cargo deny` fails on and are therefore what `deny.toml` names; the
seventeenth is an `unsound` advisory that `deny` does not fail on.

---

## 7. What would make the next sweep better

- Run `gitleaks` and `trufflehog` over the history, or enable GitHub's secret
  scanning and let its retroactive pass do it. The current coverage is the
  weakest part of this audit and it is the part guarding the least recoverable
  failure.
- Drive the updater against a fixture server and exercise the capability ACL in
  a running app. Three open findings (A2, A4, A5) are open partly because this
  sweep was static.
- Audit the per-API protocol decoders listed in §1 as unexamined.
- Re-run the dependency gate the day a Linux release target is added; twelve of
  the accepted advisories are accepted only because Linux is not built.
