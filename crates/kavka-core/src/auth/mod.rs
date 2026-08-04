//! OAUTHBEARER token providers, backing `AuthConfig::OauthBearer` and
//! `AuthConfig::AwsMskIam`.
//!
//! librdkafka can run the OIDC client-credentials flow itself
//! (`sasl.oauthbearer.method=oidc`), but only when it is linked against
//! libcurl — the `cmake-build` we use (docs/ARCHITECTURE.md D2) is not. So
//! Kavka mints the tokens and hands them to librdkafka through rdkafka's
//! token-refresh callback on [`KavkaClientContext`]. AWS MSK IAM rides the
//! same path by design (D2): its "token" is a SigV4-presigned
//! `kafka-cluster:Connect` URL.
//!
//! Everything here BLOCKS, like the rest of the core: rdkafka invokes
//! [`ClientContext::generate_oauth_token`] on whichever thread polls the
//! client, which in Kavka is always a blocking-pool thread.
//!
//! Secret discipline (D5): the values held here are resolved from the OS
//! keychain / the AWS credential chain at connect time, are never written back
//! to a profile, and never appear in an error or a log line — including via
//! `Debug`, which is implemented by hand below to redact them.

#[cfg(feature = "msk-iam")]
mod msk;
mod oidc;

use crate::{Error, Result};
use rdkafka::client::{ClientContext, OAuthToken};
use rdkafka::consumer::ConsumerContext;
use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// How close to expiry a token has to be before the event queue is worth
/// draining again. librdkafka asks for a replacement ahead of the deadline
/// rather than on it, so the margin has to cover that lead time.
const REFRESH_MARGIN_MS: i64 = 60_000;

/// Where an OAUTHBEARER token comes from for a given connection.
pub enum TokenSource {
    /// OIDC client-credentials grant (RFC 6749 §4.4) against an IdP.
    Oidc {
        token_endpoint: String,
        client_id: String,
        /// Resolved from the OS keychain when the connection is built.
        client_secret: String,
    },
    /// AWS MSK IAM. Credentials come from the standard AWS chain
    /// (env → profile → SSO → IMDS), optionally pinned to a named profile.
    /// Gated on `msk-iam`: the AWS SDK is a heavier build than the base `kafka`
    /// tier promises (see the feature comments in Cargo.toml).
    #[cfg(feature = "msk-iam")]
    MskIam {
        region: String,
        profile: Option<String>,
    },
}

impl TokenSource {
    /// `pub(crate)` for [`crate::protocol`], which presents the same token on
    /// its own SASL/OAUTHBEARER exchange (KIP-255) instead of through
    /// librdkafka's refresh callback. One token source, two consumers — the
    /// alternative was a second OIDC and MSK IAM implementation.
    pub(crate) fn generate(&self) -> Result<OAuthToken> {
        match self {
            TokenSource::Oidc {
                token_endpoint,
                client_id,
                client_secret,
            } => oidc::token(token_endpoint, client_id, client_secret),
            #[cfg(feature = "msk-iam")]
            TokenSource::MskIam { region, profile } => msk::token(region, profile.as_deref()),
        }
    }
}

impl fmt::Debug for TokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenSource::Oidc {
                token_endpoint,
                client_id,
                ..
            } => f
                .debug_struct("TokenSource::Oidc")
                .field("token_endpoint", token_endpoint)
                .field("client_id", client_id)
                .field("client_secret", &"<redacted>")
                .finish(),
            #[cfg(feature = "msk-iam")]
            TokenSource::MskIam { region, profile } => f
                .debug_struct("TokenSource::MskIam")
                .field("region", region)
                .field("profile", profile)
                .finish(),
        }
    }
}

/// The rdkafka client context every Kavka client is built with. Its only job
/// today is minting OAUTHBEARER tokens; log/stats/error hooks keep rdkafka's
/// defaults.
#[derive(Debug)]
pub struct KavkaClientContext {
    token_source: Option<TokenSource>,
    /// Why the last token fetch failed, if it did — cleared by the next
    /// success. librdkafka swallows the error we hand back (it only logs it and
    /// lets the connection stall), so without this the user sees whatever
    /// generic timeout the pending metadata call eventually reports.
    /// `Arc<Mutex<_>>` because `generate_oauth_token` gets only `&self`, and
    /// the callback can run on a different thread from the reader.
    last_auth_error: Arc<Mutex<Option<String>>>,
    /// Epoch millis at which the current token lapses; `0` means "none yet, or
    /// the last attempt failed". Read by
    /// [`crate::connection::ClusterConnection`] to decide whether draining the
    /// event queue can serve any purpose.
    token_expiry_ms: AtomicI64,
}

impl KavkaClientContext {
    pub fn new(token_source: Option<TokenSource>) -> Self {
        Self {
            token_source,
            last_auth_error: Arc::new(Mutex::new(None)),
            token_expiry_ms: AtomicI64::new(0),
        }
    }

    /// Whether this client will be asked for OAUTHBEARER tokens — i.e. whether
    /// it has to be polled for librdkafka's refresh events to be served.
    pub fn needs_oauth_token(&self) -> bool {
        self.token_source.is_some()
    }

    /// Why the most recent token fetch failed, if the most recent one did.
    pub fn last_auth_error(&self) -> Option<String> {
        self.auth_error().clone()
    }

    /// Whether a token is in hand and far enough from expiry that librdkafka
    /// cannot have a refresh event waiting for us.
    pub fn has_fresh_token(&self) -> bool {
        let expires_at = self.token_expiry_ms.load(Ordering::Relaxed);
        expires_at > 0 && expires_at.saturating_sub(now_ms()) > REFRESH_MARGIN_MS
    }

    /// Poison-tolerant: an `Option<String>` has no invariant a panicking holder
    /// could have broken, and turning every later token fetch into a panic
    /// would be the worse failure.
    fn auth_error(&self) -> MutexGuard<'_, Option<String>> {
        self.last_auth_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Records the outcome of a token fetch and passes it through — the single
    /// place expiry and [`last_auth_error`](Self::last_auth_error) are written.
    fn record(&self, outcome: Result<OAuthToken>) -> Result<OAuthToken> {
        match &outcome {
            Ok(token) => {
                self.token_expiry_ms
                    .store(token.lifetime_ms, Ordering::Relaxed);
                *self.auth_error() = None;
            }
            Err(err) => {
                self.token_expiry_ms.store(0, Ordering::Relaxed);
                *self.auth_error() = Some(err.to_string());
            }
        }
        outcome
    }

    fn mint(&self) -> Result<OAuthToken> {
        self.token_source
            .as_ref()
            .ok_or_else(|| {
                Error::Other(
                    "librdkafka asked for an OAUTHBEARER token but this profile configures none"
                        .into(),
                )
            })?
            .generate()
    }
}

fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    i64::try_from(millis).unwrap_or(i64::MAX)
}

impl ClientContext for KavkaClientContext {
    /// Always on. librdkafka only ever emits a token-refresh event for the
    /// OAUTHBEARER mechanism, so this is inert for every other auth kind, and
    /// keeping it a plain `true` avoids a second context type (and a second
    /// `BaseConsumer` instantiation) just to carry a compile-time flag.
    const ENABLE_REFRESH_OAUTH_TOKEN: bool = true;

    fn generate_oauth_token(
        &self,
        _oauthbearer_config: Option<&str>,
    ) -> std::result::Result<OAuthToken, Box<dyn std::error::Error>> {
        Ok(self.record(self.mint())?)
    }
}

impl ConsumerContext for KavkaClientContext {}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> KavkaClientContext {
        KavkaClientContext::new(Some(TokenSource::Oidc {
            token_endpoint: "https://idp.example/token".into(),
            client_id: "kavka".into(),
            client_secret: "shhh".into(),
        }))
    }

    fn token(expires_at_ms: i64) -> OAuthToken {
        OAuthToken {
            token: "opaque".into(),
            principal_name: "kavka".into(),
            lifetime_ms: expires_at_ms,
        }
    }

    #[test]
    fn a_failed_fetch_is_kept_for_the_connection_error_to_quote() {
        let context = context();
        assert_eq!(context.last_auth_error(), None);

        let failed = context.record(Err(Error::Other(
            "OIDC token endpoint returned HTTP 401: invalid_client".into(),
        )));
        assert!(failed.is_err());
        assert_eq!(
            context.last_auth_error().as_deref(),
            Some("OIDC token endpoint returned HTTP 401: invalid_client")
        );
        // A failure must not leave a stale expiry behind that would suppress
        // the event drain on the next call.
        assert!(!context.has_fresh_token());
    }

    #[test]
    fn a_later_success_clears_the_remembered_failure() {
        let context = context();
        let _ = context.record(Err(Error::Other("transient".into())));
        assert!(context.last_auth_error().is_some());

        let _ = context.record(Ok(token(now_ms() + 900_000)));
        assert_eq!(context.last_auth_error(), None);
    }

    #[test]
    fn a_token_counts_as_fresh_only_outside_the_refresh_margin() {
        let context = context();
        assert!(!context.has_fresh_token(), "no token fetched yet");

        let _ = context.record(Ok(token(now_ms() + 900_000)));
        assert!(context.has_fresh_token());

        // Inside the margin librdkafka may already be asking for the next one.
        let _ = context.record(Ok(token(now_ms() + REFRESH_MARGIN_MS - 1_000)));
        assert!(!context.has_fresh_token());

        let _ = context.record(Ok(token(now_ms() - 1)));
        assert!(!context.has_fresh_token());
    }

    #[test]
    fn a_context_without_a_token_source_reports_that_rather_than_panicking() {
        let context = KavkaClientContext::new(None);
        assert!(!context.needs_oauth_token());

        let Err(err) = context.record(context.mint()) else {
            panic!("a context with no token source cannot mint one");
        };
        assert!(err.to_string().contains("configures none"), "got {err}");
        assert!(context.last_auth_error().is_some());
    }
}
