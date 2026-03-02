use anyhow::Result;
use std::collections::BTreeMap;

const FRONTMATTER_DELIM: &str = "---";

pub fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with(FRONTMATTER_DELIM) {
        return (None, content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[FRONTMATTER_DELIM.len()..];
    if let Some(end) = after_first.find("\n---") {
        let yaml = after_first[..end].trim().to_string();
        let rest_start = end + "\n---".len();
        let body = after_first[rest_start..].to_string();
        // Strip leading newline from body
        let body = body.strip_prefix('\n').unwrap_or(&body).to_string();
        (Some(yaml), body)
    } else {
        (None, content.to_string())
    }
}

pub fn merge_frontmatter(
    content: &str,
    overrides: &BTreeMap<String, serde_yaml::Value>,
) -> Result<String> {
    if overrides.is_empty() {
        return Ok(content.to_string());
    }

    let (existing_yaml, body) = split_frontmatter(content);

    let mut fm: BTreeMap<String, serde_yaml::Value> = if let Some(yaml) = &existing_yaml {
        serde_yaml::from_str(yaml)?
    } else {
        BTreeMap::new()
    };

    // Merge overrides (overrides win)
    for (k, v) in overrides {
        fm.insert(k.clone(), v.clone());
    }

    let new_yaml = serde_yaml::to_string(&fm)?;
    // serde_yaml adds a trailing newline, trim it
    let new_yaml = new_yaml.trim_end();

    Ok(format!("---\n{new_yaml}\n---\n{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_no_frontmatter() {
        let (fm, body) = split_frontmatter("hello world");
        assert!(fm.is_none());
        assert_eq!(body, "hello world");
    }

    #[test]
    fn test_split_with_frontmatter() {
        let content = "---\ntitle: test\n---\nbody here";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm.unwrap(), "title: test");
        assert_eq!(body, "body here");
    }

    #[test]
    fn test_merge_adds_override() {
        let content = "---\ntitle: test\n---\nbody";
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "paths".to_string(),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
                "apps/api/**".to_string(),
            )]),
        );
        let result = merge_frontmatter(content, &overrides).unwrap();
        assert!(result.contains("paths:"));
        assert!(result.contains("apps/api/**"));
        assert!(result.contains("body"));
    }
}
