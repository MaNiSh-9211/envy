use crate::prompt;
use crate::schema::{EnvySchema, VarSpec};
use crate::suggest;
use crate::vault;
use std::collections::BTreeMap;

pub const SOURCE_ENV: &str = "env";
pub const SOURCE_LOCAL: &str = "local";
pub const SOURCE_OVERLAY: &str = "overlay";
pub const SOURCE_DEFAULT: &str = "default";
pub const SOURCE_MOCK: &str = "mock";
pub const SOURCE_VAULT: &str = "vault";
pub const SOURCE_PROMPTED: &str = "prompted";

type Values = BTreeMap<String, serde_yaml::Value>;

pub struct Layers<'a> {
    pub base: &'a Values,
    pub overlay: Option<&'a Values>,
}

pub struct Options {
    pub interactive: bool,
    pub resolve_vault: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            interactive: false,
            resolve_vault: true,
        }
    }
}

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

impl Resolved {
    #[allow(dead_code)]
    pub fn secret_names(&self) -> Vec<String> {
        self.sources
            .iter()
            .filter(|(_, source)| **source == SOURCE_VAULT)
            .map(|(k, _)| k.clone())
            .collect()
    }
}

pub fn resolve(schema: &EnvySchema, layers: &Layers, opts: &Options) -> Resolved {
    let mut out = Resolved::default();

    for (key, spec) in &schema.config {
        match place_value(key, spec, layers, opts, &mut out) {
            Placement::Resolved(value, source) => {
                if let Err(problem) = validate(spec, &value) {
                    out.errors.push(format!("{key}: {problem}"));
                }
                out.values.insert(key.clone(), value);
                out.sources.insert(key.clone(), source);
            }
            Placement::Missing => out.missing.push(key.clone()),
            Placement::Errored => {}
            Placement::Handled => {}
        }
    }

    let mut seen_unknown = std::collections::BTreeSet::new();
    for (layer_keys, tag) in [
        (layers.overlay.map(|o| o.keys().cloned().collect::<Vec<_>>()), SOURCE_OVERLAY),
        (Some(layers.base.keys().cloned().collect::<Vec<_>>()), SOURCE_LOCAL),
    ] {
        if let Some(keys) = layer_keys {
            for key in keys {
                if schema.config.contains_key(&key) || !seen_unknown.insert(key.clone()) {
                    continue;
                }
                let hint = suggest::best_match(&key, schema.config.keys())
                    .map(|suggestion| format!(" — did you mean {suggestion}?"))
                    .unwrap_or_default();
                out.warnings
                    .push(format!("{key} is set in a {tag} file but not declared in envy.yaml (typo?){hint}"));
            }
        }
    }

    out
}

enum Placement {
    Resolved(String, &'static str),
    Missing,
    Errored,
    Handled,
}

fn place_value(
    key: &str,
    spec: &VarSpec,
    layers: &Layers,
    opts: &Options,
    out: &mut Resolved,
) -> Placement {
    if let Ok(raw) = std::env::var(key) {
        return Placement::Resolved(raw, SOURCE_ENV);
    }

    for (layer, source) in [
        (layers.overlay, SOURCE_OVERLAY),
        (Some(layers.base), SOURCE_LOCAL),
    ] {
        let Some(layer_values) = layer else { continue };
        let Some(node) = layer_values.get(key) else { continue };

        return match scalar_to_string(node) {
            Err(reason) => {
                out.errors.push(format!("{key}: {reason}"));
                Placement::Errored
            }
            Ok(raw) => {
                if vault::is_ref(&raw) {
                    handle_vault_ref(key, &raw, source, opts, out)
                } else {
                    Placement::Resolved(raw, source)
                }
            }
        };
    }

    if let Some(default) = &spec.default {
        return match scalar_to_string(default) {
            Err(reason) => {
                out.errors.push(format!("{key}: bad default — {reason}"));
                Placement::Errored
            }
            Ok(raw) => Placement::Resolved(raw, SOURCE_DEFAULT),
        };
    }

    if spec.mock {
        let mock = mock_value(key);
        out.warnings.push(format!(
            "{key} has no value — using generated mock ({mock}) because mock: true is set"
        ));
        return Placement::Resolved(mock, SOURCE_MOCK);
    }

    if spec.required {
        if opts.interactive {
            match prompt_for(key, spec, &mut out.errors) {
                Some(value) => {
                    out.values.insert(key.to_string(), value.clone());
                    out.sources.insert(key.to_string(), SOURCE_PROMPTED);
                    out.prompted.push((key.to_string(), value, spec.secret));
                    return Placement::Handled;
                }
                None => return Placement::Missing,
            }
        }
        return Placement::Missing;
    }

    Placement::Handled
}

fn handle_vault_ref(
    key: &str,
    raw: &str,
    _file_source: &'static str,
    opts: &Options,
    out: &mut Resolved,
) -> Placement {
    if !opts.resolve_vault {
        out.warnings
            .push(format!("{key} holds a vault reference — left unresolved (--offline mode)"));
        return Placement::Handled;
    }
    match vault::resolve(raw) {
        Ok(secret_value) => Placement::Resolved(secret_value, SOURCE_VAULT),
        Err(reason) => {
            out.errors.push(format!("{key}: {reason}"));
            Placement::Errored
        }
    }
}

fn prompt_for(key: &str, spec: &VarSpec, errors: &mut Vec<String>) -> Option<String> {
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

/// Deterministic placeholder values for `mock: true` variables.
pub fn mock_value(key: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let seed = hasher.finish();
    let hex: String = format!("{seed:016x}")
        .chars()
        .cycle()
        .take(32)
        .collect();
    format!("mock_{hex}")
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

fn enrich(problem: String, spec: &VarSpec, raw: &str) -> String {
    let hint = match spec.format.as_deref() {
        Some("uri") | Some("url") => suggest::scheme_hint(raw).map(|fixed| format!(" did you mean {fixed}?")),
        _ => {
            if spec.r#type == "boolean" || spec.r#type == "bool" {
                suggest::boolean_hint(raw).map(|fixed| format!(" did you mean {fixed}?"))
            } else {
                None
            }
        }
    };
    match hint {
        Some(hint) => format!("{problem} —{hint}"),
        None => problem,
    }
}

fn check_type(spec: &VarSpec, raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    let result = match spec.r#type.as_str() {
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
    };
    result.map_err(|problem| enrich(problem, spec, raw))
}

fn check_format(spec: &VarSpec, raw: &str) -> Result<(), String> {
    let Some(format) = spec.format.as_deref() else {
        return Ok(());
    };
    let ok = match format {
        "uri" | "url" => {
            let Some((scheme, rest)) = raw.split_once("://") else {
                return Err(enrich(
                    format!("does not satisfy format '{format}': \"{raw}\""),
                    spec,
                    raw,
                ));
            };
            let valid_chars = !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
                && !rest.contains(char::is_whitespace)
                && !raw.contains(char::is_whitespace);
            if !valid_chars {
                false
            } else {
                let known = crate::suggest::KNOWN_SCHEMES.contains(&scheme);
                match crate::suggest::best_match(scheme, crate::suggest::KNOWN_SCHEMES.iter().map(|s| s.to_string()).collect::<Vec<_>>().iter()) {
                    Some(_) if !known => false,
                    _ => true,
                }
            }
        }
        "email" => match raw.split_once('@') {
            Some((local_part, domain)) => {
                !local_part.is_empty() && domain.contains('.') && !domain.starts_with('.')
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
        Err(enrich(
            format!("does not satisfy format '{format}': \"{raw}\""),
            spec,
            raw,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::VarSpec;
    use std::collections::BTreeMap;

    type V = BTreeMap<String, serde_yaml::Value>;

    fn spec(yaml: &str) -> VarSpec {
        serde_yaml::from_str(yaml).expect("valid spec yaml")
    }

    fn value(yaml: &str) -> serde_yaml::Value {
        serde_yaml::from_str(yaml).expect("valid yaml")
    }

    fn empty_layers<'a>(base: &'a V) -> Layers<'a> {
        Layers {
            base,
            overlay: None,
        }
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
    fn uri_format_is_checked_with_hints() {
        let s = spec("{type: string, format: uri}");
        assert!(check_format(&s, "postgresql://user@localhost:5432/db").is_ok());
        assert!(check_format(&s, "localhost:5432/db").is_err());

        let err = check_format(&s, "postgersql://x").unwrap_err();
        assert!(err.contains("did you mean postgresql://"), "{err}");
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
        assert_eq!(scalar_to_string(&value("true")).unwrap(), "true");
        assert_eq!(scalar_to_string(&value("8080")).unwrap(), "8080");
        assert_eq!(scalar_to_string(&value("hello")).unwrap(), "hello");
        assert!(scalar_to_string(&value("[a]")).is_err());
    }

    #[test]
    fn overlay_beats_base_local() {
        let schema: EnvySchema =
            serde_yaml::from_str("version: \"1\"\nconfig:\n  PORT:\n    type: integer\n").unwrap();
        let mut base = V::new();
        base.insert("PORT".to_string(), value("3000"));
        let mut overlay = V::new();
        overlay.insert("PORT".to_string(), value("9999"));

        let resolved = resolve(
            &schema,
            &Layers {
                base: &base,
                overlay: Some(&overlay),
            },
            &Options::default(),
        );
        assert_eq!(resolved.values.get("PORT").unwrap(), "9999");
        assert_eq!(resolved.sources.get("PORT"), Some(&SOURCE_OVERLAY));
    }

    #[test]
    fn env_beats_everything() {
        let schema: EnvySchema =
            serde_yaml::from_str("version: \"1\"\nconfig:\n  PORT:\n    type: integer\n    default: 1\n").unwrap();
        let base = V::new();
        std::env::set_var("ENVY_TEST_PORT", "7777");
        let renamed_schema = rename_schema_key(&schema, "PORT", "ENVY_TEST_PORT");
        let resolved = resolve(&renamed_schema, &empty_layers(&base), &Options::default());
        assert_eq!(resolved.values.get("ENVY_TEST_PORT").unwrap(), "7777");
        assert_eq!(resolved.sources.get("ENVY_TEST_PORT"), Some(&SOURCE_ENV));
        std::env::remove_var("ENVY_TEST_PORT");
    }

    fn rename_schema_key(schema: &EnvySchema, from: &str, to: &str) -> EnvySchema {
        let mut config = schema.config.clone();
        let spec = config.remove(from).expect("key exists");
        config.insert(to.to_string(), spec);
        EnvySchema {
            version: schema.version.clone(),
            service: schema.service.clone(),
            config,
        }
    }

    #[test]
    fn mock_fills_missing_optional_vars() {
        let schema: EnvySchema = serde_yaml::from_str(
            "version: \"1\"\nconfig:\n  THIRD_PARTY_TOKEN:\n    type: string\n    mock: true\n",
        )
        .unwrap();
        let base = V::new();
        let resolved = resolve(&schema, &empty_layers(&base), &Options::default());
        let token = resolved.values.get("THIRD_PARTY_TOKEN").expect("mocked");
        assert!(token.starts_with("mock_"));
        assert_eq!(resolved.sources.get("THIRD_PARTY_TOKEN"), Some(&SOURCE_MOCK));
        assert!(!resolved.missing.contains(&"THIRD_PARTY_TOKEN".to_string()));
    }

    #[test]
    fn mock_is_deterministic() {
        assert_eq!(mock_value("STRIPE_KEY"), mock_value("STRIPE_KEY"));
        assert_ne!(mock_value("A"), mock_value("B"));
    }

    #[test]
    fn unknown_local_keys_get_suggestions() {
        let schema: EnvySchema = serde_yaml::from_str(
            "version: \"1\"\nconfig:\n  DATABASE_URL:\n    type: string\n",
        )
        .unwrap();
        let mut base = V::new();
        base.insert("DATABASEURL".to_string(), value("postgres://x"));

        let resolved = resolve(&schema, &empty_layers(&base), &Options::default());
        assert!(resolved
            .warnings
            .iter()
            .any(|w| w.contains("did you mean DATABASE_URL?")));
    }

    #[test]
    fn vault_refs_fail_gracefully_offline() {
        let schema: EnvySchema = serde_yaml::from_str(
            "version: \"1\"\nconfig:\n  API_SECRET:\n    type: string\n    secret: true\n    required: true\n",
        )
        .unwrap();
        let mut base = V::new();
        base.insert("API_SECRET".to_string(), value("op://dev-vault/stripe/key"));

        let opts = Options {
            interactive: false,
            resolve_vault: false,
        };
        let resolved = resolve(&schema, &empty_layers(&base), &opts);
        assert!(!resolved.values.contains_key("API_SECRET"));
        assert!(resolved.warnings.iter().any(|w| w.contains("left unresolved")));
    }

    #[test]
    fn required_missing_reported_when_not_interactive() {
        let schema: EnvySchema = serde_yaml::from_str(
            "version: \"1\"\nconfig:\n  DATABASE_URL:\n    type: string\n    required: true\n",
        )
        .unwrap();
        let base = V::new();
        let resolved = resolve(&schema, &empty_layers(&base), &Options::default());
        assert_eq!(resolved.missing, vec!["DATABASE_URL".to_string()]);
    }
}
