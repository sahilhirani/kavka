//! The write gate: three checks, one pure function, one test per cell — and
//! the one other thing this process is started with, [`MaskPolicy`].
//!
//! Everything about what this server is allowed to change lives here. The tool
//! implementations call [`authorize_write`] and nothing else, so there is
//! exactly one answer to "may this write happen", and it can be driven through
//! the whole matrix in a millisecond without a broker.

use kavka_core::environments::EffectiveEnvironment;
use kavka_core::profiles::ConnectionProfile;

/// Must be `1` in the environment **when the server starts** for any write tool
/// to run.
pub const ALLOW_WRITES_ENV: &str = "KAVKA_MCP_ALLOW_WRITES";

/// Must additionally be `1` for a write against a profile whose environment is
/// **marked protected** in the Kavka app.
///
/// Environments are user-defined ([`kavka_core::environments`]) — an
/// enterprise runs `dev`, `QA`, `UAT` and `Production` — so what this covers
/// is the box somebody ticked, not the word `prod`. A protected environment
/// called `Production` needs this variable exactly as `prod` did; a `prod`
/// somebody deliberately unprotected does not.
///
/// **The variable is still spelled `KAVKA_MCP_ALLOW_PROD`.** It lives in MCP
/// client config files that are checked into repositories, and renaming it
/// would silently disable writes for every one of them to improve a word.
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
    /// Whether [`ALLOW_PROD_ENV`] was set — i.e. whether writes to *protected*
    /// environments are permitted. Named for the variable, which is named for
    /// the environment that was protected when there were only three.
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
/// `environment` is the profile's environment already resolved against this
/// machine's definitions
/// ([`kavka_core::environments::EnvironmentStore::resolve`]). It is passed
/// rather than looked up so this stays one pure function with no disk under
/// it, and so the whole matrix runs in a millisecond.
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
    environment: &EffectiveEnvironment,
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
    if environment.protected && !policy.prod_allowed {
        return Err(format!(
            "{tool} refused: {name:?} is in {env:?}, an environment marked protected in the Kavka \
             app, and this server was started without {ALLOW_PROD_ENV}=1. {ALLOW_WRITES_ENV}=1 \
             alone does not cover protected environments. Set {ALLOW_PROD_ENV}=1 as well and \
             restart the server if writing to {env} is genuinely what you want.",
            name = profile.name,
            env = environment.name,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kavka_core::environments::{defaults, resolve, EnvironmentDef};
    use serde_json::json;

    /// Built from JSON rather than as a struct literal: `ConnectionProfile`
    /// gains an optional field most phases, every one of them
    /// `#[serde(default)]`, and a gate test has no business breaking because
    /// somebody added a Connect cluster or a WASM decoder to the profile.
    fn profile(environment: &str, read_only: bool) -> ConnectionProfile {
        serde_json::from_value(json!({
            "id": "p1",
            "name": "orders",
            "environment": environment,
            "bootstrap_servers": ["localhost:9092"],
            "auth": { "kind": "plaintext" },
            "read_only": read_only,
        }))
        .expect("a ConnectionProfile")
    }

    /// The environment as a machine with the shipped definitions resolves it —
    /// which is every machine that has not opened the environment manager.
    fn shipped(name: &str) -> EffectiveEnvironment {
        resolve(&defaults(), name)
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
            (false, false, "dev", false, false),
            (false, false, "dev", true, false),
            (false, false, "prod", false, false),
            (false, false, "prod", true, false),
            (false, true, "dev", false, false),
            (false, true, "dev", true, false),
            (false, true, "prod", false, false),
            (false, true, "prod", true, false),
            (true, false, "dev", false, true),
            (true, false, "dev", true, false),
            (true, false, "staging", false, true),
            (true, false, "prod", false, false),
            (true, false, "prod", true, false),
            (true, true, "dev", false, true),
            (true, true, "prod", false, true),
            // read-only wins even with every variable set. This is the cell the
            // whole ordering exists for.
            (true, true, "prod", true, false),
        ];
        for (writes, prod, environment, read_only, allowed) in cases {
            let outcome = authorize_write(
                policy(writes, prod),
                &profile(environment, read_only),
                &shipped(environment),
                "kavka_produce",
            );
            assert_eq!(
                outcome.is_ok(),
                allowed,
                "writes={writes} prod={prod} env={environment} read_only={read_only} -> \
                 {outcome:?}"
            );
        }
    }

    /// **The migration, asserted.** An enterprise's `Production` — a name this
    /// build has never heard of, in a colour that is not red — needs
    /// `KAVKA_MCP_ALLOW_PROD=1` exactly as `prod` did, because the box that
    /// decides is `protected`. And `prod` itself stops needing it the moment
    /// somebody unticks that box.
    #[test]
    fn a_protected_custom_environment_gates_exactly_like_prod_did() {
        let defs = vec![
            EnvironmentDef::new("Production", "violet", true),
            EnvironmentDef::new("UAT", "blue", false),
            EnvironmentDef::new("prod", "red", false),
        ];
        for (environment, needs_the_variable) in
            [("Production", true), ("UAT", false), ("prod", false)]
        {
            let effective = resolve(&defs, environment);
            let profile = profile(environment, false);
            assert_eq!(
                authorize_write(policy(true, false), &profile, &effective, "kavka_produce")
                    .is_err(),
                needs_the_variable,
                "{environment} with writes but not prod"
            );
            assert!(
                authorize_write(policy(true, true), &profile, &effective, "kavka_produce").is_ok(),
                "{environment} with both variables"
            );
        }

        // The refusal names the environment the user actually invented, so a
        // model can repeat it back to the person who has to set the variable.
        let refusal = authorize_write(
            policy(true, false),
            &profile("Production", false),
            &resolve(&defs, "Production"),
            "kavka_produce",
        )
        .unwrap_err();
        assert!(refusal.contains("Production"), "{refusal}");
        assert!(refusal.contains(ALLOW_PROD_ENV), "{refusal}");
    }

    /// An environment nothing on this machine defines carries no protection —
    /// see the reasoning on [`kavka_core::environments`].
    /// `KAVKA_MCP_ALLOW_WRITES` still governs it, so this is not a hole in the
    /// write gate, only in the protected-environment gate.
    #[test]
    fn an_undefined_environment_does_not_need_the_prod_variable() {
        let effective = shipped("QA");
        assert!(!effective.known);
        assert!(authorize_write(
            policy(true, false),
            &profile("QA", false),
            &effective,
            "kavka_produce"
        )
        .is_ok());
        // …and is still refused with writes off.
        assert!(authorize_write(
            policy(false, true),
            &profile("QA", false),
            &effective,
            "kavka_produce"
        )
        .is_err());
    }

    #[test]
    fn a_disabled_server_names_the_writes_variable() {
        let refusal = authorize_write(
            policy(false, false),
            &profile("dev", false),
            &shipped("dev"),
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
    fn a_protected_profile_names_the_prod_variable_and_says_writes_alone_is_not_enough() {
        let refusal = authorize_write(
            policy(true, false),
            &profile("prod", false),
            &shipped("prod"),
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
            &profile("prod", true),
            &shipped("prod"),
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
    fn an_unprotected_environment_needs_no_prod_variable() {
        assert!(authorize_write(
            policy(true, false),
            &profile("staging", false),
            &shipped("staging"),
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
