//! The TLS half of the transport matrix, on rustls.
//!
//! Why rustls and not librdkafka's OpenSSL: this module opens its own sockets
//! (docs/ARCHITECTURE.md D2 — "the specific Kafka protocol frames directly in
//! Rust"), so it needs a TLS stack of its own, and rustls is already linked
//! into every build of this crate through `ureq` (see Cargo.toml). Reusing it
//! means the protocol client needs no build tooling that the crate did not
//! already require — in particular it does NOT need the `kafka-ssl` feature,
//! whose cost is librdkafka's *vendored OpenSSL* build (Strawberry Perl + NASM
//! on Windows). A `kafka`-only build therefore reaches a TLS cluster through
//! this module even though `ClusterConnection` would refuse it, which is the
//! honest answer: the refusal there is about a missing OpenSSL, and there is no
//! missing OpenSSL here.
//!
//! The crypto provider is passed explicitly rather than taken from rustls's
//! process default. That is not ceremony: `kavka-desktop` links other rustls
//! users (Tauri's HTTP stack) that may enable the `aws-lc-rs` provider, and
//! with two providers compiled in `ClientConfig::builder()` panics at runtime
//! with "no process-level CryptoProvider available". Naming `ring` — the
//! provider `ureq` already selects — makes that a compile-time fact instead.

use crate::profiles::AuthConfig;
use crate::{secrets, Error, Result};
// Through rustls's own re-export rather than a direct `rustls-pki-types`
// dependency: it exists so downstreams cannot drift onto a second version of
// the types rustls itself is compiled against, and PEM parsing has lived in
// pki-types since 1.11 — which is also why there is no `rustls-pemfile` here.
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;

/// Builds the rustls config for a profile, or `None` when the profile is not
/// an encrypted one.
pub(crate) fn client_config(auth: &AuthConfig) -> Result<Option<Arc<ClientConfig>>> {
    match auth {
        AuthConfig::Plaintext => Ok(None),
        AuthConfig::SaslPlain { tls, .. } | AuthConfig::SaslScram { tls, .. } => {
            if *tls {
                Ok(Some(build(None, None, None)?))
            } else {
                Ok(None)
            }
        }
        // OAUTHBEARER and MSK IAM are bearer credentials: putting one on a
        // plaintext socket hands a replayable token to anything on the path.
        // `ClusterConnection` has a documented dev-only escape hatch for the
        // OIDC case; this module has none, because the frames it carries are
        // administrative writes.
        AuthConfig::OauthBearer { .. } | AuthConfig::AwsMskIam { .. } => {
            Ok(Some(build(None, None, None)?))
        }
        AuthConfig::Tls {
            ca_pem_path,
            client_cert_pem_path,
            client_key,
        } => Ok(Some(build(
            ca_pem_path.as_deref(),
            client_cert_pem_path.as_deref(),
            client_key.as_ref(),
        )?)),
        AuthConfig::Kerberos { .. } => Err(Error::Other(
            "Kerberos (SASL/GSSAPI) is not supported — it needs a platform GSSAPI \
             implementation (SSPI on Windows, cyrus-sasl elsewhere), which Kavka does not \
             bundle"
                .into(),
        )),
    }
}

fn build(
    ca_pem_path: Option<&str>,
    client_cert_pem_path: Option<&str>,
    client_key: Option<&crate::profiles::SecretRef>,
) -> Result<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Other(format!("could not initialise TLS: {e}")))?;

    // A CA file, when given, is the ONLY trust anchor — the private CA an
    // internal cluster is issued by is normally not in the machine store, and
    // adding the platform roots alongside it would quietly re-admit every
    // public CA to a connection the operator meant to pin.
    let builder = match ca_pem_path {
        Some(path) => {
            let mut roots = RootCertStore::empty();
            for cert in read_certs(path)? {
                roots.add(cert).map_err(|e| {
                    Error::Other(format!("{path} is not a usable CA certificate: {e}"))
                })?;
            }
            if roots.is_empty() {
                return Err(Error::Other(format!(
                    "{path} contains no CERTIFICATE blocks — point this at the CA's PEM file"
                )));
            }
            builder.with_root_certificates(roots)
        }
        None => {
            use rustls_platform_verifier::BuilderVerifierExt as _;
            builder.with_platform_verifier().map_err(|e| {
                Error::Other(format!(
                    "could not use this machine's certificate store: {e}"
                ))
            })?
        }
    };

    let config =
        match (client_cert_pem_path, client_key) {
            (Some(cert_path), Some(key)) => {
                let certs = read_certs(cert_path)?;
                if certs.is_empty() {
                    return Err(Error::Other(format!(
                        "{cert_path} contains no CERTIFICATE blocks"
                    )));
                }
                // The key comes out of the OS keychain, never off disk (D5), and is
                // parsed straight from the resolved string.
                let pem = secrets::resolve(key)?;
                let key = PrivateKeyDer::from_pem_slice(pem.as_bytes()).map_err(|_| {
                    // Deliberately does not echo the parse error: the input is a
                    // private key and some PEM errors quote the offending line.
                    Error::Other(
                        "the stored client key is not a PEM private key — re-enter it in the \
                     connection's settings"
                            .into(),
                    )
                })?;
                builder.with_client_auth_cert(certs, key).map_err(|e| {
                    Error::Other(format!("the client certificate was rejected: {e}"))
                })?
            }
            (None, None) => builder.with_no_client_auth(),
            // Half a client credential is a configuration mistake that would
            // otherwise surface as an opaque broker-side handshake failure.
            (Some(_), None) => return Err(Error::Other(
                "this connection has a client certificate but no private key — add the key, or \
                 remove the certificate"
                    .into(),
            )),
            (None, Some(_)) => {
                return Err(Error::Other(
                    "this connection has a client private key but no certificate — add the \
                 certificate, or remove the key"
                        .into(),
                ))
            }
        };
    Ok(Arc::new(config))
}

fn read_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let pem = std::fs::read(path)
        .map_err(|e| Error::Other(format!("could not read the certificate file {path}: {e}")))?;
    CertificateDer::pem_slice_iter(&pem)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Other(format!("{path} is not a valid PEM certificate file: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::{ScramMechanism, SecretRef};

    fn secret(entry: &str) -> SecretRef {
        SecretRef {
            entry: entry.to_string(),
        }
    }

    #[test]
    fn plaintext_profiles_get_no_tls_config() {
        assert!(client_config(&AuthConfig::Plaintext).unwrap().is_none());
        assert!(client_config(&AuthConfig::SaslPlain {
            username: "u".into(),
            password: secret("p"),
            tls: false,
        })
        .unwrap()
        .is_none());
        assert!(client_config(&AuthConfig::SaslScram {
            mechanism: ScramMechanism::Sha512,
            username: "u".into(),
            password: secret("p"),
            tls: false,
        })
        .unwrap()
        .is_none());
    }

    #[test]
    fn tls_flagged_sasl_profiles_get_a_config_from_the_platform_store() {
        assert!(client_config(&AuthConfig::SaslPlain {
            username: "u".into(),
            password: secret("p"),
            tls: true,
        })
        .unwrap()
        .is_some());
    }

    /// A bearer token on a plaintext socket is a replayable credential, so
    /// unlike `ClusterConnection` this module offers no way to ask for one.
    #[test]
    fn bearer_token_profiles_are_always_encrypted() {
        assert!(client_config(&AuthConfig::OauthBearer {
            token_endpoint: "https://idp.example/token".into(),
            client_id: "kavka".into(),
            client_secret: secret("s"),
        })
        .unwrap()
        .is_some());
        assert!(client_config(&AuthConfig::AwsMskIam {
            region: "eu-west-1".into(),
            profile: None,
        })
        .unwrap()
        .is_some());
    }

    #[test]
    fn kerberos_says_kerberos_specifically_is_the_gap() {
        let err = client_config(&AuthConfig::Kerberos {
            service_name: "kafka".into(),
            principal: "alice@EXAMPLE".into(),
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("Kerberos"), "got {err}");
        assert!(err.contains("GSSAPI"), "got {err}");
    }

    #[test]
    fn half_a_client_credential_is_named_rather_than_left_to_the_broker() {
        let err = client_config(&AuthConfig::Tls {
            ca_pem_path: None,
            client_cert_pem_path: Some("/etc/kavka/client.pem".into()),
            client_key: None,
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("no private key"), "got {err}");

        let err = client_config(&AuthConfig::Tls {
            ca_pem_path: None,
            client_cert_pem_path: None,
            client_key: Some(secret("k")),
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("no certificate"), "got {err}");
    }

    #[test]
    fn a_missing_ca_file_names_the_path() {
        let err = client_config(&AuthConfig::Tls {
            ca_pem_path: Some("/no/such/ca.pem".into()),
            client_cert_pem_path: None,
            client_key: None,
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("/no/such/ca.pem"), "got {err}");
    }

    #[test]
    fn a_ca_file_with_no_certificates_in_it_says_so() {
        let path = std::env::temp_dir().join(format!("kavka-ca-test-{}.pem", std::process::id()));
        std::fs::write(&path, b"not a certificate\n").expect("write");
        let err = client_config(&AuthConfig::Tls {
            ca_pem_path: Some(path.to_string_lossy().into_owned()),
            client_cert_pem_path: None,
            client_key: None,
        })
        .unwrap_err()
        .to_string();
        let _ = std::fs::remove_file(&path);
        assert!(err.contains("CERTIFICATE"), "got {err}");
    }
}
