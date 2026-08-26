mod cli;
mod commands;
mod guard;

use clap::Parser;
use cli::{Cli, Commands};
use colored::Colorize;

fn main() {
    let cli = Cli::parse();
    let offline = cli.offline;

    let result = match cli.command {
        Commands::Init => commands::init::execute().map(|_| 0),
        Commands::Run { guard, command } => {
            commands::run::execute(commands::run::RunArgs { guard, command }, offline)
        }
        Commands::Validate => commands::inspect::validate(offline).map(|ok| i32::from(!ok)),
        Commands::List => commands::inspect::list(offline).map(|_| 0),
        Commands::Setup { depth } => commands::setup::execute(depth, offline).map(|_| 0),
        Commands::Diff { env } => commands::inspect::diff(commands::inspect::DiffArgs { env }, offline).map(|_| 0),
        Commands::Doctor => commands::doctor::execute(offline).map(|_| 0),
        Commands::Lock => commands::secops::lock().map(|_| 0),
        Commands::Unlock => commands::secops::unlock().map(|_| 0),
        Commands::Hook { action } => commands::secops::hook(match action {
            cli::HookAction::Install => commands::secops::HookAction::Install,
            cli::HookAction::Uninstall => commands::secops::HookAction::Uninstall,
        })
        .map(|_| 0),
        Commands::Scan { all, .. } => commands::secops::execute_scan(commands::secops::ScanArgs { all }, offline),
        Commands::Gen { target, out, package } => commands::gen::execute(commands::gen::GenArgs {
            target: (&target).into(),
            out,
            package,
        })
        .map(|_| 0),
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
