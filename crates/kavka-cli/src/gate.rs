//! The write gate: two checks, one pure function, one test per cell.
//!
//! Everything about what this program is allowed to change lives here.
//! `kavka produce` calls [`authorize_write`] and nothing else, so there is
//! exactly one answer to "may this write happen", and the whole matrix can be
//! driven in a millisecond without a broker.
//!
//! It is the same shape as `kavka-mcp`'s gate with one deliberate difference:
//! the MCP server reads its permission from the environment **once, at
//! startup**, because a model must not be able to ask for it mid-conversation
//! and the answer has to be auditable in a config file. A CLI's operator is the
//! person typing, so the permission is a flag on the command that writes —
//! `--yes-prod` — which is the terminal's version of docs/DESIGN.md §6 layer 4,
//! type-to-confirm, environment-gated rather than action-gated. A protected
//! environment always asks; an unprotected one never does.
//!
//! # What the flag is keyed on, and what its name means now
//!
//! Environments are user-defined ([`kavka_core::environments`]): an enterprise
//! runs `dev`, `QA`, `UAT` and `Production`, not three fixed words. So the gate
//! reads [`EffectiveEnvironment::protected`] — the box somebody ticked in the
//! app — and never the environment's *name*. A protected environment called
//! `Production` gates exactly as `prod` did, and an environment called `prod`
//! that somebody deliberately unprotected does not gate at all.
//!
//! **The flag is still spelled `--yes-prod`.** It is in shell histories,
//! runbooks and CI scripts, and renaming it would break every one of them to
//! improve a word. What changed is what it is documented to mean: *required
//! when the connection's environment is marked protected*.

use crate::errors::{CliError, ExitCode};
use kavka_core::environments::EffectiveEnvironment;
use kavka_core::profiles::ConnectionProfile;

/// The flag a connection in a protected environment needs before this program
/// writes to it. Named for the environment that was protected when there were
/// only three; see the module docs.
pub const YES_PROD_FLAG: &str = "--yes-prod";

/// Decides whether `command` may write to `profile`, and explains a refusal in
/// two sentences the person can act on.
///
/// `environment` is the profile's environment already resolved against this
/// machine's definitions ([`kavka_core::environments::EnvironmentStore::resolve`]).
/// It is passed rather than looked up so this stays one pure function with no
/// disk under it — the whole matrix runs in a millisecond, which is what makes
/// a table test of it worth reading.
///
/// **The order is the policy.** `read_only` is checked first because it is the
/// only one no flag can lift: naming `--yes-prod` to somebody whose real
/// obstacle is the connection's own flag would send them off to retype a
/// command for nothing. The refusal still names the flag — as the thing that
/// does *not* help — because "which knob is this" is the question being asked.
///
/// Both refusals restate the CLUSTER, not just the connection name
/// (docs/DESIGN.md §6 layer 3: most prod accidents are
/// right-action-wrong-cluster).
pub fn authorize_write(
    profile: &ConnectionProfile,
    environment: &EffectiveEnvironment,
    command: &str,
    yes_prod: bool,
) -> Result<(), CliError> {
    let servers = profile.bootstrap_servers.join(", ");
    if profile.read_only {
        return Err(CliError::stated(
            ExitCode::Refused,
            format!(
                "Read-only connection — nothing was sent to {name}",
                name = profile.name
            ),
            format!(
                "{name} ({servers}) is marked read-only, so Kavka sent nothing. This is a \
                 property of the connection, not of this command — {YES_PROD_FLAG} does not lift \
                 it. Turn read-only off in the connection's settings in the Kavka app if you \
                 meant to write.",
                name = profile.name,
            ),
        ));
    }
    if environment.protected && !yes_prod {
        return Err(CliError::stated(
            ExitCode::Refused,
            format!(
                "{name} is on {env}, a protected environment — nothing was sent",
                name = profile.name,
                env = environment.name,
            ),
            format!(
                "{command} would write to {servers}. Add {YES_PROD_FLAG} to the command if \
                 writing to {env} is genuinely what you want. Which environments are protected is \
                 set under Manage environments in the Kavka app.",
                env = environment.name,
            ),
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
            "bootstrap_servers": ["kafka-1.internal:9092"],
            "auth": { "kind": "plaintext" },
            "read_only": read_only,
        }))
        .expect("a ConnectionProfile")
    }

    /// The environment as a machine with the shipped definitions resolves it.
    fn shipped(name: &str) -> EffectiveEnvironment {
        resolve(&defaults(), name)
    }

    /// The whole matrix: {dev, staging, prod} × {writable, read-only} ×
    /// {--yes-prod, without}. Twelve cells, one table, so a rule that changes
    /// changes visibly.
    #[test]
    fn the_gating_matrix() {
        // (environment, read_only, yes_prod, allowed)
        let cases = [
            ("dev", false, false, true),
            ("dev", false, true, true),
            ("dev", true, false, false),
            ("dev", true, true, false),
            ("staging", false, false, true),
            ("staging", false, true, true),
            ("staging", true, false, false),
            ("staging", true, true, false),
            ("prod", false, false, false),
            ("prod", false, true, true),
            ("prod", true, false, false),
            // read-only wins even with the flag. This is the cell the ordering
            // exists for.
            ("prod", true, true, false),
        ];
        for (environment, read_only, yes_prod, allowed) in cases {
            let outcome = authorize_write(
                &profile(environment, read_only),
                &shipped(environment),
                "produce",
                yes_prod,
            );
            assert_eq!(
                outcome.is_ok(),
                allowed,
                "env={environment} read_only={read_only} yes_prod={yes_prod} -> {outcome:?}"
            );
        }
    }

    /// **The migration, asserted.** An enterprise's `Production` — a name this
    /// build has never heard of, in a colour that is not red — gates exactly as
    /// `prod` did, because the box that decides is `protected`. And `prod`
    /// itself stops gating the moment somebody unticks it.
    #[test]
    fn a_protected_custom_environment_gates_exactly_like_prod_did() {
        let defs = vec![
            EnvironmentDef::new("Production", "violet", true),
            EnvironmentDef::new("UAT", "blue", false),
            // A `prod` somebody deliberately unprotected — a throwaway cluster
            // that happens to carry the word.
            EnvironmentDef::new("prod", "red", false),
        ];

        for (environment, needs_flag) in [("Production", true), ("UAT", false), ("prod", false)] {
            let effective = resolve(&defs, environment);
            let profile = profile(environment, false);
            assert_eq!(
                authorize_write(&profile, &effective, "produce", false).is_err(),
                needs_flag,
                "{environment} without the flag"
            );
            assert!(
                authorize_write(&profile, &effective, "produce", true).is_ok(),
                "{environment} with the flag"
            );
        }

        // Nothing is keyed on the name: the refusal for `Production` is the
        // same refusal `prod` used to get, and it names the environment the
        // user actually invented.
        let refusal = authorize_write(
            &profile("Production", false),
            &resolve(&defs, "Production"),
            "produce",
            false,
        )
        .expect_err("a protected environment without the flag");
        assert_eq!(refusal.code, ExitCode::Refused);
        assert!(refusal.title.contains("Production"), "{refusal:?}");
        assert!(refusal.detail.contains(YES_PROD_FLAG), "{refusal:?}");
    }

    /// An environment nothing on this machine defines carries no protection —
    /// see the reasoning on [`kavka_core::environments`]. It is not silently
    /// gated *and* not silently ungated: it is the same answer the sidebar
    /// gives, which is what makes the two explainable together.
    #[test]
    fn an_undefined_environment_does_not_gate() {
        let effective = shipped("QA");
        assert!(!effective.known);
        assert!(authorize_write(&profile("QA", false), &effective, "produce", false).is_ok());
    }

    #[test]
    fn a_protected_refusal_names_the_flag_and_the_cluster() {
        let refusal = authorize_write(&profile("prod", false), &shipped("prod"), "produce", false)
            .expect_err("prod without the flag");
        assert_eq!(refusal.code, ExitCode::Refused);
        assert!(refusal.title.contains("nothing was sent"), "{refusal:?}");
        assert!(refusal.detail.contains(YES_PROD_FLAG), "{refusal:?}");
        // The address, not only the name: right-action-wrong-cluster is the
        // accident this line exists to prevent.
        assert!(
            refusal.detail.contains("kafka-1.internal:9092"),
            "{refusal:?}"
        );
        assert!(refusal.detail.contains("produce"), "{refusal:?}");
    }

    #[test]
    fn a_read_only_refusal_says_no_flag_lifts_it() {
        let refusal = authorize_write(&profile("prod", true), &shipped("prod"), "produce", true)
            .expect_err("read-only always refuses");
        assert!(
            refusal.title.contains("Read-only connection"),
            "{refusal:?}"
        );
        assert!(refusal.detail.contains("does not lift it"), "{refusal:?}");
        // It still names the flag, because "which knob is this" is the question.
        assert!(refusal.detail.contains(YES_PROD_FLAG), "{refusal:?}");
        // And the connection, because the caller may have several.
        assert!(refusal.detail.contains("orders"), "{refusal:?}");
    }

    /// Staging is not protected. The one cell people get wrong when they read
    /// "environment-gated" as "non-dev-gated".
    #[test]
    fn an_unprotected_environment_needs_no_flag() {
        assert!(authorize_write(
            &profile("staging", false),
            &shipped("staging"),
            "produce",
            false
        )
        .is_ok());
    }

    /// A refusal is exit 3, never exit 1: a script has to be able to tell
    /// "Kavka declined" from "the cluster said no".
    #[test]
    fn every_refusal_is_the_refused_code() {
        for (environment, read_only) in [("prod", false), ("dev", true)] {
            let refusal = authorize_write(
                &profile(environment, read_only),
                &shipped(environment),
                "produce",
                false,
            )
            .expect_err("a refusal");
            assert_eq!(refusal.code.code(), 3);
        }
    }
}
