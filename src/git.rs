use std::path::{Path, PathBuf};

/// Detect the current git branch for the repo containing `start`.
pub fn current_branch(start: &Path) -> Option<String> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let dot = d.join(".git");
        if dot.is_dir() {
            return parse_head(&dot);
        }
        if dot.is_file() {
            let content = std::fs::read_to_string(&dot).ok()?;
            let target = content.trim().strip_prefix("gitdir:")?.trim();
            let gitdir = if Path::new(target).is_absolute() {
                PathBuf::from(target)
            } else {
                d.join(target)
            };
            return parse_head(&gitdir);
        }
        dir = d.parent();
    }
    None
}

fn parse_head(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let branch = head.trim().strip_prefix("ref: refs/heads/")?;
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

pub fn sanitize_branch(branch: &str) -> String {
    branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub fn overlay_filename(branch: &str) -> String {
    format!("envy.local.{}.yaml", sanitize_branch(branch))
}

pub fn overlay_path(schema_dir: &Path, branch: &str) -> PathBuf {
    schema_dir.join(overlay_filename(branch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_branch_names() {
        assert_eq!(sanitize_branch("feat/awesome_ui v2"), "feat-awesome_ui-v2");
        assert_eq!(sanitize_branch("main"), "main");
    }

    #[test]
    fn overlay_filename_matches_sanitization() {
        assert_eq!(overlay_filename("feat/x"), "envy.local.feat-x.yaml");
    }
}
