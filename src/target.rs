use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::path::Path;

pub struct TargetPaths {
    pub rules_dir: &'static str,
    pub skills_dir: &'static str,
}

pub fn get_target_paths(target: &str) -> Result<TargetPaths> {
    match target {
        "claude" => Ok(TargetPaths {
            rules_dir: ".claude/rules",
            skills_dir: ".claude/skills",
        }),
        other => bail!("unsupported target: {other}"),
    }
}

pub fn write_rules(
    project_root: &Path,
    target: &TargetPaths,
    rules: &BTreeMap<String, String>,
) -> Result<()> {
    let rules_dir = project_root.join(target.rules_dir);
    std::fs::create_dir_all(&rules_dir)?;

    for (name, content) in rules {
        let path = rules_dir.join(format!("{name}.md"));
        std::fs::write(&path, content)?;
    }
    Ok(())
}

pub fn write_skills(
    project_root: &Path,
    target: &TargetPaths,
    skills: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<()> {
    let skills_dir = project_root.join(target.skills_dir);

    for (skill_name, files) in skills {
        let skill_dir = skills_dir.join(skill_name);
        std::fs::create_dir_all(&skill_dir)?;

        for (file_name, content) in files {
            let path = skill_dir.join(file_name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, content)?;
        }
    }
    Ok(())
}
