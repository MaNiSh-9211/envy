use super::{interactive, load_app};
use envy::suggest;
use anyhow::Result;
use colored::Colorize;

pub fn execute(offline: bool) -> Result<()> {
    let app = load_app()?;
    let opts = envy::resolver::Options {
        interactive: interactive(),
        resolve_vault: !offline,
    };
    let resolved = app.resolve(&opts);

    println!(
        "{} {} — {}\n",
        "doctor".dimmed(),
        app.project.schema.service_name().bold(),
        app.project.schema_path.display().to_string().dimmed()
    );

    let mut issues = 0usize;
    let mut fixes_applied = 0usize;

    for error in &resolved.errors {
        issues += 1;
        println!("{} {error}", "✖".red().bold());
    }
    for missing in &resolved.missing {
        issues += 1;
        println!(
            "{} missing required variable {}",
            "!".yellow().bold(),
            missing.yellow()
        );
    }

    if interactive() {
        fixes_applied += offer_value_fixes(&app, &opts)?;
    }

    for warning in &resolved.warnings {
        issues += 1;
        println!("{} {warning}", "·".dimmed());
    }

    if issues == 0 {
        println!("{}", "everything looks healthy — no configuration problems found".green().bold());
    } else {
        println!("\n{} found {issues} issue(s)", "!".yellow().bold());
        if interactive() && fixes_applied > 0 {
            println!("{} applied {fixes_applied} fix(es) to your local config", "✔".green());
        }
    }
    Ok(())
}

fn offer_value_fixes(app: &super::App, opts: &envy::resolver::Options) -> Result<usize> {
    use std::io::{self, BufRead, Write};

    let mut fixed = 0usize;
    for (key, spec) in &app.project.schema.config {
        let Some(raw) = app.resolve(opts).values.get(key).cloned() else {
            continue;
        };
        let problem = match envy::resolver::validate(spec, &raw) {
            Ok(()) => continue,
            Err(problem) => problem,
        };

        let suggestion = match spec.format.as_deref() {
            Some("uri") | Some("url") => suggest::scheme_hint(&raw),
            _ => None,
        }
        .or_else(|| {
            (spec.r#type == "boolean" || spec.r#type == "bool")
                .then(|| suggest::boolean_hint(&raw))
                .flatten()
                .map(str::to_string)
        });

        let Some(fixed_value) = suggestion else {
            println!("{} {key}: {problem}", "✖".red().bold());
            continue;
        };

        print!(
            "\n{} {key}: {problem}\n{} apply fix \"{}\" → \"{}\"? [y/N] ",
            "✖".red().bold(),
            "?".cyan().bold(),
            raw.red(),
            fixed_value.green()
        );
        io::stdout().flush()?;

        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if matches!(answer.trim(), "y" | "Y" | "yes") {
            let (target_path, current) = app.save_target();
            let mut updated: crate::commands::Values =
                current.cloned().unwrap_or_default();
            updated.insert(key.clone(), serde_yaml::Value::String(fixed_value));
            envy::store::save_smart(target_path, &updated)?;
            println!("{} wrote corrected value for {key}", "✔".green());
            fixed += 1;
        }
    }

    for key in local_keys(app) {
        if app.project.schema.config.contains_key(&key) {
            continue;
        }
        let Some(suggestion) = suggest::best_match(&key, app.project.schema.config.keys()) else {
            continue;
        };
        println!(
            "{} unknown key {} in your local file — did you mean {}?",
            "!".yellow().bold(),
            key.yellow(),
            suggestion.cyan()
        );
    }
    Ok(fixed)
}

fn local_keys(app: &super::App) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    keys.extend(app.project.base.keys().cloned());
    if let Some(overlay) = &app.overlay {
        keys.extend(overlay.keys().cloned());
    }
    keys.sort();
    keys.dedup();
    keys
}
