use std::{process::ExitCode, sync::Arc};
use tokio::sync::{Mutex, Notify};

use crate::{
    cli::Commands,
    proxy::{acme, run_dummy_proxy, run_proxy, ProxySignals},
    service::{
        manager::{ServiceMangerConfig, ServicesManager},
        vars::ServiceConfigError,
    },
};

mod cli;
mod proxy;
mod secrets;
mod service;
mod signals;

#[tokio::main]
async fn main() -> ExitCode {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let args = cli::get_cli_args();

    if let Some(signal) = &args.signal {
        return signals::send_signal(signal.clone()).await;
    }

    // Initialize the logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let service_filter = match &args.command {
        Some(Commands::Dev { services }) => services.as_ref().map(Vec::as_slice),
        _ => None,
    };

    if args.test {
        match ServiceMangerConfig::try_init(service_filter).await {
            Ok(_) => {
                eprintln!("Dispenser config is ok.");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("Error validating config: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }

    log::info!("Dispenser running with PID: {}", std::process::id());

    if let Err(err) = tokio::fs::write(&args.pid_file, std::process::id().to_string()).await {
        log::error!("Unable to write pid file: {err}");
        return ExitCode::FAILURE;
    }

    // Signals for lifecycle control
    let reload_signal = Arc::new(Notify::new());
    let shutdown_signal = Arc::new(Notify::new());
    let proxy_restart_notify = Arc::new(Notify::new());

    signals::handle_reload(reload_signal.clone());
    signals::handle_sigint(shutdown_signal.clone());

    // Initial manager setup
    let service_manager_config = match ServiceMangerConfig::try_init(service_filter).await {
        Ok(conf) => conf,
        Err(e) => {
            match e {
                ServiceConfigError::Template((path, e)) => {
                    log::error!("Error rendering: {path:?}");
                    log::error!("{e}");
                }
                e => log::error!("Failed to initialize config: {e}"),
            }
            return ExitCode::FAILURE;
        }
    };

    let manager = match ServicesManager::from_config(service_manager_config, None).await {
        Ok(m) => Arc::new(m),
        Err(e) => {
            log::error!("Failed to create services manager: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = manager.validate_containers_not_present().await {
        log::error!("{e}");
        log::error!("Containers already exist. Please remove them and restart dispenser.");
        return ExitCode::FAILURE;
    }

    // This is at a restart level.
    let proxy_enabled = manager.proxy_enabled();

    let manager_holder = Arc::new(Mutex::new(manager));
    let proxy_signals = ProxySignals::new();

    // Start dummy proxy to hold the signal lock
    if proxy_enabled {
        tokio::task::spawn_blocking({
            let signals = proxy_signals.clone();
            move || run_dummy_proxy(signals)
        });
    }

    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);

    loop {
        // OUTER LOOP: Manager Lifecycle
        let current_manager = manager_holder.lock().await.clone();

        // 1. Polling Task (Maintains 'init' and timer state)
        let polling_handle = tokio::spawn({
            let manager = current_manager.clone();
            async move { manager.start_polling().await }
        });

        // 2. ACME Task (Watchdog for certificates)
        let acme_handle = proxy_enabled.then(|| {
            tokio::spawn(acme::maintain_certificates(
                current_manager.clone(),
                proxy_restart_notify.clone(),
                service_filter.is_some(), // If the filters exists we are in dev mode.
            ))
        });

        // Inner proxy loop;
        loop {
            if proxy_enabled {
                // INNER LOOP: Proxy Lifecycle
                log::info!("Starting proxy instance...");

                // Start Proxy (Blocking in a thread)
                std::thread::spawn({
                    let manager = current_manager.clone();
                    let signals = proxy_signals.clone();
                    move || run_proxy(manager, signals)
                });

                // Handover: Signal the previous proxy (dummy or old generation) to gracefully upgrade
                // This releases the Mutex lock in ProxySignals, allowing the new proxy to start listening.
                proxy_signals
                    .send_signal(pingora::server::ShutdownSignal::GracefulUpgrade)
                    .await;
            }

            tokio::select! {
                _ = proxy_restart_notify.notified() => {
                    log::info!("Certificates updated. Restarting proxy...");
                    continue; // inner loop: start a new proxy instance
                }
                _ = reload_signal.notified() => {
                    log::info!("Reload signal received. Refreshing manager...");

                    // Abort manager-bound tasks
                    polling_handle.abort();
                    acme_handle.map(|t| t.abort());

                    if let Err(e) = signals::reload_manager(manager_holder.clone(), service_filter).await {
                        log::error!("Reload failed: {e}");
                    }

                    break; // inner loop -> outer loop to restart manager tasks
                }
                _ = shutdown_signal.notified() => {
                    log::info!("Shutdown signal received. Exiting...");

                    // Abort manager-bound tasks
                    polling_handle.abort();
                    acme_handle.map(|t| t.abort());

                    let manager = manager_holder.lock().await;
                    manager.cancel().await;
                    manager.shutdown().await;

                    proxy_signals.send_signal(pingora::server::ShutdownSignal::GracefulTerminate).await;

                    let _ = tokio::fs::remove_file(&cli::get_cli_args().pid_file).await;

                    // Exit the process
                    std::process::exit(0);
                }
            }
        }
    }
}
