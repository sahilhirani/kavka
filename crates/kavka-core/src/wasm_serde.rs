//! Custom decoders as sandboxed WebAssembly modules (docs/ARCHITECTURE.md D4,
//! docs/ROADMAP.md Phase 5b).
//!
//! A team whose payloads are in a format nobody outside their company has heard
//! of writes forty lines in any language that compiles to wasm, points a
//! profile at the `.wasm` file, and reads their topics in Kavka. The built-in
//! ladder in [`crate::serdes`] can never cover that case, and shipping a plugin
//! API that runs native code in the app's process would trade the whole
//! security story for it.
//!
//! # ABI v1 — the stable contract
//!
//! This section is the specification. `docs/examples/wasm-serde/` is a working
//! implementation of it, with build instructions.
//!
//! A plugin is a WebAssembly module (`.wasm`, or `.wat` text — both load) that
//! exports:
//!
//! | export | signature | meaning |
//! |---|---|---|
//! | `memory` | `memory` | the module's linear memory, exported so the host can read and write it |
//! | `kavka_abi_version` | `() -> i32` | must return [`ABI_VERSION`]. Checked once, at load |
//! | `kavka_alloc` | `(i32) -> i32` | reserve N bytes, answer their address. `0` means "cannot" |
//! | `kavka_free` | `(i32, i32) -> ()` | release the address/length pair the host passed |
//! | `kavka_decode` | `(i32, i32) -> i32` | decode the bytes at address/length; answer the address of a **result block** |
//!
//! It imports **nothing**. There is no WASI, no clock, no randomness, no
//! filesystem and no host function of any kind — a decoder is pure compute over
//! bytes, and a module that asks for an import is refused at load with the
//! import named. That is what makes the sandbox describable in one sentence.
//!
//! ## The result block
//!
//! A run of bytes in the module's own memory, little-endian (wasm is):
//!
//! ```text
//! offset 0        u8    status:  0 = decoded, 1 = declined, 2 = failed
//! offset 1        u32   json_len
//! offset 5        [u8]  json_len bytes of UTF-8
//! ```
//!
//! - **status 0** — the UTF-8 is the decoded record as JSON. Kavka parses it;
//!   text that is not JSON is a plugin bug and is reported as one.
//! - **status 1** — "these bytes are not mine". The built-in ladder runs as if
//!   the plugin were not there. This is the answer for a plugin registered on a
//!   topic that also carries other formats, and it costs nothing.
//! - **status 2** — the plugin failed, and the UTF-8 is why. The ladder still
//!   runs; the message is recorded once for the session.
//!
//! ## Who frees what
//!
//! The **host** owns the input buffer: it calls `kavka_alloc`, writes the
//! record, calls `kavka_decode`, and calls `kavka_free` on that same
//! address/length when the call returns. A plugin must not free it or take
//! ownership of it. The host then calls `kavka_free(result, 5 + json_len)` on
//! the result block. A plugin that ignores `kavka_free` entirely still works —
//! its memory grows until the 10 MB cap, and each record starts from a fresh
//! instance after that — but it is the wrong thing to write.
//!
//! # The sandbox
//!
//! - **No imports**, as above. A module cannot reach anything.
//! - **[`MAX_MEMORY_BYTES`] of linear memory.** Enforced by wasmtime's own
//!   store limiter, at instantiation *and* at every `memory.grow`, so a module
//!   whose initial memory is already over the cap fails to load and one that
//!   grows into it gets `-1` from `memory.grow` like any other allocation
//!   failure.
//! - **[`FUEL_PER_RECORD`] of fuel per record**, reset before every call. Fuel
//!   is a count of executed WebAssembly operations, so it bounds *work* rather
//!   than time — the wall clock it corresponds to is approximate and depends on
//!   the machine. See the constant.
//! - **A trapped instance is thrown away.** After a trap or fuel exhaustion the
//!   sandbox is not returned to the pool, so the next record runs against a
//!   freshly instantiated module rather than against whatever state the trap
//!   left behind — and that includes a `kavka_free` that trapped after the
//!   answer was already read.
//! - **The module comes from an absolute path on this machine.** A relative
//!   path and a UNC path are both refused, at save time and again here: the
//!   sandbox bounds what a plugin can DO, and this bounds what gets to be the
//!   plugin. See [`validate_plugin_path`].
//!
//! # Errors are recorded once per session
//!
//! Exactly like [`crate::sr::SchemaRegistry`], and for exactly the same reason:
//! a plugin that fails on one record fails on all of them, and a 2000-message
//! fetch must produce one line for the UI rather than two thousand. See
//! [`WasmSerde::note_error`].

use crate::profiles::WasmSerdeConfig;
use crate::serdes::CustomDecoder;
use std::path::Path;
use std::sync::{Arc, Mutex};
use wasmtime::{
    Config, Engine, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, Trap,
    TypedFunc,
};

/// The ABI version this build speaks. A plugin's `kavka_abi_version` must
/// return exactly this.
///
/// Bumping it is a breaking change for every plugin in the world, so it is a
/// last resort: an *additive* change (a new optional export, a new status
/// value) keeps v1 and is detected by asking the module whether it has the
/// export.
pub const ABI_VERSION: i32 = 1;

/// The linear-memory ceiling for one plugin instance.
///
/// Ten megabytes is comfortably more than a decoder needs — the largest record
/// Kafka will hand it is bounded by the broker's `message.max.bytes`, and the
/// decoder's own working set is a parse of that — and small enough that eight
/// concurrent sandboxes (one per search worker) cannot become the reason the
/// app is swapping.
pub const MAX_MEMORY_BYTES: usize = 10 * 1024 * 1024;

/// Fuel granted before each record.
///
/// **Fuel is a count of executed WebAssembly operations, not milliseconds, and
/// the time it corresponds to is approximate.** That is the right way round:
/// the limit is deterministic, so a plugin that works on one laptop does not
/// mysteriously time out on a slower one — a slow machine gets the same amount
/// of *work* and more time.
///
/// The calibration, measured by `the_fuel_budget_is_calibrated` in this
/// module's tests (release build, Windows, x86-64):
///
/// ```text
/// 100,000,000 fuel of a branch loop      18 ms   ← the cheapest operators
/// 100,000,000 fuel of a load/store loop   2 ms   ← fewer, heavier iterations
/// ```
///
/// So the per-record ceiling this buys is roughly **2–20 ms**, comfortably
/// inside the ~100 ms the Phase 5b contract asks for. The headroom is
/// deliberate and it is not stinginess: a decoder that reads one Kafka record
/// spends thousands of operations on it, not tens of millions, so a plugin that
/// reaches this budget is looping — and the sooner a looping plugin is stopped,
/// the less of a million-record search it eats.
pub const FUEL_PER_RECORD: u64 = 100_000_000;

/// Fuel for one instantiation, which runs the module's start function and any
/// data-segment initialization. Separate from the per-record budget so a
/// pathological module cannot spend the record's budget before the record
/// arrives.
const FUEL_PER_INSTANTIATION: u64 = 10_000_000;

/// How many instances one plugin keeps warm. Matches the search engine's worker
/// ceiling (`crate::search::MAX_WORKERS` is 8), so a full-width parallel scan
/// never serializes on instantiation.
const MAX_SANDBOXES: usize = 8;

/// The result block's fixed header: one status byte, then a little-endian u32
/// length.
const RESULT_HEADER_LEN: usize = 5;

/// What Kavka could not do with a plugin, classified.
///
/// The classification is the feature: "your plugin did not work" is not
/// actionable, and the six things that actually go wrong — a path Kavka will
/// not execute from, a file that is not there, a module that is not wasm, an
/// import that cannot be satisfied, an export that is missing or the wrong
/// shape, and a run that trapped — each have a different fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmSerdeError {
    /// The configured path is not one Kavka will execute code from — see
    /// [`validate_plugin_path`].
    Path { path: String, cause: String },
    /// The file could not be read.
    Read { path: String, cause: String },
    /// The bytes are not a WebAssembly module this build can compile.
    Compile { cause: String },
    /// The module asks for a host import. ABI v1 provides none.
    Import { module: String, name: String },
    /// A required export is missing, or has the wrong signature.
    MissingExport { name: String, cause: String },
    /// `kavka_abi_version` answered something other than [`ABI_VERSION`].
    AbiVersion { found: i32 },
    /// Instantiating the module failed — most often the 10 MB memory cap.
    Instantiate { cause: String },
    /// The module trapped: an `unreachable`, an out-of-bounds access, a divide
    /// by zero.
    Trap { cause: String },
    /// The module used its whole fuel budget for this record.
    FuelExhausted,
    /// `kavka_alloc` could not give the host a buffer for the record.
    OutOfMemory { wanted: usize },
    /// The call returned, and what it returned is not an ABI-v1 result block:
    /// a pointer outside memory, a length that runs off the end, bytes that are
    /// not UTF-8, or text that is not JSON.
    MalformedOutput { cause: String },
}

impl std::fmt::Display for WasmSerdeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path { path, cause } => {
                write!(f, "Kavka won't load a serde plugin from {path}: {cause}")
            }
            Self::Read { path, cause } => write!(f, "can't read the plugin at {path}: {cause}"),
            Self::Compile { cause } => write!(
                f,
                "this file isn't a WebAssembly module Kavka can load: {cause}"
            ),
            Self::Import { module, name } => write!(
                f,
                "this plugin imports {module}::{name}, and a Kavka serde plugin may import \
                 nothing at all — build it without WASI (for Rust: the \
                 wasm32-unknown-unknown target, not wasm32-wasip1)"
            ),
            Self::MissingExport { name, cause } => write!(
                f,
                "this plugin doesn't export {name} the way ABI v{ABI_VERSION} needs it: {cause}"
            ),
            Self::AbiVersion { found } => write!(
                f,
                "this plugin says it speaks ABI v{found}; this build of Kavka speaks \
                 v{ABI_VERSION}"
            ),
            Self::Instantiate { cause } => write!(f, "this plugin wouldn't start: {cause}"),
            Self::Trap { cause } => write!(f, "the plugin crashed on a record: {cause}"),
            Self::FuelExhausted => write!(
                f,
                "the plugin used its whole budget on one record ({FUEL_PER_RECORD} WebAssembly \
                 operations) — it is probably looping"
            ),
            Self::OutOfMemory { wanted } => write!(
                f,
                "the plugin couldn't allocate {wanted} bytes for a record inside its \
                 {} MB of memory",
                MAX_MEMORY_BYTES / (1024 * 1024)
            ),
            Self::MalformedOutput { cause } => write!(
                f,
                "the plugin answered something that isn't an ABI v{ABI_VERSION} result: {cause}"
            ),
        }
    }
}

/// One loaded plugin.
///
/// `Send + Sync`: the engine and the compiled module are both shareable, and
/// the instances live behind a mutex-guarded free list so the search engine's
/// eight workers can decode in parallel without eight copies of the module.
pub struct WasmSerde {
    name: String,
    engine: Engine,
    module: Module,
    sandboxes: Mutex<Vec<Sandbox>>,
    first_error: Mutex<Option<String>>,
}

impl std::fmt::Debug for WasmSerde {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSerde")
            .field("name", &self.name)
            .finish()
    }
}

/// One warm instance and the four functions ABI v1 needs from it.
struct Sandbox {
    store: Store<StoreLimits>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    free: TypedFunc<(i32, i32), ()>,
    decode: TypedFunc<(i32, i32), i32>,
    /// Still fit to serve another record.
    ///
    /// Cleared when a call trapped *after* the verdict was already decided —
    /// which is exactly `kavka_free`. That failure is not worth failing the
    /// record over (the answer has already been read out of the module's
    /// memory) but it is still a trap, and an instance that trapped must not
    /// be the one the next record runs against. See [`WasmSerde::run`].
    healthy: bool,
}

/// What one call to `kavka_decode` said.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Decoded(String),
    Declined,
    Failed(String),
}

/// Whether Kavka will load a plugin from this path — checked at **save** time
/// by the shell and again at **load** time here.
///
/// A `.wasm` file is code this process executes, so where it comes from is a
/// security question rather than a convenience one, and there are exactly two
/// refusals:
///
/// - **A relative path.** It resolves against the process's *current working
///   directory*, which for a desktop app is whatever the launcher happened to
///   set — the Explorer shortcut, the terminal it was started from, the
///   installer. And a connection is a document that **travels**: a profile
///   export is a thing people mail each other, so `./decoder.wasm` names the
///   file beside the repo on one machine and something else entirely on the
///   next. An absolute path either exists or does not, on both.
/// - **A UNC path** (`\\build-server\share\decoder.wasm`, and its
///   `\\?\UNC\…` spelling). That is a file on somebody else's machine, fetched
///   over the network by the loader every time a connection opens — the
///   execution source moves off this computer, silently, and whoever can write
///   that share decides what runs here. `\\?\C:\…` is the verbatim spelling of
///   a LOCAL path and is not this; it is allowed.
///
/// Both are refused rather than repaired: Kavka cannot know which directory a
/// relative path meant, and resolving it against the config dir would be a
/// guess with an execution consequence.
///
/// It says nothing about whether the file **exists** — that is
/// [`WasmSerde::load`]'s answer, at the moment it matters, and a plugin that is
/// fine at save time and missing tomorrow would pass an existence check anyway.
pub fn validate_plugin_path(path: &str) -> Result<(), WasmSerdeError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(WasmSerdeError::Path {
            path: "an empty path".to_string(),
            cause: "a decoder needs the full path to a .wasm (or .wat) file".to_string(),
        });
    }
    if is_unc(trimmed) {
        return Err(WasmSerdeError::Path {
            path: trimmed.to_string(),
            cause: "that is a network path, and a plugin is code Kavka runs — the file would be \
                    fetched from another machine every time this connection opens, and whoever \
                    can write that share would decide what runs here. Copy it to this machine \
                    and point at the local copy"
                .to_string(),
        });
    }
    if !Path::new(trimmed).is_absolute() {
        return Err(WasmSerdeError::Path {
            path: trimmed.to_string(),
            cause: "that path is relative, so what it names depends on the folder Kavka happens \
                    to have been started from — and a connection travels: an exported profile \
                    would resolve it against a different folder on the machine that imports it. \
                    Give the full path"
                .to_string(),
        });
    }
    Ok(())
}

/// `\\server\share\decoder.wasm`, and its verbatim spelling
/// `\\?\UNC\server\share\…`.
///
/// Recognised by hand rather than through [`std::path::Prefix`], which only
/// parses prefixes on Windows: the same UNC path read by a Linux or macOS build
/// is merely *relative* there and would be refused with the wrong sentence.
/// What a profile may CARRY is the same question on every platform, so it is
/// answered the same way on every platform.
fn is_unc(path: &str) -> bool {
    let slashed = path.replace('\\', "/");
    let Some(rest) = slashed.strip_prefix("//") else {
        return false;
    };
    match rest.strip_prefix("?/") {
        // `\\?\UNC\server\share` is a UNC path; `\\?\C:\…` beside it is the
        // verbatim spelling of a local one, and is not.
        Some(verbatim) => verbatim
            .get(..4)
            .is_some_and(|kind| kind.eq_ignore_ascii_case("UNC/")),
        None => true,
    }
}

impl WasmSerde {
    /// Loads a plugin from disk. `name` is the profile's name for it, and the
    /// name that appears in an error and in the payload's provenance.
    ///
    /// The path rule is enforced here as well as at save time, and that is
    /// defence in depth rather than belt and braces: `profiles.json` is a file
    /// on disk that a person can edit, an older build can have written and an
    /// import can have brought in from somewhere else, so the check that
    /// matters is the one standing in front of the loader.
    pub fn load(name: &str, path: &Path) -> Result<Self, WasmSerdeError> {
        validate_plugin_path(&path.display().to_string())?;
        let bytes = std::fs::read(path).map_err(|e| WasmSerdeError::Read {
            path: path.display().to_string(),
            cause: e.to_string(),
        })?;
        Self::from_bytes(name, &bytes)
    }

    /// Loads a plugin from bytes — WebAssembly binary, or WebAssembly text,
    /// which is what lets this module's own tests carry their fixtures as
    /// readable source rather than as committed blobs.
    ///
    /// **Everything that can be checked once is checked here**: the imports,
    /// the exports, their signatures and the ABI version. A plugin that loads
    /// is one that will not fail for any of those reasons on record 400,000.
    pub fn from_bytes(name: &str, bytes: &[u8]) -> Result<Self, WasmSerdeError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        // A decoder is one memory and pure compute. Threads, the GC proposal
        // and the component model are not compiled into this build of wasmtime
        // at all (see Cargo.toml), so they are off by construction rather than
        // by a call that could be deleted; `multi_memory` is compiled in and is
        // turned off here, because the ABI names one export called `memory` and
        // a module with several is not one of ours.
        config.wasm_multi_memory(false);
        let engine = Engine::new(&config).map_err(|e| WasmSerdeError::Compile {
            cause: e.to_string(),
        })?;
        let module = Module::new(&engine, bytes).map_err(|e| WasmSerdeError::Compile {
            cause: first_line(&e.to_string()),
        })?;

        // Named before instantiation, so the message says *which* import rather
        // than "unknown import" from three layers down.
        if let Some(import) = module.imports().next() {
            return Err(WasmSerdeError::Import {
                module: import.module().to_string(),
                name: import.name().to_string(),
            });
        }

        let plugin = Self {
            name: name.to_string(),
            engine,
            module,
            sandboxes: Mutex::new(Vec::new()),
            first_error: Mutex::new(None),
        };
        // Instantiating once proves the exports, their signatures and the ABI
        // version, and leaves the instance warm for the first record.
        let sandbox = plugin.instantiate()?;
        plugin.checkin(sandbox);
        Ok(plugin)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Builds one sandbox: a store with the limits on, an instance, and the
    /// four typed functions.
    fn instantiate(&self) -> Result<Sandbox, WasmSerdeError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_MEMORY_BYTES)
            // One of each: a decoder needs one instance and one memory, and a
            // module that declares more is not one of ours.
            .instances(1)
            .memories(1)
            .tables(1)
            .build();
        let mut store = Store::new(&self.engine, limits);
        store.limiter(|limits| limits);
        store
            .set_fuel(FUEL_PER_INSTANTIATION)
            .map_err(|e| WasmSerdeError::Instantiate {
                cause: e.to_string(),
            })?;

        let instance = Instance::new(&mut store, &self.module, &[]).map_err(|e| {
            WasmSerdeError::Instantiate {
                cause: first_line(&e.to_string()),
            }
        })?;

        let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| {
            WasmSerdeError::MissingExport {
                name: "memory".to_string(),
                cause: "the module exports no linear memory called `memory`".to_string(),
            }
        })?;
        let version: TypedFunc<(), i32> = typed(&instance, &mut store, "kavka_abi_version")?;
        let alloc: TypedFunc<i32, i32> = typed(&instance, &mut store, "kavka_alloc")?;
        let free: TypedFunc<(i32, i32), ()> = typed(&instance, &mut store, "kavka_free")?;
        let decode: TypedFunc<(i32, i32), i32> = typed(&instance, &mut store, "kavka_decode")?;

        let found = version
            .call(&mut store, ())
            .map_err(|e| classify_trap(&e))?;
        if found != ABI_VERSION {
            return Err(WasmSerdeError::AbiVersion { found });
        }

        Ok(Sandbox {
            store,
            memory,
            alloc,
            free,
            decode,
            healthy: true,
        })
    }

    fn checkout(&self) -> Result<Sandbox, WasmSerdeError> {
        if let Some(sandbox) = self.sandboxes.lock().unwrap().pop() {
            return Ok(sandbox);
        }
        self.instantiate()
    }

    /// Returns a sandbox to the pool. Called **only** for a sandbox that
    /// finished cleanly: one that trapped is dropped instead, so no record ever
    /// runs against state a trap left behind.
    fn checkin(&self, sandbox: Sandbox) {
        let mut pool = self.sandboxes.lock().unwrap();
        if pool.len() < MAX_SANDBOXES {
            pool.push(sandbox);
        }
    }

    /// Runs one record through the plugin.
    ///
    /// `Ok(None)` is a decline (ABI status 1) — the caller falls through to the
    /// built-in ladder, which is the normal path for a plugin registered on a
    /// topic that carries more than one format.
    pub fn decode_bytes(&self, bytes: &[u8]) -> Result<Option<serde_json::Value>, WasmSerdeError> {
        let mut sandbox = self.checkout()?;
        let verdict = self.run(&mut sandbox, bytes);
        // A trap leaves the instance in whatever state it was in; the next
        // record gets a new one. `healthy` is the same rule for the trap that
        // happens too late to be the verdict — a `kavka_free` that crashed.
        let trapped = matches!(
            &verdict,
            Err(WasmSerdeError::Trap { .. } | WasmSerdeError::FuelExhausted)
        );
        if !trapped && sandbox.healthy {
            self.checkin(sandbox);
        }
        match verdict? {
            Verdict::Declined => Ok(None),
            Verdict::Failed(message) => Err(WasmSerdeError::MalformedOutput { cause: message }),
            Verdict::Decoded(json) => {
                serde_json::from_str(&json)
                    .map(Some)
                    .map_err(|e| WasmSerdeError::MalformedOutput {
                        cause: format!(
                            "it said it decoded a record, and the text is not JSON: {e}"
                        ),
                    })
            }
        }
    }

    fn run(&self, sandbox: &mut Sandbox, bytes: &[u8]) -> Result<Verdict, WasmSerdeError> {
        let len = i32::try_from(bytes.len()).map_err(|_| WasmSerdeError::OutOfMemory {
            wanted: bytes.len(),
        })?;
        sandbox
            .store
            .set_fuel(FUEL_PER_RECORD)
            .map_err(|e| WasmSerdeError::Instantiate {
                cause: e.to_string(),
            })?;

        let input = sandbox
            .alloc
            .call(&mut sandbox.store, len)
            .map_err(|e| classify_trap(&e))?;
        if input <= 0 && len > 0 {
            return Err(WasmSerdeError::OutOfMemory {
                wanted: bytes.len(),
            });
        }
        write(&sandbox.memory, &mut sandbox.store, input, bytes)?;

        let result = sandbox
            .decode
            .call(&mut sandbox.store, (input, len))
            .map_err(|e| classify_trap(&e))?;
        let verdict = read_result(&sandbox.memory, &mut sandbox.store, result);

        // The input is the host's, always; the result block is the plugin's and
        // is handed back whatever the verdict was.
        //
        // A FAILURE TO FREE DOES NOT FAIL THE RECORD — the answer has already
        // been read out of the module's memory, so the verdict stands — BUT IT
        // DOES RETIRE THE SANDBOX. `kavka_free` trapping is a trap like any
        // other: the instance is now in whatever state the trap left it in, and
        // returning it to the warm pool would hand that state to the next
        // record. Both results are captured for that reason; ignoring either
        // one would let a poisoned instance back into circulation.
        let freed_input = sandbox.free.call(&mut sandbox.store, (input, len));
        let freed_result = match &verdict {
            Ok((_, block_len)) => sandbox.free.call(&mut sandbox.store, (result, *block_len)),
            Err(_) => Ok(()),
        };
        if freed_input.is_err() || freed_result.is_err() {
            sandbox.healthy = false;
        }
        verdict.map(|(verdict, _)| verdict)
    }

    /// Records the session's first failure. Later ones are dropped: the first
    /// is the one that explains the rest, and a per-message error is noise the
    /// UI has nowhere to put.
    pub fn note_error(&self, message: String) {
        let mut slot = self.first_error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    /// The session's first failure, if there was one.
    pub fn error(&self) -> Option<String> {
        self.first_error.lock().unwrap().clone()
    }

    /// The session's first failure, clearing it — for a caller that reports it
    /// and must not report it twice.
    pub fn take_error(&self) -> Option<String> {
        self.first_error.lock().unwrap().take()
    }
}

impl CustomDecoder for WasmSerde {
    fn name(&self) -> &str {
        &self.name
    }

    fn decode(&self, bytes: &[u8]) -> Option<serde_json::Value> {
        match self.decode_bytes(bytes) {
            Ok(json) => json,
            Err(e) => {
                self.note_error(format!("the serde plugin {:?}: {e}", self.name));
                None
            }
        }
    }
}

fn typed<Params, Results>(
    instance: &Instance,
    store: &mut Store<StoreLimits>,
    name: &str,
) -> Result<TypedFunc<Params, Results>, WasmSerdeError>
where
    Params: wasmtime::WasmParams,
    Results: wasmtime::WasmResults,
{
    if instance.get_func(&mut *store, name).is_none() {
        return Err(WasmSerdeError::MissingExport {
            name: name.to_string(),
            cause: "the module does not export it".to_string(),
        });
    }
    instance
        .get_typed_func(store, name)
        .map_err(|e| WasmSerdeError::MissingExport {
            name: name.to_string(),
            cause: first_line(&e.to_string()),
        })
}

fn write(
    memory: &Memory,
    store: &mut Store<StoreLimits>,
    at: i32,
    bytes: &[u8],
) -> Result<(), WasmSerdeError> {
    let at = usize::try_from(at).map_err(|_| WasmSerdeError::MalformedOutput {
        cause: "kavka_alloc answered a negative address".to_string(),
    })?;
    memory
        .write(store, at, bytes)
        .map_err(|e| WasmSerdeError::MalformedOutput {
            cause: format!("kavka_alloc answered an address the record does not fit at: {e}"),
        })
}

/// Reads a result block, answering the verdict and the block's total length
/// (which is what the host has to hand back to `kavka_free`).
fn read_result(
    memory: &Memory,
    store: &mut Store<StoreLimits>,
    at: i32,
) -> Result<(Verdict, i32), WasmSerdeError> {
    let data = memory.data(&*store);
    let start = usize::try_from(at).map_err(|_| WasmSerdeError::MalformedOutput {
        cause: "kavka_decode answered a negative address".to_string(),
    })?;
    let header = data.get(start..start + RESULT_HEADER_LEN).ok_or_else(|| {
        WasmSerdeError::MalformedOutput {
            cause: format!(
                "kavka_decode answered address {start}, and there is no {RESULT_HEADER_LEN}-byte \
                 header there"
            ),
        }
    })?;
    let status = header[0];
    let json_len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let body = data
        .get(start + RESULT_HEADER_LEN..start + RESULT_HEADER_LEN + json_len)
        .ok_or_else(|| WasmSerdeError::MalformedOutput {
            cause: format!(
                "it says its answer is {json_len} bytes long, and that runs off the end of its \
                 own memory"
            ),
        })?;
    let text = std::str::from_utf8(body).map_err(|e| WasmSerdeError::MalformedOutput {
        cause: format!("its answer is not UTF-8: {e}"),
    })?;
    let block_len = i32::try_from(RESULT_HEADER_LEN + json_len).unwrap_or(i32::MAX);
    let verdict = match status {
        0 => Verdict::Decoded(text.to_string()),
        1 => Verdict::Declined,
        2 => Verdict::Failed(if text.is_empty() {
            "it reported a failure and said nothing about it".to_string()
        } else {
            text.to_string()
        }),
        other => {
            return Err(WasmSerdeError::MalformedOutput {
                cause: format!(
                    "status {other} is not one ABI v{ABI_VERSION} defines (0 decoded, 1 declined, \
                     2 failed)"
                ),
            })
        }
    };
    Ok((verdict, block_len))
}

/// Fuel exhaustion is a trap like any other as far as wasmtime's API is
/// concerned, and completely unlike one as far as the user is concerned: a trap
/// is a bug in the plugin's logic and fuel exhaustion is a plugin that is too
/// slow or looping. They get different sentences, so they are told apart here.
fn classify_trap(error: &wasmtime::Error) -> WasmSerdeError {
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => WasmSerdeError::FuelExhausted,
        _ => WasmSerdeError::Trap {
            cause: first_line(&error.to_string()),
        },
    }
}

/// wasmtime's errors carry a backtrace under the message; the first line is the
/// sentence, and the rest belongs in a log rather than under a form field.
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).trim().to_string()
}

// ---------------------------------------------------------------------------
// A profile's plugins.
// ---------------------------------------------------------------------------

/// Every plugin one profile configures, with the globs that decide which topic
/// each one reads.
///
/// Built once per session, like [`crate::masking::MaskSet`] and for the same
/// reason: compiling a WebAssembly module is milliseconds, and a decode path
/// that did it per record would be slower than the format it is decoding.
#[derive(Debug, Default)]
pub struct WasmSerdes {
    plugins: Vec<(WasmSerdeConfig, Arc<WasmSerde>)>,
}

impl WasmSerdes {
    /// Loads a profile's plugins, answering the ones that loaded and a sentence
    /// for each one that did not.
    ///
    /// A plugin that fails to load does not stop the others or the session: the
    /// built-in ladder is always there, so the honest degradation is "this
    /// topic reads as hex and here is why", not "this connection will not
    /// open".
    pub fn load(configs: &[WasmSerdeConfig]) -> (Self, Vec<String>) {
        let mut plugins = Vec::new();
        let mut problems = Vec::new();
        for config in configs {
            match WasmSerde::load(&config.name, Path::new(&config.path)) {
                Ok(plugin) => plugins.push((config.clone(), Arc::new(plugin))),
                Err(e) => problems.push(format!("the serde plugin {:?}: {e}", config.name)),
            }
        }
        (Self { plugins }, problems)
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// The plugin for this topic, if one claims it.
    ///
    /// **The first match wins**, in the order the profile lists them, so two
    /// overlapping globs resolve deterministically and the user can reorder
    /// them to say which. Two plugins running over the same bytes would only
    /// raise the question of which answer to believe.
    pub fn for_topic(&self, topic: &str) -> Option<Arc<WasmSerde>> {
        self.plugins
            .iter()
            .find(|(config, _)| config.matches_topic(topic))
            .map(|(_, plugin)| Arc::clone(plugin))
    }

    /// Every plugin's session error, for a caller reporting once.
    pub fn take_errors(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter_map(|(_, plugin)| plugin.take_error())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A complete, correct ABI v1 plugin, in WebAssembly text.
    ///
    /// It decodes a made-up format: a record whose first byte is `B` is
    /// "ours" and decodes to a fixed JSON document; anything else is declined,
    /// so the built-in ladder takes it. That branch is the point — it proves
    /// the input bytes actually reached the guest, and it exercises the decline
    /// path that makes a plugin safe to register on a mixed topic.
    ///
    /// The two result blocks live in a data segment at fixed addresses. A real
    /// plugin builds its answer at run time; this one does not need to, and a
    /// fixture that is readable is worth more here than one that is realistic.
    const MODULE_V1: &str = r#"
(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))

  ;; 64:  status 0, len 23 (0x17), {"decoded":"by-plugin"}
  (data (i32.const 64) "\00\17\00\00\00{\22decoded\22:\22by-plugin\22}")
  ;; 128: status 1, len 0 — declined
  (data (i32.const 128) "\01\00\00\00\00")

  (func (export "kavka_abi_version") (result i32) (i32.const 1))

  (func (export "kavka_alloc") (param $len i32) (result i32)
    (local $at i32)
    (local.set $at (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $len)))
    (local.get $at))

  (func (export "kavka_free") (param i32 i32))

  (func (export "kavka_decode") (param $ptr i32) (param $len i32) (result i32)
    (if (i32.eqz (local.get $len)) (then (return (i32.const 128))))
    (if (i32.eq (i32.load8_u (local.get $ptr)) (i32.const 66))
      (then (return (i32.const 64))))
    (i32.const 128))
)
"#;

    /// The same module with the four exports and a different version number.
    const MODULE_WRONG_ABI: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 2))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 0))
)
"#;

    fn plugin(wat: &str) -> WasmSerde {
        WasmSerde::from_bytes("acme", wat.as_bytes()).expect("the fixture loads")
    }

    fn refusal(wat: &str) -> WasmSerdeError {
        WasmSerde::from_bytes("acme", wat.as_bytes()).expect_err("the fixture is refused")
    }

    // -----------------------------------------------------------------------
    // The happy path
    // -----------------------------------------------------------------------

    #[test]
    fn a_plugin_decodes_a_record_it_claims() {
        let plugin = plugin(MODULE_V1);
        let json = plugin
            .decode_bytes(b"Banana")
            .expect("no error")
            .expect("decoded");
        assert_eq!(json, serde_json::json!({"decoded": "by-plugin"}));
        assert_eq!(plugin.error(), None);
        assert_eq!(CustomDecoder::name(&plugin), "acme");
    }

    /// The decline path: the plugin looked at the bytes and said they are not
    /// its format. That is not an error, and it must not be recorded as one.
    #[test]
    fn a_plugin_declines_bytes_that_are_not_its_format() {
        let plugin = plugin(MODULE_V1);
        assert_eq!(plugin.decode_bytes(b"Apple").expect("no error"), None);
        assert_eq!(plugin.decode_bytes(b"").expect("no error"), None);
        assert_eq!(plugin.error(), None, "a decline is not a failure");
    }

    /// The pool: many records through one plugin, and the answer never drifts.
    /// This is also what would catch a leak of the input buffer across records.
    #[test]
    fn many_records_reuse_the_warm_instance() {
        let plugin = plugin(MODULE_V1);
        for i in 0..2_000 {
            let bytes = format!("B{i}");
            assert!(
                plugin.decode_bytes(bytes.as_bytes()).unwrap().is_some(),
                "record {i}"
            );
        }
        assert_eq!(plugin.error(), None);
    }

    /// A sandbox whose `kavka_free` TRAPS must not go back into the warm pool.
    ///
    /// The trap happens after the answer has been read out of the module's
    /// memory, so the record still decodes — that is the easy half, and it is
    /// why the failure is silent. The half that matters is what happens next:
    /// the instance is in whatever state the trap left it in, and the pool is
    /// precisely the place another record would meet it.
    #[test]
    fn a_sandbox_whose_free_traps_never_returns_to_the_pool() {
        let trapping = plugin(&MODULE_V1.replace(
            r#"(func (export "kavka_free") (param i32 i32))"#,
            r#"(func (export "kavka_free") (param i32 i32) (unreachable))"#,
        ));
        // Loading instantiates once and parks that instance.
        assert_eq!(trapping.sandboxes.lock().unwrap().len(), 1);

        for i in 0..5 {
            assert!(
                trapping.decode_bytes(b"Banana").unwrap().is_some(),
                "record {i} still decodes: the free trapped after the answer was read"
            );
            assert!(
                trapping.sandboxes.lock().unwrap().is_empty(),
                "record {i} put a trapped sandbox back in the pool"
            );
        }

        // …and the correct fixture DOES keep its instance warm, so the above is
        // a difference rather than a pool that never fills.
        let healthy = plugin(MODULE_V1);
        assert!(healthy.decode_bytes(b"Banana").unwrap().is_some());
        assert_eq!(healthy.sandboxes.lock().unwrap().len(), 1);
    }

    /// A PLUGIN THAT EXPANDS is bounded by the display cap like every other
    /// payload — the size guard in `decode_with` bounds the bytes going IN, and
    /// nothing but this bounds what a plugin hands back.
    ///
    /// Through the real engine rather than a stand-in, because the number that
    /// matters is the length of the JSON a *module* wrote into its own memory.
    #[test]
    fn an_expanding_plugins_answer_is_cut_to_the_display_cap() {
        let json = format!(r#"{{"blob":"{}"}}"#, "x".repeat(8_192));
        let mut block = vec![0u8]; // status 0 — decoded
        block.extend_from_slice(&(json.len() as u32).to_le_bytes());
        block.extend_from_slice(json.as_bytes());
        // Every byte as a hex escape: the JSON is full of quotes, and a wat
        // data string that has to be hand-escaped is a fixture that breaks the
        // day somebody edits it.
        let data: String = block.iter().map(|byte| format!("\\{byte:02x}")).collect();
        let plugin = plugin(&format!(
            r#"
(module
  (memory (export "memory") 2)
  (data (i32.const 64) "{data}")
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 32768))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 64))
)
"#
        ));

        let decoded = crate::serdes::decode_with(b"x", None, 1_024, Some(&plugin));
        assert_eq!(decoded.decoded_by.as_deref(), Some("acme"));
        assert!(decoded.truncated, "the cut has to be reported");
        assert_eq!(decoded.text.len(), 1_024);
        assert!(decoded.json.is_none(), "the tree goes with the text");
        assert_eq!(decoded.raw_len, 1, "raw_len is the length on the wire");

        // Under the cap the same plugin's answer is whole and unmarked.
        let whole = crate::serdes::decode_with(b"x", None, 1 << 20, Some(&plugin));
        assert!(!whole.truncated);
        assert_eq!(
            whole.json.expect("a tree")["blob"].as_str().map(str::len),
            Some(8_192)
        );
    }

    /// The plugin is shared across threads by the search engine's workers.
    #[test]
    fn a_plugin_decodes_from_several_threads_at_once() {
        let plugin = Arc::new(plugin(MODULE_V1));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let plugin = Arc::clone(&plugin);
                std::thread::spawn(move || {
                    for _ in 0..200 {
                        assert!(plugin.decode_bytes(b"Bee").unwrap().is_some());
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("no panic");
        }
    }

    // -----------------------------------------------------------------------
    // Load-time refusals, each named
    // -----------------------------------------------------------------------

    #[test]
    fn a_module_that_is_not_wasm_is_refused() {
        let error = WasmSerde::from_bytes("acme", b"\x00not a module").expect_err("refused");
        assert!(matches!(error, WasmSerdeError::Compile { .. }), "{error:?}");
        assert!(error.to_string().contains("isn't a WebAssembly module"));
    }

    #[test]
    fn a_missing_file_names_the_path() {
        let missing = std::env::temp_dir().join("kavka-no-such-plugin.wasm");
        let error = WasmSerde::load("acme", &missing).expect_err("refused");
        assert!(matches!(error, WasmSerdeError::Read { .. }), "{error:?}");
        assert!(error.to_string().contains("plugin.wasm"), "{error}");
    }

    // -----------------------------------------------------------------------
    // Where a plugin may come from
    // -----------------------------------------------------------------------

    /// The two paths a profile must not carry, each with the sentence that
    /// explains why it is a security answer rather than a fussy one.
    ///
    /// Asserted on the pure function, because that is what the shell calls at
    /// save time — before anything is written — and what `load` calls again.
    #[test]
    fn a_relative_or_network_plugin_path_is_refused_with_the_reason() {
        for relative in [
            "decoder.wasm",
            "./decoder.wasm",
            "../plugins/decoder.wasm",
            "plugins/decoder.wasm",
        ] {
            let error = validate_plugin_path(relative).expect_err(relative);
            assert!(
                matches!(&error, WasmSerdeError::Path { .. }),
                "{relative} -> {error:?}"
            );
            // The reason is the one that survives an export, not "it is not
            // absolute".
            assert!(error.to_string().contains("travels"), "{error}");
            assert!(error.to_string().contains(relative), "{error}");
        }

        for unc in [
            r"\\build-server\share\decoder.wasm",
            r"//build-server/share/decoder.wasm",
            r"\\?\UNC\build-server\share\decoder.wasm",
            r"\\?\unc\build-server\share\decoder.wasm",
        ] {
            let error = validate_plugin_path(unc).expect_err(unc);
            assert!(
                error.to_string().contains("network path"),
                "{unc} -> {error}"
            );
            assert!(
                error.to_string().contains("code Kavka runs"),
                "{unc} -> {error}"
            );
        }

        let empty = validate_plugin_path("   ").expect_err("empty");
        assert!(empty.to_string().contains(".wasm"), "{empty}");
    }

    /// …and the paths that are fine. `\\?\C:\…` is the verbatim spelling of a
    /// LOCAL path and must not be caught by the UNC rule.
    #[test]
    fn an_absolute_local_path_is_accepted() {
        let here = std::env::temp_dir().join("acme.wasm");
        validate_plugin_path(&here.display().to_string()).expect("a temp path is absolute");
        #[cfg(windows)]
        {
            validate_plugin_path(r"C:\plugins\decoder.wasm").expect("a drive path");
            validate_plugin_path(r"\\?\C:\plugins\decoder.wasm").expect("verbatim, and local");
        }
        #[cfg(not(windows))]
        validate_plugin_path("/opt/kavka/decoder.wasm").expect("a unix path");
    }

    /// The check is in front of the LOADER too, not only in front of the
    /// editor: `profiles.json` is a file a person can edit and an import can
    /// bring in from elsewhere.
    #[test]
    fn the_loader_refuses_the_same_paths_the_editor_does() {
        let error = WasmSerde::load("acme", Path::new("decoder.wasm")).expect_err("refused");
        assert!(matches!(error, WasmSerdeError::Path { .. }), "{error:?}");
        let error =
            WasmSerde::load("acme", Path::new(r"\\host\share\decoder.wasm")).expect_err("refused");
        assert!(matches!(error, WasmSerdeError::Path { .. }), "{error:?}");
    }

    /// A module that wants WASI is the single most likely mistake — it is what
    /// `cargo build --target wasm32-wasip1` produces — so the message names the
    /// import and the fix.
    #[test]
    fn a_module_that_imports_anything_is_refused_by_name() {
        let error = refusal(
            r#"
(module
  (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 0))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 0))
)
"#,
        );
        assert_eq!(
            error,
            WasmSerdeError::Import {
                module: "wasi_snapshot_preview1".to_string(),
                name: "fd_write".to_string(),
            }
        );
        assert!(
            error.to_string().contains("wasm32-unknown-unknown"),
            "{error}"
        );
    }

    #[test]
    fn every_missing_export_is_named() {
        // One module per missing export, so each message is checked on its own.
        let full = [
            (
                "kavka_abi_version",
                "(func (export \"kavka_abi_version\") (result i32) (i32.const 1))",
            ),
            (
                "kavka_alloc",
                "(func (export \"kavka_alloc\") (param i32) (result i32) (i32.const 1024))",
            ),
            (
                "kavka_free",
                "(func (export \"kavka_free\") (param i32 i32))",
            ),
            (
                "kavka_decode",
                "(func (export \"kavka_decode\") (param i32 i32) (result i32) (i32.const 0))",
            ),
        ];
        for (missing, _) in full {
            let body: String = full
                .iter()
                .filter(|(name, _)| *name != missing)
                .map(|(_, text)| *text)
                .collect::<Vec<_>>()
                .join("\n  ");
            let error = refusal(&format!(
                "(module (memory (export \"memory\") 1)\n  {body}\n)"
            ));
            assert_eq!(
                error,
                WasmSerdeError::MissingExport {
                    name: missing.to_string(),
                    cause: "the module does not export it".to_string(),
                },
                "dropping {missing}"
            );
            assert!(error.to_string().contains(missing), "{error}");
        }

        // …and the memory, which is an export too.
        let error = refusal(
            r#"
(module
  (memory 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 0))
)
"#,
        );
        assert!(
            matches!(&error, WasmSerdeError::MissingExport { name, .. } if name == "memory"),
            "{error:?}"
        );
    }

    /// An export that is there with the wrong shape is a different mistake from
    /// one that is absent, and gets a different sentence.
    #[test]
    fn an_export_with_the_wrong_signature_says_so() {
        let error = refusal(
            r#"
(module
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i64) (i64.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 0))
)
"#,
        );
        let WasmSerdeError::MissingExport { name, cause } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(name, "kavka_abi_version");
        assert_ne!(cause, "the module does not export it");
        assert!(error.to_string().contains("ABI v1"), "{error}");
    }

    #[test]
    fn a_plugin_speaking_another_abi_version_is_refused() {
        let error = refusal(MODULE_WRONG_ABI);
        assert_eq!(error, WasmSerdeError::AbiVersion { found: 2 });
        assert!(error.to_string().contains("v2"), "{error}");
        assert!(error.to_string().contains("v1"), "{error}");
    }

    /// The memory cap, at instantiation: 200 pages is 12.8 MB, over the 10 MB
    /// ceiling, so the module never runs at all.
    #[test]
    fn a_module_whose_initial_memory_is_over_the_cap_never_starts() {
        let error = refusal(
            r#"
(module
  (memory (export "memory") 200)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 0))
)
"#,
        );
        assert!(
            matches!(error, WasmSerdeError::Instantiate { .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains("wouldn't start"), "{error}");
    }

    // -----------------------------------------------------------------------
    // Run-time refusals
    // -----------------------------------------------------------------------

    #[test]
    fn a_trap_is_reported_as_a_crash_and_not_as_a_timeout() {
        let plugin = plugin(
            r#"
(module
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (unreachable))
)
"#,
        );
        let error = plugin.decode_bytes(b"x").expect_err("trapped");
        assert!(matches!(error, WasmSerdeError::Trap { .. }), "{error:?}");
        assert!(error.to_string().contains("crashed on a record"), "{error}");

        // The decoder interface swallows it into the session's one error, and
        // the record falls through to the built-in ladder.
        assert_eq!(CustomDecoder::decode(&plugin, b"x"), None);
        let noted = plugin.error().expect("recorded once");
        assert!(noted.contains("acme"), "{noted}");
        // …once, however many records hit it.
        assert_eq!(CustomDecoder::decode(&plugin, b"y"), None);
        assert_eq!(plugin.error().as_deref(), Some(noted.as_str()));
    }

    /// The budget, and the reason it exists: a plugin that loops forever must
    /// cost one record's worth of time, not the search.
    #[test]
    fn an_endless_loop_runs_out_of_fuel_rather_than_running_forever() {
        let plugin = plugin(
            r#"
(module
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32)
    (loop $forever (br $forever))
    (i32.const 0))
)
"#,
        );
        let error = plugin.decode_bytes(b"x").expect_err("out of fuel");
        assert_eq!(error, WasmSerdeError::FuelExhausted);
        assert!(error.to_string().contains("looping"), "{error}");

        // The budget is per record: a second record gets its own, and runs out
        // of it too rather than failing instantly on an empty tank.
        assert_eq!(
            plugin.decode_bytes(b"y").expect_err("out of fuel again"),
            WasmSerdeError::FuelExhausted
        );
    }

    /// The calibration behind [`FUEL_PER_RECORD`], as a runnable measurement
    /// rather than a number somebody typed into a comment once.
    ///
    /// Ignored by default: it is a timing, and a CI runner's wall clock is not
    /// this machine's — asserting on it would be a test that fails when the
    /// build agent is busy. Run it when the constant changes:
    ///
    /// ```text
    /// cargo test -p kavka-core --release --features wasm-serdes -- \
    ///     --ignored --nocapture the_fuel_budget_is_calibrated
    /// ```
    #[test]
    #[ignore = "a measurement, not an assertion"]
    fn the_fuel_budget_is_calibrated() {
        // The cheapest possible operators, and a mix that touches memory —
        // the two ends of what a budget denominated in *operations* can mean
        // in milliseconds.
        for (what, body) in [
            ("a branch loop", "(loop $l (br $l))"),
            (
                "a load/store loop",
                "(loop $l (i32.store (i32.const 8) (i32.add (i32.load (i32.const 8)) \
                 (i32.const 1))) (br $l))",
            ),
        ] {
            let plugin = plugin(&format!(
                r#"
(module
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) {body} (i32.const 0))
)
"#
            ));
            let started = std::time::Instant::now();
            assert_eq!(
                plugin.decode_bytes(b"x").expect_err("out of fuel"),
                WasmSerdeError::FuelExhausted
            );
            println!(
                "{FUEL_PER_RECORD} fuel of {what}: {} ms",
                started.elapsed().as_millis()
            );
        }
    }

    /// The memory cap at run time: `memory.grow` past the ceiling answers -1,
    /// the way a failed allocation does, rather than taking the host with it.
    #[test]
    fn a_memory_bomb_is_refused_by_the_cap_and_not_by_the_operating_system() {
        let plugin = plugin(
            r#"
(module
  (memory (export "memory") 1)
  ;; 64: status 2, len 6, "denied"
  (data (i32.const 64) "\02\06\00\00\00denied")
  ;; 128: status 0, len 2, {}
  (data (i32.const 128) "\00\02\00\00\00{}")
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32)
    (if (i32.eq (memory.grow (i32.const 10000)) (i32.const -1))
      (then (return (i32.const 64))))
    (i32.const 128))
)
"#,
        );
        // The plugin reports its own failure, which is exactly what a refused
        // allocation looks like from inside the sandbox.
        let error = plugin.decode_bytes(b"x").expect_err("the grow was refused");
        assert_eq!(
            error,
            WasmSerdeError::MalformedOutput {
                cause: "denied".to_string()
            }
        );
    }

    /// The three ways a result block can be wrong, each with its own sentence —
    /// and none of them reads a byte outside the module's memory.
    #[test]
    fn a_malformed_result_block_is_told_apart_from_a_trap() {
        let cases = [
            // A pointer past the end of a one-page memory.
            ("(i32.const 70000)", "header there"),
            // A length that runs off the end from a valid header.
            ("(i32.const 256)", "runs off the end"),
            // A status ABI v1 does not define.
            ("(i32.const 320)", "is not one ABI v1 defines"),
            // Bytes that are not UTF-8.
            ("(i32.const 384)", "not UTF-8"),
            // Text that is not JSON.
            ("(i32.const 448)", "is not JSON"),
        ];
        for (answer, expected) in cases {
            let plugin = plugin(&format!(
                r#"
(module
  (memory (export "memory") 1)
  ;; 256: status 0 with a length far past the end of one page
  (data (i32.const 256) "\00\ff\ff\ff\7f")
  ;; 320: status 9
  (data (i32.const 320) "\09\00\00\00\00")
  ;; 384: status 0, len 2, two bytes that are not UTF-8
  (data (i32.const 384) "\00\02\00\00\00\ff\fe")
  ;; 448: status 0, len 5, "hello" — valid UTF-8, not JSON
  (data (i32.const 448) "\00\05\00\00\00hello")
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) {answer})
)
"#
            ));
            let error = plugin.decode_bytes(b"x").expect_err(answer);
            let WasmSerdeError::MalformedOutput { cause } = &error else {
                panic!("{answer} -> {error:?}");
            };
            assert!(cause.contains(expected), "{answer} -> {cause}");
        }
    }

    /// A plugin whose allocator cannot serve the record says so, rather than
    /// having the host write over whatever is at address zero.
    #[test]
    fn an_allocator_that_answers_zero_is_an_out_of_memory() {
        let plugin = plugin(
            r#"
(module
  (memory (export "memory") 1)
  (func (export "kavka_abi_version") (result i32) (i32.const 1))
  (func (export "kavka_alloc") (param i32) (result i32) (i32.const 0))
  (func (export "kavka_free") (param i32 i32))
  (func (export "kavka_decode") (param i32 i32) (result i32) (i32.const 0))
)
"#,
        );
        let error = plugin.decode_bytes(b"x").expect_err("out of memory");
        assert_eq!(error, WasmSerdeError::OutOfMemory { wanted: 1 });
    }

    // -----------------------------------------------------------------------
    // A profile's set of plugins
    // -----------------------------------------------------------------------

    #[test]
    fn a_missing_plugin_file_is_a_sentence_and_not_a_dead_session() {
        let missing = std::env::temp_dir().join("kavka-no-such-plugin.wasm");
        let (plugins, problems) = WasmSerdes::load(&[WasmSerdeConfig {
            name: "acme".to_string(),
            path: missing.display().to_string(),
            applies_to_topics: vec!["*".to_string()],
        }]);
        assert!(plugins.is_empty());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("\"acme\""), "{problems:?}");
        assert!(
            problems[0].contains("can't read the plugin"),
            "{problems:?}"
        );

        // A path the editor would have refused is refused here too, and it is
        // still one sentence rather than a dead session.
        let (plugins, problems) = WasmSerdes::load(&[WasmSerdeConfig {
            name: "acme".to_string(),
            path: "no/such/plugin.wasm".to_string(),
            applies_to_topics: vec!["*".to_string()],
        }]);
        assert!(plugins.is_empty());
        assert!(
            problems[0].contains("won't load a serde plugin"),
            "{problems:?}"
        );
        // …and with nothing configured there is nothing to report.
        let (none, quiet) = WasmSerdes::load(&[]);
        assert!(none.is_empty() && quiet.is_empty());
        assert_eq!(none.for_topic("orders.v2").map(|_| ()), None);
    }

    /// The committed example (`docs/examples/wasm-serde/`) against the engine
    /// that will run it, so the README, the ABI documentation and this module
    /// cannot drift apart without somebody noticing.
    ///
    /// **Skipped unless the example has been built**, because building it needs
    /// the `wasm32-unknown-unknown` target and no committed binary is going to
    /// appear in this repository:
    ///
    /// ```text
    /// rustup target add wasm32-unknown-unknown
    /// cd docs/examples/wasm-serde && cargo build --release --target wasm32-unknown-unknown
    /// ```
    #[test]
    fn the_committed_example_plugin_speaks_this_abi() {
        let module = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/examples/wasm-serde/target/wasm32-unknown-unknown/release")
            .join("kavka_acme_serde.wasm");
        if !module.exists() {
            eprintln!(
                "skipping: build docs/examples/wasm-serde first (see its README) — looked at {}",
                module.display()
            );
            return;
        }
        let plugin = WasmSerde::load("acme-orders", &module).expect("the example loads");

        // `ACME1` + order id 8412 (big-endian) + status 2 + the customer name.
        let mut record = b"ACME1".to_vec();
        record.extend_from_slice(&8412u32.to_be_bytes());
        record.push(2);
        record.extend_from_slice(b"Ada Lovelace");

        assert_eq!(
            plugin.decode_bytes(&record).expect("no error"),
            Some(serde_json::json!({
                "orderId": 8412,
                "status": "shipped",
                "customer": "Ada Lovelace",
            }))
        );
        // …and it declines what is not its format, so the ladder still runs.
        assert_eq!(plugin.decode_bytes(br#"{"orderId":1}"#).unwrap(), None);
        assert_eq!(plugin.error(), None);
    }

    /// The whole pipeline through the real entry point: a plugin file on disk,
    /// a profile that points at it, and a record that comes back decoded.
    #[test]
    fn a_profile_routes_a_topic_to_its_plugin() {
        let dir = std::env::temp_dir().join(format!("kavka-wasm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("acme.wat");
        std::fs::write(&path, MODULE_V1).unwrap();

        let (plugins, problems) = WasmSerdes::load(&[
            WasmSerdeConfig {
                name: "acme".to_string(),
                path: path.display().to_string(),
                applies_to_topics: vec!["acme.*".to_string()],
            },
            WasmSerdeConfig {
                name: "catch-all".to_string(),
                path: path.display().to_string(),
                applies_to_topics: vec!["*".to_string()],
            },
        ]);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(plugins.len(), 2);

        // First match wins, in the order the profile lists them.
        assert_eq!(plugins.for_topic("acme.orders").unwrap().name(), "acme");
        assert_eq!(plugins.for_topic("orders.v2").unwrap().name(), "catch-all");
        assert!(plugins.take_errors().is_empty());

        let json = plugins
            .for_topic("acme.orders")
            .unwrap()
            .decode_bytes(b"Bytes")
            .unwrap()
            .unwrap();
        assert_eq!(json["decoded"], "by-plugin");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
