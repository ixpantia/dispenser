use std::{path::PathBuf, sync::OnceLock};

use clap::{Parser, ValueEnum};

/// Continuous delivery for un-complicated infrastructure.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the config file.
    #[arg(short, long, default_value = "dispenser.toml")]
    pub config: PathBuf,

    /// Test the configuration file and exit.
    #[arg(short, long)]
    pub test: bool,

    /// Path to the pid file
    #[arg(short, long, default_value = "dispenser.pid")]
    pub pid_file: PathBuf,

    /// Send a signal to the running dispenser instance
    #[arg(short, long)]
    pub signal: Option<Signal>,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Signal {
    Reload,
    Stop,
}

impl From<Signal> for nix::sys::signal::Signal {
    fn from(signal: Signal) -> Self {
        match signal {
            Signal::Reload => nix::sys::signal::Signal::SIGHUP,
            Signal::Stop => nix::sys::signal::Signal::SIGINT,
        }
    }
}

static ARGS: OnceLock<Args> = OnceLock::new();

pub fn get_cli_args() -> &'static Args {
    ARGS.get_or_init(Args::parse)
}
