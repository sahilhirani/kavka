# Security Policy

## Reporting a vulnerability

Please report security issues **privately**, through GitHub's private
vulnerability reporting:

**https://github.com/sahilhirani/kavka/security/advisories/new**

That opens a draft advisory only the maintainer can see. Please do not open a
public issue for a security problem first — Kavka auto-updates, so a public
report reaches attackers and users at the same moment, and users cannot patch
faster than the disclosure travels.

Include what you have: affected version (the About panel shows it, including the
build number), platform, and the smallest reproduction you can manage. A
proof-of-concept is welcome but not required.

## What to expect

Kavka is maintained by one person as an unpaid open-source project. That sets
the honest expectations:

- **Acknowledgement:** within 7 days.
- **Assessment:** within 30 days, including a decision on whether it is a
  vulnerability and a rough fix timeline.
- **Fix:** no guaranteed date. Serious issues in the updater, the credential
  storage, or the wire protocol's authentication paths are prioritised over
  everything else.

There is no bug bounty. Reporters are credited in the advisory and the release
notes unless they ask not to be.

## Supported versions

Only the most recent release is supported. There are no backports and no
maintained release branches — a fix ships in the next version and reaches users
through the in-app updater.

| Version          | Supported |
| ---------------- | --------- |
| Latest release   | Yes       |
| Anything earlier | No        |

`v*-build.*` pre-releases are automated builds of `main`. They are supported in
the same sense `main` is: report problems, but the fix is "use a newer build".

## Scope

In scope: the desktop app, `kavka-core`, the MCP server (`kavka-mcp`), the CLI
(`kavka-cli`), the signed auto-updater, and this repository's CI and release
pipeline.

Out of scope: vulnerabilities in Apache Kafka itself or in brokers you connect
to; issues that require an attacker to already have code execution or filesystem
access on the user's machine (Kavka stores secrets in the OS keychain, which
defends against other users and remote theft, not against malware running as
you); and reports produced by a scanner with no demonstrated impact.

## What Kavka does with your credentials

Cluster secrets go to the OS keychain (Keychain on macOS, Credential Manager on
Windows, Secret Service on Linux) — never to a Kavka server, because there is no
Kavka server. The only request Kavka makes that is not to a broker, schema
registry, Connect cluster or metrics endpoint **you** configured is the update
check: at most once a day, to github.com, carrying nothing that identifies you.
It is on by default and *Settings → Updates* turns it off. There is no
telemetry endpoint in the app — not a disabled one, not one behind a flag. If
you find any of that to be untrue, that is a security bug and this is the right
form for it.

## Release signing

Releases carry `SHA256SUMS.txt`. In-app updates are minisign-signed and verified
against a public key compiled into the app; an update that fails verification is
not installed. Installers are **not** yet code-signed for Windows Authenticode
or notarised for macOS Gatekeeper — see the release notes for what that means
when you install.

## Known and accepted

Findings from the project's own security review that are recorded rather than
fixed — including which dependency advisories are accepted and why — are in
[docs/SECURITY-AUDIT.md](docs/SECURITY-AUDIT.md). If you are about to report
something, it is worth a look first; if it is on that list and you think the
reasoning is wrong, that is still worth reporting.
