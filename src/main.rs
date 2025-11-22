use config_file::DispenserConfigFile;
use std::{
    process::ExitCode,
    sync::{Arc, Mutex},
};
mod cli;
mod config;
mod config_file;
mod instance;
mod manifests;
mod master;
mod signals;

const LOOP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
const NUM_THREADS: usize = 250;

fn main() -> ExitCode {
    let config_file = match DispenserConfigFile::try_init() {
        Ok(config_file) => config_file,
        Err(e) => {
            eprintln!("{e:?}");
            // Early return
            return ExitCode::FAILURE;
        }
    };

    // Initialize the loggr
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // If the user set the test flag it
    // will just validate the config
    if cli::get_cli_args().test {
        eprintln!("Dispenser config is ok.");
        return ExitCode::SUCCESS;
    }

    log::info!("Dispenser running with PID: {}", std::process::id());

    if let Err(e) = rayon::ThreadPoolBuilder::new()
        .num_threads(NUM_THREADS)
        .build_global()
    {
        eprintln!("Unable to start thread pool: {e}");
        return ExitCode::FAILURE;
    }

    let config = config_file.into_config();

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
