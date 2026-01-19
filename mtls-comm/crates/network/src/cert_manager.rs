use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;

pub struct CertificateManager {
    ca_cert_path: String,
    node_cert_path: String,
    node_key_path: String,
}

impl CertificateManager {
    pub fn new(ca_path: &str, cert_path: &str, key_path: &str) -> Self {
        Self {
            ca_cert_path: ca_path.to_string(),
            node_cert_path: cert_path.to_string(),
            node_key_path: key_path.to_string(),
        }
    }

    pub fn load_certificates(
        &self,
    ) -> anyhow::Result<(
        Vec<CertificateDer<'static>>,
        PrivateKeyDer<'static>,
        Vec<CertificateDer<'static>>,
    )> {
        // Load node certificate
        let cert_file = File::open(&self.node_cert_path)?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()?;

        // Load node private key
        let key_file = File::open(&self.node_key_path)?;
        let mut key_reader = BufReader::new(key_file);
        let pkcs8_keys: Vec<_> = rustls_pemfile::pkcs8_private_keys(&mut key_reader)
            .collect::<Result<Vec<_>, _>>()?;

        let key = if let Some(k) = pkcs8_keys.into_iter().next() {
            PrivateKeyDer::Pkcs8(k)
        } else {
            // Try RSA format
            let key_file = File::open(&self.node_key_path)?;
            let mut key_reader = BufReader::new(key_file);
            let rsa_keys: Vec<_> = rustls_pemfile::rsa_private_keys(&mut key_reader)
                .collect::<Result<Vec<_>, _>>()?;

            if let Some(k) = rsa_keys.into_iter().next() {
                PrivateKeyDer::Pkcs1(k)
            } else {
                return Err(anyhow::anyhow!("No private key found"));
            }
        };

        // Load CA certificate
        let ca_file = File::open(&self.ca_cert_path)?;
        let mut ca_reader = BufReader::new(ca_file);
        let ca_certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut ca_reader)
            .collect::<Result<Vec<_>, _>>()?;

        Ok((certs, key, ca_certs))
    }

    pub fn verify_certificate_expiry(&self) -> anyhow::Result<bool> {
        // Check if certificate is about to expire
        // Return true if renewal needed
        use x509_parser::prelude::*;

        let cert_file = File::open(&self.node_cert_path)?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(cert) = certs.first() {
            let parsed = X509Certificate::from_der(cert.as_ref())
                .map_err(|e| anyhow::anyhow!("Failed to parse certificate: {}", e))?;

            let validity = parsed.1.validity();
            let now = std::time::SystemTime::now();

            // Check if cert expires within 30 days
            let thirty_days = std::time::Duration::from_secs(30 * 24 * 60 * 60);
            let expires_soon = validity
                .not_after
                .timestamp()
                .checked_sub(now.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64)
                .map(|secs| secs < thirty_days.as_secs() as i64)
                .unwrap_or(true);

            return Ok(expires_soon);
        }

        Ok(false)
    }
}
