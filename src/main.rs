use std::{process::ExitCode, sync::Arc};

use crate::service::{
    manager::{ServiceMangerConfig, ServicesManager},
    vars::ServiceConfigError,
};
use tokio::sync::Mutex;
mod cli;
mod secrets;
mod service;
mod signals;

#[tokio::main]
async fn main() -> ExitCode {
    if let Some(signal) = &cli::get_cli_args().signal {
        return signals::send_signal(signal.clone());
    }
    let service_manager_config = match ServiceMangerConfig::try_init().await {
        Ok(conf) => conf,
        Err(e) => {
            match e {
                ServiceConfigError::Template((path, template_err)) => {
                    eprintln!("Could not render {path:#?}: {:#}", template_err);
                }
                _ => {
                    eprintln!("Error initializing service manager: {}", e);
                }
            }
            return ExitCode::FAILURE;
        }
    };

    // If the user set the test flag it
    // will just validate the config
    if cli::get_cli_args().test {
        eprintln!("Dispenser config is ok.");
        return ExitCode::SUCCESS;
    }

    // Initialize the loggr
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Dispenser running with PID: {}", std::process::id());

    if let Err(err) = std::fs::write(
        &cli::get_cli_args().pid_file,
        std::process::id().to_string(),
    ) {
        log::error!("Unable to write pid file: {err}");
        return ExitCode::FAILURE;
    }

    let manager = match ServicesManager::from_config(service_manager_config).await {
        Ok(manager) => Arc::new(manager),
        Err(e) => {
            log::error!("Failed to create services manager: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = manager.validate_containers_not_present().await {
        log::error!("{e}");
        log::error!("It seems that some of the containers declared already exist. This prevents dispenser from properly managing the life-cycle of these containers. Please remove them and restart dispenser.");
        std::process::exit(1);
    }

    // Wrap the manager in a Mutex so we can replace it on reload
    let manager_holder = Arc::new(Mutex::new(manager));

    // Create a notification channel for reload signals
    let reload_signal = Arc::new(tokio::sync::Notify::new());
    let shutdown_signal = Arc::new(tokio::sync::Notify::new());

    // Initialize signal handlers for the new system
    signals::handle_reload(reload_signal.clone());
    signals::handle_sigint(shutdown_signal.clone());

    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);

    // Main loop: start polling and wait for reload signals
    loop {
        let current_manager = manager_holder.lock().await.clone();

        tokio::select! {
            _ = current_manager.start_polling() => {
                // Polling ended normally (shouldn't happen unless cancelled)
                log::info!("Polling ended");
            }
            _ = reload_signal.notified() => {
                // Reload signal received
                if let Err(e) = signals::reload_manager(manager_holder.clone()).await {
                    log::error!("Reload failed: {e}");
                    // Continue with the old manager
                } else {
                    log::info!("Starting new manager...");
                    // Continue the loop with the new manager
                }
            }
            _ = shutdown_signal.notified() => {
                // Reload signal received
                if let Err(e) = signals::sigint_manager(manager_holder.clone()).await {
                    log::error!("Shutdown failed: {e}");
                    // Continue with the old manager
                }
                std::process::exit(0);
            }
        }
    }
}
