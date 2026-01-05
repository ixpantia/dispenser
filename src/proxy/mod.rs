use async_trait::async_trait;
use pingora::listeners::tls::TlsSettings;
use pingora::server::{RunArgs, ShutdownSignal};
use pingora_error::ErrorType::HTTPStatus;
use std::sync::Arc;

use pingora_core::server::configuration::Opt;
use pingora_core::server::Server;

use pingora::prelude::*;

use crate::service::manager::ServicesManager;

/// Router that holds multiple Service configurations and routes based on SNI/Host
pub struct DispenserProxy {
    services_manager: Arc<ServicesManager>,
}

#[async_trait]
impl ProxyHttp for DispenserProxy {
    type CTX = ();

    fn new_ctx(&self) {}

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        // Get the Host header from the request
        let host = session.req_header().uri.host();
        let host = host.or_else(|| {
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
    // Read command line arguments
    let opt = Opt::default();
    let mut my_server = Server::new(Some(opt)).unwrap();

    // Create a single HTTP proxy service with the router
    let mut proxy_service = http_proxy_service(
        &my_server.configuration,
        DispenserProxy { services_manager },
    );

    // Configure TLS settings
    // TODO: We need to add certbot support!
    // This could be a good reference: https://raw.githubusercontent.com/koompi/pingora-proxy-server/refs/heads/master/src/cert/issuer.rs
    let mut tls_settings =
        TlsSettings::intermediate("certs/dispenser.crt", "certs/dispenser.key").unwrap();
    tls_settings.enable_h2();

    // Single TLS listener on port 8443 - routes all traffic based on Host header
    proxy_service.add_tls_with_settings("0.0.0.0:8443", None, tls_settings);

    my_server.add_service(proxy_service);

    my_server.bootstrap();
    // This is blocking. Which means that we need to run this whole
    // procedure on a separate thread.
    my_server.run(RunArgs {
        shutdown_signal: Box::new(signals),
    });
}
