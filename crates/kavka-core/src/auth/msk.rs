//! AWS MSK IAM as an OAUTHBEARER token source (docs/ARCHITECTURE.md D2). The
//! "token" is a SigV4-presigned `kafka-cluster:Connect` URL, base64url-encoded;
//! the broker verifies the signature against the caller's IAM identity.
//!
//! Dependency note — why not `aws-msk-iam-sasl-signer`: that crate does exactly
//! this, but it is a thin wrapper over dependencies we would carry anyway. The
//! standard credential chain (env → profile → SSO → IMDS) means `aws-config`
//! either way, and the presign means `aws-sigv4` either way. On top of those
//! the crate adds `chrono`, the `futures` facade, `aws-sdk-sts` and a second
//! major version of `thiserror` — 9 extra crates measured with `cargo tree`
//! (148 → 139) — to offer assume-role helpers Kavka does not use and to parse
//! `X-Amz-Date` back out of the URL it has just signed. Signing here is ~30
//! lines, keeps the tree lighter, and lets the expiry come straight from the
//! signing instant we already hold.

use crate::{Error, Result};
use aws_config::BehaviorVersion;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_credential_types::Credentials;
use aws_sigv4::http_request::{
    sign, SignableBody, SignableRequest, SignatureLocation, SigningSettings,
};
use aws_sigv4::sign::v4;
use aws_types::region::Region;
use base64::prelude::{Engine, BASE64_URL_SAFE_NO_PAD};
use rdkafka::client::OAuthToken;
use std::collections::HashMap;
use std::sync::{mpsc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const ACTION: &str = "kafka-cluster:Connect";
const SIGNING_NAME: &str = "kafka-cluster";
/// MSK accepts presigned URLs valid for at most 15 minutes.
const EXPIRES_IN: Duration = Duration::from_secs(900);
/// Ceiling on one whole sign-in. Every leg of the credential chain (SSO, STS,
/// IMDS) does real network I/O with a retry budget of its own, so a misrouted
/// or firewalled endpoint can otherwise wedge librdkafka's poll thread for far
/// longer than the metadata call that triggered it is willing to wait.
const SIGN_TIMEOUT: Duration = Duration::from_secs(10);

/// The resolved credential *providers* — the chain, not the credentials, which
/// the provider caches and refreshes on its own schedule. Building the chain
/// re-reads `~/.aws`, resolves the SSO token cache and may probe IMDS; it is
/// immutable once built, so doing that again on every 15-minute token refresh
/// is pure latency. Keyed by the region/profile pair that produced it, since a
/// session can hold connections to several accounts at once.
///
/// Cross-runtime note: a cached provider outlives the throwaway runtime that
/// built it; later refreshes drive its SDK clients from a different runtime.
/// hyper discards pool connections whose dispatch task is gone and the SDK's
/// credential cache only re-enters the network at credential expiry (~1h), so
/// this is believed safe — but if long-session MSK sign-ins ever error
/// intermittently after the first hour, suspect this first.
type ProviderKey = (String, Option<String>);
static PROVIDERS: LazyLock<Mutex<HashMap<ProviderKey, SharedCredentialsProvider>>> =
    LazyLock::new(Mutex::default);

/// Poison-tolerant: the map holds no invariant a panic elsewhere could break,
/// and refusing every later sign-in over it would be the worse failure.
fn providers() -> MutexGuard<'static, HashMap<ProviderKey, SharedCredentialsProvider>> {
    PROVIDERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mints one token — resolve credentials, presign, base64url — under a hard
/// [`SIGN_TIMEOUT`]. librdkafka calls this inline on whichever thread polls the
/// client, so it must not be able to block indefinitely.
pub(super) fn token(region: &str, aws_profile: Option<&str>) -> Result<OAuthToken> {
    // Detached rather than `thread::scope`: a scope joins on exit, which would
    // make the deadline below decorative. An abandoned worker finishes into a
    // dropped channel and goes away on its own.
    let key: ProviderKey = (region.to_string(), aws_profile.map(str::to_string));
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(sign_in(&key.0, key.1.as_deref()));
    });
    match rx.recv_timeout(SIGN_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::Other(format!(
            "AWS MSK IAM sign-in for region {region} gave up after {}s: the AWS credential chain \
             did not answer — check that your SSO session is still valid and that the profile's \
             STS/IMDS endpoints are reachable from here",
            SIGN_TIMEOUT.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(Error::Other("the AWS MSK IAM sign-in panicked".into()))
        }
    }
}

/// Resolve credentials, then presign. Runs on the worker thread spawned by
/// [`token`]: the credential chain is async and `Runtime::block_on` panics when
/// the calling thread is already inside a tokio runtime, which the thread
/// polling the client may well be.
fn sign_in(region: &str, aws_profile: Option<&str>) -> Result<OAuthToken> {
    let credentials = load_credentials(region, aws_profile)?;
    let signed_at = SystemTime::now();
    let url = presign(region, &credentials, signed_at)?;
    Ok(OAuthToken {
        token: BASE64_URL_SAFE_NO_PAD.encode(url.as_bytes()),
        // The acting IAM identity. Not a secret (the signature is derived from
        // the secret key, which never leaves `Credentials`), and it is already
        // inside the signed URL as `X-Amz-Credential`.
        principal_name: credentials.access_key_id().to_string(),
        lifetime_ms: lifetime_ms(signed_at),
    })
}

fn load_credentials(region: &str, aws_profile: Option<&str>) -> Result<Credentials> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Other(format!("starting the AWS credential runtime: {e}")))?;
    runtime.block_on(async {
        credentials_provider(region, aws_profile)
            .await?
            .provide_credentials()
            .await
            .map_err(|e| Error::Other(format!("resolving AWS credentials: {e}")))
    })
}

/// The cached chain for this region/profile, building it on first use.
async fn credentials_provider(
    region: &str,
    aws_profile: Option<&str>,
) -> Result<SharedCredentialsProvider> {
    let key: ProviderKey = (region.to_string(), aws_profile.map(str::to_string));
    // Cloned out and the guard dropped before any await: the lock is a plain
    // `std::sync` one and must not be held across a suspension point.
    if let Some(cached) = providers().get(&key).cloned() {
        return Ok(cached);
    }

    let mut loader =
        aws_config::defaults(BehaviorVersion::latest()).region(Region::new(region.to_string()));
    if let Some(name) = aws_profile {
        loader = loader.profile_name(name);
    }
    let provider = loader
        .load()
        .await
        .credentials_provider()
        .ok_or_else(|| Error::Other("no AWS credentials provider is configured".into()))?;
    providers().insert(key, provider.clone());
    Ok(provider)
}

/// Builds `https://kafka.<region>.amazonaws.com/?Action=kafka-cluster:Connect`
/// and presigns it. Nothing is appended afterwards: the `User-Agent` parameter
/// the AWS-authored signers tack on post-signing is not part of the canonical
/// request, and MSK does not require it.
fn presign(region: &str, credentials: &Credentials, signed_at: SystemTime) -> Result<String> {
    let mut url = Url::parse(&format!("https://kafka.{region}.amazonaws.com"))
        .map_err(|e| Error::Other(format!("invalid AWS region {region:?}: {e}")))?;
    url.query_pairs_mut().append_pair("Action", ACTION);

    let mut settings = SigningSettings::default();
    settings.signature_location = SignatureLocation::QueryParams;
    settings.expires_in = Some(EXPIRES_IN);

    let identity = credentials.clone().into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name(SIGNING_NAME)
        .time(signed_at)
        .settings(settings)
        .build()
        .map_err(|e| Error::Other(format!("building the SigV4 parameters: {e}")))?;

    let signable = SignableRequest::new(
        "GET",
        url.as_str(),
        std::iter::empty(),
        SignableBody::Bytes(&[]),
    )
    .map_err(|e| Error::Other(format!("building the signable request: {e}")))?;

    let (instructions, _signature) = sign(signable, &params.into())
        .map_err(|e| Error::Other(format!("SigV4 signing failed: {e}")))?
        .into_parts();

    {
        let mut query = url.query_pairs_mut();
        for (name, value) in instructions.params() {
            query.append_pair(name, value);
        }
    }
    Ok(url.into())
}

fn lifetime_ms(signed_at: SystemTime) -> i64 {
    let signed_at_ms = signed_at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    i64::try_from(signed_at_ms.saturating_add(EXPIRES_IN.as_millis())).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2023-11-14T22:13:20Z — fixed so the signature and credential scope are
    /// reproducible.
    const SIGNED_AT_SECS: u64 = 1_700_000_000;

    fn fixture() -> (Credentials, SystemTime) {
        (
            Credentials::new(
                "AKIDEXAMPLE",
                "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
                None,
                None,
                "kavka-test",
            ),
            UNIX_EPOCH + Duration::from_secs(SIGNED_AT_SECS),
        )
    }

    fn signed_url() -> String {
        let (credentials, signed_at) = fixture();
        presign("us-east-1", &credentials, signed_at).expect("presign")
    }

    #[test]
    fn presigned_url_has_the_msk_connect_shape() {
        let url = signed_url();
        assert!(
            url.starts_with("https://kafka.us-east-1.amazonaws.com/?"),
            "got {url}"
        );
        for expected in [
            "Action=kafka-cluster%3AConnect",
            "X-Amz-Algorithm=AWS4-HMAC-SHA256",
            "X-Amz-Credential=AKIDEXAMPLE%2F20231114%2Fus-east-1%2Fkafka-cluster%2Faws4_request",
            "X-Amz-Date=20231114T221320Z",
            "X-Amz-Expires=900",
            "X-Amz-SignedHeaders=host",
            "X-Amz-Signature=",
        ] {
            assert!(url.contains(expected), "missing {expected} in {url}");
        }
        // The secret key must not leak into the URL — only a signature derived
        // from it.
        assert!(!url.contains("wJalrXUtnFEMI"), "secret key in {url}");
    }

    #[test]
    fn signing_is_deterministic_for_a_fixed_instant() {
        assert_eq!(signed_url(), signed_url());
    }

    #[test]
    fn token_is_the_base64url_of_the_signed_url() {
        let (credentials, signed_at) = fixture();
        let token = BASE64_URL_SAFE_NO_PAD.encode(signed_url().as_bytes());
        assert!(!token.contains('='), "base64url tokens carry no padding");
        assert!(
            !token.contains('+') && !token.contains('/'),
            "url-safe alphabet"
        );

        let decoded = BASE64_URL_SAFE_NO_PAD.decode(&token).expect("decode");
        assert_eq!(
            String::from_utf8(decoded).expect("utf8"),
            presign("us-east-1", &credentials, signed_at).expect("presign")
        );
    }

    #[test]
    fn lifetime_is_the_signing_instant_plus_the_presign_window() {
        let (_, signed_at) = fixture();
        assert_eq!(
            lifetime_ms(signed_at),
            SIGNED_AT_SECS as i64 * 1000 + 900_000
        );
    }
}
