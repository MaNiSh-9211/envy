use envy::discovery;
use anyhow::{bail, Context, Result};
use colored::Colorize;

const TEMPLATE: &str = include_str!("../template.envy.yaml");

pub fn execute() -> Result<()> {
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
        std::fs::write(&gitignore, contents)
            .with_context(|| format!("writing {}", gitignore.display()))?;
        println!("{} added {} to .gitignore", "·".dimmed(), discovery::LOCAL_FILE);
    }

    println!("\nNext steps:");
    println!("  1. Edit {} to declare every variable your app needs", "envy.yaml".cyan());
    println!("  2. Put real secrets in {} (never committed)", "envy.local.yaml".cyan());
    println!("  3. Lock it down with {} once you're ready", "envy lock".cyan());
    println!("  4. Start anything with {}", "envy run <your-command>".cyan());
    Ok(())
}
