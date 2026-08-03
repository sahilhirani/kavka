# Example: a Kavka WASM serde plugin

A complete, working custom decoder for Kavka, in about 120 lines of Rust with
**no dependencies**. It decodes a made-up in-house wire format called `ACME1`
and declines everything else.

Source is committed here; **the compiled `.wasm` is not**. A binary in a git
repository is a binary nobody reviews, and the whole point of the sandbox is
that you can read what you are about to run. Build it yourself — it takes a
couple of seconds.

---

## What a plugin is for

Kavka's built-in decode ladder (`crates/kavka-core/src/serdes.rs`) reads JSON,
Avro via Schema Registry, MessagePack, CBOR, UTF-8 and hex. That covers most of
the world and none of *your* company's twelve-year-old binary format. A plugin
is how you read that format in Kavka without Kavka shipping it:

- it runs **before** the built-in ladder, so an in-house format whose bytes
  happen to look like MessagePack is not misread;
- it can **decline** a record, which falls straight through to the ladder — so a
  topic carrying both your format and plain JSON works with the plugin on;
- it runs in a WebAssembly sandbox with **no imports at all**: no filesystem, no
  network, no clock, no randomness. It gets bytes and returns bytes.

---

## Build it

```sh
rustup target add wasm32-unknown-unknown
cd docs/examples/wasm-serde
cargo build --release --target wasm32-unknown-unknown
```

The module is at:

```
target/wasm32-unknown-unknown/release/kavka_acme_serde.wasm
```

About 20 KB. Optionally squeeze it further with
[`wasm-opt`](https://github.com/WebAssembly/binaryen):

```sh
wasm-opt -Oz -o acme.wasm target/wasm32-unknown-unknown/release/kavka_acme_serde.wasm
```

> **Use `wasm32-unknown-unknown`, not `wasm32-wasip1`.** A WASI build imports
> host functions (`fd_write` and friends), and a Kavka plugin may import
> **nothing** — Kavka refuses it at load and names the import. If you see
> *"this plugin imports wasi_snapshot_preview1::…"*, this is why.

Run the decoder's own tests on your host machine — they are ordinary Rust tests
and do not need the wasm target:

```sh
cargo test
```

---

## Install it

In Kavka: **connection settings → Custom decoders (WASM)**, or by hand in
`profiles.json`:

```json
{
  "wasm_serdes": [
    {
      "name": "acme-orders",
      "path": "/opt/kavka/acme.wasm",
      "applies_to_topics": ["acme.*", "orders.v?"]
    }
  ]
}
```

- `name` — what you call it. It appears in the payload inspector's provenance
  line (*Decoded by acme-orders*) and in any error the plugin produces.
- `path` — absolute path to the `.wasm` file, **on this machine**. A `.wat` text
  file also loads, which is handy while you are developing. A relative path and
  a network (UNC) path are both refused, at save time and again at load: a
  plugin is code Kavka executes, so a path that resolves against whatever folder
  the app was launched from — or against somebody else's file server — is an
  execution source nobody chose. A profile export travels; the path in it has to
  mean the same thing where it lands or fail loudly.
- `applies_to_topics` — globs: `*` is any run of characters, `?` is exactly one.
  Matched against the whole topic name, case-sensitively. **An empty list claims
  nothing**, not everything. The first plugin in the list whose globs match wins.

---

## ABI v1

The normative specification is the module documentation of
`crates/kavka-core/src/wasm_serde.rs` — read that if the two ever disagree.
The summary:

### Exports

| export | signature | meaning |
|---|---|---|
| `memory` | `memory` | your linear memory, exported so Kavka can read and write it |
| `kavka_abi_version` | `() -> i32` | return `1` |
| `kavka_alloc` | `(i32) -> i32` | reserve N bytes, answer their address; `0` means "cannot" |
| `kavka_free` | `(i32, i32) -> ()` | release the address/length pair |
| `kavka_decode` | `(i32, i32) -> i32` | decode the record at address/length; answer a **result block** |

### Imports

None. Not one.

### The result block

A run of bytes in your memory, **little-endian**:

```text
offset 0   u8    status: 0 decoded, 1 declined, 2 failed
offset 1   u32   json_len
offset 5   [u8]  json_len bytes of UTF-8
```

- **0 — decoded.** The UTF-8 is the record as JSON. Kavka parses it; text that
  is not JSON is reported as a plugin bug.
- **1 — declined.** "These bytes are not mine." Kavka's built-in ladder reads
  the record instead. `json_len` is normally 0.
- **2 — failed.** The UTF-8 is why. The ladder still runs, and the message is
  shown once for the session rather than once per record.

### Who frees what

Kavka owns the **input** buffer: it calls `kavka_alloc`, writes the record,
calls `kavka_decode`, and calls `kavka_free` on that same pair afterwards. Do
not free it and do not take ownership of it.

Your plugin owns the **result block** until Kavka has read it; Kavka then calls
`kavka_free(block, 5 + json_len)`. That is why `result()` in `src/lib.rs` leaks
its `Vec` on purpose.

### The sandbox

| limit | value | what happens when you hit it |
|---|---|---|
| imports | none allowed | refused at load, with the import named |
| linear memory | 10 MB | `memory.grow` answers `-1`; a module whose *initial* memory is over the cap never starts |
| fuel | 100,000,000 WebAssembly operations per record | the record fails with *"the plugin used its whole budget"*, and the next record gets a fresh instance |

Fuel counts operations, not milliseconds — so the limit is the same on every
machine, and a plugin that works on your laptop cannot time out on someone
else's. On the development machine 100 M operations is 2–20 ms depending on
what they are; a decoder that reads one Kafka record spends thousands, not
millions.

---

## Writing your own

1. Copy this directory.
2. Replace `decode()` — it is safe code over a `&[u8]`, returning
   `Option<String>` where `None` means "decline".
3. Leave the four exported functions alone; they are the boundary.
4. Return **canonical JSON**. Kavka parses it into the same tree the built-in
   decoders produce, so everything downstream — the payload inspector, CEL
   filters (`value.orderId > 100`), SQL over topics, search, export — works on
   your format exactly as it does on JSON.

### Other languages

Anything that compiles to WebAssembly with no imports will do. In C, export the
five names with `__attribute__((export_name(...)))` and compile with
`-Wl,--no-entry -nostdlib`. In TinyGo, use `//export` and build with
`-target=wasm-unknown` (not `wasi`). The ABI is four functions and a five-byte
header — there is deliberately nothing language-specific in it.
