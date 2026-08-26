//! Smart tracking: notices when teammates add or remove variables in
//! `envy.yaml` since the last local run, so a fresh pull never surprises you.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const STATE_DIR: &str = ".envy";
const STATE_FILE: &str = "state.json";

#[derive(Serialize, Deserialize, Default)]
struct State {
    schema_hash: String,
    keys: Vec<String>,
}

pub struct Drift {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Drift {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

fn state_path(project_dir: &Path) -> PathBuf {
    project_dir.join(STATE_DIR).join(STATE_FILE)
}

fn hash_schema(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Compare the last-seen schema snapshot with the current one.
pub fn check(
    project_dir: &Path,
    schema_text: &str,
    current_keys: &BTreeMap<String, crate::schema::VarSpec>,
) -> anyhow::Result<Option<Drift>> {
    let path = state_path(project_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let state: State = serde_json::from_str(&text).unwrap_or_default();

    let old_keys: Vec<String> = state.keys;
    let new_keys: Vec<String> = current_keys.keys().cloned().collect();
    let (added, removed) = diff_keys(&old_keys, &new_keys);

    let _ = hash_schema(schema_text); // kept for future content-level drift

    Ok(if added.is_empty() && removed.is_empty() && state.schema_hash == hash_schema(schema_text) {
        None
    } else {
        Some(Drift { added, removed })
    })
}

/// Persist the current schema snapshot after a successful command.
pub fn update(
    project_dir: &Path,
    schema_text: &str,
    keys: &BTreeMap<String, crate::schema::VarSpec>,
) -> anyhow::Result<()> {
    let dir = project_dir.join(STATE_DIR);
    std::fs::create_dir_all(&dir)?;
    let state = State {
        schema_hash: hash_schema(schema_text),
        keys: keys.keys().cloned().collect(),
    };
    std::fs::write(state_path(project_dir), serde_json::to_string(&state)?)?;
    Ok(())
}

pub fn diff_keys(old: &[String], new: &[String]) -> (Vec<String>, Vec<String>) {
    let old_set: std::collections::BTreeSet<&String> = old.iter().collect();
    let new_set: std::collections::BTreeSet<&String> = new.iter().collect();
    let added = new.iter().filter(|k| !old_set.contains(k)).cloned().collect();
    let removed = old.iter().filter(|k| !new_set.contains(k)).cloned().collect();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_added_and_removed() {
        let old = keys(&["PORT", "OLD_KEY"]);
        let new = keys(&["PORT", "NEW_KEY"]);
        let (added, removed) = diff_keys(&old, &new);
        assert_eq!(added, vec!["NEW_KEY".to_string()]);
        assert_eq!(removed, vec!["OLD_KEY".to_string()]);
    }

    #[test]
    fn identical_sets_are_clean() {
        let same = keys(&["A", "B"]);
        let (added, removed) = diff_keys(&same, &same);
        assert!(added.is_empty());
        assert!(removed.is_empty());
    }

    #[test]
    fn first_run_has_no_drift() {
        // no state file -> check returns None (handled by caller via is_file guard)
        let dir = std::env::temp_dir().join("envy-drift-none-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            "K".to_string(),
            serde_yaml::from_str::<crate::schema::VarSpec>("{type: string}").unwrap(),
        );
        let result = check(&dir, "version: \"1\"\n", &map).unwrap();
        assert!(result.is_none());
    }
}
