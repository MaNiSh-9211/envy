use super::{abort_if_broken, find_in_path, interactive, load_app, report_problems};
use crate::guard;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

pub struct RunArgs {
    pub guard: bool,
    pub command: Vec<OsString>,
}

pub fn execute(args: RunArgs, offline: bool) -> Result<i32> {
    if args.command.is_empty() {
        bail!("nothing to run — usage: envy run [--guard] <command> [args...]");
    }

    let app = load_app()?;
    let opts = envy::resolver::Options {
        interactive: interactive(),
        resolve_vault: !offline,
    };
    let resolved = app.resolve(&opts);

    super::persist_prompted(&app, &resolved)?;
    report_problems(&resolved);
    if abort_if_broken(&resolved) {
        return Ok(1);
    }

    let display = args
        .command
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let mode = if args.guard { " (guard on)" } else { "" };
    println!(
        "{} injecting {} variable(s) into `{}`{mode}",
        "·".dimmed(),
        resolved.values.len(),
        display.cyan()
    );

    if let Some(branch) = &app.branch {
        if app.overlay_path.is_some() {
            println!("{} branch overlay active: {}", "·".dimmed(), branch.cyan());
        }
    }

    if args.guard {
        let secret_names: Vec<String> = app
            .project
            .schema
            .config
            .iter()
            .filter(|(_, spec)| spec.secret)
            .map(|(key, _)| key.clone())
            .collect();
        guard::run(&args.command, &resolved.values, &secret_names)
    } else {
        launch(&args.command, &resolved.values)
    }
}

fn launch(command: &[OsString], vars: &BTreeMap<String, String>) -> Result<i32> {
    use std::process::Command;

    let program_str = command[0].to_string_lossy().to_string();
    let program = find_in_path(&program_str).unwrap_or_else(|| PathBuf::from(&command[0]));

    let mut cmd = Command::new(program);
    cmd.args(&command[1..]);
    for (key, value) in vars {
        cmd.env(key, value);
    }

    let status = cmd.status().with_context(|| format!("spawning `{program_str}`"))?;

    Ok(match status.code() {
        Some(code) => code,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    return Ok(128 + signal);
                }
            }
            1
        }
    })
}
