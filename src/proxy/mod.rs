pub mod acme;
pub mod certs;

use async_trait::async_trait;
use http::{header, HeaderValue, StatusCode};
use log::{debug, info};
use openssl::ssl::{NameType, SniError};
use pingora::listeners::tls::TlsSettings;
use pingora::server::{RunArgs, ShutdownSignal};
use pingora_error::ErrorType::HTTPStatus;
use std::sync::Arc;

use pingora_core::server::configuration::Opt;
use pingora_core::server::Server;
use pingora_http::ResponseHeader;

use pingora::prelude::*;

use crate::service::file::ProxyStrategy;
use crate::service::manager::ServicesManager;

/// Router that holds multiple Service configurations and routes based on SNI/Host
pub struct DispenserProxy {
    pub services_manager: Arc<ServicesManager>,
    pub is_ssl: bool,
    pub strategy: ProxyStrategy,
}

#[async_trait]
impl ProxyHttp for DispenserProxy {
    type CTX = ();

    fn new_ctx(&self) {}

    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        let path = session.req_header().uri.path();

        // 1. Handle ACME challenges (only on Port 80)
        if !self.is_ssl && path.starts_with("/.well-known/acme-challenge/") {
            let token = path
                .strip_prefix("/.well-known/acme-challenge/")
                .unwrap_or("");
            let challenge_path = std::path::Path::new(".dispenser/challenges").join(token);

            if let Ok(content) = tokio::fs::read(challenge_path).await {
                let mut resp_header = ResponseHeader::build(StatusCode::OK, None).unwrap();
                resp_header
                    .insert_header(header::CONTENT_TYPE, "text/plain")
                    .unwrap();
                resp_header
                    .insert_header(header::CONTENT_LENGTH, content.len())
                    .unwrap();
                session.set_keepalive(None);
                session
                    .write_response_header(Box::new(resp_header), false)
                    .await?;
                session
                    .write_response_body(Some(content.into()), true)
                    .await?;
                return Ok(true);
            }
        }

        // 2. Handle HTTPS Redirects
        if !self.is_ssl && self.strategy == ProxyStrategy::HttpsOnly {
            let host_header = session
                .get_header(header::HOST)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("localhost");

            let path_and_query = session
                .req_header()
                .uri
                .path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or("/");

            let body = "<html><body>301 Moved Permanently</body></html>"
                .as_bytes()
                .to_owned();

            let mut resp_header =
                ResponseHeader::build(StatusCode::MOVED_PERMANENTLY, None).unwrap();
            resp_header
                .insert_header(header::CONTENT_TYPE, "text/html")
                .unwrap();
            resp_header
                .insert_header(header::CONTENT_LENGTH, body.len())
                .unwrap();
            resp_header
                .insert_header(
                    header::LOCATION,
                    format!("https://{}{}", host_header, path_and_query),
                )
                .unwrap();

            session.set_keepalive(None);
            session
                .write_response_header(Box::new(resp_header), false)
                .await?;
            session.write_response_body(Some(body.into()), true).await?;
            return Ok(true);
        }

        Ok(false)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        let proto = if self.is_ssl { "https" } else { "http" };
        upstream_request.insert_header("X-Forwarded-Proto", HeaderValue::from_static(proto))?;
        Ok(())
    }

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        // Get the Host header from the request
        let host = session.req_header().uri.host().or_else(|| {
            session
                .req_header()
                .headers
                .get("host")
                .and_then(|h| h.to_str().ok())
        });

        let path = session.req_header().uri.path();

        let upstream = host
            .and_then(|host| self.services_manager.resolve_route(host, path))
            .ok_or_else(|| {
                Error::explain(
                    HTTPStatus(502),
                    format!(
                        "No upstream configured for host: {:?} with path: {}",
                        host, path
                    ),
                )
            })?;

        let peer = Box::new(HttpPeer::new(upstream, false, String::new()));

        Ok(peer)
    }
}

#[derive(Debug, Clone)]
pub struct ProxySignals {
    receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<ShutdownSignal>>>,
    sender: tokio::sync::mpsc::Sender<ShutdownSignal>,
}

impl ProxySignals {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        ProxySignals {
            sender: tx,
            receiver: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }
    pub async fn send_signal(&self, signal: ShutdownSignal) {
        let _ = self.sender.send(signal).await;
    }
}

#[async_trait]
impl pingora::server::ShutdownSignalWatch for ProxySignals {
    async fn recv(&self) -> ShutdownSignal {
        let mut rx = self.receiver.lock().await;
        match rx.recv().await {
            None => unreachable!(),
            Some(signal) => signal,
        }
    }
}

pub fn run_dummy_proxy(signals: ProxySignals) {
    let opt = Opt::default();
    let mut my_server = Server::new(Some(opt)).unwrap();
    my_server.bootstrap();
    my_server.run(RunArgs {
        shutdown_signal: Box::new(signals),
    });
}

pub fn run_proxy(services_manager: Arc<ServicesManager>, signals: ProxySignals) {
    let opt = Opt::default();
    let mut my_server = Server::new(Some(opt)).unwrap();
    let strategy = services_manager.get_proxy_strategy();

    // 1. Setup HTTP Proxy (Port 80)
    let http_proxy = DispenserProxy {
        services_manager: services_manager.clone(),
        is_ssl: false,
        strategy,
    };
    let mut http_service = http_proxy_service(&my_server.configuration, http_proxy);
    http_service.add_tcp("0.0.0.0:80");
    my_server.add_service(http_service);

    // 2. Setup HTTPS Proxy (Port 443) if enabled by strategy
    if strategy != ProxyStrategy::HttpOnly {
        // Load certificates
        let cert_map = Arc::new(certs::load_all_certificates(&services_manager));
        let (default_cert, default_key) = certs::ensure_default_cert();

        let https_proxy = DispenserProxy {
            services_manager: services_manager.clone(),
            is_ssl: true,
            strategy,
        };
        let mut https_service = http_proxy_service(&my_server.configuration, https_proxy);

        // Configure TLS with SNI callback
        let mut tls_settings = TlsSettings::intermediate(
            default_cert.to_str().unwrap(),
            default_key.to_str().unwrap(),
        )
        .expect("Failed to load default fallback certificate");

        tls_settings.enable_h2();

        // Set SNI callback
        let cert_map_for_sni = cert_map.clone();
        tls_settings.set_servername_callback(move |ssl, _| {
            let host = ssl.servername(NameType::HOST_NAME);
            debug!("SNI callback for host: {:?}", host);
            if let Some(host) = host {
                if let Some(ctx) = cert_map_for_sni.get(host) {
                    let _ = ssl.set_ssl_context(ctx);
                }
            }
            Ok::<(), SniError>(())
        });

        https_service.add_tls_with_settings("0.0.0.0:443", None, tls_settings);
        my_server.add_service(https_service);
        info!(
            "Proxy starting on port 80 and 443 (Strategy: {:?})",
            strategy
        );
    } else {
        info!("Proxy starting on port 80 (Strategy: HttpOnly)");
    }

    my_server.bootstrap();
    my_server.run(RunArgs {
        shutdown_signal: Box::new(signals),
    });
}
