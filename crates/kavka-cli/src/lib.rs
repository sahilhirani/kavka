//! `kavka` — the Kavka desktop app's connections, topics, search, SQL and
//! produce, on a terminal.
//!
//! It is the same product as the desktop app, without the window, and the same
//! product as the MCP server, without the model: the same `profiles.json`, the
//! same OS keychain, the same [`kavka_core`] connection, consume, search, SQL
//! and admin code, the same read-only enforcement and the same masking rules.
//! There is no second copy of anything that talks to Kafka, and no
//! configuration of its own — install it beside the app and it sees exactly the
//! clusters the app sees.
//!
//! ```text
//!   a terminal / a shell script ──argv──▶ kavka ──▶ kavka-core ──▶ Kafka
//!                                           │
//!                     profiles.json + OS keychain + masking.json
//! ```
//!
//! # WHY A CRATE, not a second binary in the Tauri package
//!
//! The same answer [`kavka_mcp`](../../kavka-mcp/src/lib.rs) gives, for the
//! same reason. A `[[bin]]` in `apps/desktop/src-tauri` inherits that package's
//! dependency list and its build script, so every build of this program would
//! compile `tauri`, WebView2 and three plugins, and `generate_context!` would
//! still want a built frontend to exist. A CLI a user should be able to
//! `cargo install`, or a CI job should be able to run, cannot need Node to
//! build. It also keeps the repo's one arrow pointing the right way: the shell
//! is a thin front end over the core, and a third front end belongs beside the
//! first two rather than inside one of them.
//!
//! # The output contract, which is most of what a CLI is
//!
//! **stdout is the answer; stderr is everything Kavka has to say about it.**
//! Every progress line, every warning, every cap notice and every error goes to
//! stderr, so `kavka fetch orders | jq` and `kavka search orders failed > hits`
//! are correct without a single flag.
//!
//! The shape of stdout depends on where it is pointed, which is
//! [`output::Mode::detect`]:
//!
//! - **A terminal gets a table** — docs/DESIGN.md §5.2's table, as far as a
//!   terminal can carry it: no borders, no zebra, identifiers left, quantities
//!   right, `∅` for absent, and every state carrying a word (Law 2).
//! - **A pipe or a file gets NDJSON** — one JSON object per line, so the answer
//!   streams into `jq`, `wc -l` or a file without a `--format` flag anyone has
//!   to remember. This is the whole reason the detection exists: the format a
//!   human wants and the format a script wants are different, and asking for it
//!   every time is a worse default than getting it right.
//! - **`--output json`** overrides both with one pretty-printed document
//!   carrying the answer *and* its limits, progress and masking state — the
//!   shape to keep when the caps matter as much as the rows.
//!
//! `--output table` forces the table into a pipe, which is what a person
//! reading through `less` wants.
//!
//! # Exit codes
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | The command answered. **An empty answer is still an answer** — a search with no matches exits 0, deliberately unlike `grep`, because a non-zero code here means Kavka could not answer at all. |
//! | 1 | The cluster, the keychain or the profile file failed. The message is [`errors::classify`]'s. |
//! | 2 | The command line was wrong (clap's own code, and the one this program raises for a flag combination clap cannot express). |
//! | 3 | **A guardrail refused it**: a read-only connection, or a write to a connection whose environment is marked protected, without `--yes-prod`. Nothing was sent. See [`gate`]. |
//! | 4 | The connection named does not exist on this machine, or there are none saved at all. |
//!
//! 3 is separate from 1 on purpose: a script has to be able to tell "Kavka
//! declined" from "the cluster said no", because only one of them is worth
//! retrying.
//!
//! # The write policy
//!
//! One write surface — `kavka produce` — and it passes two gates before a
//! socket is opened, in this order (see [`gate::authorize_write`], a pure
//! function with a test per cell of the matrix):
//!
//! 1. **The profile's `read_only` flag wins over everything.** No flag lifts
//!    it; the core would refuse the call anyway
//!    ([`kavka_core::connection::ClusterConnection::ensure_writable`]), and
//!    this refuses it before the connection authenticates on behalf of a write.
//! 2. **A profile whose environment is marked PROTECTED needs `--yes-prod` on
//!    the command line.** This is the terminal's version of docs/DESIGN.md §6
//!    layer 4 — type-to-confirm, environment-gated rather than action-gated. A
//!    protected environment always asks; an unprotected one never does.
//!
//!    Environments are the user's to define ([`kavka_core::environments`]):
//!    `dev` and `prod`, or `QA`, `UAT` and `Production`. The gate reads the
//!    `protected` flag on the environment's definition and never its name, so
//!    a protected `Production` behaves exactly as `prod` always did. The flag
//!    keeps its spelling because it is in shell histories and CI scripts;
//!    `kavka profiles list --output json` reports `environment_protected` per
//!    connection so a script never has to guess from a name.
//!
//! There is deliberately no `kavka delete-topic`, no config editing, no ACLs,
//! no offset reset. Produce is the write with an obvious undo story (another
//! record); the rest have a blast radius that belongs behind the app's
//! confirmations, where the blast radius can be *stated* before the click.
//!
//! # Masking
//!
//! **This program HONOURS a connection's display-masking rules by default**,
//! from the same `masking.json` the app writes, for the same reason the MCP
//! server does: a terminal is exactly the surface those rules were written for
//! — the screen share, the pasted snippet, the scrollback. `--unmasked`
//! returns raw payloads, and every answer a rule rewrote says so on stderr.

pub mod cli;
pub mod config;
pub mod errors;
pub mod gate;
pub mod output;
pub mod run;

pub use errors::{CliError, ExitCode};
