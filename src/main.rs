mod cli;
mod commands;
mod discovery;
mod local;
mod prompt;
mod resolver;
mod schema;

use clap::Parser;
use cli::{Cli, Commands};
use colored::Colorize;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => commands::init().map(|_| 0),
        Commands::Run { command } => commands::run(command),
        Commands::Validate => commands::validate().map(|ok| i32::from(!ok)),
        Commands::List => commands::list().map(|_| 0),
        Commands::Setup { depth } => commands::setup(depth).map(|_| 0),
    };

    match result {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("{} {err:#}", "error:".red().bold());
            std::process::exit(1);
        }
    }
}
