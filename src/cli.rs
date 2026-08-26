use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "envy",
    version,
    about = "Universal environment manager — one config format for every stack",
    long_about = None
)]
pub struct Cli {
    /// Skip vault resolution (op://, vault://, aws://) for offline work
    #[arg(long, global = true)]
    pub offline: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scaffold an envy.yaml template and ignore envy.local.yaml
    Init,

    /// Validate config, prompt for missing keys, then run a command with injected env
    Run {
        /// Stream child output through a secret scanner; kill the process on leak
        #[arg(long)]
        guard: bool,

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

    /// Compare local values against an environment file (envy.<name>.yaml)
    Diff {
        /// Environment name — reads envy.<name>.yaml next to envy.yaml
        env: String,
    },

    /// Interactive configuration doctor: explains problems and offers auto-fixes
    Doctor,

    /// Encrypt envy.local.yaml at rest with an OS-keystore-held key (AES-256-GCM)
    Lock,

    /// Decrypt envy.local.yaml back to plaintext for manual editing
    Unlock,

    /// Manage the pre-commit leak blocker
    Hook {
        #[arg(value_enum)]
        action: HookAction,
    },

    /// Scan code for leaked secrets (staged diff by default)
    Scan {
        /// Scan every file in the tree instead of just the staged diff
        #[arg(long)]
        all: bool,

        /// Explicit staged-diff mode (default); kept for scripts/hooks
        #[arg(long, conflicts_with = "all")]
        staged: bool,
    },

    /// Generate type-safe configuration bindings for your language
    Gen {
        #[arg(value_enum)]
        target: GenTarget,

        /// Output path (defaults to a conventional filename next to envy.yaml)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Package name for Go/Java output
        #[arg(long, default_value = "config")]
        package: String,
    },
}

#[derive(ValueEnum, Clone)]
pub enum HookAction {
    Install,
    Uninstall,
}

#[derive(ValueEnum, Clone)]
pub enum GenTarget {
    #[value(name = "typescript", alias = "ts")]
    TypeScript,
    Go,
    Java,
    Python,
}

impl From<&GenTarget> for envy::gencode::Target {
    fn from(target: &GenTarget) -> Self {
        match target {
            GenTarget::TypeScript => envy::gencode::Target::TypeScript,
            GenTarget::Go => envy::gencode::Target::Go,
            GenTarget::Java => envy::gencode::Target::Java,
            GenTarget::Python => envy::gencode::Target::Python,
        }
    }
}
