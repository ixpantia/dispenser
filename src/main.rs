use config::ContposeConfig;
use std::sync::{Arc, Mutex};
mod cli;
mod config;
mod instance;
mod manifests;
mod master;
mod signals;

const LOOP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
const NUM_THREADS: usize = 500;

fn main() {
    rayon::ThreadPoolBuilder::new()
        .num_threads(NUM_THREADS)
        .build_global()
        .expect("Unable to start thread pool.");

    // Initialize the loggr
    env_logger::init();

    let config = ContposeConfig::init();
    let instances = Arc::new(Mutex::new(config.get_instances()));
    signals::handle_reload(instances.clone());
    signals::handle_sigint(instances.clone());
    let mut last_image_poll = std::time::Instant::now();

    loop {
        let instances = instances.lock().expect("Poisoned mutex").clone();
        // Check if enough time has passed to re poll the images
        let poll_images = last_image_poll.elapsed() >= instances.delay;
        if poll_images {
            last_image_poll = std::time::Instant::now();
        }
        std::thread::sleep(LOOP_INTERVAL);
        for instance in instances.inner.iter().cloned() {
            rayon::spawn(move || {
                instance
                    .lock()
                    .expect("Lock Poisonned. This is a bug. Please report it.")
                    .poll(poll_images);
            });
        }
    }
}
