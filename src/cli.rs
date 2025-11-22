use std::{path::PathBuf, sync::OnceLock};

use clap::Parser;

/// Continuous delivery for un-complicated infrastructure.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the config file.
    #[arg(short, long, default_value = "dispenser.toml")]
    pub config: PathBuf,
    /// Path to the vars file.
    #[arg(short, long, default_value = "dispenser.vars")]
    pub vars: PathBuf,

    /// Test the configuration file and exit.
    #[arg(short, long)]
    pub test: bool,
}

static ARGS: OnceLock<Args> = OnceLock::new();

pub fn get_cli_args() -> &'static Args {
    ARGS.get_or_init(Args::parse)
}
