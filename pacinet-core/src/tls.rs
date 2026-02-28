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

/// Load server-side TLS config (for gRPC servers).
pub fn load_server_tls(
    config: &TlsConfig,
) -> Result<tonic::transport::ServerTlsConfig, Box<dyn std::error::Error + Send + Sync>> {
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
    let cert_pem = std::fs::read_to_string(&config.cert)?;
    let key_pem = std::fs::read_to_string(&config.key)?;
    let ca_pem = std::fs::read_to_string(&config.ca_cert)?;

    let identity = tonic::transport::Identity::from_pem(cert_pem, key_pem);
    let ca = tonic::transport::Certificate::from_pem(ca_pem);

    Ok(tonic::transport::ClientTlsConfig::new()
        .ca_certificate(ca)
        .identity(identity))
}
