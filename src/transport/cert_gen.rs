//! Self-signed certificate generation for DoT/DoH
//!
//! This module provides functionality to generate self-signed certificates
//! for DNS-over-TLS and DNS-over-HTTPS when no certificate is provided.

use rcgen::{CertificateParams, DistinguishedName, DnType, Ia5String, KeyPair, SanType};
use std::path::Path;
use tokio::fs;
use tracing::{info, warn};

/// Generate a self-signed certificate for the DNS server
pub fn generate_self_signed_cert(
    hostname: &str,
    additional_sans: Vec<String>,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let mut params = CertificateParams::default();

    // Set certificate details
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, hostname);
    distinguished_name.push(DnType::OrganizationName, "Heimdall DNS Server");
    distinguished_name.push(DnType::CountryName, "US");
    params.distinguished_name = distinguished_name;

    // Add Subject Alternative Names
    params.subject_alt_names = vec![
        SanType::DnsName(
            Ia5String::try_from(hostname.to_string())
                .map_err(|e| format!("Invalid hostname: {}", e))?,
        ),
        SanType::DnsName(
            Ia5String::try_from("localhost").map_err(|e| format!("Invalid localhost: {}", e))?,
        ),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)),
    ];

    // Add any additional SANs
    for san in additional_sans {
        if let Ok(ip) = san.parse::<std::net::IpAddr>() {
            params.subject_alt_names.push(SanType::IpAddress(ip));
        } else {
            params.subject_alt_names.push(SanType::DnsName(
                Ia5String::try_from(san).map_err(|e| format!("Invalid SAN: {}", e))?,
            ));
        }
    }

    // rcgen handles certificate validity internally, no need to set explicitly

    // Generate the certificate
    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    // Get the certificate and key in PEM format
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    Ok((cert_pem, key_pem))
}

/// Generate and save a self-signed certificate to disk
pub async fn generate_and_save_cert(
    cert_path: &Path,
    key_path: &Path,
    hostname: &str,
    additional_sans: Vec<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Generating self-signed certificate for {}", hostname);

    // Generate the certificate
    let (cert_pem, key_pem) = generate_self_signed_cert(hostname, additional_sans)?;

    // Create parent directories if they don't exist
    if let Some(parent) = cert_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Save certificate and key
    fs::write(cert_path, cert_pem).await?;
    fs::write(key_path, key_pem).await?;

    // Set appropriate permissions on the key file (600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(key_path).await?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(key_path, perms).await?;
    }

    info!(
        "Self-signed certificate generated and saved to {:?} and {:?}",
        cert_path, key_path
    );

    Ok(())
}

/// Load or generate TLS certificate
pub async fn load_or_generate_cert(
    cert_path: Option<&Path>,
    key_path: Option<&Path>,
    hostname: &str,
    additional_sans: Vec<String>,
) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    match (cert_path, key_path) {
        (Some(cert_path), Some(key_path)) => {
            // Try to load existing certificate
            if cert_path.exists() && key_path.exists() {
                info!("Loading TLS certificate from {:?}", cert_path);
                let cert_pem = fs::read(cert_path).await?;
                let key_pem = fs::read(key_path).await?;
                Ok((cert_pem, key_pem))
            } else {
                warn!("Certificate files not found, generating self-signed certificate");
                // Generate and save new certificate
                generate_and_save_cert(cert_path, key_path, hostname, additional_sans).await?;
                let cert_pem = fs::read(cert_path).await?;
                let key_pem = fs::read(key_path).await?;
                Ok((cert_pem, key_pem))
            }
        }
        _ => {
            // No paths provided, generate temporary certificate
            warn!("No certificate paths provided, generating temporary self-signed certificate");
            let (cert_pem, key_pem) = generate_self_signed_cert(hostname, additional_sans)?;
            Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_self_signed_cert() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (cert_pem, key_pem) = generate_self_signed_cert(
            "test.heimdall.dns",
            vec!["alt.heimdall.dns".to_string(), "192.168.1.1".to_string()],
        )?;

        // Verify PEM format
        assert!(cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(cert_pem.ends_with("-----END CERTIFICATE-----\n"));
        assert!(key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        assert!(key_pem.ends_with("-----END PRIVATE KEY-----\n"));

        Ok(())
    }

    #[tokio::test]
    async fn test_generate_and_save_cert() {
        let temp_dir = TempDir::new().unwrap();
        let cert_path = temp_dir.path().join("cert.pem");
        let key_path = temp_dir.path().join("key.pem");

        generate_and_save_cert(&cert_path, &key_path, "test.heimdall.dns", vec![])
            .await
            .unwrap();

        // Verify files were created
        assert!(cert_path.exists());
        assert!(key_path.exists());

        // Verify file permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key_perms = tokio::fs::metadata(&key_path).await.unwrap().permissions();
            assert_eq!(key_perms.mode() & 0o777, 0o600);
        }
    }
}
