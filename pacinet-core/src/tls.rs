use std::path::PathBuf;

/// TLS configuration for a PaciNet process.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// CA certificate for verifying peers
    pub ca_cert: PathBuf,
    /// This process's certificate
    pub cert: PathBuf,
    /// This process's private key
    pub key: PathBuf,
}

impl TlsConfig {
    pub fn new(ca_cert: PathBuf, cert: PathBuf, key: PathBuf) -> Self {
        Self { ca_cert, cert, key }
    }
}

/// Ensure the rustls ring crypto provider is installed (idempotent).
/// Call early in main() before any TLS operations.
pub fn ensure_crypto_provider() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Load server-side TLS config (for gRPC servers).
pub fn load_server_tls(
    config: &TlsConfig,
) -> Result<tonic::transport::ServerTlsConfig, Box<dyn std::error::Error + Send + Sync>> {
    ensure_crypto_provider();
    let cert_pem = std::fs::read_to_string(&config.cert)?;
    let key_pem = std::fs::read_to_string(&config.key)?;
    let ca_pem = std::fs::read_to_string(&config.ca_cert)?;

    let identity = tonic::transport::Identity::from_pem(cert_pem, key_pem);
    let ca = tonic::transport::Certificate::from_pem(ca_pem);

    Ok(tonic::transport::ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(ca))
}

/// Load client-side TLS config (for gRPC clients).
pub fn load_client_tls(
    config: &TlsConfig,
) -> Result<tonic::transport::ClientTlsConfig, Box<dyn std::error::Error + Send + Sync>> {
    ensure_crypto_provider();
    let cert_pem = std::fs::read_to_string(&config.cert)?;
    let key_pem = std::fs::read_to_string(&config.key)?;
    let ca_pem = std::fs::read_to_string(&config.ca_cert)?;

    let identity = tonic::transport::Identity::from_pem(cert_pem, key_pem);
    let ca = tonic::transport::Certificate::from_pem(ca_pem);

    Ok(tonic::transport::ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ca)
        .identity(identity))
}
