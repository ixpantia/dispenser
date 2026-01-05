use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use instant_acme::{
    Account, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use log::{error, info};
use rcgen::{CertificateParams, DistinguishedName, KeyPair, SanType};
use tokio::sync::Notify;
use x509_parser::prelude::*;

use crate::cli::get_cli_args;
use crate::service::manager::ServicesManager;

const CERTS_DIR: &str = ".dispenser/certs";
const CHALLENGES_DIR: &str = ".dispenser/challenges";
const RENEW_BEFORE_DAYS: i64 = 30;

/// Background task that ensures all managed hosts have valid SSL certificates.
/// It supports both a simulation mode (self-signed) and ACME (Let's Encrypt).
pub async fn maintain_certificates(manager: Arc<ServicesManager>, notify: Arc<Notify>) {
    let simulate = get_cli_args().simulate;

    // Ensure directories exist
    let _ = tokio::fs::create_dir_all(CERTS_DIR).await;
    let _ = tokio::fs::create_dir_all(CHALLENGES_DIR).await;

    loop {
        info!("Starting certificate maintenance check...");
        let mut changed = false;

        let proxy_configs = manager.get_proxy_configs();
        for proxy in proxy_configs {
            let host = &proxy.host;

            // Skip if manually configured
            if proxy.cert_file.is_some() || proxy.key_file.is_some() {
                continue;
            }

            if simulate {
                if ensure_simulated_cert(host).await {
                    changed = true;
                }
            } else {
                match ensure_acme_cert(&manager, host).await {
                    Ok(true) => changed = true,
                    Ok(false) => {}
                    Err(e) => error!("ACME error for {}: {}", host, e),
                }
            }
        }

        if changed {
            info!("Certificates updated, notifying proxy for reload.");
            notify.notify_one();
        }

        // Check every hour
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

/// Checks if a certificate exists and is valid for at least 30 days.
async fn needs_renewal(host: &str) -> bool {
    let cert_path = Path::new(CERTS_DIR).join(format!("{}.crt", host));
    if !cert_path.exists() {
        return true;
    }

    let Ok(content) = tokio::fs::read(&cert_path).await else {
        return true;
    };

    // Simple check using x509-parser
    let cert_der = if content.starts_with(b"-----BEGIN CERTIFICATE-----") {
        let s = String::from_utf8_lossy(&content);
        let lines: Vec<_> = s.lines().filter(|l| !l.starts_with("-----")).collect();
        let b64 = lines.join("");
        base64::Engine::decode(&base64::prelude::BASE64_STANDARD, b64).unwrap_or_default()
    } else {
        content
    };

    let Ok((_, cert)) = X509Certificate::from_der(&cert_der) else {
        return true;
    };

    let now = chrono::Utc::now().timestamp();
    let not_after = cert.validity().not_after.timestamp();
    let remaining_days = (not_after - now) / 86400;

    remaining_days < RENEW_BEFORE_DAYS
}

async fn ensure_simulated_cert(host: &str) -> bool {
    if !needs_renewal(host).await {
        return false;
    }

    info!("Generating self-signed certificate for {}", host);

    let mut params = CertificateParams::default();
    params.subject_alt_names = vec![SanType::DnsName(host.to_string().try_into().unwrap())];
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, host);
    params.distinguished_name = dn;

    let keypair = KeyPair::generate().unwrap();
    let cert = params.self_signed(&keypair).unwrap();

    let cert_path = Path::new(CERTS_DIR).join(format!("{}.crt", host));
    let key_path = Path::new(CERTS_DIR).join(format!("{}.key", host));

    tokio::fs::write(cert_path, cert.pem()).await.unwrap();
    tokio::fs::write(key_path, keypair.serialize_pem())
        .await
        .unwrap();

    true
}

async fn ensure_acme_cert(
    manager: &ServicesManager,
    host: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !needs_renewal(host).await {
        return Ok(false);
    }

    let settings = manager
        .get_certbot_settings()
        .ok_or("No certbot settings (email) found in dispenser.toml")?;

    info!("Starting ACME flow for {}", host);

    // 1. Setup ACME account
    let contact = format!("mailto:{}", settings.email);
    let (account, _) = Account::builder()?
        .create(
            &NewAccount {
                contact: &[&contact],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            LetsEncrypt::Production.url().to_string(),
            None,
        )
        .await?;

    // 2. Create order
    let identifiers = vec![Identifier::Dns(host.to_string())];
    let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;

    // 3. Handle challenges
    let mut auths = order.authorizations();
    while let Some(auth_res) = auths.next().await {
        let mut auth = auth_res?;
        let mut challenge = auth
            .challenge(ChallengeType::Http01)
            .ok_or("No HTTP-01 challenge found")?;

        let key_auth = challenge.key_authorization();
        let token = challenge.token.clone();

        // Save challenge to disk for DispenserProxy to serve
        let challenge_path = Path::new(CHALLENGES_DIR).join(&token);
        tokio::fs::write(challenge_path, key_auth.as_str()).await?;

        // Tell ACME provider we are ready
        challenge.set_ready().await?;
    }

    // 4. Poll for completion
    let retry_policy = RetryPolicy::new()
        .timeout(Duration::from_secs(30))
        .initial_delay(Duration::from_secs(2));

    let status = order.poll_ready(&retry_policy).await?;
    if status != OrderStatus::Ready {
        return Err(format!("ACME order failed with status: {:?}", status).into());
    }

    // 5. Finalize and get certificate
    // finalize() generates CSR and returns the private key PEM
    let key_pem = order.finalize().await?;

    // poll_certificate() waits for the order to become valid and returns the certificate chain PEM
    let cert_chain_pem = order.poll_certificate(&retry_policy).await?;

    // 6. Save results
    let cert_path = Path::new(CERTS_DIR).join(format!("{}.crt", host));
    let key_path = Path::new(CERTS_DIR).join(format!("{}.key", host));

    tokio::fs::write(cert_path, cert_chain_pem).await?;
    tokio::fs::write(key_path, key_pem).await?;

    // Cleanup challenge
    let _ = tokio::fs::remove_dir_all(CHALLENGES_DIR).await;
    let _ = tokio::fs::create_dir_all(CHALLENGES_DIR).await;

    Ok(true)
}
