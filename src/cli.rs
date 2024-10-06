use std::{path::PathBuf, sync::OnceLock};

use clap::Parser;

/// Continuous delivery for un-complicated infrastructure.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the config file.
    #[arg(short, long, default_value = "compose-watcher.toml")]
    pub config: PathBuf,
}

static ARGS: OnceLock<Args> = OnceLock::new();

pub fn get_cli_args() -> &'static Args {
    ARGS.get_or_init(Args::parse)
}
