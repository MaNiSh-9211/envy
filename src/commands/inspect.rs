use super::{load_app, report_problems};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;

pub fn validate(offline: bool) -> Result<bool> {
    let app = load_app()?;
    let resolved = app.resolve(&crate::resolver::Options {
        interactive: false,
        resolve_vault: !offline,
    });

    println!(
        "{} {} — {}\n",
        "validating".dimmed(),
        app.project.schema.service_name().bold(),
        app.project.schema_path.display().to_string().dimmed()
    );
    report_problems(&resolved);

    if resolved.errors.is_empty() && resolved.missing.is_empty() {
        println!(
            "{} {} variable(s) validated successfully",
            "✔".green().bold(),
            app.project.schema.config.len()
        );
        Ok(true)
    } else {
        println!("{}", "validation failed".red().bold());
        Ok(false)
    }
}

pub fn list(offline: bool) -> Result<()> {
    let app = load_app()?;
    let resolved = app.resolve(&crate::resolver::Options {
        interactive: false,
        resolve_vault: !offline,
    });
    report_problems(&resolved);

    if let Some(branch) = &app.branch {
        println!(
            "{} branch: {}{}",
            "·".dimmed(),
            branch.cyan(),
            if app.overlay_path.is_some() {
                format!(" {}", "(branch overlay active)".dimmed())
            } else {
                String::new()
            }
        );
    }

    let width = app.project.schema.config.keys().map(String::len).max().unwrap_or(4);
    println!("{}", app.project.schema.service_name().bold());
    for (key, spec) in &app.project.schema.config {
        let shown = match resolved.values.get(key) {
            Some(_value) if spec.secret => "********".to_string(),
            Some(value) => value.clone(),
            None => "(not set)".dimmed().to_string(),
        };
        let source = resolved.sources.get(key).copied().unwrap_or("-");
        println!(
            "  {:<width$} = {:<28} {}",
            key.cyan(),
            shown,
            format!("[{source}]").dimmed()
        );
    }
    Ok(())
}

pub struct DiffArgs {
    pub env: String,
}

pub fn diff(args: DiffArgs, offline: bool) -> Result<()> {
    let app = load_app()?;
    let local_resolved = app.resolve(&crate::resolver::Options {
        interactive: false,
        resolve_vault: !offline,
    });
    report_problems(&local_resolved);

    let remote_path: PathBuf = app
        .project
        .dir()
        .to_path_buf()
        .join(format!("envy.{}.yaml", args.env));
    let remote = crate::store::load(&remote_path).with_context(|| {
        format!(
            "loading environment '{}' (expected {})",
            args.env,
            remote_path.display()
        )
    })?;

    println!(
        "{} local ↔ {}\n",
        "diffing".dimmed(),
        remote_path.display().to_string().cyan()
    );

    let width = app
        .project
        .schema
        .config
        .keys()
        .map(String::len)
        .max()
        .unwrap_or(4);

    let mut matches = 0usize;
    let mut mismatches = 0usize;
    let mut only_local = 0usize;
    let mut only_env = 0usize;

    println!("  {:<width$}  {:<14} {:<14}", "VARIABLE", "LOCAL", &args.env.to_uppercase());
    for (key, spec) in &app.project.schema.config {
        let local_value = local_resolved.values.get(key);
        let remote_value = remote
            .values
            .get(key)
            .and_then(|v| crate::resolver::scalar_to_string(v).ok());

        let mask = |v: Option<&String>| -> String {
            match v {
                Some(_raw) if spec.secret => "********".to_string(),
                Some(raw) => raw.clone(),
                None => "(absent)".dimmed().to_string(),
            }
        };

        let status = match (local_value, &remote_value) {
            (Some(a), Some(b)) if a == b => {
                matches += 1;
                "=".green().to_string()
            }
            (Some(_), Some(_)) => {
                mismatches += 1;
                "≠".red().bold().to_string()
            }
            (Some(_), None) => {
                only_local += 1;
                "<".yellow().to_string()
            }
            _ => {
                only_env += 1;
                ">".yellow().to_string()
            }
        };

        println!(
            "  {:<width$}  {:<14} {:<14} {}",
            key.cyan(),
            mask(local_value),
            mask(remote_value.as_ref()),
            status
        );
    }

    println!(
        "\n{} equal · {} differ · {} only local · {} only in env file",
        matches.to_string().green(),
        mismatches.to_string().red(),
        only_local.to_string().yellow(),
        only_env.to_string().yellow()
    );
    Ok(())
}
