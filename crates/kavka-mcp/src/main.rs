//! `kavka-mcp` — the Kavka MCP server, one process per MCP client.
//!
//! It is not meant to be typed at: an MCP client (Claude Code, Cursor) spawns
//! it, speaks JSON-RPC down its stdin and reads answers off its stdout. The
//! Kavka app's Settings → MCP server section prints the exact configuration
//! snippet for both, and `--help` prints the same summary for anyone who found
//! this binary the other way round.
//!
//! **stdout belongs to the protocol.** Every diagnostic here goes to stderr,
//! which MCP clients collect as the server's log.

use kavka_mcp::{
    config,
    gate::{MaskPolicy, WritePolicy},
    Server,
};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => {}
        Some("--help" | "-h") => return help(),
        Some("--version" | "-V") => return println!("kavka-mcp {}", env!("CARGO_PKG_VERSION")),
        Some(other) => {
            eprintln!(
                "kavka-mcp: unknown argument {other:?}. This program takes none — it \
                       speaks MCP over stdin/stdout. Try --help."
            );
            std::process::exit(2);
        }
    }

    let policy = WritePolicy::from_env();
    let masking = MaskPolicy::from_env();
    let dir = match config::config_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("kavka-mcp: {e}");
            std::process::exit(1);
        }
    };

    // One startup line on stderr, because the three questions asked of a server
    // that "isn't working" are which connections it can see, whether it can
    // write, and — since it honours the app's redactions — whether what it
    // returns is the topic's own bytes. All three are answered here, before the
    // first request.
    eprintln!(
        "kavka-mcp {version} — connections: {file}; writes: {writes}, production writes: {prod}; \
         masking: {masking}",
        version = env!("CARGO_PKG_VERSION"),
        file = config::profiles_file(&dir).display(),
        writes = enabled(policy.writes_enabled),
        prod = enabled(policy.prod_allowed),
        masking = if masking.honors_rules() {
            "honoured (the app's rules apply)"
        } else {
            "OFF — payloads are returned raw"
        },
    );

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut server = Server::new(dir, policy, masking);
    if let Err(e) = server.run(stdin.lock(), stdout.lock()) {
        eprintln!("kavka-mcp: the connection to the MCP client ended: {e}");
        std::process::exit(1);
    }
}

fn enabled(flag: bool) -> &'static str {
    if flag {
        "enabled"
    } else {
        "disabled"
    }
}

fn help() {
    println!(
        "kavka-mcp {version} — Kavka's Kafka tools, for an MCP client.

USAGE
  Spawned by an MCP client; speaks JSON-RPC 2.0 over stdin/stdout. It takes no
  arguments and has no configuration of its own: connections come from the
  Kavka desktop app's profiles.json and the OS keychain.

    claude mcp add kavka -- <path to this binary>

ENVIRONMENT
  (Names are substituted from the gate itself, so the description below and the
  variable a refusal names can never drift apart.)

  {writes}=1
      Allow the two write tools. Read ONCE, at startup — setting it later, or in
      another shell, changes nothing about a running server.

  {prod}=1
      Additionally allow writes to connections whose ENVIRONMENT IS MARKED
      PROTECTED in the Kavka app. Environments are yours to define — `dev`,
      `QA`, `UAT`, `Production` — and each one is protected or not; the gate
      reads that flag, never the name. kavka_list_profiles reports
      `environment_protected` per connection. Connections marked read-only
      refuse either way, and no variable lifts that.

  {unmasked}=1
      Return payloads RAW. By default this server applies the display-masking
      rules saved for a connection in the Kavka app, and every answer they
      rewrote says so. Read once, at startup, like the two above.

  {config_dir}=<folder>
      Read profiles.json (and masking.json and environments.json beside it)
      from this folder instead of the app's own config directory.

READ TOOLS  list profiles · cluster overview · list topics · topic detail ·
            fetch messages · search · SQL · consumer groups · group detail
WRITE TOOLS produce one record · reset a consumer group's offsets
            Nothing else: no topic or config changes, no ACLs, no Connect, no
            Schema Registry writes.",
        version = env!("CARGO_PKG_VERSION"),
        writes = gate_env_names().0,
        prod = gate_env_names().1,
        unmasked = kavka_mcp::gate::UNMASKED_ENV,
        config_dir = config::CONFIG_DIR_ENV,
    );
}

/// The two gate variables, so `--help` cannot drift from the gate itself.
fn gate_env_names() -> (&'static str, &'static str) {
    (
        kavka_mcp::gate::ALLOW_WRITES_ENV,
        kavka_mcp::gate::ALLOW_PROD_ENV,
    )
}
