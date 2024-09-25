use config::ContposeConfig;
use manifests::DockerWatcherStatus;
use master::{DockerComposeMaster, MasterMsg};
use std::sync::Arc;
mod config;
mod manifests;
mod master;

/// What should we do when the user stops
/// this program?
fn handle_ctrlc(master: Arc<DockerComposeMaster>) {
    let _ = ctrlc::set_handler(move || {
        log::warn!("Stopping contpose");
        master.send_msg(MasterMsg::Stop);
    });
}

fn main() {
    // Allow the user to set environment
    // variables on a .env file
    dotenv::dotenv().ok();

    // Initialize the loggr
    env_logger::init();

    // Read the config in the current working directory
    let config = ContposeConfig::init();

    // Create a docker-compose master.
    // This represents a process that manages
    // when docker compose is lifted or destroyed
    let master = Arc::new(DockerComposeMaster::initialize("example"));

    handle_ctrlc(master.clone());

    // Create the watchers for the differents
    // images and tags
    let mut watchers = config.get_watchers();

    loop {
        // Sleep for 5 seconds
        std::thread::sleep(std::time::Duration::from_secs(5));

        // try to update the watchers and check
        // if any of them were updated
        let any_updated = watchers
            .iter_mut()
            .any(|img| matches!(img.update(), DockerWatcherStatus::Updated));

        // If any of the watchers were updated then we
        // send a message to the master to update
        if any_updated {
            master.send_msg(MasterMsg::Update);
        }

        // If the master is done (exited) then break the loop
        if master.is_done() {
            break;
        }
    }
}
