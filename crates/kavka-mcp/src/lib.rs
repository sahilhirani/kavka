//! The Kavka MCP server: Kavka's read APIs — and two env-gated write APIs —
//! spoken to an AI agent over stdio, in the Model Context Protocol.
//!
//! It is the same product as the desktop app, without the window: the same
//! `profiles.json`, the same OS keychain, the same [`kavka_core`] connection,
//! consume, search, SQL and admin code. There is no second copy of anything
//! that talks to Kafka, and no configuration of its own — point Claude Code or
//! Cursor at the binary and it sees exactly the clusters the app sees.
//!
//! ```text
//!   Claude Code / Cursor ──stdio (newline-delimited JSON-RPC 2.0)──▶ kavka-mcp
//!                                                                      │
//!                        profiles.json + OS keychain ◀─────────────────┤
//!                                             kavka-core ◀─────────────┘
//!                                                  └─▶ Kafka clusters
//! ```
//!
//! # WHY A CRATE, not a second binary in the Tauri package
//!
//! Both were viable; this one is smaller in every direction that matters.
//!
//! A `[[bin]]` in `apps/desktop/src-tauri` inherits that package's dependency
//! list and its build script. That means every build of this server compiles
//! `tauri`, `wry`/WebView2, three plugins and `tauri-build`, and
//! `generate_context!` in `main.rs` — the one that reads `tauri.conf.json` and
//! requires `../dist` to exist — is a *package*-level fact: `cargo build -p
//! kavka-desktop --bin kavka-mcp` still runs the build script and still wants a
//! built frontend. A headless program that a container or a CI job should be
//! able to `cargo install` would have needed Node to build.
//!
//! It would also have inverted the dependency the whole repo is arranged
//! around. The shell is documented as "thin IPC layer over kavka-core"; a
//! second front end of the core belongs beside the first, not inside it. The
//! cost of a separate crate is one workspace member and one line of CI, and the
//! shell keeps exactly one addition — [`mcp_info`'s three
//! strings](../../../apps/desktop/src-tauri/src/lib.rs).
//!
//! # WHY NOT rmcp (the official Rust SDK)
//!
//! Measured on this machine (Windows, cargo 1.97.1, rmcp 3.1.0 with
//! `server,transport-io`), against the same repo discipline that priced
//! DataFusion in kavka-core's Cargo.toml:
//!
//! ```text
//!   crates in rmcp's tree                                    54
//!   …of them NOT already compiled by a kafka-ssl build        8
//!     schemars, schemars_derive, serde_derive_internals,
//!     dyn-clone, ref-cast, ref-cast-impl, rmcp, rmcp-macros
//!   cold build of that tree, alone                        31.4 s
//! ```
//!
//! **Eight crates is a fair price, and it is not why the answer is no.** The
//! reasons are three:
//!
//! 1. **It needs an async runtime this process has no use for.** rmcp's service
//!    loop is `tokio`, and every call it would dispatch lands in blocking
//!    librdkafka code (`kavka-core` is blocking by design — see its module
//!    docs), so each one would immediately be handed back to
//!    `spawn_blocking`. This server reads one pipe and answers one request at a
//!    time; a runtime here is scheduling machinery around a queue of one.
//! 2. **The tool schemas are the product.** Every description in
//!    [`tools`] states its units, its cap and what its errors mean, because the
//!    reader is a model deciding what to call next. `schemars` derives shapes
//!    from Rust types — which is the half that was never the hard part.
//! 3. **The subset is genuinely four methods.** `initialize`, `tools/list`,
//!    `tools/call`, `ping`, plus two notifications to swallow, over
//!    newline-delimited JSON. That is [`jsonrpc`] and the dispatch in
//!    [`server`], and it is the same trade this repo already made in
//!    `kavka_core::protocol`: eleven hand-written Kafka messages rather than a
//!    generated codec for two hundred.
//!
//! The SDK's real advantage is conformance drift — when MCP revises, rmcp
//! tracks it. That is answered here by negotiating explicitly rather than
//! assuming: [`server::SUPPORTED_PROTOCOL_VERSIONS`] is a list, and adding a
//! revision is an entry in it.
//!
//! # The write policy, which is the whole safety story
//!
//! Nine tools read. Two write, and each call passes three gates in this order —
//! see [`gate::authorize_write`], which is a pure function with a test per cell
//! of the matrix:
//!
//! 1. **The profile's `read_only` flag wins over everything.** No environment
//!    variable overrides it; the core would refuse the call anyway
//!    ([`kavka_core::connection::ClusterConnection::ensure_writable`],
//!    docs/ARCHITECTURE.md D5), and this gate refuses it before a socket is
//!    opened so a read-only connection never authenticates on behalf of a
//!    write.
//! 2. **`KAVKA_MCP_ALLOW_WRITES=1` must have been set when the server
//!    started.** Read once at startup, never re-read: what a running server may
//!    do cannot change under it.
//! 3. **A profile whose environment is marked PROTECTED additionally needs
//!    `KAVKA_MCP_ALLOW_PROD=1`.** Environments are the user's to define
//!    ([`kavka_core::environments`]) — `QA`, `UAT`, `Production` — and the
//!    gate reads the `protected` flag on the definition, never the name. The
//!    variable keeps its spelling because it lives in MCP client config files
//!    that are checked in; `kavka_list_profiles` reports
//!    `environment_protected` per connection so a model never guesses from a
//!    name.
//!
//! Every refusal names the exact variable, and the server's `initialize`
//! response says all of this in `instructions` so a model knows the rules
//! before it calls anything.
//!
//! # The masking policy, which is the other half of it
//!
//! **This server HONOURS a connection's masking rules, by default, in every
//! tool that returns a record.**
//!
//! Masking is not a property of a session in the app: it is
//! [`kavka_core::masking`] rules stored per connection in `masking.json`,
//! **beside the `profiles.json` this server already reads**, in the same
//! directory, written by the same person about the same cluster. So they do
//! reach here, and the only question was whether to apply them.
//!
//! They are applied, for a reason that is stronger here than in the app: an
//! agent's context is logged, replayed, and sent to a third party. Someone who
//! wrote a rule so a card number is not on their screen during a screen share
//! has said something quite specific about that field, and an MCP server that
//! streamed it into a model's transcript would be the one door in the product
//! that ignores them — a hole with the app's own name on it.
//!
//! - **Where.** [`server::Server::mask_set`] loads the profile's rules and
//!   compiles them **per call** (a tool call is at most 500 records; the app
//!   compiles once per session because it tails hundreds of thousands). Records
//!   are masked before anything renders them, and SQL rows go through
//!   [`kavka_core::masking::mask_sql_rows`] — the same function the desktop
//!   app's SQL view uses, so a column means the same thing in both.
//! - **Saying so.** An answer that was rewritten carries a `masking` object:
//!   `applied`, the rule count, and a sentence telling the model those values
//!   are redactions rather than data. An answer with no such object was not
//!   rewritten — the indicator tracks what actually changed, not what rules
//!   exist, because "your data was redacted" said about data that was not is
//!   the same class of lie as the reverse.
//! - **Opting out.** [`gate::UNMASKED_ENV`]`=1` in the environment **when the
//!   server starts**, like the two write variables and for the same reason:
//!   what a running server may hand out is a property of how it was launched,
//!   named in the MCP client's config file, and not something the agent can ask
//!   for mid-conversation. It is stated in `initialize`'s `instructions` and in
//!   every masked answer.
//! - **Writes are untouched.** `kavka_produce` sends the bytes it was given;
//!   masking is a read-side transform and applying it to a write would corrupt
//!   a topic while redacting nothing (the same reasoning that keeps the app's
//!   cross-cluster copy unmasked).
//!
//! **There is no other write surface.** No create/delete topic, no ACLs, no
//! broker or topic config, no reassignment, no Connect, no Schema Registry
//! writes — all of which kavka-core can do and the app exposes. An agent
//! deleting a topic is a blast radius nobody chose; produce and offset reset
//! are the two writes with an obvious undo story (another record; another
//! reset), and they are the two an incident actually needs.

pub mod config;
pub mod gate;
pub mod jsonrpc;
pub mod server;
pub mod tools;

pub use server::Server;
