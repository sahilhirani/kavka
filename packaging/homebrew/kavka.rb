# TEMPLATE. `{{VERSION}}` and the two `{{SHA256_DMG_*}}` are filled by
# packaging/render.mjs; everything else is Ruby that Homebrew evaluates, so the
# `#{version}` and `#{arch}` interpolations below stay as they are.
#
# DO NOT SUBMIT THIS UNNOTARIZED. Homebrew will accept the cask, and macOS will
# then tell every user that "Kavka.app is damaged and can't be opened" — which
# is Gatekeeper's message for a quarantined app with no Developer ID, and it is
# a far worse first impression than no cask at all. See packaging/README.md.
cask "kavka" do
  # macOS is `x64`/`aarch64` in Tauri's DMG names; Windows is `x64`/`arm64` in
  # its installer names. Same bundler, different vocabulary — see the artifact
  # table in packaging/README.md before changing either.
  arch arm: "aarch64", intel: "x64"

  version "{{VERSION}}"
  sha256 arm:   "{{SHA256_DMG_ARM64}}",
         intel: "{{SHA256_DMG_X64}}"

  url "https://github.com/sahilhirani/kavka/releases/download/v#{version}/Kavka_#{version}_#{arch}.dmg",
      verified: "github.com/sahilhirani/kavka/"
  name "Kavka"
  desc "Open-source desktop client for Apache Kafka"
  homepage "https://github.com/sahilhirani/kavka"

  livecheck do
    url :url
    strategy :github_latest
  end

  # Monterey, not Catalina. Kavka's design system is audited against the
  # WKWebView that ships with macOS 12 (Safari 15.6) — no `color-mix()`, no
  # container queries without a fallback — and that is the oldest webview the
  # UI has been verified in. See docs/DESIGN.md §3.
  depends_on macos: ">= :monterey"

  app "Kavka.app"

  # Everything Kavka writes, and nothing else. `zap` is the "leave no trace"
  # path, so it has to name the real directories:
  #   Application Support/io.kavka.desktop  profiles.json, alerts.json,
  #                                         masking.json, diagnostics.json,
  #                                         history/, logs/
  # Connection passwords live in the login keychain and are DELIBERATELY NOT
  # zapped: a keychain item is the user's, deleting one silently is not a
  # package manager's business, and `security delete-generic-password` in a zap
  # stanza would fail the audit anyway.
  zap trash: [
    "~/Library/Application Support/io.kavka.desktop",
    "~/Library/Caches/io.kavka.desktop",
    "~/Library/Saved Application State/io.kavka.desktop.savedState",
    "~/Library/WebKit/io.kavka.desktop",
  ]
end
