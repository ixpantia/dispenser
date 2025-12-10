use config_file::DispenserConfigFile;
use std::{process::ExitCode, sync::Arc};
use tokio::sync::Mutex;

use crate::instance::Instances;
mod cli;
mod config;
mod config_file;
mod instance;
mod manifests;
mod master;
mod secrets;
mod signals;

const LOOP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

#[tokio::main]
async fn main() -> ExitCode {
    if let Some(signal) = &cli::get_cli_args().signal {
        return signals::send_signal(signal.clone());
    }

    let config_file = match DispenserConfigFile::try_init().await {
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

    if let Err(err) = std::fs::write(
        &cli::get_cli_args().pid_file,
        std::process::id().to_string(),
    ) {
        log::error!("Unable to write pid file: {err}");
        return ExitCode::FAILURE;
    }

    let config = config_file.into_config().await;

    let instances = Arc::new(Mutex::new(Instances::default()));
    // We need to initialize the reload and interrupt handlers
    signals::handle_reload(instances.clone(), tokio::runtime::Handle::current());
    signals::handle_sigint(instances.clone());
    // Override the instances
    *instances.lock().await = config.get_instances().await;
    let mut last_image_poll = std::time::Instant::now();

    loop {
        let instances = instances.lock().await.clone();
        // Check if enough time has passed to re poll the images
        let poll_images = last_image_poll.elapsed() >= instances.delay;
        if poll_images {
            last_image_poll = std::time::Instant::now();
        }
        tokio::time::sleep(LOOP_INTERVAL).await;
        for instance in instances.inner.into_iter() {
            tokio::spawn(async move {
                instance.lock().await.poll(poll_images).await;
            });
        }
    }
}
