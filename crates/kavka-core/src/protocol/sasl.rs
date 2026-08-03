//! SASL over the wire: SaslHandshake, then one SaslAuthenticate round trip per
//! mechanism token.
//!
//! Four mechanisms, matching the profile's transport matrix: PLAIN,
//! SCRAM-SHA-256/512 (RFC 5802, in [`super::scram`]), and OAUTHBEARER — which
//! reuses [`TokenSource`] rather than growing a second OIDC/MSK-IAM
//! implementation. That reuse is why this whole module sits behind the `kafka`
//! feature: `TokenSource` does, because it hands librdkafka an
//! `rdkafka::client::OAuthToken`.
//!
//! Secret discipline (docs/ARCHITECTURE.md D5): passwords and client secrets
//! are resolved from the OS keychain here, at connect time, used once, and
//! never logged or returned in an error.

use super::conn::{BrokerConnection, SASL_AUTHENTICATE, SASL_HANDSHAKE};
use super::errors;
use super::scram::{ScramClient, ScramHash};
use super::wire::{Decoder, Encoder};
use crate::connection::auth::TokenSource;
use crate::profiles::AuthConfig;
use crate::{secrets, Error, Result};

/// Runs the whole exchange for `auth`, or returns immediately for the
/// mechanism-free transports.
///
/// Every arm resolves its secret BEFORE the handshake: a keychain entry that
/// isn't on this machine is a configuration problem, and finding out about it
/// after two round trips only makes the message arrive later.
pub(crate) fn authenticate(conn: &mut BrokerConnection, auth: &AuthConfig) -> Result<()> {
    match auth {
        // TLS client certificates and plaintext both authenticate (or don't)
        // below the Kafka protocol: there is no SASL exchange to run.
        AuthConfig::Plaintext | AuthConfig::Tls { .. } => Ok(()),
        AuthConfig::Kerberos { .. } => Err(super::kerberos_unsupported()),
        AuthConfig::SaslPlain {
            username, password, ..
        } => {
            let password = secrets::resolve(password)?;
            handshake(conn, "PLAIN")?;
            let version = conn.negotiate(SASL_AUTHENTICATE)?;
            authenticate_step(conn, version, &plain_token(username, &password))?;
            Ok(())
        }
        AuthConfig::SaslScram {
            mechanism,
            username,
            password,
            ..
        } => {
            let hash = ScramHash::from(*mechanism);
            let password = secrets::resolve(password)?;
            let mut client = ScramClient::new(hash, username, &password)?;
            handshake(conn, hash.mechanism())?;
            let version = conn.negotiate(SASL_AUTHENTICATE)?;

            let server_first = authenticate_step(conn, version, client.client_first().as_bytes())?;
            let client_final = client.client_final(&utf8(&server_first)?)?;
            let server_final = authenticate_step(conn, version, client_final.as_bytes())?;
            // The broker's half of the mutual authentication — see
            // `ScramClient::verify_server_final` for why it is not optional.
            client.verify_server_final(&utf8(&server_final)?)
        }
        AuthConfig::OauthBearer { .. } | AuthConfig::AwsMskIam { .. } => {
            let token = token_source(auth)?.generate()?;
            handshake(conn, "OAUTHBEARER")?;
            let version = conn.negotiate(SASL_AUTHENTICATE)?;
            authenticate_step(conn, version, oauthbearer_token(&token.token).as_bytes())?;
            Ok(())
        }
    }
}

/// SaslHandshake v1 — legacy encoding at every version, because it is parsed
/// before the connection has agreed anything.
fn handshake(conn: &mut BrokerConnection, mechanism: &str) -> Result<()> {
    let version = conn.negotiate(SASL_HANDSHAKE)?;
    let mut body = Encoder::new();
    // Fallible only in principle — every mechanism name is a short literal —
    // but the encoder no longer clamps a length it cannot represent, so the
    // refusal is propagated rather than silently truncating the request.
    body.legacy_string(mechanism)?;
    let payload = conn.call(SASL_HANDSHAKE, version, body.finish())?;

    let mut decoder = Decoder::new(&payload);
    let code = decoder.int16()?;
    let count = decoder.legacy_array_len()?.unwrap_or(0);
    let mut offered = Vec::with_capacity(count);
    for _ in 0..count {
        offered.push(decoder.legacy_string()?);
    }

    if code == errors::NONE {
        return Ok(());
    }
    // Wording chosen for the `sasl-mechanism` row of the desktop error library
    // (apps/desktop/src/errors.ts): it looks for "unsupported sasl mechanism"
    // and then takes the LAST mechanism named as the broker's, so the broker's
    // list has to come after ours.
    Err(Error::Other(format!(
        "unsupported SASL mechanism {mechanism} for {} — {}",
        conn.address(),
        if offered.is_empty() {
            format!(
                "the broker named no alternatives ({})",
                errors::describe(code, None)
            )
        } else {
            format!("this broker supports: {}", offered.join(", "))
        }
    )))
}

/// One SaslAuthenticate round trip, returning the broker's challenge bytes.
fn authenticate_step(conn: &mut BrokerConnection, version: i16, token: &[u8]) -> Result<Vec<u8>> {
    let mut body = Encoder::new();
    body.compact_bytes(token).tagged_fields();
    let payload = conn.call(SASL_AUTHENTICATE, version, body.finish())?;

    let mut decoder = Decoder::new(&payload);
    let code = decoder.int16()?;
    let message = decoder.compact_nullable_string()?;
    let auth_bytes = decoder.compact_bytes()?;
    if code != errors::NONE {
        // No mechanism detail is echoed beyond what the broker itself sent —
        // the token that failed is a credential.
        return Err(Error::Other(format!(
            "SASL authentication failed against {}: {}",
            conn.address(),
            errors::describe(code, message.as_deref())
        )));
    }
    Ok(auth_bytes)
}

/// RFC 4616: `authzid \0 authcid \0 passwd`, with an empty authorization id.
fn plain_token(username: &str, password: &str) -> Vec<u8> {
    let mut token = Vec::with_capacity(username.len() + password.len() + 2);
    token.push(0);
    token.extend_from_slice(username.as_bytes());
    token.push(0);
    token.extend_from_slice(password.as_bytes());
    token
}

/// KIP-255 / RFC 7628 §3.1 client first message:
/// `n,,` (GS2 header, no channel binding, no authzid), `\x01`, the
/// `auth=Bearer <token>` key-value, `\x01`, then the empty line `\x01` that
/// terminates the key-value list.
fn oauthbearer_token(token: &str) -> String {
    format!("n,,\x01auth=Bearer {token}\x01\x01")
}

fn utf8(bytes: &[u8]) -> Result<String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| Error::Other("the broker's SASL challenge was not valid UTF-8".into()))
}

/// The same [`TokenSource`] `ClusterConnection` builds — OIDC client
/// credentials, or a SigV4-presigned MSK IAM URL — so a profile mints its
/// token exactly one way regardless of which client is asking.
#[cfg(feature = "msk-iam")]
fn token_source(auth: &AuthConfig) -> Result<TokenSource> {
    match auth {
        AuthConfig::OauthBearer {
            token_endpoint,
            client_id,
            client_secret,
        } => Ok(TokenSource::Oidc {
            token_endpoint: token_endpoint.clone(),
            client_id: client_id.clone(),
            client_secret: secrets::resolve(client_secret)?,
        }),
        AuthConfig::AwsMskIam { region, profile } => Ok(TokenSource::MskIam {
            region: region.clone(),
            profile: profile.clone(),
        }),
        _ => Err(Error::Other(
            "this profile configures no bearer token".into(),
        )),
    }
}

/// Same, for a build without the AWS SDK: an MSK IAM profile is refused rather
/// than authenticated some other way (see the feature notes in Cargo.toml).
#[cfg(not(feature = "msk-iam"))]
fn token_source(auth: &AuthConfig) -> Result<TokenSource> {
    match auth {
        AuthConfig::OauthBearer {
            token_endpoint,
            client_id,
            client_secret,
        } => Ok(TokenSource::Oidc {
            token_endpoint: token_endpoint.clone(),
            client_id: client_id.clone(),
            client_secret: secrets::resolve(client_secret)?,
        }),
        AuthConfig::AwsMskIam { .. } => Err(Error::Other(
            "this build lacks MSK IAM support — rebuild kavka-core with the `msk-iam` feature \
             (or `kafka-ssl`, which includes it)"
                .into(),
        )),
        _ => Err(Error::Other(
            "this profile configures no bearer token".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4616's `\0user\0password`, which is the whole of SASL/PLAIN.
    #[test]
    fn a_plain_token_is_two_nul_separated_fields_with_an_empty_authzid() {
        assert_eq!(plain_token("alice", "s3cr3t"), b"\0alice\0s3cr3t");
        // An empty password is still two separators, not one.
        assert_eq!(plain_token("alice", ""), b"\0alice\0");
    }

    /// KIP-255's client first message, byte for byte. The two trailing `\x01`s
    /// are the single most commonly dropped part of this format: the first ends
    /// the `auth` key-value, the second ends the (now empty) list.
    #[test]
    fn an_oauthbearer_first_message_matches_kip_255() {
        let message = oauthbearer_token("eyJhbGciOiJSUzI1NiJ9.payload.sig");
        assert_eq!(
            message,
            "n,,\u{1}auth=Bearer eyJhbGciOiJSUzI1NiJ9.payload.sig\u{1}\u{1}"
        );
        assert_eq!(
            message.as_bytes(),
            b"n,,\x01auth=Bearer eyJhbGciOiJSUzI1NiJ9.payload.sig\x01\x01"
        );

        // Structure, spelled out: GS2 header, then exactly two SOH-terminated
        // sections, the second of which is empty.
        let (gs2, rest) = message
            .split_once('\u{1}')
            .expect("a SOH after the GS2 header");
        assert_eq!(gs2, "n,,");
        let sections: Vec<&str> = rest.split('\u{1}').collect();
        assert_eq!(sections.len(), 3, "two SOH terminators: {sections:?}");
        assert!(sections[0].starts_with("auth=Bearer "));
        assert_eq!(sections[1], "");
        assert_eq!(sections[2], "");
    }

    /// A build without `msk-iam` must refuse an MSK profile by name rather than
    /// fall through to some other mechanism.
    #[cfg(not(feature = "msk-iam"))]
    #[test]
    fn msk_iam_without_the_feature_says_the_build_lacks_it() {
        let err = token_source(&AuthConfig::AwsMskIam {
            region: "eu-west-1".into(),
            profile: None,
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("lacks MSK IAM support"), "got {err}");
    }

    #[cfg(feature = "msk-iam")]
    #[test]
    fn an_msk_profile_produces_an_msk_token_source_carrying_its_region() {
        let source = token_source(&AuthConfig::AwsMskIam {
            region: "eu-west-1".into(),
            profile: Some("prod".into()),
        })
        .expect("token source");
        let rendered = format!("{source:?}");
        assert!(rendered.contains("MskIam"), "got {rendered}");
        assert!(rendered.contains("eu-west-1"), "got {rendered}");
    }

    /// A profile with no bearer credential must not reach the OAUTHBEARER path
    /// by accident — the arm exists so a future auth variant fails loudly here
    /// rather than authenticating as nobody.
    #[test]
    fn a_profile_with_no_bearer_credential_is_refused() {
        let err = token_source(&AuthConfig::Plaintext)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no bearer token"), "got {err}");
    }
}
