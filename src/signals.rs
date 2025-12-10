use crate::config_file::DispenserConfigFile;
use crate::instance::Instances;
use crate::master::MasterMsg;
use signal_hook::{
    consts::{SIGHUP, SIGINT},
    iterator::Signals,
};
use std::sync::Arc;
use tokio::sync::Mutex;

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
                    std::process::exit(0);
                }
            }
        });
    });
}

pub fn handle_reload(instances: Arc<Mutex<Instances>>) {
    let mut signals = Signals::new([SIGHUP]).expect("No signals :(");

    tokio::spawn(async move {
        for _ in signals.forever() {
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Reloading]);
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
                        if current_instances
                            .inner
                            .iter()
                            .all(|inst| inst.blocking_lock().master.is_stopped())
                        {
                            break;
                        }
                    }

                    let mut instances = instances.lock().await;
                    *instances = new_config.get_instances().await;
                }
                Err(err) => log::error!("Unable to read new config: {err}"),
            };

            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
        }
    });
}
