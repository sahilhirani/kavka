//! The error library from docs/DESIGN.md §7, on a terminal — and the four exit
//! codes that carry it into a shell script.
//!
//! Three layers, always: plain title → cause and fix → the raw broker string
//! verbatim. The app puts the third behind `Show details ▾`; a terminal has no
//! disclosure widget, so the third layer is `--details` (and it is printed
//! unasked when Kavka did not recognise the cause, because then the raw string
//! is all there is).
//!
//! The generalizable rule: WHEN THE BROKER TELLS US THE ANSWER, PUT THE ANSWER
//! IN THE MESSAGE. Every branch below either names the address that failed or
//! names the setting to change — never both a shrug and a stack trace.
//!
//! # This is a port of `apps/desktop/src/errors.ts`, and it is tripwired to it
//!
//! [`classify`] is a line-for-line port of that file's `classifyError`: same
//! rows, same order, same needles, same four regexes. The order is load-bearing
//! — librdkafka nests its causes ("Failed to get metadata: Local: Broker
//! transport failure" is a transport failure, not a metadata timeout) — so it
//! is the part a second implementation is most likely to get subtly wrong.
//!
//! Two copies of one rule in two languages is exactly the thing docs/DESIGN.md
//! §7 warns about ("two copies of that rule in two files is how the banner and
//! the checkbox come to disagree"), so the tests at the bottom read
//! `errors.ts` at compile time and fail if a cause, a needle or a load-bearing
//! sentence exists on one side and not the other. A row added to the app is a
//! failing test here, not a CLI that answers differently about the same string.
//!
//! PURE AND TOTAL, like its twin: no I/O, no clock, no globals. Every branch is
//! decided by its two arguments alone.

use kavka_core::profiles::Environment;
use regex::Regex;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

/// What the process exits with. The table is in the crate docs, and in
/// `--help`, because a code nobody can look up is a code nobody branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// The command answered. An empty answer is still an answer.
    Ok = 0,
    /// The cluster, the keychain or the profile file failed.
    Failed = 1,
    /// The command line was wrong.
    Usage = 2,
    /// A guardrail refused it and nothing was sent.
    Refused = 3,
    /// The connection named is not on this machine.
    NotFound = 4,
}

impl ExitCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// A failure on its way to stderr: a code for the script, and the three layers
/// for the person.
#[derive(Debug, Clone)]
pub struct CliError {
    pub code: ExitCode,
    pub title: String,
    pub detail: String,
    /// The raw string underneath, when there was one. Shown with `--details`,
    /// and always when [`Self::known`] is false.
    pub raw: Option<String>,
    /// Whether the cause was recognised. `false` means `title` IS the raw
    /// string, and the caller is looking at a broker message Kavka could not
    /// translate.
    pub known: bool,
}

impl CliError {
    /// A failure of the cluster, the keychain or the profile file: exit 1, with
    /// the raw string put through [`classify`].
    pub fn failed(raw: impl Into<String>, ctx: &Context) -> Self {
        Self::from_raw(ExitCode::Failed, raw.into(), ctx)
    }

    /// A failure Kavka already has words for — a guardrail refusal, an unknown
    /// connection, an impossible flag combination. `known`, because it is not a
    /// broker string anybody has to decode.
    pub fn stated(code: ExitCode, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code,
            title: title.into(),
            detail: detail.into(),
            raw: None,
            known: true,
        }
    }

    /// Any raw string, at a chosen code.
    pub fn from_raw(code: ExitCode, raw: String, ctx: &Context) -> Self {
        let classified = classify(&raw, ctx);
        Self {
            code,
            title: classified.title,
            detail: classified.detail,
            raw: Some(raw),
            known: classified.known,
        }
    }

    /// The three layers, as they go to stderr.
    ///
    /// Layer 3 is printed when `details` was asked for, and when the cause was
    /// not recognised — in which case the title is only the FIRST LINE of the
    /// raw string, so printing the rest is the difference between a truncated
    /// clue and the broker's whole reply. It is never printed twice.
    pub fn render(&self, details: bool) -> String {
        let mut out = format!("kavka: {}\n", self.title);
        if !self.detail.is_empty() {
            out.push_str(&format!("  {}\n", self.detail));
        }
        let raw = self.raw.as_deref().unwrap_or_default();
        let show_raw = !raw.is_empty() && (details || !self.known);
        if show_raw {
            // Not "Show details ▾": the disclosure already happened.
            out.push_str(&format!("  broker reply: {}\n", raw.trim()));
        } else if !raw.is_empty() && self.known {
            out.push_str("  (the broker's own reply is under --details)\n");
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The §7 table
// ---------------------------------------------------------------------------

/// Which row of the §7 table answered.
///
/// Kept even where this program has no second use for it — the app's renderers
/// switch on it — so that the two ports can be compared row for row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    UnknownProfile,
    ReadOnly,
    ActiveGroup,
    SaslMechanism,
    SaslRejected,
    Authorization,
    UntrustedCert,
    TlsNotExpected,
    NotABroker,
    TlsExpected,
    Dns,
    Refused,
    NoAnswer,
    Transport,
    Timeout,
    Unrecognised,
}

impl Cause {
    /// The name the TypeScript union uses. The tripwire compares these.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownProfile => "unknown-profile",
            Self::ReadOnly => "read-only",
            Self::ActiveGroup => "active-group",
            Self::SaslMechanism => "sasl-mechanism",
            Self::SaslRejected => "sasl-rejected",
            Self::Authorization => "authorization",
            Self::UntrustedCert => "untrusted-cert",
            Self::TlsNotExpected => "tls-not-expected",
            Self::NotABroker => "not-a-broker",
            Self::TlsExpected => "tls-expected",
            Self::Dns => "dns",
            Self::Refused => "refused",
            Self::NoAnswer => "no-answer",
            Self::Transport => "transport",
            Self::Timeout => "timeout",
            Self::Unrecognised => "unrecognised",
        }
    }

    /// Every row, for the tripwire.
    pub const ALL: &'static [Cause] = &[
        Cause::UnknownProfile,
        Cause::ReadOnly,
        Cause::ActiveGroup,
        Cause::SaslMechanism,
        Cause::SaslRejected,
        Cause::Authorization,
        Cause::UntrustedCert,
        Cause::TlsNotExpected,
        Cause::NotABroker,
        Cause::TlsExpected,
        Cause::Dns,
        Cause::Refused,
        Cause::NoAnswer,
        Cause::Transport,
        Cause::Timeout,
        Cause::Unrecognised,
    ];
}

#[derive(Debug, Clone)]
pub struct Classified {
    /// Line 1 — what happened, in the user's vocabulary.
    pub title: String,
    /// Line 2 — the next thing to do. Empty only if there genuinely isn't one.
    pub detail: String,
    /// False means `title` is the raw string.
    pub known: bool,
    /// The §7 row that matched. `Unrecognised` exactly when `known` is false.
    pub cause: Cause,
}

/// What the caller knows that the broker's reply doesn't say.
///
/// Every field is optional and every branch degrades to the string-only answer
/// without it — which is what keeps [`classify`] pure while still letting it
/// answer the two rows of §7's table that are not derivable from a string.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The consumer group's state as Kafka reports it — `Empty`, `Stable`, …
    pub group_state: Option<String>,
    /// How many members that group has right now.
    pub member_count: Option<usize>,
    /// The profile's environment.
    pub environment: Option<Environment>,
}

impl Context {
    /// The context a command has once it knows which connection it is on. This
    /// is the half `apps/desktop/src/errors.ts` documents as *unresolved* —
    /// "Timeout, prod" has a branch and no renderer threading the environment
    /// into it. A CLI has no store to reach into and no component tree to
    /// thread through: the profile is right there, so it is passed, and the
    /// prod wording actually reaches a user here.
    pub fn on(environment: Environment) -> Self {
        Self {
            environment: Some(environment),
            ..Self::default()
        }
    }
}

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a literal pattern compiles"))
}

/// Pull the first `host:port` out of a broker string. librdkafka writes
/// addresses as `broker-1.internal:9092/bootstrap` or `localhost:9092/1`, so
/// the trailing `/id` is left behind. Returns `None` rather than guessing.
pub fn extract_address(raw: &str) -> Option<&str> {
    static V6: OnceLock<Regex> = OnceLock::new();
    static HOST: OnceLock<Regex> = OnceLock::new();
    // Bracketed IPv6 first: [::1]:9092
    if let Some(found) = regex(&V6, r"(\[[0-9A-Fa-f:.]+\]:\d{2,5})").find(raw) {
        return Some(found.as_str());
    }
    regex(&HOST, r"\b([A-Za-z0-9][A-Za-z0-9._-]*:\d{2,5})\b")
        .find(raw)
        .map(|found| found.as_str())
}

/// The SASL mechanism a broker named as acceptable, if it named one.
fn offered_mechanism(raw: &str) -> Option<&str> {
    static MECH: OnceLock<Regex> = OnceLock::new();
    // The LAST mechanism named is the broker's, not ours: librdkafka writes
    // "…mechanism PLAIN is not enabled, supported: SCRAM-SHA-512".
    regex(
        &MECH,
        r"\b(SCRAM-SHA-(?:256|512)|GSSAPI|OAUTHBEARER|PLAIN)\b",
    )
    .find_iter(raw)
    .last()
    .map(|found| found.as_str())
}

/// The group id out of the core's own refusal, which quotes it.
fn quoted_name(raw: &str) -> Option<&str> {
    static QUOTED: OnceLock<Regex> = OnceLock::new();
    regex(&QUOTED, "\"([^\"\n]{1,120})\"")
        .captures(raw)
        .and_then(|caps| caps.get(1))
        .map(|found| found.as_str())
}

/// `with 3 member(s)` → 3. The broker and the core both write it this way.
fn stated_members(raw: &str) -> Option<usize> {
    static MEMBERS: OnceLock<Regex> = OnceLock::new();
    regex(&MEMBERS, r"(?i)\b(\d{1,6})\s+members?\b")
        .captures(raw)
        .and_then(|caps| caps.get(1))
        .and_then(|found| found.as_str().parse().ok())
}

/// Does `raw` contain any of these, case-insensitively? The needles are always
/// written lowercase.
fn has(lowered: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| lowered.contains(needle))
}

/// Map a raw error string to the plain-language pair from the §7 table.
///
/// Order matters: the specific causes are tested before the generic ones. See
/// the module docs — this is the half a port gets wrong.
pub fn classify(raw: &str, ctx: &Context) -> Classified {
    let text = raw.trim();
    let lowered = text.to_lowercase();
    let at = extract_address(text);
    let where_ = at.unwrap_or("that broker");
    let group_is_live = ctx
        .group_state
        .as_deref()
        .is_some_and(|state| !state.eq_ignore_ascii_case("empty"));
    let on_prod = ctx.environment == Some(Environment::Prod);

    let row = |title: String, detail: &str, cause: Cause| Classified {
        title,
        detail: detail.to_string(),
        known: true,
        cause,
    };

    // ── Kavka's own plumbing, before anything librdkafka says ──────────────
    if has(
        &lowered,
        &["unknown profile", "no such profile", "profile not found"],
    ) {
        return row(
            "That connection isn't on this machine any more".into(),
            "It may have been deleted in the app. Run `kavka profiles list` to see what is \
             saved here, or add it again in Kavka.",
            Cause::UnknownProfile,
        );
    }
    if has(&lowered, &["read-only", "read only connection"]) {
        return row(
            "Read-only connection — nothing was sent".into(),
            "This connection is marked read-only, so Kavka didn't write anything. Turn \
             read-only off in the connection's settings if you meant to.",
            Cause::ReadOnly,
        );
    }

    // ── Active group (§7's "Active group" row) ─────────────────────────────
    // Matched on the core's own `ensure_group_resettable` wording, and on what
    // the broker itself says. The context-only case is far below: a terse
    // failure on a group we happen to know is live must not outrank a message
    // that names its own cause.
    let active_group =
        || {
            let name = quoted_name(text)
                .unwrap_or("This consumer group")
                .to_string();
            let members = ctx.member_count.or_else(|| stated_members(text));
            let detail =
                match members {
                    None => "Offsets can't be reset while its members are consuming. Stop the \
                     application, wait for the group to report Empty, then try again — Kafka \
                     will reject the reset otherwise."
                        .to_string(),
                    Some(count) => format!(
                "Offsets can't be reset while {count} {verb} consuming. Stop the application, \
                 wait for the group to report Empty, then try again — Kafka will reject the \
                 reset otherwise.",
                verb = if count == 1 { "member is" } else { "members are" },
            ),
                };
            Classified {
                title: format!("{name} is running"),
                detail,
                known: true,
                cause: Cause::ActiveGroup,
            }
        };
    if has(
        &lowered,
        &[
            "rejects an offset reset",
            "while members are consuming",
            "unknown_member_id",
            "unknown member id",
            "rebalance in progress",
            "group is rebalancing",
            "non-empty group",
            "group is not empty",
        ],
    ) {
        return active_group();
    }

    // ── Sign-in ────────────────────────────────────────────────────────────
    // Mechanism mismatch is checked first: it also matches "authentication
    // failed", and the broker has already told us which mechanism it wants.
    if has(
        &lowered,
        &[
            "not enabled",
            "unsupported sasl mechanism",
            "unsupported_sasl_mechanism",
            "mechanism handshake failed",
            "does not support",
        ],
    ) && has(&lowered, &["sasl", "scram", "mechanism", "plain"])
    {
        return row(
            "The broker doesn't accept this sign-in mechanism".into(),
            &match offered_mechanism(text) {
                Some(offered) => {
                    format!("It offered {offered}. Switch the mechanism and connect again.")
                }
                None => "Switch the SCRAM mechanism (or the sign-in method) and connect again."
                    .to_string(),
            },
            Cause::SaslMechanism,
        );
    }
    if has(
        &lowered,
        &[
            "authentication failed",
            "sasl authentication",
            "saslauthentication",
            "invalid username or password",
            "authentication_failed",
            "err_sasl_authentication",
        ],
    ) {
        return row(
            "The broker rejected these credentials".into(),
            "Check the username, then re-enter the password in the Kavka app — Kavka can't \
             tell whether the stored one is still valid.",
            Cause::SaslRejected,
        );
    }
    if has(
        &lowered,
        &[
            "topic_authorization",
            "cluster_authorization",
            "authorization failed",
            "not authorized",
        ],
    ) {
        return row(
            "Connected, but this account can't list topics".into(),
            "It needs Describe on the cluster. Ask whoever issued the credentials for that \
             permission.",
            Cause::Authorization,
        );
    }

    // ── TLS ────────────────────────────────────────────────────────────────
    if has(
        &lowered,
        &[
            "certificate verify failed",
            "unable to get local issuer",
            "self signed certificate",
            "self-signed certificate",
            "unable to verify the first certificate",
            "certificate is not trusted",
        ],
    ) {
        return row(
            "The broker's certificate isn't trusted".into(),
            &format!(
                "{where_} presented a certificate this machine's trust store doesn't recognise. \
                 Add the CA certificate as a PEM file — Kavka doesn't need a keystore."
            ),
            Cause::UntrustedCert,
        );
    }
    // Broker speaks plaintext, we spoke TLS. OpenSSL says so very distinctly.
    if has(
        &lowered,
        &[
            "wrong version number",
            "packet length too long",
            "unknown protocol",
            "record layer failure",
        ],
    ) {
        return row(
            "This broker isn't using TLS".into(),
            "Turn off \"Encrypt the connection (TLS)\" in the connection's settings and try \
             again.",
            Cause::TlsNotExpected,
        );
    }
    // librdkafka conflates two causes in this one string: the port answered but
    // spoke something other than Kafka (wrong port), or it is a TLS listener.
    if has(&lowered, &["disconnected while requesting apiversion"]) {
        return row(
            format!("{where_} answered, but not like a Kafka broker"),
            "Either that port isn't Kafka — it usually runs on 9092, or 9093/9094 with TLS — or \
             the broker wants an encrypted connection. Check the port first, then try turning on \
             \"Encrypt the connection (TLS)\".",
            Cause::NotABroker,
        );
    }
    // Broker speaks TLS, we spoke plaintext. This is librdkafka's own hint.
    if has(
        &lowered,
        &[
            "incorrect security.protocol",
            "connecting to a ssl listener",
            "ssl handshake failed",
        ],
    ) {
        return row(
            "This broker expects an encrypted connection".into(),
            "Turn on \"Encrypt the connection (TLS)\" in the connection's settings and try again.",
            Cause::TlsExpected,
        );
    }

    // ── Reaching the host at all ───────────────────────────────────────────
    if has(
        &lowered,
        &[
            "failed to resolve",
            "name or service not known",
            "nodename nor servname",
            "no address associated",
            "getaddrinfo",
            "temporary failure in name resolution",
            "host not found",
        ],
    ) {
        return row(
            format!("Can't reach {where_}"),
            "The hostname didn't resolve. Check the spelling, or whether you need to be on the \
             VPN.",
            Cause::Dns,
        );
    }
    if has(&lowered, &["connection refused", "econnrefused"]) {
        return row(
            format!("{where_} refused the connection"),
            "Nothing is listening there. If you're running Kafka in Docker, check the port is \
             published to the host.",
            Cause::Refused,
        );
    }
    if has(
        &lowered,
        &[
            "connection timed out",
            "etimedout",
            "connect timed out",
            "connection setup timed out",
            "no route to host",
        ],
    ) {
        return row(
            format!("{where_} didn't answer"),
            // §7's "Timeout, prod" row. On a production cluster the first
            // suspect is not the user's own machine, and saying so stops a
            // support engineer taking their laptop apart at 3am over a broker
            // restart.
            if on_prod {
                "Nothing changed on your machine — this is usually the VPN or a broker restart."
            } else {
                "The address is routable but nothing answered on that port. Check the port \
                 number, or whether the broker is running."
            },
            Cause::NoAnswer,
        );
    }
    if has(
        &lowered,
        &[
            "broker transport failure",
            "all broker connections are down",
        ],
    ) {
        return row(
            format!("Can't reach {where_}"),
            if on_prod {
                "Nothing changed on your machine — this is usually the VPN or a broker restart."
            } else {
                "The connection didn't get far enough to speak Kafka. Check the address and \
                 port, then whether a VPN or firewall is in the way."
            },
            Cause::Transport,
        );
    }

    // ── Connected, but the cluster didn't finish the job ────────────────────
    // Timeout wording is required; "metadata" alone proves nothing about why.
    if has(&lowered, &["timed out", "timeout"]) {
        return row(
            "Connected, but the cluster didn't answer in time".into(),
            "The broker accepted the connection but didn't return metadata. It may be \
             overloaded, or a firewall may be blocking the address the broker advertises — which \
             can differ from the one you typed.",
            Cause::Timeout,
        );
    }

    // ── Context-only: a terse failure on a group we know has live members ──
    // Last, deliberately. Every branch above names its own cause in the text;
    // this one is an inference from what the CALLER knows, so it only gets to
    // answer once nothing else has.
    if group_is_live && has(&lowered, &["reset", "commit"]) {
        return active_group();
    }

    // ── Fallback: keep the broker's own words as the title ──────────────────
    // No apology, no "Something went wrong". The raw string is still the most
    // informative thing we have, so it is shown rather than buried.
    Classified {
        title: if text.is_empty() {
            "The connection attempt failed".to_string()
        } else {
            first_line(text)
        },
        detail: "Kavka doesn't recognise this one. The broker's full reply is above — it usually \
                 names the host or the setting at fault."
            .to_string(),
        known: false,
        cause: Cause::Unrecognised,
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() > 160 {
        let cut: String = line.chars().take(157).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app's own copy of this table, read at compile time.
    const TS: &str = include_str!("../../../apps/desktop/src/errors.ts");

    fn classify_of(raw: &str) -> Classified {
        classify(raw, &Context::default())
    }

    // -- The tripwires ------------------------------------------------------

    /// Every row this port knows exists in the app's union, and every row the
    /// app's union declares exists here. A row added on one side fails here.
    #[test]
    fn the_rows_are_the_apps_rows() {
        let union = TS
            .split_once("export type ErrorCause =")
            .expect("errors.ts declares ErrorCause")
            .1
            .split_once(';')
            .expect("the union ends")
            .0;
        for cause in Cause::ALL {
            assert!(
                union.contains(&format!("\"{}\"", cause.as_str())),
                "{} is a row here and not in errors.ts",
                cause.as_str()
            );
        }
        let declared = union.matches('"').count() / 2;
        assert_eq!(
            declared,
            Cause::ALL.len(),
            "errors.ts declares {declared} causes, this port has {}",
            Cause::ALL.len()
        );
    }

    /// Every needle this port matches on is a needle the app matches on. This
    /// is the assertion that catches a broker string the app learned to
    /// recognise and the CLI did not.
    #[test]
    fn every_needle_is_the_apps_needle() {
        let lowered = TS.to_lowercase();
        for needle in [
            "unknown profile",
            "no such profile",
            "profile not found",
            "read-only",
            "read only connection",
            "rejects an offset reset",
            "while members are consuming",
            "unknown_member_id",
            "unknown member id",
            "rebalance in progress",
            "group is rebalancing",
            "non-empty group",
            "group is not empty",
            "not enabled",
            "unsupported sasl mechanism",
            "unsupported_sasl_mechanism",
            "mechanism handshake failed",
            "does not support",
            "authentication failed",
            "sasl authentication",
            "saslauthentication",
            "invalid username or password",
            "authentication_failed",
            "err_sasl_authentication",
            "topic_authorization",
            "cluster_authorization",
            "authorization failed",
            "not authorized",
            "certificate verify failed",
            "unable to get local issuer",
            "self signed certificate",
            "self-signed certificate",
            "unable to verify the first certificate",
            "certificate is not trusted",
            "wrong version number",
            "packet length too long",
            "unknown protocol",
            "record layer failure",
            "disconnected while requesting apiversion",
            "incorrect security.protocol",
            "connecting to a ssl listener",
            "ssl handshake failed",
            "failed to resolve",
            "name or service not known",
            "nodename nor servname",
            "no address associated",
            "getaddrinfo",
            "temporary failure in name resolution",
            "host not found",
            "connection refused",
            "econnrefused",
            "connection timed out",
            "etimedout",
            "connect timed out",
            "connection setup timed out",
            "no route to host",
            "broker transport failure",
            "all broker connections are down",
            "timed out",
            "timeout",
        ] {
            assert!(
                lowered.contains(needle),
                "{needle:?} is matched here and not in errors.ts"
            );
        }
    }

    /// The four regexes are the four in errors.ts, character for character.
    /// They are the part where "close enough" produces a different address.
    #[test]
    fn the_patterns_are_the_apps_patterns() {
        for pattern in [
            r"(\[[0-9A-Fa-f:.]+\]:\d{2,5})",
            r"\b([A-Za-z0-9][A-Za-z0-9._-]*:\d{2,5})\b",
            r"\b(SCRAM-SHA-(?:256|512)|GSSAPI|OAUTHBEARER|PLAIN)\b",
            r#"([^"\n]{1,120})"#,
            r"\b(\d{1,6})\s+members?\b",
        ] {
            assert!(
                TS.contains(pattern),
                "{pattern} is not the pattern errors.ts uses"
            );
        }
    }

    /// The sentences a user actually reads. Not all of them — the two files
    /// address different surfaces, so the app says "sidebar" where this says
    /// `kavka profiles list` — but every one that names a CAUSE or a FIX.
    #[test]
    fn the_load_bearing_sentences_are_shared() {
        for sentence in [
            "The hostname didn't resolve. Check the spelling, or whether you need to be on the",
            "Nothing is listening there. If you're running Kafka in Docker, check the port is",
            "The broker rejected these credentials",
            "The broker's certificate isn't trusted",
            "This broker expects an encrypted connection",
            "This broker isn't using TLS",
            "Connected, but this account can't list topics",
            "Connected, but the cluster didn't answer in time",
            "Read-only connection — nothing was sent",
            "Nothing changed on your machine — this is usually the VPN or a broker restart.",
            "It needs Describe on the cluster. Ask whoever issued the credentials for that",
        ] {
            assert!(
                TS.contains(sentence),
                "errors.ts no longer says {sentence:?}"
            );
        }
    }

    // -- The table itself ---------------------------------------------------

    #[test]
    fn an_address_is_pulled_out_of_a_broker_string() {
        assert_eq!(
            extract_address("Failed to resolve 'broker-1.internal:9092/bootstrap'"),
            Some("broker-1.internal:9092")
        );
        assert_eq!(extract_address("[::1]:9092/1 refused"), Some("[::1]:9092"));
        assert_eq!(extract_address("something went wrong"), None);
    }

    #[test]
    fn dns_refused_and_timeout_name_the_address() {
        let dns = classify_of(
            "Failed to resolve 'kafka-1.internal:9092': Name or service not known (after 4ms)",
        );
        assert_eq!(dns.cause, Cause::Dns);
        assert_eq!(dns.title, "Can't reach kafka-1.internal:9092");

        let refused = classify_of(
            "localhost:9092/1: Connect to ipv4#127.0.0.1:9092 failed: Connection refused",
        );
        assert_eq!(refused.cause, Cause::Refused);
        assert!(
            refused.title.starts_with("localhost:9092 refused"),
            "{refused:?}"
        );

        let timeout = classify_of("payments-prod-1:9093/2: Connection setup timed out");
        assert_eq!(timeout.cause, Cause::NoAnswer);
        assert_eq!(timeout.title, "payments-prod-1:9093 didn't answer");
    }

    /// §7's "Timeout, prod" row — the one the app documents as having a branch
    /// and no renderer. Here the profile is always in scope, so it reaches a
    /// user.
    #[test]
    fn prod_changes_the_advice_for_a_timeout() {
        let raw = "payments-prod-1:9093/2: Connection setup timed out";
        let dev = classify(raw, &Context::default());
        let prod = classify(raw, &Context::on(Environment::Prod));
        assert_eq!(dev.cause, prod.cause);
        assert!(dev.detail.contains("Check the port number"), "{dev:?}");
        assert!(prod.detail.contains("VPN or a broker restart"), "{prod:?}");
    }

    #[test]
    fn the_mechanism_branch_wins_over_the_credentials_branch_and_names_what_was_offered() {
        let classified = classify_of(
            "SASL authentication failed: mechanism PLAIN is not enabled, supported: \
             SCRAM-SHA-512",
        );
        assert_eq!(classified.cause, Cause::SaslMechanism);
        assert!(
            classified.detail.contains("SCRAM-SHA-512"),
            "{classified:?}"
        );
    }

    #[test]
    fn transport_and_metadata_timeouts_are_different_rows() {
        // The nesting librdkafka does, and the reason order matters.
        assert_eq!(
            classify_of("Failed to get metadata: Local: Broker transport failure").cause,
            Cause::Transport
        );
        assert_eq!(
            classify_of("Failed to get metadata: Local: Timed out").cause,
            Cause::Timeout
        );
    }

    #[test]
    fn an_active_group_names_the_group_and_counts_its_members() {
        let classified = classify_of(
            "\"checkout-service\" is Stable with 3 member(s) — Kafka rejects an offset reset \
             while members are consuming",
        );
        assert_eq!(classified.cause, Cause::ActiveGroup);
        assert_eq!(classified.title, "checkout-service is running");
        assert!(
            classified.detail.contains("3 members are"),
            "{classified:?}"
        );

        // One member is singular, and the caller's count wins over the string's.
        let ctx = Context {
            member_count: Some(1),
            ..Context::default()
        };
        let one = classify("\"a\" — group is not empty, 9 members", &ctx);
        assert!(one.detail.contains("1 member is"), "{one:?}");
    }

    /// The context-only inference is LAST: a message that names its own cause
    /// must not be re-read as an active group just because one is running.
    #[test]
    fn a_named_cause_outranks_the_context_only_inference() {
        let ctx = Context {
            group_state: Some("Stable".into()),
            member_count: Some(3),
            ..Context::default()
        };
        assert_eq!(
            classify("kafka-1:9092: Connection refused while committing", &ctx).cause,
            Cause::Refused
        );
        assert_eq!(
            classify("could not commit: broker said no", &ctx).cause,
            Cause::ActiveGroup
        );
        // …and an Empty group is not live, so the inference does not fire.
        let empty = Context {
            group_state: Some("Empty".into()),
            ..Context::default()
        };
        assert_eq!(
            classify("could not commit: broker said no", &empty).cause,
            Cause::Unrecognised
        );
    }

    #[test]
    fn an_unrecognised_string_becomes_its_own_first_line() {
        let classified = classify_of("Local: Fatal error\nsecond line nobody needs in a title");
        assert!(!classified.known);
        assert_eq!(classified.cause, Cause::Unrecognised);
        assert_eq!(classified.title, "Local: Fatal error");
        let long = "x".repeat(400);
        assert_eq!(classify_of(&long).title.chars().count(), 158);
    }

    #[test]
    fn an_empty_string_still_answers() {
        let classified = classify_of("   ");
        assert_eq!(classified.title, "The connection attempt failed");
        assert!(!classified.known);
    }

    // -- Rendering ----------------------------------------------------------

    #[test]
    fn a_recognised_failure_hides_the_broker_reply_behind_details() {
        let raw = "kafka-1:9092/bootstrap: Connection refused";
        let error = CliError::failed(raw, &Context::default());
        assert_eq!(error.code, ExitCode::Failed);
        let quiet = error.render(false);
        assert!(quiet.starts_with("kavka: kafka-1:9092 refused"), "{quiet}");
        assert!(!quiet.contains(raw), "{quiet}");
        assert!(quiet.contains("--details"), "{quiet}");
        let loud = error.render(true);
        assert!(loud.contains(raw), "{loud}");
        assert!(!loud.contains("--details"), "{loud}");
    }

    /// An unrecognised failure prints the whole reply unasked: the title is
    /// only its first line, so there is nothing else to disclose.
    #[test]
    fn an_unrecognised_failure_prints_the_reply_unasked() {
        let raw = "Local: Fatal error\nwith a second line";
        let rendered = CliError::failed(raw, &Context::default()).render(false);
        assert!(rendered.contains("with a second line"), "{rendered}");
        assert!(!rendered.contains("--details"), "{rendered}");
    }

    #[test]
    fn a_stated_failure_carries_no_broker_reply_at_all() {
        let error = CliError::stated(ExitCode::Refused, "refused", "add --yes-prod");
        let rendered = error.render(true);
        assert_eq!(rendered, "kavka: refused\n  add --yes-prod\n");
        assert_eq!(error.code.code(), 3);
    }

    #[test]
    fn the_codes_are_the_documented_ones() {
        assert_eq!(ExitCode::Ok.code(), 0);
        assert_eq!(ExitCode::Failed.code(), 1);
        assert_eq!(ExitCode::Usage.code(), 2);
        assert_eq!(ExitCode::Refused.code(), 3);
        assert_eq!(ExitCode::NotFound.code(), 4);
        // clap's own parse failures exit 2, which is what makes Usage 2 rather
        // than a number of this program's choosing.
        assert_eq!(ExitCode::Usage.code(), 2);
    }
}
