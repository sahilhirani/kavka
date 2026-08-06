//! OIDC client-credentials grant (RFC 6749 §4.4) — one blocking POST to the
//! IdP's token endpoint, exchanged for a bearer token librdkafka presents over
//! SASL/OAUTHBEARER.

use crate::{Error, Result};
use rdkafka::client::OAuthToken;
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ureq::tls::{RootCerts, TlsConfig};

/// Tokens are minted on librdkafka's demand, inline on the thread that polls
/// the client — a wedged IdP must not stall a metadata call indefinitely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// `expires_in` is OPTIONAL in RFC 6749 §5.1. When it is missing we assume the
/// common one-hour default; librdkafka re-requests a token before it lapses.
const DEFAULT_EXPIRES_IN_SECS: u64 = 3600;

/// Cap on how much of an IdP error body we quote back to the user.
const ERROR_BODY_LIMIT: usize = 200;

/// Dev-only escape hatch: allow a plain-`http` token endpoint. Named after
/// `KAVKA_DEV_OAUTH_ALLOW_PLAINTEXT` in `connection.rs` and parsed the same
/// way, because it is the same kind of local-IdP concession.
const ALLOW_PLAINTEXT_ENV: &str = "KAVKA_DEV_OIDC_ALLOW_PLAINTEXT";

/// Refuses a token endpoint that is not `https`.
///
/// The client secret is posted to this URL in a form body on every connect.
/// Over `http` that is a long-lived, replayable credential — longer-lived than
/// the bearer token it buys — readable by anything on the path. The product
/// already refuses to put the SHORTER-lived half on a plaintext socket
/// (`protocol/tls.rs` blocks a bearer token without TLS, and `connection.rs`
/// gates OAUTHBEARER-over-plaintext behind a dev variable); this closes the
/// asymmetry.
///
/// It is checked HERE rather than only in the profile editor because a profile
/// can arrive by import and never pass through the editor at all.
fn require_https(token_endpoint: &str) -> Result<()> {
    let scheme_is_https = token_endpoint
        .split_once("://")
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"));
    if scheme_is_https {
        return Ok(());
    }
    if matches!(std::env::var(ALLOW_PLAINTEXT_ENV).as_deref(), Ok("1")) {
        return Ok(());
    }
    Err(Error::Other(format!(
        "the OIDC token endpoint {token_endpoint} is not https — Kavka posts this connection's \
         client secret to it, and on a plain connection anything on the network path can read \
         and reuse that secret. Use an https endpoint, or set {ALLOW_PLAINTEXT_ENV}=1 if this \
         is a local development IdP."
    )))
}

/// RFC 6749 §5.1 success response. Deliberately no `Debug`: `access_token` is
/// a live bearer credential.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

/// RFC 6749 §5.2 error response.
#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
    error_description: Option<String>,
}

pub(super) fn token(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<OAuthToken> {
    let body = post_client_credentials(token_endpoint, client_id, client_secret)?;
    let parsed = parse_token_response(&body)?;
    Ok(OAuthToken {
        token: parsed.access_token,
        // librdkafka only uses the principal for its own bookkeeping and logs;
        // the broker derives the real identity from the token itself.
        principal_name: client_id.to_string(),
        lifetime_ms: lifetime_ms(SystemTime::now(), parsed.expires_in),
    })
}

/// The `client_secret` travels in the form body only — never in the URL — so
/// no ureq error or status message can carry it.
fn post_client_credentials(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    require_https(token_endpoint)?;
    let mut response = ureq::post(token_endpoint)
        .config()
        // Verify the IdP against the OS trust store rather than ureq's bundled
        // webpki roots (its default). Enterprise IdPs — ADFS, an internal
        // Keycloak, anything fronted by a corporate proxy — are routinely
        // issued by a private CA that is in the machine store and nowhere else;
        // with the bundled roots those simply cannot be reached. Needs ureq's
        // `platform-verifier` feature, which alone does NOT switch it on: the
        // root source is a per-request/agent setting (ureq 3 docs, "TLS
        // config"), hence this line.
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        // Read the body ourselves: RFC 6749 puts the useful diagnosis
        // (`invalid_client`, `unauthorized_client`, …) in a 400's body.
        .http_status_as_error(false)
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_form([
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .map_err(|e| {
            Error::Other(format!(
                "OIDC token request to {token_endpoint} failed: {e}"
            ))
        })?;

    let status = response.status();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Other(format!("reading the OIDC token response: {e}")))?;

    if !status.is_success() {
        return Err(Error::Other(format!(
            "OIDC token endpoint returned HTTP {}: {}",
            status.as_u16(),
            describe_error(&body)
        )));
    }
    Ok(body)
}

fn parse_token_response(body: &str) -> Result<TokenResponse> {
    serde_json::from_str(body)
        .map_err(|e| Error::Other(format!("unexpected OIDC token response: {e}")))
}

/// librdkafka wants the expiry as an absolute wall-clock instant in
/// milliseconds since the epoch, not a duration.
fn lifetime_ms(now: SystemTime, expires_in: Option<u64>) -> i64 {
    let expires_in = expires_in.unwrap_or(DEFAULT_EXPIRES_IN_SECS);
    let now_ms = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    i64::try_from(now_ms.saturating_add(u128::from(expires_in).saturating_mul(1000)))
        .unwrap_or(i64::MAX)
}

fn describe_error(body: &str) -> String {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(err) => match err.error_description {
            Some(description) => format!("{} ({description})", err.error),
            None => err.error,
        },
        Err(_) => truncate(body.trim(), ERROR_BODY_LIMIT),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canned Keycloak-shaped response.
    const KEYCLOAK: &str = r#"{
        "access_token": "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig",
        "expires_in": 300,
        "refresh_expires_in": 0,
        "token_type": "Bearer",
        "not-before-policy": 0,
        "scope": "kafka"
    }"#;

    #[test]
    fn parses_canned_token_response() {
        let parsed = parse_token_response(KEYCLOAK).expect("parse");
        assert_eq!(
            parsed.access_token,
            "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig"
        );
        assert_eq!(parsed.expires_in, Some(300));
    }

    #[test]
    fn expires_in_is_optional() {
        let parsed = parse_token_response(r#"{"access_token":"t","token_type":"Bearer"}"#)
            .expect("parse without expires_in");
        assert_eq!(parsed.expires_in, None);

        let epoch = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            lifetime_ms(epoch, parsed.expires_in),
            1_700_000_000_000 + DEFAULT_EXPIRES_IN_SECS as i64 * 1000
        );
    }

    #[test]
    fn lifetime_is_absolute_epoch_millis() {
        let epoch = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(lifetime_ms(epoch, Some(300)), 1_700_000_300_000);
    }

    #[test]
    fn missing_access_token_is_an_error() {
        // `unwrap_err` is off the table by design: `TokenResponse` has no
        // `Debug`, so a failing assert can never print a bearer token.
        let Err(err) = parse_token_response(r#"{"token_type":"Bearer"}"#) else {
            panic!("a response without access_token must not parse");
        };
        assert!(
            err.to_string().contains("unexpected OIDC token response"),
            "got {err}"
        );
    }

    /// The client secret is posted to this endpoint in a form body on every
    /// connect, so its scheme is a security answer rather than a formatting
    /// one — and it is checked HERE, not only in the profile editor, because a
    /// profile can arrive by import and never pass through the editor.
    ///
    /// The dev escape hatch is deliberately not exercised: reaching it means
    /// mutating this process's environment while other tests read it. Its
    /// absence is asserted the other way round — every refusal below NAMES the
    /// variable, so a user with a local IdP is told the way out.
    #[test]
    fn a_token_endpoint_that_is_not_https_is_refused() {
        require_https("https://idp.example/oauth2/token").expect("https is the whole point");
        // A scheme is case-insensitive, and a URL typed by hand sometimes says so.
        require_https("HTTPS://idp.example/oauth2/token").expect("case is not a scheme");

        for endpoint in [
            "http://idp.example/oauth2/token",
            "HTTP://idp.example/oauth2/token",
            // `https` in the query string is not the scheme. The check reads
            // the part before `://` and nothing else.
            "http://idp.example/oauth2/token?next=https://idp.example",
            // A scheme that merely starts with the right five letters.
            "https-everywhere://idp.example/token",
            "ftp://idp.example/token",
            "file:///tmp/token",
            // No scheme at all: ureq would guess, and a guess here is a
            // long-lived credential on the wire.
            "idp.example/oauth2/token",
            "",
        ] {
            let Err(err) = require_https(endpoint) else {
                panic!("{endpoint:?} was accepted");
            };
            let err = err.to_string();
            assert!(err.contains(endpoint), "{err}");
            assert!(err.contains("is not https"), "{err}");
            // The sentence has to say WHY — the asymmetry with the bearer
            // token is the reason this refusal is not fussiness — and it has
            // to name the way out, or a local-IdP developer works around it by
            // turning off something larger.
            assert!(err.contains("client secret"), "{err}");
            assert!(err.contains(ALLOW_PLAINTEXT_ENV), "{err}");
        }
    }

    /// …and the guard is in front of the REQUEST, not merely available beside
    /// it. `127.0.0.1:1` is a port nothing listens on: if the check were not
    /// wired in, this would come back as a connection failure instead.
    #[test]
    fn the_token_request_refuses_before_it_reaches_the_network() {
        let Err(err) = post_client_credentials("http://127.0.0.1:1/token", "kavka", "s3cret")
        else {
            panic!("a plaintext token endpoint must not be posted to");
        };
        let err = err.to_string();
        assert!(err.contains("is not https"), "{err}");
        assert!(!err.contains("s3cret"), "the secret is not in the message");
    }

    #[test]
    fn rfc6749_error_bodies_are_summarised() {
        assert_eq!(
            describe_error(r#"{"error":"invalid_client","error_description":"bad secret"}"#),
            "invalid_client (bad secret)"
        );
        assert_eq!(
            describe_error(r#"{"error":"invalid_grant"}"#),
            "invalid_grant"
        );
        assert_eq!(describe_error("  <html>nope</html> "), "<html>nope</html>");
    }
}
