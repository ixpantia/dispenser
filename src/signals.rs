use crate::config_file::DispenserConfigFile;
use crate::instance::Instances;
use crate::master::MasterMsg;
use futures_util::future;
use signal_hook::{
    consts::{SIGHUP, SIGINT},
    iterator::Signals,
};
use std::process::ExitCode;
use std::sync::Arc;
use tokio::sync::Mutex;

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
pub fn handle_sigint(instances: Arc<Mutex<Instances>>) {
    let mut signals =
        Signals::new([SIGINT]).expect("No signals :(. This really should never happen");

    std::thread::spawn(move || {
        signals.forever().for_each(|_| {
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Stopping]);
            // Check if there are any paths that were deleted
            let current_instances = instances.blocking_lock().clone();

            for curr_instance in &current_instances.inner {
                curr_instance
                    .blocking_lock()
                    .master
                    .send_msg(MasterMsg::Stop);
            }

            // Wait until all current instances are stopped or detached
            loop {
                if current_instances
                    .inner
                    .iter()
                    .all(|inst| inst.blocking_lock().master.is_stopped())
                {
                    let _ = std::fs::remove_file(&crate::cli::get_cli_args().pid_file);
                    std::process::exit(0);
                }
            }
        });
    });
}

pub fn handle_reload(instances: Arc<Mutex<Instances>>, rt_handle: tokio::runtime::Handle) {
    let mut signals = Signals::new([SIGHUP]).expect("No signals :(");

    std::thread::spawn(move || {
        for _ in signals.forever() {
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Reloading]);
            let instances = Arc::clone(&instances);
            rt_handle.block_on(async move {
                // Read the config again
                match DispenserConfigFile::try_init().await {
                    Ok(new_config) => {
                        let new_config = DispenserConfigFile::into_config(new_config).await;
                        // Check if there are any paths that were deleted
                        let current_instances = instances.lock().await.clone();

                        for curr_instance in &current_instances.inner {
                            let curr_instance = curr_instance.lock().await;
                            // Is the new config does not include the current instance we
                            // send a message to stop
                            if !new_config
                                .instances
                                .iter()
                                .any(|inst| inst.path == curr_instance.config.path)
                            {
                                curr_instance.master.send_msg(MasterMsg::Stop);
                            } else {
                                curr_instance.master.send_msg(MasterMsg::Detach);
                            }
                        }

                        // Wait until all current instances are stopped or detached
                        loop {
                            let is_stopped_futures = current_instances
                                .inner
                                .iter()
                                .map(|inst| async { inst.lock().await.master.is_stopped() });
                            let all_stopped = future::join_all(is_stopped_futures)
                                .await
                                .into_iter()
                                .all(|s| s);
                            if all_stopped {
                                break;
                            }
                        }

                        let mut instances = instances.lock().await;
                        *instances = new_config.get_instances().await;
                    }
                    Err(err) => log::error!("Unable to read new config: {err}"),
                };
            });

            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
        }
    });
}
