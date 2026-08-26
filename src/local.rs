use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

type Values = BTreeMap<String, serde_yaml::Value>;

#[derive(Serialize)]
pub struct LocalFileRef<'a> {
    pub values: &'a Values,
}

#[derive(Deserialize)]
struct LocalFileOwned {
    values: Values,
}

#[allow(dead_code)]
pub fn load_plain(path: &Path) -> Result<Values> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn save(path: &Path, values: &Values) -> Result<()> {
    let mut out =
        String::from("# Managed by envy — local overrides & secrets. NEVER commit this file.\n");
    let body = serde_yaml::to_string(&LocalFileRef { values })?;
    out.push_str(body.trim_start_matches("---\n").trim_end());
    out.push('\n');
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}

pub fn parse_str(text: &str) -> Result<Values> {
    let doc: serde_yaml::Value = serde_yaml::from_str(text)?;
    if doc.is_null() {
        return Ok(BTreeMap::new());
    }

    if let Ok(with) = serde_yaml::from_value::<LocalFileOwned>(doc.clone()) {
        return Ok(with.values);
    }

    let flat = serde_yaml::from_value::<Values>(doc)
        .context("expected a `values:` mapping or a flat KEY: value mapping")?;
    Ok(flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_values_form() {
        let parsed =
            parse_str("values:\n  DATABASE_URL: \"postgres://x\"\n  PORT: 3000\n").expect("parses");
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains_key("DATABASE_URL"));
    }

    #[test]
    fn parses_flat_form() {
        let parsed = parse_str("DATABASE_URL: postgres://x\nPORT: 3000\n").expect("parses");
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn empty_file_is_empty_map() {
        let parsed = parse_str("").expect("parses");
        assert!(parsed.is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_str("- just\n- a list\n").is_err());
    }
}
