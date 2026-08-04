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
//! type-to-confirm, environment-gated rather than action-gated. Prod always
//! asks; dev never does.

use crate::errors::{CliError, ExitCode};
use kavka_core::profiles::{ConnectionProfile, Environment};

/// The flag a `prod` connection needs before this program writes to it.
pub const YES_PROD_FLAG: &str = "--yes-prod";

/// Decides whether `command` may write to `profile`, and explains a refusal in
/// two sentences the person can act on.
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
    if profile.environment == Environment::Prod && !yes_prod {
        return Err(CliError::stated(
            ExitCode::Refused,
            format!(
                "{name} is a production connection — nothing was sent",
                name = profile.name
            ),
            format!(
                "{command} would write to {servers}. Add {YES_PROD_FLAG} to the command if \
                 writing to production is genuinely what you want."
            ),
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
            "bootstrap_servers": ["kafka-1.internal:9092"],
            "auth": { "kind": "plaintext" },
            "read_only": read_only,
        }))
        .expect("a ConnectionProfile")
    }

    /// The whole matrix: {dev, staging, prod} × {writable, read-only} ×
    /// {--yes-prod, without}. Twelve cells, one table, so a rule that changes
    /// changes visibly.
    #[test]
    fn the_gating_matrix() {
        // (environment, read_only, yes_prod, allowed)
        let cases = [
            (Environment::Dev, false, false, true),
            (Environment::Dev, false, true, true),
            (Environment::Dev, true, false, false),
            (Environment::Dev, true, true, false),
            (Environment::Staging, false, false, true),
            (Environment::Staging, false, true, true),
            (Environment::Staging, true, false, false),
            (Environment::Staging, true, true, false),
            (Environment::Prod, false, false, false),
            (Environment::Prod, false, true, true),
            (Environment::Prod, true, false, false),
            // read-only wins even with the flag. This is the cell the ordering
            // exists for.
            (Environment::Prod, true, true, false),
        ];
        for (environment, read_only, yes_prod, allowed) in cases {
            let outcome = authorize_write(&profile(environment, read_only), "produce", yes_prod);
            assert_eq!(
                outcome.is_ok(),
                allowed,
                "env={environment:?} read_only={read_only} yes_prod={yes_prod} -> {outcome:?}"
            );
        }
    }

    #[test]
    fn a_prod_refusal_names_the_flag_and_the_cluster() {
        let refusal = authorize_write(&profile(Environment::Prod, false), "produce", false)
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
        let refusal = authorize_write(&profile(Environment::Prod, true), "produce", true)
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

    /// Staging is not prod. The one cell people get wrong when they read
    /// "environment-gated" as "non-dev-gated".
    #[test]
    fn staging_needs_no_flag() {
        assert!(authorize_write(&profile(Environment::Staging, false), "produce", false).is_ok());
    }

    /// A refusal is exit 3, never exit 1: a script has to be able to tell
    /// "Kavka declined" from "the cluster said no".
    #[test]
    fn every_refusal_is_the_refused_code() {
        for (environment, read_only) in [(Environment::Prod, false), (Environment::Dev, true)] {
            let refusal = authorize_write(&profile(environment, read_only), "produce", false)
                .expect_err("a refusal");
            assert_eq!(refusal.code.code(), 3);
        }
    }
}
