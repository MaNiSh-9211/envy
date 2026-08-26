use super::load_app;
use crate::leakscan;
use anyhow::{anyhow, bail, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

const BLOCK_START: &str = "# >>> envy-leak-guard >>>";
const BLOCK_END: &str = "# <<< envy-leak-guard <<<";

pub fn lock() -> Result<()> {
    let app = load_app()?;
    crate::store::lock(&app.project.local_path)?;
    println!(
        "{} locked {} — key stored in the OS keystore (Windows Credential Manager / macOS Keychain / Secret Service)",
        "🔒".green().bold(),
        app.project.local_path.display()
    );
    println!("{} values are now AES-256-GCM encrypted at rest; envy run/list decrypt in memory", "·".dimmed());
    println!("{} edit secrets with: {}", "·".dimmed(), "envy unlock".cyan());
    Ok(())
}

pub fn unlock() -> Result<()> {
    let app = load_app()?;
    crate::store::unlock(&app.project.local_path)?;
    println!(
        "{} unlocked {} — file is plaintext YAML again",
        "🔓".green().bold(),
        app.project.local_path.display()
    );
    Ok(())
}

pub enum HookAction {
    Install,
    Uninstall,
}

pub fn hook(action: HookAction) -> Result<()> {
    let repo_root = find_repo_root()?;
    let hooks_dir = repo_root.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-commit");

    match action {
        HookAction::Install => install(&hook_path)?,
        HookAction::Uninstall => uninstall(&hook_path)?,
    }
    Ok(())
}

fn install(hook_path: &Path) -> Result<()> {
    let exe = current_exe_for_hook();
    let block = format!(
        "{BLOCK_START}\n\"{exe}\" scan --staged || exit 1\n{BLOCK_END}\n"
    );

    if !hook_path.exists() {
        std::fs::write(hook_path, format!("#!/bin/sh\n\n{block}"))?;
    } else {
        let existing = std::fs::read_to_string(hook_path)?;
        if existing.contains(BLOCK_START) {
            let updated = replace_block(&existing, &block);
            std::fs::write(hook_path, updated)?;
            println!("{} refreshed existing envy leak-guard block", "✔".green());
        } else {
            let mut updated = existing;
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push('\n');
            updated.push_str(&block);
            std::fs::write(hook_path, updated)?;
            println!("{} appended envy leak-guard to pre-commit", "✔".green());
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(hook_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(hook_path, perms)?;
    }

    println!(
        "{} every `git commit` now scans staged changes for leaked secrets",
        "🛡".green().bold()
    );
    Ok(())
}

fn uninstall(hook_path: &Path) -> Result<()> {
    if !hook_path.exists() {
        println!("{} no pre-commit hook present — nothing to remove", "·".dimmed());
        return Ok(());
    }
    let existing = std::fs::read_to_string(hook_path)?;
    if !existing.contains(BLOCK_START) {
        println!("{} no envy block found in pre-commit", "·".dimmed());
        return Ok(());
    }
    let updated = strip_block(&existing);
    std::fs::write(hook_path, updated)?;
    println!("{} envy leak-guard removed from pre-commit", "✔".green());
    Ok(())
}

fn replace_block(content: &str, new_block: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in content.lines() {
        if line.trim() == BLOCK_START {
            inside = true;
            out.push_str(new_block.trim_end());
            continue;
        }
        if line.trim() == BLOCK_END {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn strip_block(content: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in content.lines() {
        if line.trim() == BLOCK_START {
            inside = true;
            continue;
        }
        if line.trim() == BLOCK_END {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn current_exe_for_hook() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string().replace('\\', "/"))
        .unwrap_or_else(|_| "envy".to_string())
}

fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => bail!("not inside a git repository — cannot manage the pre-commit hook"),
        }
    }
}

pub struct ScanArgs {
    pub all: bool,
}

pub fn execute_scan(args: ScanArgs, offline: bool) -> Result<i32> {
    let app = load_app().ok();

    let declared: Vec<(String, String)> = match &app {
        Some(app) => {
            let opts = crate::resolver::Options {
                interactive: false,
                resolve_vault: !offline,
            };
            let resolved = app.resolve(&opts);
            leakscan::collect_secrets(&app.project.schema, &resolved.values)
        }
        None => Vec::new(),
    };

    if declared.is_empty() && args.all {
        println!(
            "{} no secret values found in config — scanning for known token patterns only",
            "·".dimmed()
        );
    }

    let findings = if args.all {
        let root = std::env::current_dir()?;
        leakscan::scan_all(&root, &declared)
    } else {
        let root = find_repo_root()?;
        let staged = leakscan::staged_diff_files(&root)?;
        if staged.is_empty() {
            println!("{} nothing staged — nothing to scan", "·".dimmed());
            return Ok(0);
        }
        let mut findings = Vec::new();
        for (file, line) in &staged {
            leakscan::scan_text(line, &declared, file, &mut findings);
        }
        findings
    };

    if findings.is_empty() {
        println!(
            "{} no leaks detected across {} guarded value(s)",
            "✔".green().bold(),
            declared.len()
        );
        return Ok(0);
    }

    eprintln!("{}", "── ENVY LEAK SCAN ──────────────────────────".red().bold());
    for finding in &findings {
        eprintln!(
            "{} {} {}:{}  {}",
            "🚨".red().bold(),
            finding.label.red(),
            finding.file.yellow(),
            finding.line,
            finding.preview.dimmed()
        );
    }
    eprintln!(
        "{} {} leak(s) found — commit blocked. Remove the secret(s) and stage again.",
        "✖".red().bold(),
        findings.len()
    );
    Err(anyhow!("secrets detected in staged changes"))
}
