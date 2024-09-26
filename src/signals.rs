use crate::master::MasterMsg;
use crate::{config::ContposeConfig, instance::Instances};
use signal_hook::{
    consts::{SIGHUP, SIGINT},
    iterator::Signals,
};
use std::sync::{Arc, Mutex};

/// What should we do when the user stops
/// this program?
pub fn handle_sigint(instances: Arc<Mutex<Instances>>) {
    let mut signals = Signals::new([SIGINT]).expect("No signals :(");

    std::thread::spawn(move || {
        signals.forever().for_each(|_| {
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Stopping]);
            // Check if there are any paths that were deleted
            let current_instances = instances.lock().expect("Unable to lock").clone();

            for curr_instance in &current_instances.inner {
                curr_instance.master.send_msg(MasterMsg::Stop);
            }

            // Wait until all current instances are stopped or detached
            loop {
                if current_instances
                    .inner
                    .iter()
                    .all(|inst| inst.master.is_stopped())
                {
                    std::process::exit(0);
                }
            }
        });
    });
}

pub fn handle_reload(instances: Arc<Mutex<Instances>>) {
    let mut signals = Signals::new([SIGHUP]).expect("No signals :(");

    std::thread::spawn(move || {
        for _ in signals.forever() {
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Reloading]);
            // Read the config again
            let new_config = ContposeConfig::try_init();

            match new_config {
                Ok(new_config) => {
                    // Check if there are any paths that were deleted
                    let current_instances = instances.lock().expect("Unable to lock").clone();

                    for curr_instance in &current_instances.inner {
                        // Is the new config does not include the current instance we
                        // send a message to stop
                        if !new_config
                            .instance
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
                            .all(|inst| inst.master.is_stopped())
                        {
                            break;
                        }
                    }

                    let mut instances = instances.lock().expect("Unable to lock");
                    *instances = new_config.get_instances();
                }
                Err(err) => log::error!("Unable to read new config: {err}"),
            }
            let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
        }
    });
}
