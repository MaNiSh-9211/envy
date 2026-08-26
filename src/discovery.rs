use std::path::{Path, PathBuf};

pub const SCHEMA_FILE: &str = "envy.yaml";
pub const LOCAL_FILE: &str = "envy.local.yaml";

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    "coverage",
    ".next",
    ".turbo",
    ".cache",
];

/// Walk upward from `start` looking for the nearest envy.yaml.
pub fn find_schema_upward(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(SCHEMA_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

pub fn local_path_for(schema_path: &Path) -> PathBuf {
    schema_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(LOCAL_FILE)
}

/// Find every envy.yaml under `root` up to `max_depth`, skipping junk directories.
pub fn find_all_schemas(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(root, 0, max_depth, &mut found);
    found.sort();
    found
}

fn walk(dir: &Path, depth: usize, max_depth: usize, found: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_file() {
            if name == SCHEMA_FILE {
                found.push(path);
            }
        } else if path.is_dir() && depth < max_depth && !SKIP_DIRS.contains(&name.as_str()) {
            walk(&path, depth + 1, max_depth, found);
        }
    }
}
