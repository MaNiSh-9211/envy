use clap::{Parser, Subcommand};
use std::ffi::OsString;

#[derive(Parser)]
#[command(
    name = "envy",
    version,
    about = "Universal environment manager — one config format for every stack",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scaffold an envy.yaml template and ignore envy.local.yaml
    Init,
    /// Validate config, prompt for missing keys, then run a command with injected env
    Run {
        /// Command to execute, e.g. envy run npm run dev
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<OsString>,
    },
    /// Validate schema + values without executing anything (CI friendly)
    Validate,
    /// Print the resolved configuration (secrets masked)
    List,
    /// Scan a monorepo for every envy.yaml and fill missing values service-by-service
    Setup {
        /// Maximum directory depth to scan
        #[arg(long, default_value_t = 4)]
        depth: usize,
    },
}
