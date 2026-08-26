use std::process::Command;

/// Detect whether a raw string is a dynamic secret reference.
pub fn is_ref(value: &str) -> bool {
    value.starts_with("op://")
        || value.starts_with("vault://")
        || value.starts_with("aws://")
}

/// Resolve a reference through the matching CLI (1Password CLI, Vault CLI, AWS CLI).
/// Values never touch disk — they live only in process memory.
pub fn resolve(value: &str) -> Result<String, String> {
    if let Some(rest) = value.strip_prefix("op://") {
        return run("op", &["read", "--no-newline", value])
            .map_err(|err| format!("op://{rest}: {err}"));
    }
    if let Some(rest) = value.strip_prefix("vault://") {
        let (path, field) = match rest.split_once('#') {
            Some((p, f)) if !f.is_empty() => (p.to_string(), Some(f.to_string())),
            _ => (rest.to_string(), None),
        };
        let mut args: Vec<&str> = vec!["kv", "get"];
        if let Some(field) = &field {
            args.push("-field");
            args.push(field);
        }
        args.push(&path);
        return run("vault", &args).map_err(|err| format!("vault://{rest}: {err}"));
    }
    if let Some(rest) = value.strip_prefix("aws://") {
        let (name, key) = match rest.split_once('#') {
            Some((n, k)) => (n.to_string(), Some(k.to_string())),
            _ => (rest.to_string(), None),
        };
        let output = run(
            "aws",
            &[
                "secretsmanager",
                "get-secret-value",
                "--secret-id",
                &name,
                "--query",
                "SecretString",
                "--output",
                "text",
            ],
        )
        .map_err(|err| format!("aws://{rest}: {err}"))?;
        return match key {
            None => Ok(output),
            Some(key) => extract_json_field(&output, &key).ok_or_else(|| {
                format!("aws://{rest}: key '{key}' not found in secret JSON")
            }),
        };
    }
    Err(format!("unsupported secret reference scheme: {value}"))
}

fn extract_json_field(json: &str, key: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    match &parsed {
        serde_json::Value::String(inner) => {
            let nested: serde_json::Value = serde_json::from_str(inner).ok()?;
            nested.get(key)?.as_str().map(str::to_string)
        }
        other => other.get(key)?.as_str().map(str::to_string),
    }
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|_| format!("'{program}' CLI not found on PATH (required to resolve this ref)"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("; ");
        return Err(format!("'{program}' failed: {tail}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_refs() {
        assert!(is_ref("op://vault/item/field"));
        assert!(is_ref("vault://secret/data/foo#password"));
        assert!(is_ref("aws://prod/stripe#key"));
        assert!(!is_ref("postgres://localhost/db"));
        assert!(!is_ref("sk_test_plainvalue"));
    }

    #[test]
    fn extracts_nested_json_field() {
        let raw = r#"{"username":"u","password":"p"}"#;
        assert_eq!(extract_json_field(raw, "password").as_deref(), Some("p"));

        let quoted = serde_json::Value::String(raw.to_string()).to_string();
        assert_eq!(extract_json_field(&quoted, "username").as_deref(), Some("u"));
    }
}
