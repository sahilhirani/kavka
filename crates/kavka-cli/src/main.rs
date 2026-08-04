//! `kavka` — the Kavka desktop app's clusters, on a terminal.
//!
//! Everything is in the library beside this file; `main` is argv in, an exit
//! code out. That split is what lets the whole flag surface, the output modes,
//! the write gate and the error vocabulary be unit-tested without spawning a
//! process — and the integration suite still drives this binary over a real
//! pipe, because "does it start at all" is the one thing a unit test cannot
//! see.
//!
//! **stdout belongs to the answer.** Every diagnostic goes to stderr, so a
//! pipe carries data and nothing else.

use clap::Parser;
use kavka_cli::cli::Cli;

fn main() {
    // clap handles --help, --version and a malformed command line itself, and
    // exits 2 on the last of those — which is why `ExitCode::Usage` is 2 rather
    // than a number of this program's choosing.
    let cli = Cli::parse();
    let details = cli.details;
    if let Err(error) = kavka_cli::run::run(cli) {
        eprint!("{}", error.render(details));
        std::process::exit(error.code.code());
    }
}
