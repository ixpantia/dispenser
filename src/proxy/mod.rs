pub mod acme;
pub mod certs;

use async_trait::async_trait;
use http::{header, HeaderValue, Response, StatusCode};
use log::{debug, info};
use openssl::ssl::{NameType, SniError};
use pingora::apps::http_app::ServeHttp;
use pingora::listeners::tls::TlsSettings;
use pingora::protocols::http::ServerSession;
use pingora::server::{RunArgs, ShutdownSignal};
use pingora::services::listening::Service;
use pingora_error::ErrorType::HTTPStatus;
use std::sync::Arc;

use pingora_core::server::configuration::Opt;
use pingora_core::server::Server;

use pingora::prelude::*;

use crate::service::manager::ServicesManager;

pub struct AcmeService;

#[async_trait]
impl ServeHttp for AcmeService {
    async fn response(&self, http_stream: &mut ServerSession) -> Response<Vec<u8>> {
        let path = http_stream.req_header().uri.path();

        // 1. Handle ACME challenges
        if path.starts_with("/.well-known/acme-challenge/") {
            let token = path
                .strip_prefix("/.well-known/acme-challenge/")
                .unwrap_or("");
            let challenge_path = std::path::Path::new(".dispenser/challenges").join(token);

            if let Ok(content) = tokio::fs::read(challenge_path).await {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/plain")
                    .header(header::CONTENT_LENGTH, content.len())
                    .body(content)
                    .unwrap();
            }
        }

        let host_header = http_stream
            .get_header(header::HOST)
            .unwrap()
            .to_str()
            .unwrap();
        debug!("host header: {host_header}");

        let path_and_query = http_stream
            .req_header()
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        let body = "<html><body>301 Moved Permanently</body></html>"
            .as_bytes()
            .to_owned();

        Response::builder()
            .status(StatusCode::MOVED_PERMANENTLY)
            .header(header::CONTENT_TYPE, "text/html")
            .header(header::CONTENT_LENGTH, body.len())
            .header(
                header::LOCATION,
                format!("https://{}{}", host_header, path_and_query),
            )
            .body(body)
            .unwrap()
    }
}

/// Router that holds multiple Service configurations and routes based on SNI/Host
pub struct DispenserProxy {
    pub services_manager: Arc<ServicesManager>,
}

#[async_trait]
impl ProxyHttp for DispenserProxy {
    type CTX = ();

    fn new_ctx(&self) {}

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_request.insert_header("X-Forwarded-Proto", HeaderValue::from_static("https"))?;
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

        let upstream = host
            .and_then(|host| self.services_manager.resolve_host(host))
            .ok_or_else(|| {
                Error::explain(
                    HTTPStatus(502),
                    format!("No upstream configured for host: {:?}", host),
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

    // 1. Load certificates
    let cert_map = Arc::new(certs::load_all_certificates(&services_manager));
    let (default_cert, default_key) = certs::ensure_default_cert();

    // 2. Setup Proxy
    let mut proxy_service = http_proxy_service(
        &my_server.configuration,
        DispenserProxy {
            services_manager: services_manager.clone(),
        },
    );

    // 3. Configure TLS with SNI callback
    // We use intermediate settings and then override with callback
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

    proxy_service.add_tls_with_settings("0.0.0.0:443", None, tls_settings);

    let mut acme_service = Service::new("Echo Service HTTP".to_string(), AcmeService);
    acme_service.add_tcp("0.0.0.0:80");

    my_server.add_service(proxy_service);
    my_server.add_service(acme_service);
    my_server.bootstrap();

    info!("Proxy starting on port 443");
    my_server.run(RunArgs {
        shutdown_signal: Box::new(signals),
    });
}
