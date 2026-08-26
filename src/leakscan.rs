use std::path::{Path, PathBuf};

const KNOWN_PATTERNS: &[(&str, &str)] = &[
    ("sk_live_", "likely live Stripe key"),
    ("AKIA", "likely AWS access key id"),
    ("ghp_", "likely GitHub token"),
    ("xoxb-", "likely Slack bot token"),
    ("xoxp-", "likely Slack user token"),
    ("AIza", "likely Google API key"),
];

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;
const SCAN_DEPTH: usize = 8;

#[derive(Debug)]
pub struct Finding {
    pub label: String,
    pub file: String,
    pub line: usize,
    pub preview: String,
}

fn preview_of(line: &str) -> String {
    let trimmed = line.trim();
    let cut = trimmed.char_indices().nth(60).map(|(i, _)| i).unwrap_or(trimmed.len());
    trimmed[..cut].replace('\t', " ")
}

fn scan_line(line: &str, secrets: &[(String, String)]) -> Option<(String, &'static str)> {
    for (var, value) in secrets {
        if value.len() >= 8 && line.contains(value.as_str()) {
            return Some((var.clone(), "declared secret"));
        }
    }
    for (needle, description) in KNOWN_PATTERNS {
        if let Some(pos) = line.find(needle) {
            let tail = &line[pos + needle.len()..];
            let token_len = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').count();
            if token_len >= 16 {
                return Some((format!("{needle}…"), description));
            }
        }
    }
    None
}

pub fn scan_text(text: &str, secrets: &[(String, String)], file_label: &str, findings: &mut Vec<Finding>) {
    for (idx, line) in text.lines().enumerate() {
        if let Some((label, kind)) = scan_line(line, secrets) {
            findings.push(Finding {
                label: format!("{label} ({kind})"),
                file: file_label.to_string(),
                line: idx + 1,
                preview: preview_of(line),
            });
        }
    }
}

pub fn collect_secrets(schema: &crate::schema::EnvySchema, values: &std::collections::BTreeMap<String, String>) -> Vec<(String, String)> {
    schema
        .config
        .iter()
        .filter(|(_, spec)| spec.secret)
        .filter_map(|(key, _)| values.get(key).map(|v| (key.clone(), v.clone())))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

pub fn staged_diff_files(project_root: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["diff", "--cached", "-U0", "--no-color"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git diff --cached failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    let mut current_file = String::new();
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current_file = path.to_string();
        } else if let Some(added) = line.strip_prefix('+') {
            result.push((current_file.clone(), added.to_string()));
        }
    }
    Ok(result)
}

pub fn scan_all(root: &Path, secrets: &[(String, String)]) -> Vec<Finding> {
    let mut files = Vec::new();
    walk(root, 0, &mut files);
    let mut findings = Vec::new();
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if bytes.is_empty() || bytes[..bytes.len().min(1024)].contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");
        scan_text(&text, secrets, &rel, &mut findings);
    }
    findings
}

fn walk(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > SCAN_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() {
            if meta.len() <= MAX_FILE_SIZE {
                files.push(path);
            }
        } else if meta.is_dir() && !crate::discovery::SKIP_DIRS.contains(&name.as_str()) {
            walk(&path, depth + 1, files);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> Vec<(String, String)> {
        vec![("API_KEY".to_string(), "supersecret123".to_string())]
    }

    #[test]
    fn finds_exact_secret() {
        let mut findings = Vec::new();
        scan_text("let key = \"supersecret123\";", &secrets(), "app.js", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "app.js");
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn short_values_ignored_to_avoid_noise() {
        let mut findings = Vec::new();
        scan_text("port 8080", &[("PORT".into(), "8080".into())], "f", &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn known_token_patterns_detected() {
        let mut findings = Vec::new();
        scan_text(
            "const gh = \"ghp_0123456789abcdefghijklmnopqrstuvwxyz\";",
            &[],
            "ci.ts",
            &mut findings,
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].label.contains("GitHub"));
    }

    #[test]
    fn clean_text_has_no_findings() {
        let mut findings = Vec::new();
        scan_text("hello world", &secrets(), "f.txt", &mut findings);
        assert!(findings.is_empty());
    }
}
