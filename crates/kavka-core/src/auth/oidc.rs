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
