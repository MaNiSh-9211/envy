use super::{interactive, load_project_at, report_problems};
use envy::discovery;
use envy::git;
use envy::resolver::{Layers, Options};
use envy::store;
use anyhow::Result;
use colored::Colorize;

pub fn execute(depth: usize, offline: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    let schemas = discovery::find_all_schemas(&root, depth);
    if schemas.is_empty() {
        println!(
            "no {} files found within depth {depth}",
            discovery::SCHEMA_FILE.cyan()
        );
        return Ok(());
    }

    let interactive_session = interactive();
    let (mut filled_total, mut incomplete_services) = (0usize, 0usize);

    for schema_path in &schemas {
        let project = match load_project_at(schema_path) {
            Ok(project) => project,
            Err(err) => {
                eprintln!("{} skipping {}: {err:#}", "!".yellow(), schema_path.display());
                continue;
            }
        };

        let branch = git::current_branch(project.dir());
        let overlay_path = branch
            .as_ref()
            .map(|b| git::overlay_path(project.dir(), b))
            .filter(|path| path.is_file());
        let overlay = match &overlay_path {
            Some(path) => Some(store::load(path)?.values),
            None => None,
        };

        let rel = schema_path
            .strip_prefix(&root)
            .unwrap_or(schema_path)
            .display()
            .to_string();
        println!(
            "\n{} {} {}{}",
            "▶".cyan(),
            project.schema.service_name().bold(),
            rel.dimmed(),
            match (&branch, &overlay_path) {
                (Some(b), Some(_)) => format!(" {}", format!("[{b}]").dimmed()),
                (Some(b), None) => format!(" {}", format!("({b})").dimmed()),
                _ => String::new(),
            }
        );

        let layers = Layers {
            base: &project.base,
            overlay: overlay.as_ref(),
        };
        let opts = Options {
            interactive: interactive_session,
            resolve_vault: !offline,
        };
        let resolved = envy::resolver::resolve(&project.schema, &layers, &opts);
        report_problems(&resolved);

        if !resolved.prompted.is_empty() {
            let target = match (&overlay_path, &overlay) {
                (Some(path), Some(values)) => (path.as_path(), values.clone()),
                _ => (project.local_path.as_path(), project.base.clone()),
            };
            let mut updated = target.1;
            for (key, value, _) in &resolved.prompted {
                updated.insert(key.clone(), serde_yaml::Value::String(value.clone()));
            }
            store::save_smart(target.0, &updated)?;
            println!(
                "{} wrote {} value(s) to {}",
                "✔".green(),
                resolved.prompted.len(),
                target.0.display()
            );
            filled_total += resolved.prompted.len();
        }

        if !resolved.missing.is_empty() {
            incomplete_services += 1;
        }
    }

    println!(
        "\n{} scanned {} service(s), filled {filled_total} value(s)",
        "✔".green().bold(),
        schemas.len()
    );
    if incomplete_services > 0 {
        println!(
            "{} {incomplete_services} service(s) still have missing required values",
            "!".yellow()
        );
    }
    Ok(())
}
