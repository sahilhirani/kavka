//! Kafka wire primitives — the encoding layer every message in this module is
//! built from.
//!
//! Kafka describes each field as one of a small set of primitives, and since
//! KIP-482 every message *version* is either **legacy** (int16-prefixed
//! strings, int32-prefixed arrays, `-1` for null, no trailing tag buffer) or
//! **flexible** (varint "compact" lengths carrying `n + 1` so that `0` can mean
//! null, plus a tagged-field buffer at the end of every struct and of the
//! message itself).
//!
//! The two encodings live side by side inside one connection — the request
//! header is legacy for a non-flexible API and flexible for a flexible one —
//! so the methods here are named for the ENCODING rather than for the logical
//! type, and each message states which one it is writing. That is deliberately
//! more verbose than a `flexible: bool` switch on the encoder: the golden-byte
//! tests assert exact bytes, and a hidden switch is exactly the sort of thing
//! that silently flips a whole message to the wrong encoding while every unit
//! test still passes because it flipped the decoder too.

use crate::{Error, Result};

/// Every decode failure in this module. Phrased for a user rather than for a
/// protocol author: the actionable half is "the broker answered something this
/// build doesn't understand", not the byte offset.
pub(crate) fn malformed(detail: impl std::fmt::Display) -> Error {
    Error::Other(format!(
        "could not decode the broker's reply ({detail}) — this build may be \
         older than the cluster it is talking to"
    ))
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// A growable request body. Field methods chain; [`finish`](Encoder::finish)
/// hands back the bytes.
#[derive(Default)]
pub(crate) struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.buf
    }

    pub(crate) fn bool(&mut self, value: bool) -> &mut Self {
        self.buf.push(u8::from(value));
        self
    }

    pub(crate) fn int8(&mut self, value: i8) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub(crate) fn int16(&mut self, value: i16) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub(crate) fn int32(&mut self, value: i32) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    /// Test-only: no REQUEST this module builds carries an `int64`. It exists
    /// so the canned responses the decode tests are driven with can be written
    /// with the encoder rather than as hand-laid byte arrays.
    #[cfg(test)]
    pub(crate) fn int64(&mut self, value: i64) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub(crate) fn float64(&mut self, value: f64) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    /// KIP-482's unsigned varint: seven payload bits per byte, least-significant
    /// group first, high bit set on every byte but the last.
    pub(crate) fn uvarint(&mut self, mut value: u32) -> &mut Self {
        while value >= 0x80 {
            self.buf.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        self.buf.push(value as u8);
        self
    }

    /// Legacy nullable string: `int16` length (`-1` = null) then UTF-8 bytes.
    /// The request header's `client_id` is one of these even in header v2 —
    /// the header schema marks that single field `"flexibleVersions": "none"`,
    /// which is the most commonly mis-encoded byte in a hand-rolled client.
    ///
    /// The one fallible encoder here, and deliberately so: a string longer than
    /// `i16::MAX` has no legacy encoding at all, and clamping the length (which
    /// this used to do) writes a frame whose declared length disagrees with its
    /// payload — the broker then reads the overflow as the NEXT field, which is
    /// the worst possible failure mode for a client. **A request that cannot be
    /// encoded must not be sent**, so this refuses instead.
    pub(crate) fn legacy_nullable_string(&mut self, value: Option<&str>) -> Result<&mut Self> {
        match value {
            None => Ok(self.int16(-1)),
            Some(text) => {
                let len = i16::try_from(text.len()).map_err(|_| {
                    Error::Other(format!(
                        "a {}-byte string does not fit in a Kafka legacy string field, which \
                         carries an int16 length (max {})",
                        text.len(),
                        i16::MAX
                    ))
                })?;
                self.int16(len);
                self.buf.extend_from_slice(text.as_bytes());
                Ok(self)
            }
        }
    }

    /// Legacy non-nullable string.
    pub(crate) fn legacy_string(&mut self, value: &str) -> Result<&mut Self> {
        self.legacy_nullable_string(Some(value))
    }

    /// Compact non-nullable string: `uvarint(len + 1)` then UTF-8 bytes.
    pub(crate) fn compact_string(&mut self, value: &str) -> &mut Self {
        self.uvarint(value.len() as u32 + 1);
        self.buf.extend_from_slice(value.as_bytes());
        self
    }

    /// Compact nullable string: `0` for null, otherwise `uvarint(len + 1)`.
    pub(crate) fn compact_nullable_string(&mut self, value: Option<&str>) -> &mut Self {
        match value {
            None => self.uvarint(0),
            Some(text) => self.compact_string(text),
        }
    }

    /// Compact bytes: `uvarint(len + 1)` then the bytes.
    pub(crate) fn compact_bytes(&mut self, value: &[u8]) -> &mut Self {
        self.uvarint(value.len() as u32 + 1);
        self.buf.extend_from_slice(value);
        self
    }

    /// The length prefix of a compact array — `0` for null, `n + 1` otherwise.
    /// The elements follow, written by the caller.
    pub(crate) fn compact_array_len(&mut self, len: Option<usize>) -> &mut Self {
        match len {
            None => self.uvarint(0),
            Some(n) => self.uvarint(n as u32 + 1),
        }
    }

    /// A whole compact array of `int32`, or null.
    ///
    /// Null is load-bearing rather than decorative: in
    /// AlterPartitionReassignments a null replica list is how a reassignment is
    /// CANCELLED, so encoding it as an empty array would silently ask for a
    /// partition with no replicas instead.
    pub(crate) fn compact_int32_array(&mut self, values: Option<&[i32]>) -> &mut Self {
        match values {
            None => self.compact_array_len(None),
            Some(items) => {
                self.compact_array_len(Some(items.len()));
                for value in items {
                    self.int32(*value);
                }
                self
            }
        }
    }

    /// The all-zero UUID — Kafka's `Uuid.ZERO_UUID`, which in a request means
    /// "I am identifying this by name, not by id".
    pub(crate) fn zero_uuid(&mut self) -> &mut Self {
        self.buf.extend_from_slice(&[0u8; 16]);
        self
    }

    /// The empty tagged-field buffer that terminates every struct of a flexible
    /// message. Kavka sends no tagged fields, so this is always a single `0`.
    pub(crate) fn tagged_fields(&mut self) -> &mut Self {
        self.uvarint(0)
    }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

pub(crate) struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// How far into the buffer the cursor has reached — the response header's
    /// length, once the header has been read, so the body can be handed on as
    /// its own slice.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| malformed("a length field overflowed"))?;
        let slice = self.buf.get(self.pos..end).ok_or_else(|| {
            malformed(format!(
                "wanted {n} more byte(s) but only {} remain",
                self.remaining()
            ))
        })?;
        self.pos = end;
        Ok(slice)
    }

    pub(crate) fn bool(&mut self) -> Result<bool> {
        Ok(self.take(1)?[0] != 0)
    }

    /// Steps over a fixed-width field this build has no use for — a topic's
    /// UUID, say. Decoding it into a value nothing reads would only invite
    /// someone to start reading it.
    pub(crate) fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// Test-only: no RESPONSE this module decodes carries an `int8`. It exists
    /// so a request encoder's `int8` fields (a quota filter's match type, an
    /// election type) can be read back and asserted, rather than checked as a
    /// magic byte offset into a hex string.
    #[cfg(test)]
    pub(crate) fn int8(&mut self) -> Result<i8> {
        Ok(self.take(1)?[0] as i8)
    }

    pub(crate) fn int16(&mut self) -> Result<i16> {
        let bytes = self.take(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn int32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn int64(&mut self) -> Result<i64> {
        let bytes = self.take(8)?;
        let mut out = [0u8; 8];
        out.copy_from_slice(bytes);
        Ok(i64::from_be_bytes(out))
    }

    pub(crate) fn float64(&mut self) -> Result<f64> {
        let bytes = self.take(8)?;
        let mut out = [0u8; 8];
        out.copy_from_slice(bytes);
        Ok(f64::from_be_bytes(out))
    }

    /// Five bytes is the most a 32-bit unsigned varint can occupy; a sixth
    /// continuation bit means the stream is not what we think it is, and
    /// looping on it is how a decoder turns a bad frame into a hang.
    ///
    /// The fifth byte carries only FOUR payload bits, because 28 + 4 = 32: a
    /// fifth byte above `0x0f` encodes a value larger than any `u32`, and
    /// shifting it in would silently discard the overflow (a `u32` shift by 28
    /// is always in range, so there is nothing for `checked_shl` to catch). The
    /// stated 32-bit invariant only holds if that case is refused.
    pub(crate) fn uvarint(&mut self) -> Result<u32> {
        let mut value: u32 = 0;
        for shift in [0u32, 7, 14, 21, 28] {
            let byte = self.take(1)?[0];
            let payload = byte & 0x7f;
            if shift == 28 && payload > 0x0f {
                return Err(malformed("a varint did not fit in 32 bits"));
            }
            value |= u32::from(payload) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(malformed("a varint ran past five bytes"))
    }

    fn utf8(&mut self, len: usize) -> Result<String> {
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| malformed("a string was not valid UTF-8"))
    }

    pub(crate) fn legacy_string(&mut self) -> Result<String> {
        match self.legacy_nullable_string()? {
            Some(text) => Ok(text),
            None => Err(malformed("a non-nullable string arrived as null")),
        }
    }

    pub(crate) fn legacy_nullable_string(&mut self) -> Result<Option<String>> {
        let len = self.int16()?;
        if len < 0 {
            return Ok(None);
        }
        self.utf8(len as usize).map(Some)
    }

    pub(crate) fn compact_string(&mut self) -> Result<String> {
        match self.compact_nullable_string()? {
            Some(text) => Ok(text),
            None => Err(malformed("a non-nullable string arrived as null")),
        }
    }

    pub(crate) fn compact_nullable_string(&mut self) -> Result<Option<String>> {
        let raw = self.uvarint()?;
        if raw == 0 {
            return Ok(None);
        }
        let len = self.bounded(raw as usize - 1, "string")?;
        self.utf8(len).map(Some)
    }

    pub(crate) fn compact_bytes(&mut self) -> Result<Vec<u8>> {
        let raw = self.uvarint()?;
        if raw == 0 {
            return Ok(Vec::new());
        }
        let len = self.bounded(raw as usize - 1, "byte string")?;
        Ok(self.take(len)?.to_vec())
    }

    /// Legacy array length; `None` for a null array.
    pub(crate) fn legacy_array_len(&mut self) -> Result<Option<usize>> {
        let len = self.int32()?;
        if len < 0 {
            return Ok(None);
        }
        self.bounded(len as usize, "array").map(Some)
    }

    /// Compact array length; `None` for a null array.
    pub(crate) fn compact_array_len(&mut self) -> Result<Option<usize>> {
        let raw = self.uvarint()?;
        if raw == 0 {
            return Ok(None);
        }
        self.bounded(raw as usize - 1, "array").map(Some)
    }

    pub(crate) fn compact_int32_array(&mut self) -> Result<Vec<i32>> {
        let len = self.compact_array_len()?.unwrap_or(0);
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(self.int32()?);
        }
        Ok(out)
    }

    /// Reads (and discards) a struct's tagged-field buffer.
    ///
    /// Unknown tags are SKIPPED rather than rejected: that is the entire point
    /// of KIP-482 tagged fields, and a client that errors on a tag it has never
    /// heard of breaks the moment the cluster is upgraded.
    pub(crate) fn tagged_fields(&mut self) -> Result<()> {
        self.tagged_fields_visit(|_tag, _payload| {})
    }

    /// The same walk, with each tag's payload handed to `visit`.
    ///
    /// One implementation rather than two, deliberately: the skipping form is
    /// this one with an empty visitor, so a buffer that walks correctly for the
    /// caller that reads a tag walks identically for the ninety-nine that do
    /// not. The payload arrives as bytes because a tagged field's contents are
    /// a whole encoded value whose type only its reader knows — see
    /// [`super::conn`], whose only tag is ApiVersions' finalized-feature list.
    pub(crate) fn tagged_fields_visit(
        &mut self,
        mut visit: impl FnMut(u32, &'a [u8]),
    ) -> Result<()> {
        let count = self.uvarint()?;
        for _ in 0..count {
            let tag = self.uvarint()?;
            let size = self.uvarint()?;
            let size = self.bounded(size as usize, "tagged field")?;
            visit(tag, self.take(size)?);
        }
        Ok(())
    }

    /// A declared length can't exceed the bytes actually left in the frame.
    /// Checking here means a corrupt (or hostile) length becomes an error
    /// instead of a multi-gigabyte `Vec::with_capacity`.
    fn bounded(&self, len: usize, what: &str) -> Result<usize> {
        if len > self.remaining() {
            return Err(malformed(format!(
                "a {what} claimed {len} byte(s) but only {} remain",
                self.remaining()
            )));
        }
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(write: impl FnOnce(&mut Encoder)) -> Vec<u8> {
        let mut enc = Encoder::new();
        write(&mut enc);
        enc.finish()
    }

    #[test]
    fn uvarints_match_the_kip_482_examples() {
        // Every boundary of the seven-bits-per-byte grouping, plus the widest
        // value a 32-bit varint can carry.
        for (value, bytes) in [
            (0u32, vec![0x00]),
            (1, vec![0x01]),
            (127, vec![0x7f]),
            (128, vec![0x80, 0x01]),
            (300, vec![0xac, 0x02]),
            (16_383, vec![0xff, 0x7f]),
            (16_384, vec![0x80, 0x80, 0x01]),
            (u32::MAX, vec![0xff, 0xff, 0xff, 0xff, 0x0f]),
        ] {
            assert_eq!(
                encoded(|e| {
                    e.uvarint(value);
                }),
                bytes,
                "encoding {value}"
            );
            assert_eq!(
                Decoder::new(&bytes).uvarint().expect("decode"),
                value,
                "decoding {value}"
            );
        }
    }

    #[test]
    fn a_varint_that_never_terminates_is_rejected_rather_than_looped_on() {
        let err = Decoder::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01])
            .uvarint()
            .expect_err("six continuation bytes cannot be a u32");
        assert!(err.to_string().contains("five bytes"), "got {err}");
    }

    /// FIVE bytes is not enough on its own: the last one carries four payload
    /// bits, so `[ff ff ff ff 7f]` is 2^35 - 1 — a legal varint, and not a
    /// `u32`. Truncating it into one would turn a corrupt (or hostile) length
    /// prefix into a plausible small number, which is exactly the value the
    /// bounds checks downstream then trust.
    #[test]
    fn a_five_byte_varint_wider_than_32_bits_is_rejected_rather_than_truncated() {
        let err = Decoder::new(&[0xff, 0xff, 0xff, 0xff, 0x7f])
            .uvarint()
            .expect_err("2^35 - 1 is not a u32");
        assert!(err.to_string().contains("32 bits"), "got {err}");

        // The widest value that IS one still decodes, byte for byte.
        assert_eq!(
            Decoder::new(&[0xff, 0xff, 0xff, 0xff, 0x0f])
                .uvarint()
                .expect("u32::MAX"),
            u32::MAX
        );
        // One more than that is refused: 0x10 in the last byte is bit 32.
        assert!(Decoder::new(&[0x80, 0x80, 0x80, 0x80, 0x10])
            .uvarint()
            .is_err());
    }

    #[test]
    fn compact_lengths_carry_n_plus_one_so_zero_can_mean_null() {
        assert_eq!(
            encoded(|e| {
                e.compact_string("kavka");
            }),
            b"\x06kavka"
        );
        assert_eq!(
            encoded(|e| {
                e.compact_nullable_string(None);
            }),
            [0x00]
        );
        assert_eq!(
            encoded(|e| {
                e.compact_string("");
            }),
            [0x01]
        );
        assert_eq!(
            encoded(|e| {
                e.compact_array_len(None);
            }),
            [0x00]
        );
        assert_eq!(
            encoded(|e| {
                e.compact_array_len(Some(0));
            }),
            [0x01]
        );
        assert_eq!(
            encoded(|e| {
                e.compact_array_len(Some(2));
            }),
            [0x03]
        );

        let mut dec = Decoder::new(b"\x06kavka\x00\x01");
        assert_eq!(dec.compact_string().unwrap(), "kavka");
        assert_eq!(dec.compact_nullable_string().unwrap(), None);
        assert_eq!(dec.compact_string().unwrap(), "");
    }

    #[test]
    fn legacy_lengths_use_minus_one_for_null() {
        assert_eq!(
            encoded(|e| {
                e.legacy_string("kavka").expect("fits");
            }),
            b"\x00\x05kavka"
        );
        assert_eq!(
            encoded(|e| {
                e.legacy_nullable_string(None).expect("null always fits");
            }),
            [0xff, 0xff]
        );

        let mut dec = Decoder::new(b"\x00\x05kavka\xff\xff");
        assert_eq!(dec.legacy_string().unwrap(), "kavka");
        assert_eq!(dec.legacy_nullable_string().unwrap(), None);
    }

    /// A string too long for an int16 length has no legacy encoding, so the
    /// encoder REFUSES rather than clamping: a clamped length declares fewer
    /// bytes than follow, and the broker reads the remainder as the next field.
    #[test]
    fn a_legacy_string_too_long_for_its_length_prefix_is_refused_not_truncated() {
        let huge = "x".repeat(i16::MAX as usize + 1);
        let mut enc = Encoder::new();
        // Mapped to `()` first: `Encoder` has no `Debug`, and the value on the
        // Ok side of this Result is a borrow of it.
        let err = enc
            .legacy_string(&huge)
            .map(|_| ())
            .expect_err("32768 bytes cannot carry an int16 length")
            .to_string();
        assert!(err.contains("32767"), "got {err}");
        assert!(err.contains("int16 length"), "got {err}");

        // Exactly i16::MAX still encodes — the boundary is inclusive.
        let biggest = "x".repeat(i16::MAX as usize);
        let mut enc = Encoder::new();
        enc.legacy_string(&biggest).expect("i16::MAX fits");
        assert_eq!(enc.finish().len(), 2 + i16::MAX as usize);
    }

    /// The cancel encoding for AlterPartitionReassignments. `Some(&[])` and
    /// `None` are different requests and must not share a byte.
    #[test]
    fn a_null_int32_array_is_not_an_empty_one() {
        assert_eq!(
            encoded(|e| {
                e.compact_int32_array(None);
            }),
            [0x00]
        );
        assert_eq!(
            encoded(|e| {
                e.compact_int32_array(Some(&[]));
            }),
            [0x01]
        );
        assert_eq!(
            encoded(|e| {
                e.compact_int32_array(Some(&[1, 2]));
            }),
            [0x03, 0, 0, 0, 1, 0, 0, 0, 2]
        );

        let mut dec = Decoder::new(&[0x00]);
        assert_eq!(dec.compact_array_len().unwrap(), None);
        let mut dec = Decoder::new(&[0x01]);
        assert_eq!(dec.compact_array_len().unwrap(), Some(0));
    }

    #[test]
    fn unknown_tagged_fields_are_skipped_not_rejected() {
        // Two tags this build has never heard of: tag 9 with 3 bytes and
        // tag 40 (two varint bytes) with 1 byte, then a marker we must land on.
        let frame = [
            0x02, // two tagged fields
            0x09, 0x03, 0xaa, 0xbb, 0xcc, // tag 9, len 3
            0xa8, 0x02, 0x01, 0xdd, // tag 296, len 1
            0x42, // the field after the buffer
        ];
        let mut dec = Decoder::new(&frame);
        dec.tagged_fields().expect("skip");
        assert_eq!(dec.remaining(), 1);
        assert_eq!(dec.take(1).unwrap(), [0x42]);
    }

    /// The reading form of the same walk: a caller that wants ONE tag gets its
    /// bytes and the rest are still stepped over correctly.
    #[test]
    fn a_visited_tagged_field_buffer_hands_back_the_tag_it_was_asked_for() {
        let frame = [
            0x02, // two tagged fields
            0x09, 0x03, 0xaa, 0xbb, 0xcc, // tag 9, len 3
            0xa8, 0x02, 0x01, 0xdd, // tag 296, len 1
            0x42, // the field after the buffer
        ];
        let mut seen = Vec::new();
        let mut dec = Decoder::new(&frame);
        dec.tagged_fields_visit(|tag, payload| seen.push((tag, payload.to_vec())))
            .expect("walk");
        assert_eq!(
            seen,
            vec![(9u32, vec![0xaa, 0xbb, 0xcc]), (296, vec![0xdd])]
        );
        assert_eq!(dec.remaining(), 1, "the walk must end where skipping does");
    }

    #[test]
    fn an_empty_tagged_field_buffer_is_a_single_zero() {
        assert_eq!(
            encoded(|e| {
                e.tagged_fields();
            }),
            [0x00]
        );
        let mut dec = Decoder::new(&[0x00, 0x42]);
        dec.tagged_fields().expect("skip");
        assert_eq!(dec.remaining(), 1);
    }

    #[test]
    fn a_length_longer_than_the_frame_is_refused_before_allocating() {
        // A compact array claiming 2^28 elements inside a 2-byte frame.
        let err = Decoder::new(&[0x80, 0x80, 0x80, 0x80, 0x01])
            .compact_array_len()
            .expect_err("absurd length");
        assert!(err.to_string().contains("only"), "got {err}");

        let err = Decoder::new(&[0x00, 0x00, 0x00, 0x7f])
            .legacy_array_len()
            .expect_err("absurd length");
        assert!(err.to_string().contains("only"), "got {err}");
    }

    #[test]
    fn running_off_the_end_names_what_was_missing() {
        let err = Decoder::new(&[0x00]).int32().expect_err("truncated");
        assert!(err.to_string().contains("wanted 4"), "got {err}");
    }

    #[test]
    fn floats_are_ieee_754_big_endian() {
        // 1 MiB/s as a quota value: 1048576.0
        let bytes = encoded(|e| {
            e.float64(1_048_576.0);
        });
        assert_eq!(bytes, [0x41, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(Decoder::new(&bytes).float64().unwrap(), 1_048_576.0);
    }

    #[test]
    fn integers_are_big_endian() {
        assert_eq!(
            encoded(|e| {
                e.int16(-1);
            }),
            [0xff, 0xff]
        );
        assert_eq!(
            encoded(|e| {
                e.int32(60_000);
            }),
            [0x00, 0x00, 0xea, 0x60]
        );
        assert_eq!(
            encoded(|e| {
                e.int8(-2);
            }),
            [0xfe]
        );
        assert_eq!(
            encoded(|e| {
                e.bool(true);
            }),
            [0x01]
        );
        assert_eq!(
            encoded(|e| {
                e.bool(false);
            }),
            [0x00]
        );

        let mut dec = Decoder::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert_eq!(dec.int64().unwrap(), -1);
    }
}
