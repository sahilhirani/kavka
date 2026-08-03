//! A complete Kavka serde plugin, ABI v1.
//!
//! It decodes a made-up in-house wire format called **ACME1** — the shape a
//! real one tends to have, and the shape Kavka's built-in ladder can never
//! guess:
//!
//! ```text
//! offset 0   [u8; 5]  the ASCII magic "ACME1"
//! offset 5   u32 BE   order id
//! offset 9   u8       status: 1 created, 2 shipped, 3 cancelled
//! offset 10  [u8]     the customer name, UTF-8, to the end of the record
//! ```
//!
//! and answers, for example:
//!
//! ```json
//! {"orderId":8412,"status":"shipped","customer":"Ada Lovelace"}
//! ```
//!
//! Anything that is not ACME1 is **declined**, and Kavka's built-in ladder
//! reads it instead. Declining is the important half: a topic that carries
//! ACME1 records *and* JSON records works correctly with this plugin
//! registered, because the plugin only claims what it recognises.
//!
//! # The contract
//!
//! The normative specification is the module documentation of
//! `crates/kavka-core/src/wasm_serde.rs`. In short: export `memory`,
//! `kavka_abi_version`, `kavka_alloc`, `kavka_free` and `kavka_decode`, import
//! nothing at all, and answer with a result block —
//!
//! ```text
//! offset 0   u8    status: 0 decoded, 1 declined, 2 failed
//! offset 1   u32   json_len, little-endian
//! offset 5   [u8]  json_len bytes of UTF-8
//! ```
//!
//! # Who frees what
//!
//! The **host** owns the buffer `kavka_alloc` returned: it writes the record
//! there, calls `kavka_decode`, and frees it afterwards. This plugin therefore
//! borrows the input and never takes ownership of it. The **result block** is
//! this plugin's, and the host hands it back to `kavka_free` when it has read
//! it — which is why `result` leaks the `Vec` deliberately (`forget`) rather
//! than dropping it before the host has looked at it.
//!
//! # No `unsafe` beyond the boundary itself
//!
//! Three `unsafe` blocks, all of them the FFI boundary: turning the host's
//! (pointer, length) into a slice, and the two halves of the allocator. The
//! decoder itself is safe code over a `&[u8]`.

use core::fmt::Write as _;

/// Must match `kavka_core::wasm_serde::ABI_VERSION`.
const ABI_VERSION: i32 = 1;

/// Result-block statuses, from the ABI.
const STATUS_DECODED: u8 = 0;
const STATUS_DECLINED: u8 = 1;
#[allow(dead_code)]
const STATUS_FAILED: u8 = 2;

const MAGIC: &[u8; 5] = b"ACME1";
/// Magic + u32 order id + u8 status.
const HEADER_LEN: usize = 10;

// ---------------------------------------------------------------------------
// The ABI exports.
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn kavka_abi_version() -> i32 {
    ABI_VERSION
}

/// Reserves `len` bytes and answers their address, or `0` when it cannot.
///
/// Byte alignment (`align = 1`) because the host writes raw record bytes here
/// and `kavka_free` has to be able to rebuild the same layout from nothing but
/// the (pointer, length) pair the ABI carries.
#[no_mangle]
pub extern "C" fn kavka_alloc(len: i32) -> i32 {
    let Ok(len) = usize::try_from(len) else {
        return 0;
    };
    if len == 0 {
        // Rust's allocator does not do zero-sized allocations, and the host
        // writes nothing to a zero-length buffer — any non-null address is a
        // correct answer, and `1` is the conventional one.
        return 1;
    }
    let Ok(layout) = core::alloc::Layout::from_size_align(len, 1) else {
        return 0;
    };
    // SAFETY: `layout` has a non-zero size.
    let pointer = unsafe { std::alloc::alloc(layout) };
    pointer as i32
}

/// Releases a (pointer, length) pair this module handed out.
#[no_mangle]
pub extern "C" fn kavka_free(pointer: i32, len: i32) {
    let (Ok(len), Ok(address)) = (usize::try_from(len), usize::try_from(pointer)) else {
        return;
    };
    if len == 0 || address == 0 {
        return;
    }
    let Ok(layout) = core::alloc::Layout::from_size_align(len, 1) else {
        return;
    };
    // SAFETY: the ABI guarantees the host only frees pairs `kavka_alloc` or
    // `result` produced, and both use this same layout.
    unsafe { std::alloc::dealloc(address as *mut u8, layout) }
}

/// Decodes one record, answering the address of a result block.
#[no_mangle]
pub extern "C" fn kavka_decode(pointer: i32, len: i32) -> i32 {
    let (Ok(address), Ok(len)) = (usize::try_from(pointer), usize::try_from(len)) else {
        return result(STATUS_DECLINED, "");
    };
    if address == 0 || len == 0 {
        return result(STATUS_DECLINED, "");
    }
    // SAFETY: the host allocated `len` bytes at `address` through `kavka_alloc`
    // and wrote the record into them before calling. The slice is only read,
    // and only for the duration of this call — the host owns the buffer.
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, len) };

    match decode(bytes) {
        Some(json) => result(STATUS_DECODED, &json),
        // Not ours. Kavka's built-in ladder reads it instead.
        None => result(STATUS_DECLINED, ""),
    }
}

/// The result block's bytes. Pure, so the ABI layout is testable on the host
/// without a wasm runtime — see the tests at the bottom of this file.
fn block(status: u8, json: &str) -> Vec<u8> {
    let mut block = Vec::with_capacity(5 + json.len());
    block.push(status);
    block.extend_from_slice(&(json.len() as u32).to_le_bytes());
    block.extend_from_slice(json.as_bytes());
    block
}

/// Puts a result block in linear memory and answers its address.
///
/// The `Vec` is deliberately leaked: the block belongs to the host until it
/// calls `kavka_free(address, 5 + json.len())`.
///
/// The `as i32` cast is correct on `wasm32-unknown-unknown`, where a pointer
/// *is* 32 bits and *is* an offset into linear memory. This crate also compiles
/// for the host so that `cargo test` can run the decoder, and on a 64-bit host
/// the cast truncates — which is why nothing in the test module below calls
/// this function or the two allocator exports.
fn result(status: u8, json: &str) -> i32 {
    let block = block(status, json);
    let address = block.as_ptr() as i32;
    core::mem::forget(block);
    address
}

// ---------------------------------------------------------------------------
// The decoder. Safe code over a slice — this is the part you replace.
// ---------------------------------------------------------------------------

/// `Some(json)` for an ACME1 record; `None` for anything else.
fn decode(bytes: &[u8]) -> Option<String> {
    if bytes.len() < HEADER_LEN || !bytes.starts_with(MAGIC) {
        return None;
    }
    let order_id = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
    let status = match bytes[9] {
        1 => "created",
        2 => "shipped",
        3 => "cancelled",
        // An unknown code is data, not a failure: the honest answer names it
        // rather than refusing the record or inventing a status.
        other => return Some(json(order_id, &format!("unknown({other})"), &bytes[HEADER_LEN..])),
    };
    Some(json(order_id, status, &bytes[HEADER_LEN..]))
}

fn json(order_id: u32, status: &str, customer: &[u8]) -> String {
    let customer = String::from_utf8_lossy(customer);
    let mut out = String::with_capacity(64 + customer.len());
    let _ = write!(out, r#"{{"orderId":{order_id},"status":""#);
    escape(status, &mut out);
    let _ = out.write_str(r#"","customer":""#);
    escape(&customer, &mut out);
    let _ = out.write_str(r#""}"#);
    out
}

/// JSON string escaping (RFC 8259 §7). Kavka parses what this module returns,
/// so a record whose customer name contains a quote has to come back as valid
/// JSON rather than as a parse error blamed on the plugin.
fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(order_id: u32, status: u8, customer: &str) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&order_id.to_be_bytes());
        bytes.push(status);
        bytes.extend_from_slice(customer.as_bytes());
        bytes
    }

    #[test]
    fn an_acme1_record_decodes() {
        assert_eq!(
            decode(&record(8412, 2, "Ada Lovelace")).as_deref(),
            Some(r#"{"orderId":8412,"status":"shipped","customer":"Ada Lovelace"}"#)
        );
    }

    #[test]
    fn an_unknown_status_code_is_named_rather_than_refused() {
        assert_eq!(
            decode(&record(1, 9, "x")).as_deref(),
            Some(r#"{"orderId":1,"status":"unknown(9)","customer":"x"}"#)
        );
    }

    #[test]
    fn anything_that_is_not_acme1_is_declined() {
        assert_eq!(decode(br#"{"orderId":1}"#), None);
        assert_eq!(decode(b""), None);
        assert_eq!(decode(b"ACME1"), None, "a header with no body is not one");
        assert_eq!(decode(b"ACME2\x00\x00\x00\x01\x01x"), None);
    }

    #[test]
    fn a_quote_in_a_name_comes_back_as_valid_json() {
        let decoded = decode(&record(1, 1, "Ada \"Countess\" Lovelace")).unwrap();
        assert!(decoded.contains(r#"\"Countess\""#), "{decoded}");
        // …and it parses, which is the whole point.
        assert!(decoded.ends_with(r#""}"#));
    }

    #[test]
    fn the_result_block_has_the_abi_layout() {
        let decoded = block(STATUS_DECODED, "{}");
        assert_eq!(decoded.len(), 7);
        assert_eq!(decoded[0], STATUS_DECODED);
        assert_eq!(
            u32::from_le_bytes([decoded[1], decoded[2], decoded[3], decoded[4]]),
            2
        );
        assert_eq!(&decoded[5..], b"{}");

        // A decline is a header and nothing else.
        let declined = block(STATUS_DECLINED, "");
        assert_eq!(declined, vec![STATUS_DECLINED, 0, 0, 0, 0]);

        // The length is little-endian, which is the half a big-endian brain
        // gets wrong: 258 is `02 01 00 00`, not `00 00 01 02`.
        let long = block(STATUS_FAILED, &"x".repeat(258));
        assert_eq!(&long[1..5], &[0x02, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn the_abi_version_is_the_one_kavka_speaks() {
        assert_eq!(kavka_abi_version(), 1);
    }
}
