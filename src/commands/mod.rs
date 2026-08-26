pub mod doctor;
pub mod gen;
pub mod init;
pub mod inspect;
pub mod run;
pub mod secops;
pub mod setup;

use crate::discovery;
use crate::git;
use crate::prompt;
use crate::resolver::{self, Layers, Options, Resolved};
use crate::schema::EnvySchema;
use crate::store;
use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub type Values = BTreeMap<String, serde_yaml::Value>;

pub struct Project {
    pub schema_path: PathBuf,
    pub local_path: PathBuf,
    pub schema: EnvySchema,
    pub base: Values,
}

impl Project {
    pub fn dir(&self) -> &Path {
        self.schema_path.parent().unwrap_or(Path::new("."))
    }
}

pub struct App {
    pub project: Project,
    pub branch: Option<String>,
    pub overlay_path: Option<PathBuf>,
    pub overlay: Option<Values>,
}

impl App {
    pub fn layers(&self) -> Layers<'_> {
        Layers {
            base: &self.project.base,
            overlay: self.overlay.as_ref(),
        }
    }

    pub fn resolve(&self, opts: &Options) -> Resolved {
        resolver::resolve(&self.project.schema, &self.layers(), opts)
    }

    /// Where newly prompted values should land: an existing branch overlay wins.
    pub fn save_target(&self) -> (&Path, Option<&Values>) {
        match (&self.overlay_path, &self.overlay) {
            (Some(path), Some(values)) => (path.as_path(), Some(values)),
            _ => (self.project.local_path.as_path(), Some(&self.project.base)),
        }
    }
}

pub fn load_app() -> Result<App> {
    let cwd = std::env::current_dir().context("resolving current directory")?;
    let schema_path = discovery::find_schema_upward(&cwd).with_context(|| {
        format!(
            "no {} found in {} or any parent — run `envy init` first",
            discovery::SCHEMA_FILE,
            cwd.display()
        )
    })?;
    let project = load_project_at(&schema_path)?;
    let branch = git::current_branch(project.dir());
    let overlay_path = branch
        .as_ref()
        .map(|b| git::overlay_path(project.dir(), b))
        .filter(|path| path.is_file());
    let overlay = match &overlay_path {
        Some(path) => Some(store::load(path)?.values),
        None => None,
    };
    Ok(App {
        project,
        branch,
        overlay_path,
        overlay,
    })
}

pub fn load_project_at(schema_path: &Path) -> Result<Project> {
    let schema = EnvySchema::load(schema_path)?;
    let local_path = discovery::local_path_for(schema_path);
    let loaded = store::load(&local_path)?;
    Ok(Project {
        schema_path: schema_path.to_path_buf(),
        local_path,
        schema,
        base: loaded.values,
    })
}

pub fn interactive() -> bool {
    prompt::stdin_is_tty()
}

/// Merge prompted values into the appropriate layer file (encryption-aware).
pub fn persist_prompted(app: &App, resolved: &Resolved) -> Result<bool> {
    if resolved.prompted.is_empty() {
        return Ok(false);
    }
    let (target_path, current) = app.save_target();
    let mut updated: Values = current.cloned().unwrap_or_default();
    for (key, value, _) in &resolved.prompted {
        updated.insert(key.clone(), serde_yaml::Value::String(value.clone()));
    }
    store::save_smart(target_path, &updated)?;
    println!(
        "{} saved {} new value(s) to {}",
        "✔".green(),
        resolved.prompted.len(),
        target_path.display()
    );
    Ok(true)
}

pub fn report_problems(resolved: &Resolved) {
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

pub fn abort_if_broken(resolved: &Resolved) -> bool {
    if !resolved.missing.is_empty() || !resolved.errors.is_empty() {
        println!("{} refusing to continue — fix the problems above first", "!".yellow().bold());
        true
    } else {
        false
    }
}

/// Resolve a bare program name against PATH (handles npm.cmd etc. on Windows).
pub(crate) fn find_in_path(program: &str) -> Option<PathBuf> {
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
