use anyhow::Result;
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Generated TLS certificate and key pair.
pub struct Certificate {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Generate a self-signed ECDSA P-256 certificate.
pub fn generate_self_signed_cert() -> Result<Certificate> {
    let key_pair = KeyPair::generate(&PKCS_ECDSA_P256_SHA256)?;
    let mut params = CertificateParams::new(vec!["LocalSend User".to_string()]);
    params.key_pair = Some(key_pair);
    let cert = rcgen::Certificate::from_params(params)?;

    Ok(Certificate {
        cert_pem: cert.serialize_pem()?,
        key_pem: cert.serialize_private_key_pem(),
    })
}

/// Compute SHA-256 fingerprint of a PEM-encoded certificate.
pub fn cert_fingerprint(cert_pem: &str) -> Result<String> {
    use x509_parser::pem::Pem;
    let mut cursor = std::io::Cursor::new(cert_pem.as_bytes());
    let (pem, _) = Pem::read(&mut cursor)?;
    let hash = Sha256::digest(pem.contents);
    Ok(format!("{:x}", hash))
}

/// Create a rustls server config that verifies client certificates.
pub fn create_server_config(cert_pem: &str, key_pem: &str) -> Result<rustls::ServerConfig> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let certs = vec![CertificateDer::from_pem_slice(cert_pem.as_bytes())?];
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes())?;

    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(AcceptAnyValidClientCertVerifier))
        .with_single_cert(certs, key)?;

    Ok(config)
}

#[derive(Debug)]
struct AcceptAnyValidClientCertVerifier;

impl rustls::server::danger::ClientCertVerifier for AcceptAnyValidClientCertVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        verify_cert_der(end_entity.as_ref()).map_err(|e| {
            tracing::warn!("Client certificate verification failed: {e:#}");
            rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            )
        })?;
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        let provider = rustls::crypto::ring::default_provider();
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        let provider = rustls::crypto::ring::default_provider();
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Verify a DER-encoded certificate is cryptographically valid.
fn verify_cert_der(cert_der: &[u8]) -> Result<()> {
    use x509_parser::certificate::X509Certificate;
    use x509_parser::prelude::FromDer;

    let (_, cert) = X509Certificate::from_der(cert_der)?;

    if !cert.validity().is_valid() {
        anyhow::bail!("Certificate time validity check failed");
    }

    cert.verify_signature(None)
        .map_err(|e| anyhow::anyhow!("Certificate signature verification failed: {e}"))?;

    Ok(())
}

/// Compute SHA-256 fingerprint from a DER-encoded certificate.
pub fn fingerprint_from_der(cert_der: &[u8]) -> String {
    let hash = Sha256::digest(cert_der);
    format!("{:x}", hash)
}

/// Create a reqwest client that uses TLS with client certificate authentication.
pub fn create_tls_client(cert_pem: &str, key_pem: &str) -> Result<reqwest::Client> {
    let identity =
        reqwest::tls::Identity::from_pem(format!("{cert_pem}{key_pem}").as_bytes())?;

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .identity(identity)
        .tls_info(true)
        .build()?;

    Ok(client)
}

/// Create a reqwest client without client cert, accepting any server cert.
pub fn create_insecure_client() -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .danger_accept_invalid_certs(true)
        .tls_info(true)
        .build()?;

    Ok(client)
}

/// Verify the server certificate fingerprint from a TLS connection.
pub fn verify_server_fingerprint(
    response: &reqwest::Response,
    expected_fingerprint: &str,
) -> Result<()> {
    let tls_info = response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .ok_or_else(|| anyhow::anyhow!("No TLS info available"))?;

    let peer_cert = tls_info
        .peer_certificate()
        .ok_or_else(|| anyhow::anyhow!("No peer certificate"))?;

    let actual_fingerprint = fingerprint_from_der(peer_cert.as_ref());

    if actual_fingerprint != expected_fingerprint {
        anyhow::bail!(
            "Certificate fingerprint mismatch: expected {}, got {}",
            expected_fingerprint,
            actual_fingerprint
        );
    }

    Ok(())
}
