//! The write gate: three checks, one pure function, one test per cell — and
//! the one other thing this process is started with, [`MaskPolicy`].
//!
//! Everything about what this server is allowed to change lives here. The tool
//! implementations call [`authorize_write`] and nothing else, so there is
//! exactly one answer to "may this write happen", and it can be driven through
//! the whole matrix in a millisecond without a broker.

use kavka_core::profiles::{ConnectionProfile, Environment};

/// Must be `1` in the environment **when the server starts** for any write tool
/// to run.
pub const ALLOW_WRITES_ENV: &str = "KAVKA_MCP_ALLOW_WRITES";

/// Must additionally be `1` for a write against a profile tagged `prod`.
pub const ALLOW_PROD_ENV: &str = "KAVKA_MCP_ALLOW_PROD";

/// What the process was started with.
///
/// Read once, at startup ([`WritePolicy::from_env`]), and then carried by
/// value: what a running server may do must not change under it because
/// something in the environment moved. Restarting is the only way to change
/// these, which is also what makes them auditable — the answer is in the MCP
/// client's config file, next to the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WritePolicy {
    pub writes_enabled: bool,
    pub prod_allowed: bool,
}

impl WritePolicy {
    /// The policy this process was started with.
    pub fn from_env() -> Self {
        Self {
            writes_enabled: enabled(std::env::var(ALLOW_WRITES_ENV).ok().as_deref()),
            prod_allowed: enabled(std::env::var(ALLOW_PROD_ENV).ok().as_deref()),
        }
    }

    /// The read-only policy: no writes, no prod. What an unset environment
    /// produces, and the default anybody gets by pasting the snippet
    /// `mcp_info` generates.
    pub fn read_only() -> Self {
        Self {
            writes_enabled: false,
            prod_allowed: false,
        }
    }
}

/// Must be `1` in the environment **when the server starts** for records to
/// come back with the connection's masking rules NOT applied.
///
/// The opt-out is spelled as a variable rather than a tool argument for the
/// same reason the write gate is: what a running server may hand an agent has
/// to be a property of how somebody launched it, in a config file next to the
/// command, and not something the agent can ask for mid-conversation.
pub const UNMASKED_ENV: &str = "KAVKA_MCP_UNMASKED";

/// Whether this process applies a connection's masking rules to the records it
/// returns.
///
/// **`Honor` is the default, and that is the decision.** Masking is stored per
/// connection in the app's own `masking.json`, beside the `profiles.json` this
/// server already reads, so the rules a person wrote for a cluster are right
/// here — and an agent is exactly the reader those rules were written for: its
/// context is logged, replayed, and sent to a third party. Returning payloads
/// the person masked in their own app would make this server the one door in
/// the product that ignores them.
///
/// See [`UNMASKED_ENV`] for the way out, and `crates/kavka-mcp/src/lib.rs` for
/// the policy in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskPolicy {
    /// The profile's enabled rules are applied to every record this server
    /// returns.
    Honor,
    /// Records come back exactly as kavka-core decoded them.
    Off,
}

impl MaskPolicy {
    /// The policy this process was started with. Read once, like the write
    /// gate, and for the same reason.
    pub fn from_env() -> Self {
        if enabled(std::env::var(UNMASKED_ENV).ok().as_deref()) {
            Self::Off
        } else {
            Self::Honor
        }
    }

    pub fn honors_rules(self) -> bool {
        self == Self::Honor
    }
}

/// Exactly `"1"`, like every other escape hatch in this codebase
/// (`KAVKA_DEV_OAUTH_ALLOW_PLAINTEXT`, `KAVKA_ALLOW_NON_AWS_MSK_ENDPOINTS`).
/// `true`, `yes` and `TRUE` are all off, deliberately: a variable that means
/// "let an agent write to Kafka" should have one spelling, and a user who typed
/// another one finds out on the first refusal rather than on the first write.
fn enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Decides whether `tool` may write to `profile`, and explains a refusal in one
/// sentence a model can act on.
///
/// **The order is the policy.** `read_only` is checked first because it is the
/// only one no environment variable can lift: naming `KAVKA_MCP_ALLOW_WRITES`
/// to a caller whose real obstacle is the profile's own flag would send them
/// off to restart the server for nothing. The refusal still names the variable
/// — as the thing that does *not* help — because "which knob is this" is the
/// question the caller is actually asking.
pub fn authorize_write(
    policy: WritePolicy,
    profile: &ConnectionProfile,
    tool: &str,
) -> Result<(), String> {
    if profile.read_only {
        return Err(format!(
            "{tool} refused: the connection {name:?} is marked read-only, so Kavka sends nothing \
             to it. This is a property of the connection, not of this server — setting \
             {ALLOW_WRITES_ENV}=1 does not lift it. Turn read-only off in the connection's \
             settings in the Kavka app if you meant to write.",
            name = profile.name,
        ));
    }
    if !policy.writes_enabled {
        return Err(format!(
            "{tool} refused: this MCP server was started without {ALLOW_WRITES_ENV}=1, so every \
             write tool is disabled. Set {ALLOW_WRITES_ENV}=1 in the server's environment (in the \
             MCP client's config, beside the command) and restart it — the value is read once, at \
             startup."
        ));
    }
    if profile.environment == Environment::Prod && !policy.prod_allowed {
        return Err(format!(
            "{tool} refused: {name:?} is tagged as a production connection, and this server was \
             started without {ALLOW_PROD_ENV}=1. {ALLOW_WRITES_ENV}=1 alone does not cover prod. \
             Set {ALLOW_PROD_ENV}=1 as well and restart the server if writing to production is \
             genuinely what you want.",
            name = profile.name,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Built from JSON rather than as a struct literal: `ConnectionProfile`
    /// gains an optional field most phases, every one of them
    /// `#[serde(default)]`, and a gate test has no business breaking because
    /// somebody added a Connect cluster or a WASM decoder to the profile.
    fn profile(environment: Environment, read_only: bool) -> ConnectionProfile {
        serde_json::from_value(json!({
            "id": "p1",
            "name": "orders",
            "environment": serde_json::to_value(environment).expect("an environment"),
            "bootstrap_servers": ["localhost:9092"],
            "auth": { "kind": "plaintext" },
            "read_only": read_only,
        }))
        .expect("a ConnectionProfile")
    }

    fn policy(writes: bool, prod: bool) -> WritePolicy {
        WritePolicy {
            writes_enabled: writes,
            prod_allowed: prod,
        }
    }

    /// The whole matrix: {writes off, on} × {prod off, on} × {dev, prod} ×
    /// {writable, read-only}. Sixteen cells, one table, so a rule that changes
    /// changes visibly.
    #[test]
    fn the_gating_matrix() {
        // (writes_enabled, prod_allowed, environment, read_only, allowed)
        let cases = [
            (false, false, Environment::Dev, false, false),
            (false, false, Environment::Dev, true, false),
            (false, false, Environment::Prod, false, false),
            (false, false, Environment::Prod, true, false),
            (false, true, Environment::Dev, false, false),
            (false, true, Environment::Dev, true, false),
            (false, true, Environment::Prod, false, false),
            (false, true, Environment::Prod, true, false),
            (true, false, Environment::Dev, false, true),
            (true, false, Environment::Dev, true, false),
            (true, false, Environment::Staging, false, true),
            (true, false, Environment::Prod, false, false),
            (true, false, Environment::Prod, true, false),
            (true, true, Environment::Dev, false, true),
            (true, true, Environment::Prod, false, true),
            // read-only wins even with every variable set. This is the cell the
            // whole ordering exists for.
            (true, true, Environment::Prod, true, false),
        ];
        for (writes, prod, environment, read_only, allowed) in cases {
            let outcome = authorize_write(
                policy(writes, prod),
                &profile(environment, read_only),
                "kavka_produce",
            );
            assert_eq!(
                outcome.is_ok(),
                allowed,
                "writes={writes} prod={prod} env={environment:?} read_only={read_only} -> \
                 {outcome:?}"
            );
        }
    }

    #[test]
    fn a_disabled_server_names_the_writes_variable() {
        let refusal = authorize_write(
            policy(false, false),
            &profile(Environment::Dev, false),
            "kavka_produce",
        )
        .unwrap_err();
        assert!(refusal.contains(ALLOW_WRITES_ENV), "{refusal}");
        assert!(refusal.contains("kavka_produce"), "{refusal}");
        // And says the value is read at startup, so a caller does not sit there
        // exporting it into its own shell and retrying.
        assert!(refusal.contains("restart"), "{refusal}");
    }

    #[test]
    fn a_prod_profile_names_the_prod_variable_and_says_writes_alone_is_not_enough() {
        let refusal = authorize_write(
            policy(true, false),
            &profile(Environment::Prod, false),
            "kavka_reset_offsets",
        )
        .unwrap_err();
        assert!(refusal.contains(ALLOW_PROD_ENV), "{refusal}");
        assert!(refusal.contains(ALLOW_WRITES_ENV), "{refusal}");
        assert!(refusal.contains("kavka_reset_offsets"), "{refusal}");
    }

    #[test]
    fn a_read_only_profile_is_refused_and_told_no_variable_lifts_it() {
        let refusal = authorize_write(
            policy(true, true),
            &profile(Environment::Prod, true),
            "kavka_produce",
        )
        .unwrap_err();
        assert!(refusal.contains("read-only"), "{refusal}");
        assert!(refusal.contains("does not lift it"), "{refusal}");
        // It still names the variable, because "which knob is this" is the
        // question being asked.
        assert!(refusal.contains(ALLOW_WRITES_ENV), "{refusal}");
        // The connection is named: the caller may have several.
        assert!(refusal.contains("orders"), "{refusal}");
    }

    #[test]
    fn staging_needs_no_prod_variable() {
        assert!(authorize_write(
            policy(true, false),
            &profile(Environment::Staging, false),
            "kavka_produce"
        )
        .is_ok());
    }

    #[test]
    fn only_the_literal_one_enables_a_variable() {
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("true"),
            Some("TRUE"),
            Some("yes"),
            Some("1 "),
        ] {
            assert!(!enabled(value), "{value:?} must not enable a write");
        }
        assert!(enabled(Some("1")));
    }

    #[test]
    fn the_default_policy_is_the_locked_one() {
        assert_eq!(
            WritePolicy::read_only(),
            WritePolicy {
                writes_enabled: false,
                prod_allowed: false
            }
        );
    }

    /// Masking is honoured unless somebody said otherwise, in the one spelling
    /// every other escape hatch here uses.
    #[test]
    fn masking_is_honoured_by_default_and_opts_out_on_exactly_one() {
        assert!(MaskPolicy::Honor.honors_rules());
        assert!(!MaskPolicy::Off.honors_rules());
        // The variable's own parsing is `enabled`, asserted above — this is
        // the mapping from it to the policy, which is the half that is easy to
        // get backwards.
        for (value, expected) in [
            (None, MaskPolicy::Honor),
            (Some("0"), MaskPolicy::Honor),
            (Some("true"), MaskPolicy::Honor),
            (Some("1"), MaskPolicy::Off),
        ] {
            let policy = if enabled(value) {
                MaskPolicy::Off
            } else {
                MaskPolicy::Honor
            };
            assert_eq!(policy, expected, "{value:?}");
        }
    }
}
