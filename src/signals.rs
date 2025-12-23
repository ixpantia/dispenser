use crate::service::file::EntrypointFile;
use crate::service::manager::ServicesManager;
use signal_hook::{
    consts::{SIGHUP, SIGINT},
    iterator::Signals,
};
use std::process::ExitCode;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn remove_unused_services(old_manager: &ServicesManager, new_manager: &ServicesManager) {
    let removed_services = old_manager
        .service_names
        .iter()
        .filter(|s| !new_manager.service_names.contains(s))
        .cloned()
        .collect();

    old_manager.remove_containers(removed_services).await;
}

pub fn send_signal(signal: crate::cli::Signal) -> ExitCode {
    let pid_file = &crate::cli::get_cli_args().pid_file;

    let pid = match std::fs::read_to_string(pid_file) {
        Ok(pid) => pid,
        Err(err) => {
            eprintln!("Unable to read pid file: {err}");
            return ExitCode::FAILURE;
        }
    };

    let pid: i32 = match pid.trim().parse() {
        Ok(pid) => pid,
        Err(err) => {
            eprintln!("Unable to parse pid: {err}");
            return ExitCode::FAILURE;
        }
    };

    let signal: nix::sys::signal::Signal = signal.into();
    if let Err(err) = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal) {
        eprintln!("Unable to send signal: {err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// What should we do when the user stops
/// this program?
pub fn handle_sigint(sigint_signal: Arc<tokio::sync::Notify>) {
    let mut signals =
        Signals::new([SIGINT]).expect("No signals :(. This really should never happen");

    std::thread::spawn(move || {
        for _ in signals.forever() {
            log::info!("Shutdown signal received");
            sigint_signal.notify_one();
        }
    });
}
pub async fn sigint_manager(
    manager_holder: Arc<Mutex<Arc<ServicesManager>>>,
) -> Result<(), String> {
    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Stopping]);

    log::info!("Shutting down...");

    let manager = manager_holder.lock().await;
    manager.cancel().await;
    manager.shutdown().await;
    Ok(())
}

pub fn handle_reload(reload_signal: Arc<tokio::sync::Notify>) {
    let mut signals = Signals::new([SIGHUP]).expect("No signals :(");

    std::thread::spawn(move || {
        for _ in signals.forever() {
            log::info!("Reload signal received");
            reload_signal.notify_one();
        }
    });
}

pub async fn reload_manager(
    manager_holder: Arc<Mutex<Arc<ServicesManager>>>,
) -> Result<(), String> {
    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Reloading]);

    log::info!("Reloading configuration...");

    // Load the new configuration
    let entrypoint_file = match EntrypointFile::try_init().await {
        Ok(entrypoint_file) => entrypoint_file,
        Err(e) => {
            log::error!("Failed to reload entrypoint file: {e:?}");
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
            return Err(format!("Failed to reload entrypoint file: {e:?}"));
        }
    };

    // Create a new manager with the new configuration
    let new_manager = match ServicesManager::from_config(entrypoint_file).await {
        Ok(manager) => Arc::new(manager),
        Err(e) => {
            log::error!("Failed to create new services manager: {e}");
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
            return Err(format!("Failed to create new services manager: {e}"));
        }
    };

    log::info!("New configuration loaded successfully");

    // Cancel the old manager
    let old_manager = {
        let mut holder = manager_holder.lock().await;
        let old = holder.clone();
        *holder = Arc::clone(&new_manager);
        old
    };

    log::info!("Canceling old manager...");
    old_manager.cancel().await;
    remove_unused_services(&old_manager, &new_manager).await;

    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
    log::info!("Reload complete");

    Ok(())
}
