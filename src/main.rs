use config::ContposeConfig;
use std::sync::{Arc, Mutex};
mod config;
mod instance;
mod login;
mod manifests;
mod master;
mod signals;

fn main() {
    // Allow the user to set environment
    // variables on a .env file
    dotenv::dotenv().ok();

    // Initialize the loggr
    env_logger::init();

    let config = ContposeConfig::init();
    let instances = Arc::new(Mutex::new(config.get_instances()));
    signals::handle_reload(instances.clone());
    signals::handle_sigint(instances.clone());

    loop {
        let instances = instances.lock().expect("Poisoned mutex").clone();
        std::thread::sleep(instances.delay);
        for instance in instances.inner {
            instance.poll();
        }
    }
}
