use log::{error, info, warn};
use openssl::ssl::{SslContext, SslMethod};
use rcgen::{CertificateParams, DistinguishedName, KeyPair, SanType};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::service::manager::ServicesManager;

pub type CertMap = HashMap<String, Arc<SslContext>>;

/// Loads all certificates for services managed by the ServicesManager.
/// This includes both manually configured certificates and those automatically
/// managed in the .dispenser/certs directory.
pub fn load_all_certificates(manager: &ServicesManager) -> CertMap {
    let mut cert_map = HashMap::new();
    let proxy_configs = manager.get_proxy_configs();

    let certs_dir = Path::new(".dispenser/certs");

    for proxy in proxy_configs {
        let host = proxy.host.clone();

        // 1. Check for manual overrides in service.toml
        if let (Some(cert_path), Some(key_path)) = (&proxy.cert_file, &proxy.key_file) {
            match create_ssl_context(cert_path, key_path) {
                Ok(context) => {
                    info!("Loaded manual SSL certificate for {}", host);
                    cert_map.insert(host, Arc::new(context));
                    continue;
                }
                Err(e) => {
                    error!("Failed to load manual SSL certificate for {}: {}", host, e);
                }
            }
        }

        // 2. Check for automatically managed certificates (ACME or Simulated)
        let auto_cert_path = certs_dir.join(format!("{}.crt", host));
        let auto_key_path = certs_dir.join(format!("{}.key", host));

        if auto_cert_path.exists() && auto_key_path.exists() {
            match create_ssl_context(&auto_cert_path, &auto_key_path) {
                Ok(context) => {
                    info!("Loaded automatic SSL certificate for {}", host);
                    cert_map.insert(host, Arc::new(context));
                }
                Err(e) => {
                    error!(
                        "Failed to load automatic SSL certificate for {}: {}",
                        host, e
                    );
                }
            }
        } else {
            warn!("No SSL certificate found for {}", host);
        }
    }

    cert_map
}

/// Ensures a default self-signed certificate exists for the proxy fallback.
/// Returns (cert_path, key_path).
pub fn ensure_default_cert() -> (PathBuf, PathBuf) {
    let dir = Path::new(".dispenser");
    let cert_path = dir.join("default.crt");
    let key_path = dir.join("default.key");

    if cert_path.exists() && key_path.exists() {
        return (cert_path, key_path);
    }

    let _ = fs::create_dir_all(dir);

    info!("Generating default fallback self-signed certificate...");

    let mut params = CertificateParams::default();
    params.subject_alt_names = vec![SanType::DnsName(
        "localhost".to_string().try_into().unwrap(),
    )];
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "dispenser-fallback");
    params.distinguished_name = dn;

    let keypair = KeyPair::generate().unwrap();
    let cert = params.self_signed(&keypair).unwrap();

    fs::write(&cert_path, cert.pem()).unwrap();
    fs::write(&key_path, keypair.serialize_pem()).unwrap();

    (cert_path, key_path)
}

/// Helper to create a Pingora-compatible SslContext from cert and key files.
fn create_ssl_context(
    cert_path: &Path,
    key_path: &Path,
) -> Result<SslContext, Box<dyn std::error::Error>> {
    let mut builder = openssl::ssl::SslAcceptor::mozilla_intermediate(SslMethod::tls())?;

    builder.set_certificate_chain_file(cert_path)?;
    builder.set_private_key_file(key_path, openssl::ssl::SslFiletype::PEM)?;
    builder.check_private_key()?;

    Ok(builder.build().into_context())
}
