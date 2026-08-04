//! SCRAM (RFC 5802) client, in the SHA-256 and SHA-512 instantiations Kafka
//! offers (RFC 7677 and KIP-84).
//!
//! Hand-rolled rather than pulled from a crate, for one reason: the primitives
//! it needs — HMAC-SHA-256/512, SHA-256/512, PBKDF2 — are already linked into
//! this binary as `ring`, which `rustls` (and therefore `ureq`, and therefore
//! the OIDC and Schema Registry clients) brings in. Every SCRAM crate on
//! crates.io arrives with a *second* implementation of those three primitives,
//! and the mechanism itself is the seventy lines below. The exchange is checked
//! against the published vectors in RFC 5802 §5 and RFC 7677 §3, which is the
//! only assurance a dependency would have bought.
//!
//! Secret discipline (docs/ARCHITECTURE.md D5): the password reaches this
//! module already resolved from the OS keychain, is used only as PBKDF2 input,
//! and there is deliberately no `Debug` on [`ScramClient`].

use crate::profiles::ScramMechanism;
use crate::{Error, Result};
use base64::prelude::{Engine, BASE64_STANDARD};
use ring::{digest, hmac, pbkdf2, rand};
use std::num::NonZeroU32;

/// GS2 header for "no channel binding, no authorization identity", and its
/// base64 — the `c=` attribute of the client's final message. RFC 5802 §7.
const GS2_HEADER: &str = "n,,";
const GS2_HEADER_B64: &str = "biws";

/// Nonce length in characters. RFC 5802 requires only "sufficiently long";
/// 24 characters of this alphabet is ~143 bits, matching what the reference
/// implementations use.
const NONCE_LEN: usize = 24;

/// Printable ASCII minus `,` (the attribute separator) and `=` (the attribute
/// assignment), so a nonce never needs escaping and can never split a message.
const NONCE_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%&()*+-./:;<>?@[]^_{|}~";

/// A ceiling on the broker's stated iteration count. PBKDF2 runs on the calling
/// thread, so a broker (or a machine-in-the-middle on a plaintext listener)
/// that answers `i=2000000000` would otherwise wedge that thread for hours.
const MAX_ITERATIONS: u32 = 1_000_000;

/// Which hash the mechanism is built on. RFC 5802 defines the exchange; the
/// hash is a parameter of it, which is why this is an enum rather than three
/// copies of the algorithm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ScramHash {
    Sha256,
    Sha512,
    /// RFC 5802's own worked example (§5) is SCRAM-SHA-1. Kafka does not offer
    /// SHA-1 and Kavka will never negotiate it — but the RFC's vector is the
    /// published one for the *exchange*, so the test module exercises it to
    /// prove the exchange is right independently of the hash.
    #[cfg(test)]
    Sha1,
}

impl ScramHash {
    pub(crate) fn mechanism(self) -> &'static str {
        match self {
            ScramHash::Sha256 => "SCRAM-SHA-256",
            ScramHash::Sha512 => "SCRAM-SHA-512",
            #[cfg(test)]
            ScramHash::Sha1 => "SCRAM-SHA-1",
        }
    }

    fn digest(self) -> &'static digest::Algorithm {
        match self {
            ScramHash::Sha256 => &digest::SHA256,
            ScramHash::Sha512 => &digest::SHA512,
            #[cfg(test)]
            ScramHash::Sha1 => &digest::SHA1_FOR_LEGACY_USE_ONLY,
        }
    }

    fn hmac(self) -> hmac::Algorithm {
        match self {
            ScramHash::Sha256 => hmac::HMAC_SHA256,
            ScramHash::Sha512 => hmac::HMAC_SHA512,
            #[cfg(test)]
            ScramHash::Sha1 => hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        }
    }

    fn pbkdf2(self) -> pbkdf2::Algorithm {
        match self {
            ScramHash::Sha256 => pbkdf2::PBKDF2_HMAC_SHA256,
            ScramHash::Sha512 => pbkdf2::PBKDF2_HMAC_SHA512,
            #[cfg(test)]
            ScramHash::Sha1 => pbkdf2::PBKDF2_HMAC_SHA1,
        }
    }
}

impl From<ScramMechanism> for ScramHash {
    fn from(mechanism: ScramMechanism) -> Self {
        match mechanism {
            ScramMechanism::Sha256 => ScramHash::Sha256,
            ScramMechanism::Sha512 => ScramHash::Sha512,
        }
    }
}

/// One SCRAM exchange. Single use: `client_first` → (server-first) →
/// `client_final` → (server-final) → `verify_server_final`.
///
/// No `Debug`, deliberately — it holds the password.
pub(crate) struct ScramClient {
    hash: ScramHash,
    username: String,
    password: String,
    client_nonce: String,
    /// `n=<user>,r=<nonce>` — half of the AuthMessage, kept from the first step.
    client_first_bare: String,
    /// Expected `v=` value, computed while building the final message.
    server_signature: Vec<u8>,
}

impl ScramClient {
    pub(crate) fn new(hash: ScramHash, username: &str, password: &str) -> Result<Self> {
        Ok(Self::with_nonce(hash, username, password, &nonce()?))
    }

    /// The nonce is an argument here so the published RFC vectors — which pin
    /// the client nonce — can be replayed exactly.
    fn with_nonce(hash: ScramHash, username: &str, password: &str, client_nonce: &str) -> Self {
        Self {
            hash,
            username: username.to_string(),
            password: password.to_string(),
            client_nonce: client_nonce.to_string(),
            client_first_bare: String::new(),
            server_signature: Vec::new(),
        }
    }

    /// `n,,n=<user>,r=<nonce>`.
    pub(crate) fn client_first(&mut self) -> String {
        self.client_first_bare = format!(
            "n={},r={}",
            saslprep_name(&self.username),
            self.client_nonce
        );
        format!("{GS2_HEADER}{}", self.client_first_bare)
    }

    /// `c=biws,r=<nonce>,p=<proof>`, from the broker's `r=`/`s=`/`i=` reply.
    pub(crate) fn client_final(&mut self, server_first: &str) -> Result<String> {
        let server_first = std::str::from_utf8(server_first.as_bytes())
            .map_err(|_| protocol_error("the broker's SCRAM reply was not valid UTF-8"))?;
        if let Some(reason) = attribute(server_first, 'e') {
            return Err(rejected(&reason));
        }
        let nonce = attribute(server_first, 'r')
            .ok_or_else(|| protocol_error("the broker's SCRAM reply carried no nonce"))?;
        let salt = attribute(server_first, 's')
            .ok_or_else(|| protocol_error("the broker's SCRAM reply carried no salt"))?;
        let iterations = attribute(server_first, 'i')
            .ok_or_else(|| protocol_error("the broker's SCRAM reply carried no iteration count"))?;

        // RFC 5802 §5.1: the server's nonce MUST start with the client's. A
        // reply that doesn't is a different exchange being replayed at us, so
        // it is refused rather than answered with a proof.
        if !nonce.starts_with(&self.client_nonce) || nonce.len() == self.client_nonce.len() {
            return Err(protocol_error(
                "the broker's SCRAM nonce does not extend the one Kavka sent",
            ));
        }
        let salt = BASE64_STANDARD
            .decode(salt.as_bytes())
            .map_err(|_| protocol_error("the broker's SCRAM salt was not valid base64"))?;
        let iterations: u32 = iterations
            .parse()
            .ok()
            .filter(|i| (1..=MAX_ITERATIONS).contains(i))
            .ok_or_else(|| {
                protocol_error(&format!(
                    "the broker asked for an implausible SCRAM iteration count ({iterations})"
                ))
            })?;
        let iterations =
            NonZeroU32::new(iterations).ok_or_else(|| protocol_error("zero SCRAM iterations"))?;

        let salted = self.salted_password(&salt, iterations);
        let client_key = self.mac(&salted, b"Client Key");
        let stored_key = digest::digest(self.hash.digest(), &client_key);
        let server_key = self.mac(&salted, b"Server Key");

        let without_proof = format!("c={GS2_HEADER_B64},r={nonce}");
        let auth_message = format!("{},{server_first},{without_proof}", self.client_first_bare);
        let client_signature = self.mac(stored_key.as_ref(), auth_message.as_bytes());
        let proof: Vec<u8> = client_key
            .iter()
            .zip(client_signature.iter())
            .map(|(key, sig)| key ^ sig)
            .collect();
        self.server_signature = self.mac(&server_key, auth_message.as_bytes());

        Ok(format!(
            "{without_proof},p={}",
            BASE64_STANDARD.encode(proof)
        ))
    }

    /// Checks the broker's `v=` against the signature computed in
    /// [`client_final`](Self::client_final).
    ///
    /// Not optional politeness: this is the half of SCRAM that proves the thing
    /// on the other end knows the stored key, i.e. that it really is the
    /// cluster. Skipping it — which a client can do and still authenticate
    /// successfully — turns mutual authentication into one-way authentication.
    pub(crate) fn verify_server_final(&self, server_final: &str) -> Result<()> {
        if let Some(reason) = attribute(server_final, 'e') {
            return Err(rejected(&reason));
        }
        let signature = attribute(server_final, 'v')
            .ok_or_else(|| protocol_error("the broker's SCRAM reply carried no signature"))?;
        let signature = BASE64_STANDARD
            .decode(signature.as_bytes())
            .map_err(|_| protocol_error("the broker's SCRAM signature was not valid base64"))?;
        // Constant time, through the same `ring` this module already links. A
        // `!=` here compares byte by byte and stops at the first difference, so
        // how long the comparison takes leaks how many leading bytes of the
        // expected signature a guess got right — which is a forgery oracle for
        // anything that can make Kavka repeat the exchange. `ring`'s helper
        // also treats a length mismatch as a plain failure, which is what a
        // truncated `v=` should be.
        //
        // `ring` 0.17.14 marks this deprecated as an INTERNAL api it no longer
        // wants to promise — not as one that stopped being constant time; it
        // still forwards to the same `bb::verify_slices_are_equal` ring's own
        // AEAD tag checks use. The alternative is a second crate (`subtle`) for
        // one comparison, which is precisely the dependency this module exists
        // not to add. So the deprecation is accepted here, narrowly, and
        // nowhere else in the crate.
        #[allow(deprecated)]
        let verified =
            ring::constant_time::verify_slices_are_equal(&signature, &self.server_signature);
        verified.map_err(|_| {
            protocol_error(
                "the broker's SCRAM signature did not verify — the endpoint does not hold this \
                 user's credential",
            )
        })?;
        Ok(())
    }

    /// `Hi(Normalize(password), salt, i)` — PBKDF2 with the mechanism's HMAC,
    /// output length equal to the hash length.
    fn salted_password(&self, salt: &[u8], iterations: NonZeroU32) -> Vec<u8> {
        let mut out = vec![0u8; self.hash.digest().output_len()];
        pbkdf2::derive(
            self.hash.pbkdf2(),
            iterations,
            salt,
            self.password.as_bytes(),
            &mut out,
        );
        out
    }

    fn mac(&self, key: &[u8], message: &[u8]) -> Vec<u8> {
        hmac::sign(&hmac::Key::new(self.hash.hmac(), key), message)
            .as_ref()
            .to_vec()
    }
}

/// The value of a single-letter SCRAM attribute in a comma-separated message.
fn attribute(message: &str, key: char) -> Option<String> {
    let prefix = format!("{key}=");
    message
        .split(',')
        .find_map(|part| part.strip_prefix(&prefix))
        .map(str::to_string)
}

/// RFC 5802 §5.1 `saslname`: `=` and `,` are the two characters that would
/// otherwise be read as message structure.
///
/// The `Normalize` (SASLprep, RFC 4013) step is deliberately not implemented:
/// it only differs from the identity for non-ASCII names, and a *wrong*
/// normalisation is worse than none — it would fail against every other client
/// for exactly the users an implementation is least likely to test with. Kafka
/// principals are ASCII in practice; a non-ASCII one that fails will fail
/// visibly at sign-in rather than silently authenticate as someone else.
fn saslprep_name(username: &str) -> String {
    username.replace('=', "=3D").replace(',', "=2C")
}

fn nonce() -> Result<String> {
    use ring::rand::SecureRandom as _;
    let mut bytes = [0u8; NONCE_LEN];
    rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| Error::Other("the operating system refused to supply random bytes".into()))?;
    Ok(bytes
        .iter()
        .map(|byte| char::from(NONCE_ALPHABET[usize::from(*byte) % NONCE_ALPHABET.len()]))
        .collect())
}

/// Phrased to hit the `sasl-rejected` row of the desktop error classifier
/// (`apps/desktop/src/errors.ts`).
fn rejected(reason: &str) -> Error {
    Error::Other(format!("SASL authentication failed: {reason}"))
}

fn protocol_error(detail: &str) -> Error {
    Error::Other(format!("SASL/SCRAM handshake failed: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 5802 §5 — the RFC's own worked example, in its own hash (SHA-1).
    /// Kavka never negotiates SHA-1; this pins the *exchange* (AuthMessage
    /// construction, GS2 header, proof XOR, server signature) against the
    /// normative text, independently of which hash is plugged into it.
    #[test]
    fn rfc5802_section_5_exchange() {
        let mut client = ScramClient::with_nonce(
            ScramHash::Sha1,
            "user",
            "pencil",
            "fyko+d2lbbFgONRv9qkxdawL",
        );
        assert_eq!(
            client.client_first(),
            "n,,n=user,r=fyko+d2lbbFgONRv9qkxdawL"
        );

        let server_first = "r=fyko+d2lbbFgONRv9qkxdawL3rfcNHYJY1ZVvWVs7j,s=QSXCR+Q6sek8bf92,i=4096";
        assert_eq!(
            client.client_final(server_first).expect("client final"),
            "c=biws,r=fyko+d2lbbFgONRv9qkxdawL3rfcNHYJY1ZVvWVs7j,p=v0X8v3Bz2T0CJGbJQyF0X+HI4Ts="
        );
        client
            .verify_server_final("v=rmF9pqV8S7suAoZWja4dJRkFsKQ=")
            .expect("server signature verifies");
    }

    /// RFC 7677 §3 — the published SCRAM-SHA-256 vector, which is the
    /// mechanism Kafka actually offers.
    #[test]
    fn rfc7677_section_3_exchange() {
        let mut client =
            ScramClient::with_nonce(ScramHash::Sha256, "user", "pencil", "rOprNGfwEbeRWgbNEkqO");
        assert_eq!(client.client_first(), "n,,n=user,r=rOprNGfwEbeRWgbNEkqO");

        let server_first = "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,\
                            s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
        assert_eq!(
            client.client_final(server_first).expect("client final"),
            "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,\
             p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="
        );
        client
            .verify_server_final("v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=")
            .expect("server signature verifies");
    }

    /// SHA-512 has no published vector, so the expected proof is re-derived
    /// here straight from RFC 5802 §3's formulas using `ring` directly. The
    /// test therefore fails if `client_final` deviates from the RFC's
    /// definition, rather than merely if it changes.
    #[test]
    fn sha512_matches_the_rfc_formulas_recomputed_independently() {
        let (user, password, client_nonce) = ("user", "pencil", "rOprNGfwEbeRWgbNEkqO");
        let server_nonce = "rOprNGfwEbeRWgbNEkqOextrabits";
        let salt = b"\x5b\x4d\x99\x69\x9d\x12\x35\x8e";
        let iterations = NonZeroU32::new(4096).unwrap();
        let server_first = format!("r={server_nonce},s={},i=4096", BASE64_STANDARD.encode(salt));

        // --- the RFC, transcribed ------------------------------------------
        let mut salted = vec![0u8; 64];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA512,
            iterations,
            salt,
            password.as_bytes(),
            &mut salted,
        );
        let mac = |key: &[u8], msg: &[u8]| {
            hmac::sign(&hmac::Key::new(hmac::HMAC_SHA512, key), msg)
                .as_ref()
                .to_vec()
        };
        let client_key = mac(&salted, b"Client Key");
        let stored_key = digest::digest(&digest::SHA512, &client_key);
        let client_first_bare = format!("n={user},r={client_nonce}");
        let without_proof = format!("c=biws,r={server_nonce}");
        let auth_message = format!("{client_first_bare},{server_first},{without_proof}");
        let client_signature = mac(stored_key.as_ref(), auth_message.as_bytes());
        let proof: Vec<u8> = client_key
            .iter()
            .zip(client_signature.iter())
            .map(|(k, s)| k ^ s)
            .collect();
        let server_signature = mac(&mac(&salted, b"Server Key"), auth_message.as_bytes());
        // -------------------------------------------------------------------

        let mut client = ScramClient::with_nonce(ScramHash::Sha512, user, password, client_nonce);
        assert_eq!(client.client_first(), format!("n,,{client_first_bare}"));
        assert_eq!(
            client.client_final(&server_first).expect("client final"),
            format!("{without_proof},p={}", BASE64_STANDARD.encode(&proof))
        );
        client
            .verify_server_final(&format!("v={}", BASE64_STANDARD.encode(&server_signature)))
            .expect("server signature verifies");
    }

    /// RFC 5802 §5.1: the server's nonce must EXTEND the client's. Without this
    /// check a replayed server-first would get a valid proof back.
    #[test]
    fn a_server_nonce_that_does_not_extend_ours_is_refused() {
        for server_first in [
            // Someone else's nonce entirely.
            "r=someoneElsesNonce,s=QSXCR+Q6sek8bf92,i=4096",
            // Our nonce echoed with nothing added — no server contribution.
            "r=rOprNGfwEbeRWgbNEkqO,s=QSXCR+Q6sek8bf92,i=4096",
        ] {
            let mut client = ScramClient::with_nonce(
                ScramHash::Sha256,
                "user",
                "pencil",
                "rOprNGfwEbeRWgbNEkqO",
            );
            client.client_first();
            let err = client
                .client_final(server_first)
                .expect_err("must not answer with a proof");
            assert!(err.to_string().contains("does not extend"), "got {err}");
        }
    }

    #[test]
    fn a_wrong_server_signature_is_reported_as_the_endpoint_not_holding_the_credential() {
        let mut client =
            ScramClient::with_nonce(ScramHash::Sha256, "user", "pencil", "rOprNGfwEbeRWgbNEkqO");
        client.client_first();
        client
            .client_final(
                "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,\
                 s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096",
            )
            .expect("client final");
        let err = client
            .verify_server_final("v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
            .expect_err("a forged signature must not verify");
        assert!(err.to_string().contains("did not verify"), "got {err}");

        // A SHORT signature is the same answer, not a panic and not a partial
        // match: the constant-time comparison refuses a length mismatch outright.
        let err = client
            .verify_server_final("v=AAAA")
            .expect_err("a truncated signature must not verify");
        assert!(err.to_string().contains("did not verify"), "got {err}");
    }

    #[test]
    fn a_server_error_attribute_becomes_a_credentials_rejection() {
        let mut client = ScramClient::with_nonce(ScramHash::Sha256, "user", "pencil", "n0nce");
        client.client_first();
        let err = client
            .client_final("e=unknown-user")
            .expect_err("an e= reply is a rejection");
        // The desktop classifier's `sasl-rejected` row keys off this wording.
        assert!(
            err.to_string().contains("authentication failed"),
            "got {err}"
        );
        assert!(err.to_string().contains("unknown-user"), "got {err}");
    }

    /// PBKDF2 runs inline on the caller's thread, so an absurd iteration count
    /// is a denial of service, not a slow login.
    #[test]
    fn an_implausible_iteration_count_is_refused_before_any_work() {
        let mut client = ScramClient::with_nonce(ScramHash::Sha256, "user", "pencil", "n0nce");
        client.client_first();
        let err = client
            .client_final("r=n0nceMore,s=QSXCR+Q6sek8bf92,i=2000000000")
            .expect_err("two billion rounds");
        assert!(err.to_string().contains("implausible"), "got {err}");
    }

    #[test]
    fn usernames_escape_the_two_characters_that_are_message_structure() {
        assert_eq!(saslprep_name("a=b,c"), "a=3Db=2Cc");
        assert_eq!(saslprep_name("plain-user"), "plain-user");

        let mut client = ScramClient::with_nonce(ScramHash::Sha256, "a,b", "pw", "n0nce");
        assert_eq!(client.client_first(), "n,,n=a=2Cb,r=n0nce");
    }

    #[test]
    fn nonces_are_random_and_carry_no_separator_characters() {
        let first = nonce().expect("nonce");
        let second = nonce().expect("nonce");
        assert_eq!(first.len(), NONCE_LEN);
        assert_ne!(first, second, "two nonces must not collide");
        assert!(
            !first.contains(',') && !first.contains('='),
            "nonce {first} carries a SCRAM separator"
        );
    }

    #[test]
    fn the_profile_mechanism_maps_onto_the_hash_kafka_names() {
        assert_eq!(
            ScramHash::from(ScramMechanism::Sha256).mechanism(),
            "SCRAM-SHA-256"
        );
        assert_eq!(
            ScramHash::from(ScramMechanism::Sha512).mechanism(),
            "SCRAM-SHA-512"
        );
    }
}
