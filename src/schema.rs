use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct EnvySchema {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub service: Option<String>,
    pub config: BTreeMap<String, VarSpec>,
}

fn default_version() -> String {
    "1".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct VarSpec {
    #[serde(default = "default_type")]
    pub r#type: String,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub mock: bool,
    /// With `mock: true`, serve a live local HTTP endpoint instead of a static value.
    #[serde(default)]
    pub mock_server: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
}

fn default_type() -> String {
    "string".into()
}

impl EnvySchema {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let schema: EnvySchema = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing {} (is the YAML valid?)", path.display()))?;
        if schema.version != "1" {
            anyhow::bail!(
                "unsupported schema version '{}' in {}",
                schema.version,
                path.display()
            );
        }
        Ok(schema)
    }

    pub fn service_name(&self) -> &str {
        self.service.as_deref().unwrap_or("unnamed-service")
    }
}
