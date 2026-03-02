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
pub struct PackageSpec {
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
        Self::parse(&content).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn parse(content: &str) -> Result<Self> {
        Ok(toml::from_str(content)?)
    }

    pub fn default_template() -> &'static str {
        r#"[project]
targets = ["claude"]

[packages]
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal() {
        let toml = r#"
[project]
targets = ["claude"]

[packages]
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.project.targets, vec!["claude"]);
        assert!(config.packages.is_empty());
    }

    #[test]
    fn test_parse_package_with_rules_and_skills() {
        let toml = r#"
[project]
targets = ["claude"]

[packages."milkyskies/api-rules"]
rules = ["api-patterns", "error-handling"]
skills = ["scaffold-resource", "update-rule"]
"#;
        let config = Config::parse(toml).unwrap();
        let pkg = config.packages.get("milkyskies/api-rules").unwrap();
        assert_eq!(pkg.rules, vec!["api-patterns", "error-handling"]);
        assert_eq!(pkg.skills, vec!["scaffold-resource", "update-rule"]);
        assert!(pkg.overrides.is_empty());
    }

    #[test]
    fn test_parse_package_with_overrides() {
        let toml = r#"
[project]
targets = ["claude"]

[packages."milkyskies/api-rules"]
rules = ["api-patterns", "error-handling"]
skills = ["scaffold-resource"]

[packages."milkyskies/api-rules".overrides.api-patterns]
paths = ["apps/api/**"]
"#;
        let config = Config::parse(toml).unwrap();
        let pkg = config.packages.get("milkyskies/api-rules").unwrap();
        assert_eq!(pkg.rules.len(), 2);
        let ovr = pkg.overrides.get("api-patterns").unwrap();
        let paths = ovr.fields.get("paths").unwrap();
        assert!(format!("{paths:?}").contains("apps/api/**"));
    }

    #[test]
    fn test_parse_multiple_packages() {
        let toml = r#"
[project]
targets = ["claude"]

[packages."milkyskies/base-rules"]
rules = ["claude-development"]
skills = []

[packages."milkyskies/api-rules"]
rules = ["api-patterns"]
skills = ["scaffold-resource"]

[packages."milkyskies/api-rules".overrides.api-patterns]
paths = ["apps/api/**"]
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.packages.len(), 2);
        assert!(config.packages.contains_key("milkyskies/base-rules"));
        assert!(config.packages.contains_key("milkyskies/api-rules"));
    }

    #[test]
    fn test_parse_empty_rules_skills_default() {
        let toml = r#"
[project]
targets = ["claude"]

[packages."milkyskies/base-rules"]
rules = ["one-rule"]
"#;
        let config = Config::parse(toml).unwrap();
        let pkg = config.packages.get("milkyskies/base-rules").unwrap();
        assert_eq!(pkg.rules, vec!["one-rule"]);
        assert!(pkg.skills.is_empty());
        assert!(pkg.overrides.is_empty());
    }

    #[test]
    fn test_default_template_parses() {
        let config = Config::parse(Config::default_template()).unwrap();
        assert_eq!(config.project.targets, vec!["claude"]);
        assert!(config.packages.is_empty());
    }
}
