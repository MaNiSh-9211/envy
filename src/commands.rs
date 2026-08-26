use crate::{discovery, local, prompt, resolver, schema::EnvySchema};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

type Values = BTreeMap<String, serde_yaml::Value>;

const TEMPLATE: &str = include_str!("template.envy.yaml");

struct Project {
    schema_path: PathBuf,
    local_path: PathBuf,
    schema: EnvySchema,
    values: Values,
}

pub fn init() -> Result<()> {
    let cwd = std::env::current_dir().context("resolving current directory")?;
    let schema_path = cwd.join(discovery::SCHEMA_FILE);

    if schema_path.exists() {
        bail!("{} already exists — refusing to overwrite", schema_path.display());
    }

    std::fs::write(&schema_path, TEMPLATE)
        .with_context(|| format!("writing {}", schema_path.display()))?;
    println!("{}", "Created envy.yaml".green().bold());

    let gitignore = cwd.join(".gitignore");
    let mut contents = std::fs::read_to_string(&gitignore).unwrap_or_default();
    if !contents.lines().any(|line| line.trim() == discovery::LOCAL_FILE) {
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str("\n# envy local secrets\nenvy.local.yaml\n");
        std::fs::write(&gitignore, contents).with_context(|| format!("writing {}", gitignore.display()))?;
        println!("{} added {} to .gitignore", "·".dimmed(), discovery::LOCAL_FILE);
    }

    println!("\nNext steps:");
    println!("  1. Edit {} to declare every variable your app needs", "envy.yaml".cyan());
    println!("  2. Put real secrets in {} (never committed)", "envy.local.yaml".cyan());
    println!("  3. Start anything with {}", "envy run <your-command>".cyan());
    Ok(())
}

pub fn run(command: Vec<OsString>) -> Result<i32> {
    if command.is_empty() {
        bail!("nothing to run — usage: envy run <command> [args...]");
    }

    let project = find_project()?;
    let resolved = resolver::resolve(&project.schema, &project.values, prompt::stdin_is_tty());

    if !resolved.prompted.is_empty() {
        let mut updated = project.values.clone();
        for (key, value, _) in &resolved.prompted {
            updated.insert(key.clone(), serde_yaml::Value::String(value.clone()));
        }
        local::save(&project.local_path, &updated)?;
        println!(
            "{} saved {} new value(s) to {}",
            "✔".green(),
            resolved.prompted.len(),
            project.local_path.display()
        );
    }

    report_problems(&resolved);
    if !resolved.missing.is_empty() || !resolved.errors.is_empty() {
        println!("{} refusing to start — fix the problems above first", "!".yellow().bold());
        return Ok(1);
    }

    let display = command
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    println!(
        "{} injecting {} variable(s) into `{}`",
        "·".dimmed(),
        resolved.values.len(),
        display.cyan()
    );

    launch(&command, &resolved.values)
}

pub fn validate() -> Result<bool> {
    let project = find_project()?;
    let resolved = resolver::resolve(&project.schema, &project.values, false);

    println!(
        "{} {} — {}\n",
        "validating".dimmed(),
        project.schema.service_name().bold(),
        project.schema_path.display().to_string().dimmed()
    );
    report_problems(&resolved);

    if resolved.errors.is_empty() && resolved.missing.is_empty() {
        println!(
            "{} {} variable(s) validated successfully",
            "✔".green().bold(),
            project.schema.config.len()
        );
        Ok(true)
    } else {
        println!("{}", "validation failed".red().bold());
        Ok(false)
    }
}

pub fn list() -> Result<()> {
    let project = find_project()?;
    let resolved = resolver::resolve(&project.schema, &project.values, false);
    report_problems(&resolved);

    let width = project.schema.config.keys().map(String::len).max().unwrap_or(4);
    println!("{}", project.schema.service_name().bold());
    for (key, spec) in &project.schema.config {
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

pub fn setup(depth: usize) -> Result<()> {
    let root = std::env::current_dir().context("resolving current directory")?;
    let schemas = discovery::find_all_schemas(&root, depth);
    if schemas.is_empty() {
        println!(
            "no {} files found within depth {depth}",
            discovery::SCHEMA_FILE.cyan()
        );
        return Ok(());
    }

    let interactive = prompt::stdin_is_tty();
    let (mut filled_total, mut incomplete_services) = (0usize, 0usize);

    for schema_path in &schemas {
        let project = match load_project_at(schema_path) {
            Ok(project) => project,
            Err(err) => {
                eprintln!("{} skipping {}: {err:#}", "!".yellow(), schema_path.display());
                continue;
            }
        };

        let rel = schema_path
            .strip_prefix(&root)
            .unwrap_or(schema_path)
            .display()
            .to_string();
        println!(
            "\n{} {} {}",
            "▶".cyan(),
            project.schema.service_name().bold(),
            rel.dimmed()
        );

        let resolved = resolver::resolve(&project.schema, &project.values, interactive);
        report_problems(&resolved);

        if !resolved.prompted.is_empty() {
            let mut updated = project.values.clone();
            for (key, value, _) in &resolved.prompted {
                updated.insert(key.clone(), serde_yaml::Value::String(value.clone()));
            }
            local::save(&project.local_path, &updated)?;
            println!(
                "{} wrote {} value(s) to {}",
                "✔".green(),
                resolved.prompted.len(),
                project.local_path.display()
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

// ---------- internals ----------

fn find_project() -> Result<Project> {
    let cwd = std::env::current_dir().context("resolving current directory")?;
    let schema_path = discovery::find_schema_upward(&cwd).with_context(|| {
        format!(
            "no {} found in {} or any parent — run `envy init` first",
            discovery::SCHEMA_FILE,
            cwd.display()
        )
    })?;
    load_project_at(&schema_path)
}

fn load_project_at(schema_path: &Path) -> Result<Project> {
    let schema = EnvySchema::load(schema_path)?;
    let local_path = discovery::local_path_for(schema_path);
    let values = if local_path.is_file() {
        local::load(&local_path)?
    } else {
        BTreeMap::new()
    };
    Ok(Project {
        schema_path: schema_path.to_path_buf(),
        local_path,
        schema,
        values,
    })
}

fn report_problems(resolved: &resolver::Resolved) {
    for error in &resolved.errors {
        println!("{} {error}", "✖".red().bold());
    }
    for missing in &resolved.missing {
        println!(
            "{} missing required variable {} — add it to {} or export it, then retry",
            "!".yellow().bold(),
            missing.yellow(),
            discovery::LOCAL_FILE.cyan()
        );
    }
    for warning in &resolved.warnings {
        println!("{} {warning}", "·".dimmed());
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

/// Resolve a bare program name against PATH (handles npm.cmd etc. on Windows).
fn find_in_path(program: &str) -> Option<PathBuf> {
    let as_path = PathBuf::from(program);
    if as_path.components().count() > 1 {
        return as_path.is_file().then_some(as_path);
    }

    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for ext in &extensions {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
