use crate::prompt;
use crate::schema::{EnvySchema, VarSpec};
use std::collections::BTreeMap;

pub const SOURCE_ENV: &str = "env";
pub const SOURCE_LOCAL: &str = "local";
pub const SOURCE_DEFAULT: &str = "default";
pub const SOURCE_PROMPTED: &str = "prompted";

#[derive(Default)]
pub struct Resolved {
    pub values: BTreeMap<String, String>,
    pub sources: BTreeMap<String, &'static str>,
    pub missing: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// (key, raw value, is_secret) newly captured from the user this session
    pub prompted: Vec<(String, String, bool)>,
}

pub fn resolve(
    schema: &EnvySchema,
    local: &BTreeMap<String, serde_yaml::Value>,
    interactive: bool,
) -> Resolved {
    let mut out = Resolved::default();

    for (key, spec) in &schema.config {
        let mut placed: Option<(String, &'static str)> = None;

        if let Ok(raw) = std::env::var(key) {
            placed = Some((raw, SOURCE_ENV));
        } else if let Some(node) = local.get(key) {
            match scalar_to_string(node) {
                Ok(raw) => placed = Some((raw, SOURCE_LOCAL)),
                Err(reason) => out.errors.push(format!("{key}: {reason}")),
            }
        }

        if placed.is_none() {
            if let Some(def) = &spec.default {
                match scalar_to_string(def) {
                    Ok(raw) => placed = Some((raw, SOURCE_DEFAULT)),
                    Err(reason) => out.errors.push(format!("{key}: bad default — {reason}")),
                }
            }
        }

        if placed.is_none() && spec.required {
            if interactive {
                match prompt_for(key, spec, &mut out.errors) {
                    Some(value) => {
                        out.values.insert(key.clone(), value.clone());
                        out.sources.insert(key.clone(), SOURCE_PROMPTED);
                        out.prompted.push((key.clone(), value, spec.secret));
                        continue;
                    }
                    None => {
                        out.missing.push(key.clone());
                        continue;
                    }
                }
            }
            out.missing.push(key.clone());
            continue;
        }

        if let Some((raw, source)) = placed {
            if let Err(problem) = validate(spec, &raw) {
                out.errors.push(format!("{key}: {problem}"));
            }
            out.values.insert(key.clone(), raw);
            out.sources.insert(key.clone(), source);
        }
    }

    for key in local.keys() {
        if !schema.config.contains_key(key) {
            out.warnings.push(format!(
                "{key} is set locally but not declared in envy.yaml (typo? or add it to the schema)"
            ));
        }
    }

    out
}

fn prompt_for(
    key: &str,
    spec: &VarSpec,
    errors: &mut Vec<String>,
) -> Option<String> {
    for attempt in 1..=3 {
        match prompt::ask(key, spec) {
            Ok(Some(raw)) => {
                if raw.is_empty() {
                    return None;
                }
                match validate(spec, &raw) {
                    Ok(()) => return Some(raw),
                    Err(problem) => errors.push(format!(
                        "{key}: {problem} (attempt {attempt}/3 — press Enter to skip)"
                    )),
                }
            }
            Ok(None) => return None,
            Err(err) => {
                errors.push(format!("{key}: failed to read input ({err})"));
                return None;
            }
        }
    }
    None
}

pub fn scalar_to_string(value: &serde_yaml::Value) -> Result<String, String> {
    match value {
        serde_yaml::Value::String(s) => Ok(s.clone()),
        serde_yaml::Value::Bool(b) => Ok(b.to_string()),
        serde_yaml::Value::Number(n) => Ok(n.to_string()),
        serde_yaml::Value::Null => Err("value is null".into()),
        other => Err(format!("expected a scalar, found {}", describe(other))),
    }
}

fn describe(value: &serde_yaml::Value) -> &'static str {
    match value {
        serde_yaml::Value::Sequence(_) => "a list",
        serde_yaml::Value::Mapping(_) => "a mapping",
        serde_yaml::Value::Tagged(_) => "a tagged value",
        _ => "an unknown node",
    }
}

pub fn validate(spec: &VarSpec, raw: &str) -> Result<(), String> {
    check_type(spec, raw)?;
    check_format(spec, raw)
}

fn check_type(spec: &VarSpec, raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    match spec.r#type.as_str() {
        "integer" => trimmed
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| format!("expected an integer, got \"{raw}\"")),
        "number" | "float" => trimmed
            .parse::<f64>()
            .map(|_| ())
            .map_err(|_| format!("expected a number, got \"{raw}\"")),
        "boolean" | "bool" => match trimmed.to_ascii_lowercase().as_str() {
            "true" | "false" | "1" | "0" | "yes" | "no" | "on" | "off" => Ok(()),
            _ => Err(format!("expected a boolean (true/false), got \"{raw}\"")),
        },
        _ => Ok(()),
    }
}

fn check_format(spec: &VarSpec, raw: &str) -> Result<(), String> {
    let Some(format) = spec.format.as_deref() else {
        return Ok(());
    };
    let ok = match format {
        "uri" | "url" => {
            let has_scheme = raw.split_once("://").is_some_and(|(scheme, rest)| {
                !scheme.is_empty()
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
                    && !rest.contains(char::is_whitespace)
            });
            has_scheme && !raw.contains(char::is_whitespace)
        }
        "email" => match raw.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
            }
            None => false,
        },
        "uuid" => {
            let parts: Vec<&str> = raw.split('-').collect();
            parts.len() == 5
                && [8, 4, 4, 4, 12]
                    .iter()
                    .zip(&parts)
                    .all(|(len, part)| part.len() == *len && part.chars().all(|c| c.is_ascii_hexdigit()))
        }
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("does not satisfy format '{format}': \"{raw}\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::VarSpec;

    fn spec(yaml: &str) -> VarSpec {
        serde_yaml::from_str(yaml).expect("valid spec yaml")
    }

    #[test]
    fn integers_are_validated() {
        assert!(check_type(&spec("{type: integer}"), "8080").is_ok());
        assert!(check_type(&spec("{type: integer}"), "-42").is_ok());
        assert!(check_type(&spec("{type: integer}"), "three-thousand").is_err());
    }

    #[test]
    fn booleans_are_validated() {
        assert!(check_type(&spec("{type: boolean}"), "true").is_ok());
        assert!(check_type(&spec("{type: boolean}"), "FALSE").is_ok());
        assert!(check_type(&spec("{type: boolean}"), "maybe").is_err());
    }

    #[test]
    fn numbers_are_validated() {
        assert!(check_type(&spec("{type: number}"), "3.14").is_ok());
        assert!(check_type(&spec("{type: number}"), "pi").is_err());
    }

    #[test]
    fn uri_format_is_checked() {
        let s = spec("{type: string, format: uri}");
        assert!(check_format(&s, "postgresql://user@localhost:5432/db").is_ok());
        assert!(check_format(&s, "localhost:5432/db").is_err());
        assert!(check_format(&s, "has space://x").is_err());
    }

    #[test]
    fn email_and_uuid_formats() {
        let e = spec("{type: string, format: email}");
        assert!(check_format(&e, "dev@example.com").is_ok());
        assert!(check_format(&e, "nope").is_err());

        let u = spec("{type: string, format: uuid}");
        assert!(check_format(&u, "123e4567-e89b-12d3-a456-426614174000").is_ok());
        assert!(check_format(&u, "not-a-uuid").is_err());
    }

    #[test]
    fn scalars_convert_cleanly() {
        let t = |s: &str| -> serde_yaml::Value { serde_yaml::from_str(s).unwrap() };
        assert_eq!(scalar_to_string(&t("true")).unwrap(), "true");
        assert_eq!(scalar_to_string(&t("8080")).unwrap(), "8080");
        assert_eq!(scalar_to_string(&t("hello")).unwrap(), "hello");
        assert!(scalar_to_string(&t("[a]")).is_err());
    }
}
