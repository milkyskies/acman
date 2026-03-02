use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub project: Project,
    pub packages: BTreeMap<String, PackageSpec>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub targets: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PackageSpec {
    Simple(String),
    Detailed(PackageDetail),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageDetail {
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub overrides: BTreeMap<String, Override>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Override {
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_yaml::Value>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn default_template() -> &'static str {
        r#"[project]
targets = ["claude"]

[packages]
# onesc/base-rules = "latest"
"#
    }
}
