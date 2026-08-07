# macOS testing results

Run of [`MACOS-TESTING.md`](MACOS-TESTING.md), executed end to end on 2026-08-07.
**All five tests pass**, including Test 3 — the updater's macOS leg, the one
path that had never been executed. One non-blocking finding is written up under
[First-launch Gatekeeper stall](#finding-first-launch-gatekeeper-stall).

```
Build tested: OLD=v0.1.0-build.22  NEW=v0.1.0-build.34
macOS version: 26.5.2             Chip: Apple Silicon (arm64)

1 Gatekeeper wording (verbatim): "Kavka" is damaged and can't be opened.
                                 You should move it to the Trash.
1 Install + first launch: PASS
2 Docker from Finder:     PASS
3 Updater offer shown:    PASS
3 Install behaviour:      auto-relaunch
3 About shows NEW build:  PASS
3 No second Gatekeeper:   PASS
4 System theme follows:   PASS
5 Notification Center:    PASS
```

Docker Desktop 29.6.2. The DMG was downloaded in Chrome (quarantine flag
intact — SHA-256 verified against the release's `SHA256SUMS.txt`), not `curl`ed.

---

## Test 1 — Fresh install and Gatekeeper's exact words

**PASS.** The dialog says, verbatim:

> **"Kavka" is damaged and can't be opened. You should move it to the Trash.**
>
> Chrome downloaded this file today at 8:41 AM.
>
> `[Move to Trash]` `[Cancel]`

Two details the README should carry, because both shape how a user reacts:

- The default button is **Move to Trash**, and it is the *only* affirmative
  button — there is no "Open anyway". A user following the dialog destroys the
  app.
- The subtext names the browser and time, which reads as corroboration that the
  download really is broken.

`xattr -cr /Applications/Kavka.app` clears it. The app then opens to the Jackdaw
shell in about 6 seconds — one rail, brand lockup top-left, warm dark following
the Mac's appearance. Settings → About reads `0.1.0 · Build 22`, matching the
OLD tag.

## Test 2 — Docker detection from Finder

**PASS, and it genuinely exercised the fallback.** Launched from Finder, Kavka
inherits launchd's minimal environment:

```
PATH=/usr/bin:/bin:/usr/sbin:/sbin
```

On this machine `docker` lives at `/usr/local/bin/docker`, which is **not** on
that PATH — `PATH=/usr/bin:/bin:/usr/sbin:/sbin command -v docker` finds
nothing. So the PATH probe must miss and the candidate list must do the work.
It did: Kavka reported *"Looking for Docker"* ✓, *"Reading the bundled compose
file"* ✓, then started `kavka-playground-kafka` and auto-connected on
`localhost:19092`. It never claimed Docker wasn't installed.

> **Methodology note, worth keeping.** The first attempt at this test was
> invalid and passed anyway. Launching via `open -a Kavka` from a terminal
> propagates the calling shell's full environment, so the app saw Homebrew and
> `/usr/local/bin` on its PATH and found Docker through the probe — proving
> nothing about the fix. Only `Finder`-initiated launch (or `osascript -e 'tell
> application "Finder" to open …'`) produces the minimal PATH. Anyone re-running
> this must check the app's actual PATH with `ps -Eww -p <pid>` before trusting
> a pass.

## Test 3 — The updater's macOS leg

**PASS on every sub-step.** This is the result that matters most; it was the
only never-executed path in the product.

The offer banner reads **"Kavka 0.1.0-build.34 is available"** — it names the
build, which is more useful than the "Kavka 0.1.0 is available" the handoff
predicted. Copy is accurate: *"Nothing has been downloaded; Kavka fetches the
installer only when you press Install, and checks it against Kavka's own signing
key before anything runs."* Buttons are `Release page` / `Install…` / `Not now`.

Pressing **Install…**:

- The app closed on its own, and **reopened by itself** as the new build.
  Elapsed from click to a running new binary: ~5 seconds.
- The bundle really was swapped — `Contents/MacOS/kavka-desktop` SHA-256 changed
  from `2c02bbd2…` to `65abc84a…`.
- Settings → About now reads `0.1.0 · Build 34`.
- **No second Gatekeeper dialog, and no `xattr` needed.** The installed bundle
  carries no `com.apple.quarantine` attribute at all, because Kavka wrote it
  rather than a browser. Confirmed with `xattr -l /Applications/Kavka.app`
  (empty).
- Saved connections survived the update.

So the macOS relaunch path (`app.request_restart()`, distinct from the Windows
installer path) works unattended on real hardware.

## Test 4 — System theme follows live

**PASS.** With Theme set to **System**, flipping macOS System Settings →
Appearance re-inks Kavka live in both directions with no restart and no visible
repaint lag. The Settings Perch sentence names what it is showing:

> Everything here applies as you change it and is saved on this machine. Kavka
> is showing the Dark theme right now.

## Test 5 — An alert reaches Notification Center

**PASS.** Against the dev cluster (`dev/docker-compose.yml`), with a
`demo-checkout` / `payments` / >50-behind / 1-minute rule and the OS notification
channel on, a hot-partition backlog produced both halves:

- **macOS Notification Center banner**, Kavka icon, titled *"demo-checkout behind
  on payments"* with body *"demo-checkout is 80 messages behind on payments
  partition 0; the threshold is 50"*.
- **In-app toast** with the **View group demo-checkout** action.

The incident round-tripped correctly in `alerts.json`: it fired, then resolved
in place on the same record (`resolved_ms` set, `detail` updated to *"demo-checkout
is at most 0 messages behind"*) rather than appending a second row.

> **One-time speed bump.** The *first* alert fired while macOS was still showing
> its notification-authorization prompt for Kavka, so that first banner was
> swallowed. Once notifications were allowed, the re-fired alert delivered
> normally. This is standard macOS first-run behaviour, not a Kavka bug, but it
> does mean **the very first alert on a fresh install may never reach the user**.
> Worth considering whether Kavka should request notification authorization at
> the point the user enables the OS-notification channel, rather than lazily on
> the first fire.

---

## Finding: first-launch Gatekeeper stall

Not a test failure — the documented path works — but it will generate support
tickets, so it is written up in full.

The release bundle is **ad-hoc / linker-signed and not notarized**:

```
$ codesign -dv /Applications/Kavka.app
CodeDirectory ... flags=0x20002(adhoc,linker-signed)
Signature=adhoc
TeamIdentifier=not set
```

On the **first ever launch of a never-before-seen build**, with the quarantine
flag already removed, the app hung with no window and no dialog for **over four
minutes**, sitting at `_dyld_start`. Concurrent system logs:

```
kernel  AMFI: '…/Kavka.app/Contents/MacOS/kavka-desktop' has no CMS blob?
kernel  AMFI: '…kavka-desktop': Unrecoverable CT signature issue, bailing out.
syspolicyd  [C569] Receive failed with error "Operation timed out"   (QUIC, 43s)
```

The same binary launched **in about 5 seconds with Wi-Fi turned off**, and every
subsequent launch was fast once the assessment was cached. That points at
Gatekeeper's online notarization/CT check stalling rather than a defect in the
binary.

**Impact.** A user on a slow, filtered, or captive-portal network can double-click
a correctly installed Kavka and get a bouncing icon and nothing else for minutes,
with no error to search for. It is once per build, not once per launch.

**Fix.** Notarize the macOS bundle (requires an Apple Developer ID). That removes
this stall *and* removes the "damaged" dialog in Test 1 entirely, which is the
single largest first-run friction point on macOS. If notarization isn't on the
table yet, the README should say that the first launch of a new build can take
several minutes on some networks.

## Also observed

- `docker_binary()`, the updater, and notification delivery write **nothing** to
  disk on any code path. `tracing` is a dependency but no subscriber is ever
  installed, so the `tracing::warn!` calls in the alert and notification paths go
  nowhere. The diagnostics log (About → write a diagnostics log) records only
  panics, webview errors and the session header — it would not have helped
  diagnose any of the above. Every finding here came from `log show`, `sample`,
  and reading `alerts.json`. Consider routing at least updater and Docker-resolver
  outcomes into the diagnostics log.
- The build number is not discoverable from outside the app. `CFBundleVersion`
  and `CFBundleShortVersionString` are both `0.1.0` for every build, nothing in
  `Contents/Resources` carries it, and the diagnostics session header uses
  `CARGO_PKG_VERSION`. The only external signals are the origin release tag or
  `xattr -p com.apple.metadata:kMDItemWhereFroms` on a browser-downloaded DMG —
  and an in-app-installed update leaves neither. This made "which build is
  actually installed?" answerable only by opening the About dialog. A build
  stamp somewhere in the bundle would make support and scripted testing easier.

## Machine state after the run

Left on **build 34**, quarantine-free, launching normally. The dev cluster was
torn down (`down -v`); the playground containers were stopped with their
`kavka-playground_kafka-data` volume left intact, per Kavka's own stated
semantics. The `local` connection, the `demo-checkout` alert rule and the
"Every build" channel setting were test fixtures created in a config directory
that was empty at the start of the session — they have been removed, so Kavka is
back to a clean first-run state. Re-enable **Settings → Updates → Every build**
to keep tracking automated builds.

Screenshots (Gatekeeper dialog, Docker-from-Finder, the update offer, both About
dialogs, the Notification Center banner) are on the test machine at
`~/Documents/kavka-macos-test-2026-08-07/`.
