//! TLS configuration and utilities for DNS-over-TLS
//!
//! Provides certificate management, TLS acceptor creation, and related utilities

#![allow(unexpected_cfgs)]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::{self, ServerConfig};
use tracing::{debug, info, warn};

/// TLS configuration for DNS-over-TLS servers
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to the TLS certificate file (PEM format)
    pub cert_path: Option<String>,

    /// Path to the private key file (PEM format)  
    pub key_path: Option<String>,

    /// Server name for SNI (Server Name Indication)
    pub server_name: Option<String>,

    /// Minimum TLS version to accept
    pub min_tls_version: TlsVersion,

    /// Maximum TLS version to accept
    pub max_tls_version: TlsVersion,

    /// Whether to require client certificates
    pub require_client_cert: bool,

    /// Path to CA certificates for client validation
    pub client_ca_path: Option<String>,

    /// Whether to auto-generate self-signed certificate if not found
    pub auto_generate_cert: bool,

    /// Additional Subject Alternative Names for auto-generated certificates
    pub additional_sans: Vec<String>,
}

/// TLS version enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    V1_2,
    V1_3,
}

impl TlsVersion {
    #[allow(dead_code)]
    fn to_rustls(self) -> &'static rustls::SupportedProtocolVersion {
        match self {
            TlsVersion::V1_2 => &rustls::version::TLS12,
            TlsVersion::V1_3 => &rustls::version::TLS13,
        }
    }
}

/// TLS-related errors
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("Failed to read certificate file: {0}")]
    CertificateRead(#[from] std::io::Error),

    #[error("Failed to parse certificate: {0}")]
    CertificateParse(String),

    #[error("Failed to parse private key: {0}")]
    PrivateKeyParse(String),

    #[error("TLS configuration error: {0}")]
    ConfigError(#[from] rustls::Error),

    #[error("No valid certificate found in file")]
    NoCertificate,

    #[error("No valid private key found in file")]
    NoPrivateKey,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cert_path: Some("/tls/server.crt".to_string()),
            key_path: Some("/tls/server.key".to_string()),
            server_name: None,
            min_tls_version: TlsVersion::V1_2,
            max_tls_version: TlsVersion::V1_3,
            require_client_cert: false,
            client_ca_path: None,
            auto_generate_cert: true,
            additional_sans: vec![],
        }
    }
}

impl TlsConfig {
    /// Create a new TLS configuration
    pub fn new(cert_path: String, key_path: String) -> Self {
        Self {
            cert_path: Some(cert_path),
            key_path: Some(key_path),
            ..Default::default()
        }
    }

    /// Set the server name for SNI
    pub fn with_server_name(mut self, server_name: String) -> Self {
        self.server_name = Some(server_name);
        self
    }

    /// Set TLS version range
    pub fn with_tls_versions(mut self, min: TlsVersion, max: TlsVersion) -> Self {
        self.min_tls_version = min;
        self.max_tls_version = max;
        self
    }

    /// Enable client certificate requirement
    pub fn with_client_cert_required(mut self, ca_path: String) -> Self {
        self.require_client_cert = true;
        self.client_ca_path = Some(ca_path);
        self
    }

    /// Create a TLS acceptor from this configuration
    pub async fn create_acceptor(&self) -> Result<TlsAcceptor, TlsError> {
        // Install default crypto provider if not already installed
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        // Load or generate certificates and private key
        let (certs, key) = self.load_or_generate_certificates().await?;

        // Create server configuration with default crypto provider
        let config = if self.require_client_cert {
            if let Some(ca_path) = &self.client_ca_path {
                let ca_certs = self.load_ca_certificates(ca_path)?;
                let mut client_auth_roots = rustls::RootCertStore::empty();
                for cert in ca_certs {
                    client_auth_roots.add(cert)?;
                }

                ServerConfig::builder()
                    .with_client_cert_verifier(
                        rustls::server::WebPkiClientVerifier::builder(Arc::new(client_auth_roots))
                            .build()
                            .map_err(|e| {
                                TlsError::CertificateParse(format!("Client verifier error: {}", e))
                            })?,
                    )
                    .with_single_cert(certs, key)
                    .map_err(TlsError::ConfigError)?
            } else {
                return Err(TlsError::ConfigError(rustls::Error::General(
                    "Client certificate required but no CA path provided".to_string(),
                )));
            }
        } else {
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(TlsError::ConfigError)?
        };

        Ok(TlsAcceptor::from(Arc::new(config)))
    }

    /// Load or generate certificates
    async fn load_or_generate_certificates(
        &self,
    ) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TlsError> {
        if self.auto_generate_cert {
            // Determine hostname for certificate generation
            let hostname = self.server_name.as_deref().unwrap_or("localhost");

            // Get paths for certificates
            let cert_path = self.cert_path.as_ref().map(Path::new);
            let key_path = self.key_path.as_ref().map(Path::new);

            // Load or generate certificate
            match crate::transport::cert_gen::load_or_generate_cert(
                cert_path,
                key_path,
                hostname,
                self.additional_sans.clone(),
            )
            .await
            {
                Ok((cert_pem, key_pem)) => {
                    // Parse the PEM data
                    let mut cert_cursor = std::io::Cursor::new(cert_pem);
                    let certs: Result<Vec<CertificateDer<'static>>, _> =
                        rustls_pemfile::certs(&mut cert_cursor).collect();
                    let certs = certs.map_err(|e| TlsError::CertificateParse(e.to_string()))?;

                    let mut key_cursor = std::io::Cursor::new(key_pem);
                    let keys: Result<Vec<_>, _> =
                        rustls_pemfile::pkcs8_private_keys(&mut key_cursor).collect();
                    let keys = keys.map_err(|e| TlsError::PrivateKeyParse(e.to_string()))?;

                    if keys.is_empty() {
                        // Try RSA format
                        key_cursor.set_position(0);
                        let keys: Result<Vec<_>, _> =
                            rustls_pemfile::rsa_private_keys(&mut key_cursor).collect();
                        let keys = keys.map_err(|e| TlsError::PrivateKeyParse(e.to_string()))?;

                        if !keys.is_empty() {
                            return Ok((certs, PrivateKeyDer::Pkcs1(keys[0].clone_key())));
                        }
                        return Err(TlsError::NoPrivateKey);
                    }

                    Ok((certs, PrivateKeyDer::Pkcs8(keys[0].clone_key())))
                }
                Err(e) => {
                    warn!("Failed to load or generate certificate: {}", e);
                    // Fall back to loading from file
                    if self.cert_path.is_some() && self.key_path.is_some() {
                        let certs = self.load_certificates()?;
                        let key = self.load_private_key()?;
                        Ok((certs, key))
                    } else {
                        Err(TlsError::CertificateRead(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "No certificate path provided and auto-generation failed",
                        )))
                    }
                }
            }
        } else {
            // Traditional loading from file
            let certs = self.load_certificates()?;
            let key = self.load_private_key()?;
            Ok((certs, key))
        }
    }

    /// Load certificates from file
    fn load_certificates(&self) -> Result<Vec<CertificateDer<'static>>, TlsError> {
        let cert_path = self.cert_path.as_ref().ok_or_else(|| {
            TlsError::CertificateRead(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No certificate path provided",
            ))
        })?;
        debug!("Loading TLS certificate from: {}", cert_path);

        let cert_data = fs::read(cert_path)?;
        let mut cursor = std::io::Cursor::new(cert_data);

        let certs: Result<Vec<CertificateDer<'static>>, _> =
            rustls_pemfile::certs(&mut cursor).collect();
        let certs = certs.map_err(|e| TlsError::CertificateParse(e.to_string()))?;

        if certs.is_empty() {
            return Err(TlsError::NoCertificate);
        }

        info!("Loaded {} certificate(s) from {}", certs.len(), cert_path);
        Ok(certs)
    }

    /// Load private key from file
    fn load_private_key(&self) -> Result<PrivateKeyDer<'static>, TlsError> {
        let key_path = self
            .key_path
            .as_ref()
            .ok_or_else(|| TlsError::PrivateKeyParse("No key path provided".to_string()))?;
        debug!("Loading private key from: {}", key_path);

        let key_data = fs::read(key_path)?;
        let mut cursor = std::io::Cursor::new(key_data);

        // Try to parse as different key formats
        let keys: Result<Vec<_>, _> = rustls_pemfile::pkcs8_private_keys(&mut cursor).collect();
        let keys = keys.map_err(|e| TlsError::PrivateKeyParse(e.to_string()))?;

        if !keys.is_empty() {
            info!("Loaded PKCS8 private key from {}", key_path);
            return Ok(PrivateKeyDer::Pkcs8(keys[0].clone_key()));
        }

        // Try RSA private key format
        cursor.set_position(0);
        let keys: Result<Vec<_>, _> = rustls_pemfile::rsa_private_keys(&mut cursor).collect();
        let keys = keys.map_err(|e| TlsError::PrivateKeyParse(e.to_string()))?;

        if !keys.is_empty() {
            info!("Loaded RSA private key from {}", key_path);
            return Ok(PrivateKeyDer::Pkcs1(keys[0].clone_key()));
        }

        Err(TlsError::NoPrivateKey)
    }

    /// Load CA certificates for client validation
    fn load_ca_certificates(
        &self,
        ca_path: &str,
    ) -> Result<Vec<CertificateDer<'static>>, TlsError> {
        debug!("Loading CA certificates from: {}", ca_path);

        let ca_data = fs::read(ca_path)?;
        let mut cursor = std::io::Cursor::new(ca_data);

        let certs: Result<Vec<CertificateDer<'static>>, _> =
            rustls_pemfile::certs(&mut cursor).collect();
        let certs = certs.map_err(|e| TlsError::CertificateParse(e.to_string()))?;

        info!("Loaded {} CA certificate(s) from {}", certs.len(), ca_path);
        Ok(certs)
    }

    /// Validate the TLS configuration (check if files exist and are readable)
    pub fn validate(&self) -> Result<(), TlsError> {
        // If auto-generation is enabled, skip validation
        if self.auto_generate_cert {
            return Ok(());
        }

        // Check certificate file
        if let Some(cert_path) = &self.cert_path {
            if !Path::new(cert_path).exists() {
                warn!("TLS certificate file not found: {}", cert_path);
                return Err(TlsError::CertificateRead(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Certificate file not found: {}", cert_path),
                )));
            }
        } else {
            return Err(TlsError::CertificateRead(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No certificate path provided".to_string(),
            )));
        }

        // Check private key file
        if let Some(key_path) = &self.key_path {
            if !Path::new(key_path).exists() {
                warn!("TLS private key file not found: {}", key_path);
                return Err(TlsError::CertificateRead(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Private key file not found: {}", key_path),
                )));
            }
        } else {
            return Err(TlsError::CertificateRead(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No private key path provided".to_string(),
            )));
        }

        // Check CA file if client certs are required
        if self.require_client_cert {
            if let Some(ca_path) = &self.client_ca_path {
                if !Path::new(ca_path).exists() {
                    warn!("TLS CA file not found: {}", ca_path);
                    return Err(TlsError::CertificateRead(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("CA file not found: {}", ca_path),
                    )));
                }
            }
        }

        // Try to load and validate the certificates
        let _certs = self.load_certificates()?;
        let _key = self.load_private_key()?;

        info!("TLS configuration validation successful");
        Ok(())
    }

    /// Generate a self-signed certificate for testing (development only)
    #[cfg(feature = "dev-tls")]
    pub fn generate_self_signed(
        hostname: &str,
        cert_path: &str,
        key_path: &str,
    ) -> Result<Self, TlsError> {
        use rcgen::{Certificate as RcgenCert, CertificateParams, DistinguishedName};
        use std::fs;

        info!(
            "Generating self-signed certificate for hostname: {}",
            hostname
        );

        let mut params = CertificateParams::new(vec![hostname.to_string()]);
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, hostname);

        let cert = RcgenCert::from_params(params)
            .map_err(|e| TlsError::CertificateParse(e.to_string()))?;

        // Ensure certificate directory exists
        if let Some(parent) = Path::new(cert_path).parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = Path::new(key_path).parent() {
            fs::create_dir_all(parent)?;
        }

        // Write certificate and key files
        fs::write(
            cert_path,
            cert.serialize_pem()
                .map_err(|e| TlsError::CertificateParse(e.to_string()))?,
        )?;
        fs::write(key_path, cert.serialize_private_key_pem())?;

        info!(
            "Generated self-signed certificate: {} and key: {}",
            cert_path, key_path
        );

        Ok(Self::new(cert_path.to_string(), key_path.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_version_conversion() {
        assert_eq!(TlsVersion::V1_2.to_rustls(), &rustls::version::TLS12);
        assert_eq!(TlsVersion::V1_3.to_rustls(), &rustls::version::TLS13);
    }

    #[test]
    fn test_default_tls_config() {
        let config = TlsConfig::default();
        assert_eq!(config.cert_path, Some("/tls/server.crt".to_string()));
        assert_eq!(config.key_path, Some("/tls/server.key".to_string()));
        assert_eq!(config.min_tls_version, TlsVersion::V1_2);
        assert_eq!(config.max_tls_version, TlsVersion::V1_3);
        assert!(!config.require_client_cert);
    }

    #[test]
    fn test_tls_config_builder() {
        let config = TlsConfig::new("test.crt".to_string(), "test.key".to_string())
            .with_server_name("example.com".to_string())
            .with_tls_versions(TlsVersion::V1_3, TlsVersion::V1_3)
            .with_client_cert_required("ca.crt".to_string());

        assert_eq!(config.cert_path, Some("test.crt".to_string()));
        assert_eq!(config.key_path, Some("test.key".to_string()));
        assert_eq!(config.server_name, Some("example.com".to_string()));
        assert_eq!(config.min_tls_version, TlsVersion::V1_3);
        assert_eq!(config.max_tls_version, TlsVersion::V1_3);
        assert!(config.require_client_cert);
        assert_eq!(config.client_ca_path, Some("ca.crt".to_string()));
    }
}
