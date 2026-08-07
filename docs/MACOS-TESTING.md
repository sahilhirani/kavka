# macOS testing handoff

Run these on the MacBook, top to bottom, and paste the report at the bottom
back into the Claude session. Everything here tests a path that **cannot be
exercised from the Windows machine** — each block says what it proves.

**This script has been run once, on 2026-08-07.** What it found is in
[`MACOS-TESTING-RESULTS.md`](MACOS-TESTING-RESULTS.md) — read that before
re-running, because two of its findings (the first-launch stall and the
swallowed first notification) change what you should expect to see.

Two builds matter. **OLD** = any build before `v0.1.0-build.29` (use
[`v0.1.0-build.22`](https://github.com/sahilhirani/kavka/releases/tag/v0.1.0-build.22)).
**NEW** = the newest on the [Releases page](https://github.com/sahilhirani/kavka/releases)
(`v0.1.0-build.31` at the time of writing; newer is fine). Install OLD first —
the whole point of Test 3 is watching it become NEW by itself.

Prerequisites: Docker Desktop installed and running (Tests 2 and 5). Nothing
else — no toolchain, no repo build.

---

## Test 1 — Fresh install and Gatekeeper's exact words

*Proves: the documented install path on current macOS, and captures the precise
Gatekeeper wording we quote in the README.*

1. Download `Kavka_0.1.0_aarch64.dmg` from the **OLD** build's release page in
   Safari or Chrome (the quarantine flag is the test — don't `curl` it).
2. Open the DMG, drag Kavka to Applications, eject, double-click Kavka.app.
3. **Write down the exact dialog text** (screenshot it). Expected: something
   like *"Kavka is damaged and can't be opened."* — false, and the wording we
   warn about.
4. In Terminal:

   ```sh
   xattr -cr /Applications/Kavka.app
   ```

5. Launch again. Expected: the app opens to the Jackdaw shell — one rail,
   brand lockup top-left with the version, warm dark (or light, following the
   Mac's appearance).
6. Settings → About → note the **Build** number. It should match the OLD tag.

## Test 2 — Docker detection from Finder (the launchd PATH fix)

*Proves: PR #9's `docker_binary()` resolver on a real Mac. GUI apps get
launchd's minimal PATH — this fix has never been verified natively.*

1. Docker Desktop running. Launch Kavka **from Finder/Dock** (not from a
   terminal — a terminal launch inherits your shell PATH and proves nothing).
2. Connections → the **Playground** connection → try to start it (or any
   surface that mentions Docker).
3. Expected: Kavka **finds Docker** and does not claim it isn't installed.
   If it refuses, the refusal must **name the paths it checked** — screenshot
   whatever it says either way.

## Test 3 — THE ONE THAT MATTERS: the updater's macOS leg

*Proves: the only never-executed path in the product — signature-verified
download, the `.app` bundle swap, and the explicit relaunch that macOS needs
(Windows relaunches via its installer; the Mac path is different code).*

1. In the OLD build: Settings → **Updates** → set **Which releases** to
   **Every build** → **Check now**.
2. Expected: the banner offers the NEW build — *"Kavka 0.1.0 is available"*
   with **Install…** and **Not now**, and copy saying nothing downloads until
   you press Install.
3. Press **Install…** and watch. Record exactly what happens:
   - [ ] downloaded and verified, then the app **closed and reopened by
     itself** as the NEW build, or
   - [ ] closed and you had to reopen it manually, or
   - [ ] anything else (error text verbatim, please).
4. After relaunch: Settings → About → **Build** must now be the NEW number.
5. **Gatekeeper check, important**: the update was installed by Kavka itself,
   not downloaded by a browser, so it should launch **without** any "damaged"
   dialog and **without** needing `xattr` again. Confirm.

## Test 4 — System theme follows live

*Proves: the `data-theme` matchMedia listener against real macOS appearance
switching.*

1. Settings → Appearance → Theme → **System**.
2. macOS System Settings → Appearance → flip Light/Dark while Kavka is
   visible.
3. Expected: Kavka re-inks live, both directions, without a restart. The
   Settings Perch sentence updates to name the theme it is showing.

## Test 5 — An alert reaches Notification Center *(optional, needs a cluster)*

*Proves: the OS-notification channel end to end on macOS.*

1. Clone the repo (any branch) just for the compose file, then:

   ```sh
   docker compose -f dev/docker-compose.yml up -d --wait
   ```

2. In Kavka: connect **local** (`localhost:9092`, PLAINTEXT — the connection
   exists if you imported connections; otherwise Add connection).
3. Alerts → **Add a rule**: group `demo-checkout`, topic `payments`, more than
   `50` behind, for `1 minute` — and the **OS notification** channel ON.
4. Produce a hot partition (single key → one partition takes all the lag):

   ```sh
   for i in $(seq 1 60); do echo "hot	{\"n\":$i}"; done | \
     docker exec -i kavka-dev-kafka /opt/kafka/bin/kafka-console-producer.sh \
     --bootstrap-server localhost:9092 --topic payments \
     --property parse.key=true --property key.separator="	"
   ```

5. Wait ~90 seconds. Expected: a **macOS Notification Center banner** naming
   the rule, plus the in-app toast with **View group demo-checkout**.

---

## Report

Paste this back, filled in:

```
Build tested: OLD=................  NEW=................
macOS version: ................  Chip: ................

1 Gatekeeper wording (verbatim): ................
1 Install + first launch: PASS / FAIL — notes:
2 Docker from Finder:     PASS / FAIL — notes:
3 Updater offer shown:    PASS / FAIL
3 Install behaviour:      auto-relaunch / manual reopen / error: ................
3 About shows NEW build:  PASS / FAIL
3 No second Gatekeeper:   PASS / FAIL
4 System theme follows:   PASS / FAIL
5 Notification Center:    PASS / FAIL / skipped
```

Screenshots welcome for anything surprising. If a step fails, stop there and
send what you have — a half-filled honest report beats a complete guessed one.
