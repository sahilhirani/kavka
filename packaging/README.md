# Packaging manifests

Templates for the three package managers Kavka wants to be in, plus the script
that fills them in from a real GitHub Release.

Nothing here is submitted automatically, and nothing here runs on a normal
build. `.github/workflows/release.yml` renders these after a `v*` tag is pushed
and attaches the rendered files to the release as artifacts — so the version,
the URLs and the checksums are always the ones that actually shipped. Opening
the pull request in each package manager's repository is a **human step**, and
it should stay one: every one of these ecosystems has a review queue with
people in it, and a bot that opens the first PR a project has ever sent them is
a bad introduction.

```
packaging/
  render.mjs                        fills the templates from a release
  winget/                           SahilHirani.Kavka — three-file manifest
  chocolatey/                       kavka — nuspec + install script
  homebrew/                         kavka — a cask
  out/                              rendered output (git-ignored)
```

## The artifact names these templates are written against

Tauri's bundler names its output, and `tauri-apps/tauri-action` uploads it under
exactly that name. For `productName: "Kavka"` at version `X.Y.Z` (see
`apps/desktop/src-tauri/tauri.conf.json`):

| Asset | Built by | Format |
|---|---|---|
| `Kavka_X.Y.Z_x64-setup.exe` | NSIS | `{product}_{version}_{arch}-setup.exe`, `arch` ∈ `x64`/`x86`/`arm64` |
| `Kavka_X.Y.Z_x64_en-US.msi` | WiX | `{product}_{version}_{arch}_{language}.msi` |
| `Kavka_X.Y.Z_aarch64.dmg` | DMG (Apple Silicon) | `{product}_{version}_{arch}.dmg`, `arch` ∈ `x64`/`aarch64`/`universal` |
| `Kavka_X.Y.Z_x64.dmg` | DMG (Intel) | as above |

Note the **inconsistency in Tauri's own naming**, which is real and is the thing
most likely to break a hand-written manifest: Windows calls 64-bit ARM `arm64`
and macOS calls it `aarch64`, and `x86_64` is `x64` on both. The templates use
the right one for each platform; `render.mjs` fails loudly if an expected asset
is missing rather than emitting a manifest with a URL that 404s.

If `productName` ever changes, or a target is added (`.app.tar.gz` appears the
moment the updater is switched on, `universal-apple-darwin` replaces the two
DMGs if the build is ever unified), these names change with it and this table is
what has to be re-checked first.

## Rendering

```sh
node packaging/render.mjs --version 0.1.0 --assets ./dist-assets
```

`--assets` is a directory holding the four files above; the script hashes them,
substitutes every `{{PLACEHOLDER}}`, and writes `packaging/out/` (or `--out
<dir>`). Run it without `--assets` and it substitutes the version and URLs but
leaves the checksums as `TODO-...`, which is useful for reading the output and
useless for submitting it — deliberately, so a manifest with a placeholder
checksum cannot be mistaken for a finished one.

**CI runs exactly that mode on every pull request** — `node packaging/render.mjs
--version 0.0.0 --out "$RUNNER_TEMP/pkg"` — because otherwise this script is
only ever executed on release day. It cannot check a checksum without a build,
but it does check the half that rots: that every template still parses and that
no template has grown a `{{PLACEHOLDER}}` the renderer does not know how to
fill. Both of those are errors, not warnings, so the job goes red at pull-request
time instead of after the tag is pushed.

## Submitting, per ecosystem

Everything below is a human action. Each is written as the first submission;
subsequent versions are the same steps with a new version number.

### winget — `SahilHirani.Kavka`

1. Fork [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs).
2. Copy `out/winget/*.yaml` to
   `manifests/s/SahilHirani/Kavka/<version>/`.
3. Validate locally: `winget validate --manifest <dir>` then
   `winget install --manifest <dir>` on a clean machine.
4. Open a PR. The pipeline runs its own validation and a smoke install.

**Blockers a human has to clear first**

- **Code signing.** An unsigned installer passes winget validation but every
  user meets SmartScreen ("Windows protected your PC") on first run, and that
  reads as malware, not as an unsigned indie build. An EV or OV code-signing
  certificate is the fix, and it is a purchase plus an identity verification —
  nothing in this repository can do it.
- **`ProductCode` for the MSI entry.** Left commented out in the installer
  manifest because it does not exist until a real MSI has been built: read it
  off the artifact with `Get-AppxPackage`-style tooling, or
  `msiexec /a Kavka_X.Y.Z_x64_en-US.msi /qb TARGETDIR=…` and inspect the
  property table. Without it winget's upgrade detection falls back to display
  name and version matching, which mostly works and occasionally does not.
- **Publisher name consistency.** `Publisher` in the locale manifest must match
  the certificate's subject once signing exists, or validation complains.

### Chocolatey — `kavka`

1. `choco pack out/chocolatey/kavka.nuspec`
2. Test on a clean box, **from an elevated shell**: `choco install kavka -s .`
   then `choco uninstall kavka`.
3. `choco push kavka.<version>.nupkg --source https://push.chocolatey.org/`
   with an API key from a chocolatey.org account.

**Decision: this package installs the MSI, per-machine.** It used to point at
the NSIS `.exe`, on the reasoning that `/S` is a real silent switch and a
per-user install always completes. That is true and it is the wrong trade for
this ecosystem:

- **Chocolatey's population is fleets, and fleets run as somebody who is not
  the user.** Ansible, Puppet, DSC, a Packer image build and a corporate
  onboarding script all invoke `choco install` elevated, often as SYSTEM.
  Tauri's NSIS bundle installs **per-user** by default, so under those runners
  it lands in the service account's profile: the package reports success, and
  the person who actually sits at the machine has no Kavka. A silent
  wrong-profile install is a worse failure than a loud refusal, and it is
  remote-diagnosed at the cost of a support thread.
- **`msiexec /qn` is exactly as silent as `/S`.** The original argument was
  about silence, and it does not survive contact with the MSI: the only thing
  the NSIS bundle actually buys is not needing elevation.
- **What it costs:** a non-admin `choco install kavka` now fails outright
  instead of installing for the current user. That case has two good answers
  that are one line each — `winget install SahilHirani.Kavka` (whose default
  scope is the NSIS per-user build) or the `.exe` on the releases page — and
  they are in the package description so nobody has to find out by failing.
- **Exit codes 3010 and 1641** are in `validExitCodes`: both mean "installed,
  wants a restart", and treating them as failures is the standard MSI
  packaging bug.

winget keeps offering both, because it can: `Scope: user` on the NSIS entry and
`Scope: machine` on the MSI, chosen with `--scope`. Chocolatey has one
installer per package, so it gets the one that is right when the shell is
elevated.

**Blockers a human has to clear first**

- **A chocolatey.org account and API key.** There is no way to publish without
  one, and the key must never enter this repository — if the release workflow is
  ever taught to push, the key belongs in a GitHub environment secret with a
  required reviewer.
- **Moderation.** The first version of a new package is reviewed by a human and
  routinely takes days. Automated verification will flag the unsigned installer.
- The package downloads from GitHub rather than embedding the installer, so no
  `VERIFICATION.txt` is required — but the checksums in
  `chocolateyinstall.ps1` are mandatory and are what the moderators check.
- **The MSI's `ProductCode`** is the same missing value the winget manifest
  records: it does not exist until WiX has run. Chocolatey does not need it —
  its auto-uninstaller reads the uninstall entry the MSI registers — which is
  why there is no `chocolateyuninstall.ps1` here and should not be one.

### Homebrew cask — `kavka`

1. Fork [`Homebrew/homebrew-cask`](https://github.com/Homebrew/homebrew-cask).
2. Copy `out/homebrew/kavka.rb` to `Casks/k/kavka.rb`.
3. `brew audit --new --cask kavka` and `brew install --cask kavka`, then
   `brew uninstall --cask kavka`.
4. Open a PR.

**Blockers a human has to clear first**

- **Notarization.** Homebrew will accept an unsigned cask, but Gatekeeper will
  not: users get "Kavka.app is damaged and can't be opened" — which is what
  macOS says about an unsigned app that arrived quarantined, and it is a worse
  message than the truth. Fixing it needs an Apple Developer Program membership
  ($99/year), a Developer ID Application certificate, and `notarytool`
  credentials in the release workflow. Until then the cask should not be
  submitted, and the README's download instructions carry the right-click →
  Open workaround instead.
- **Notability.** homebrew-cask requires a project to be somewhat established
  before it accepts a cask (the current rule of thumb is 30 forks, 30 watchers
  or 75 stars, and the repo is currently private). Submit after launch, not
  with it.
- **A stable download URL.** The `url` stanza interpolates `#{version}` and
  `#{arch}` so a version bump is a two-line change; keep the release asset
  naming stable or every future bump becomes a rewrite.

## What is deliberately not here

- **An auto-update feed.** Tauri's updater needs a signing keypair and a
  published `latest.json`; that is a separate decision with its own key
  management, and adding `createUpdaterArtifacts` changes the release asset
  list this whole directory is written against.
- **A Linux package.** Kavka targets macOS and Windows (README); AppImage and
  `.deb` come out of the same bundler, so the day that changes, add a fourth
  template rather than bending one of these.
- **Any credential.** No API keys, no certificates, no notarization profiles.
  Every one of them is a human action listed above.
