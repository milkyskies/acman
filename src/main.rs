mod config;
mod fetch;
mod frontmatter;
mod lock;
mod push;
mod target;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

use config::Config;
use lock::{Lockfile, LockedPackage};

#[derive(Parser)]
#[command(name = "acman", about = "Agent Config Manager — manage AI agent configs across projects")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create acman.toml in the current directory
    Init,
    /// Fetch all packages, apply overrides, write to target locations
    Install,
    /// Re-fetch from upstream and reapply
    Update,
    /// Add a package to acman.toml (fetches repo to discover rules/skills)
    Add {
        /// Package in user/repo format
        package: String,
    },
    /// Alias for install
    Pull,
    /// Push local changes back upstream as PRs
    Push {
        /// Optional: only push changes for this package
        package: Option<String>,
    },
    /// Show installed configs and their override status
    List,
}

fn find_config_path() -> PathBuf {
    PathBuf::from("acman.toml")
}

fn find_lock_path() -> PathBuf {
    PathBuf::from("acman.lock")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => cmd_init()?,
        Command::Install => cmd_install().await?,
        Command::Update => cmd_install().await?,
        Command::Add { package } => cmd_add(&package).await?,
        Command::Pull => cmd_install().await?,
        Command::Push { package } => cmd_push(package.as_deref()).await?,
        Command::List => cmd_list()?,
    }

    Ok(())
}

fn cmd_init() -> Result<()> {
    let path = find_config_path();
    if path.exists() {
        anyhow::bail!("acman.toml already exists");
    }
    std::fs::write(&path, Config::default_template())?;
    println!("created acman.toml");
    Ok(())
}

async fn cmd_install() -> Result<()> {
    let config_path = find_config_path();
    let config = Config::load(&config_path)?;
    let project_root = std::env::current_dir()?;
    let lock_path = find_lock_path();
    let old_lockfile = Lockfile::load(&lock_path).unwrap_or_default();
    let mut lockfile = Lockfile::default();

    for (package_name, spec) in &config.packages {
        println!("fetching {package_name}...");
        let fetched = fetch::fetch_package(package_name)
            .await
            .with_context(|| format!("failed to fetch {package_name}"))?;

        // Only include listed rules and skills
        let mut processed_rules: BTreeMap<String, String> = BTreeMap::new();
        for rule_name in &spec.rules {
            let content = fetched
                .rules
                .get(rule_name)
                .with_context(|| format!("rule '{rule_name}' not found in {package_name}"))?;

            let content = if let Some(ovr) = spec.overrides.get(rule_name) {
                frontmatter::merge_frontmatter(content, &ovr.fields)?
            } else {
                content.clone()
            };
            processed_rules.insert(rule_name.clone(), content);
        }

        let mut processed_skills: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for skill_name in &spec.skills {
            let files = fetched
                .skills
                .get(skill_name)
                .with_context(|| format!("skill '{skill_name}' not found in {package_name}"))?;
            processed_skills.insert(skill_name.clone(), files.clone());
        }

        // Write to each target
        for target_name in &config.project.targets {
            let paths = target::get_target_paths(target_name)?;
            target::write_rules(&project_root, &paths, &processed_rules)?;
            target::write_skills(&project_root, &paths, &processed_skills)?;
        }

        let rule_names: Vec<String> = processed_rules.keys().cloned().collect();
        let skill_names: Vec<String> = processed_skills.keys().cloned().collect();

        println!(
            "  installed {} rules, {} skills",
            rule_names.len(),
            skill_names.len()
        );

        lockfile.packages.insert(
            package_name.clone(),
            LockedPackage {
                commit: fetched.commit_sha,
                rules: rule_names,
                skills: skill_names,
            },
        );
    }

    // Clean up files that were in the old lockfile but aren't in the new config
    let mut removed = 0;
    for (old_pkg, old_locked) in &old_lockfile.packages {
        let new_spec = config.packages.get(old_pkg);

        for target_name in &config.project.targets {
            let paths = target::get_target_paths(target_name)?;

            // Clean old rules
            for old_rule in &old_locked.rules {
                let still_wanted = new_spec.is_some_and(|s| s.rules.contains(old_rule));
                if !still_wanted {
                    let path = project_root
                        .join(paths.rules_dir)
                        .join(format!("{old_rule}.md"));
                    if path.exists() {
                        std::fs::remove_file(&path)?;
                        removed += 1;
                    }
                }
            }

            // Clean old skills
            for old_skill in &old_locked.skills {
                let still_wanted = new_spec.is_some_and(|s| s.skills.contains(old_skill));
                if !still_wanted {
                    let path = project_root.join(paths.skills_dir).join(old_skill);
                    if path.exists() {
                        std::fs::remove_dir_all(&path)?;
                        removed += 1;
                    }
                }
            }
        }
    }

    if removed > 0 {
        println!("removed {removed} old config(s)");
    }

    lockfile.save(&lock_path)?;
    println!("done.");
    Ok(())
}

async fn cmd_add(package: &str) -> Result<()> {
    let config_path = find_config_path();
    if !config_path.exists() {
        anyhow::bail!("acman.toml not found. Run `acman init` first.");
    }

    // Validate format
    if !package.contains('/') || package.matches('/').count() != 1 {
        anyhow::bail!("package must be in user/repo format");
    }

    let content = std::fs::read_to_string(&config_path)?;

    // Check if already present
    if content.contains(&format!("\"{package}\"")) {
        anyhow::bail!("package {package} is already in acman.toml");
    }

    // Fetch the repo to discover available rules and skills
    println!("fetching {package}...");
    let fetched = fetch::fetch_package(package)
        .await
        .with_context(|| format!("failed to fetch {package}"))?;

    let rules: Vec<&String> = fetched.rules.keys().collect();
    let skills: Vec<&String> = fetched.skills.keys().collect();

    // Build the TOML entry
    let mut entry = format!("\n[packages.\"{package}\"]\n");

    if !rules.is_empty() {
        let rules_str: Vec<String> = rules.iter().map(|r| format!("\"{r}\"")).collect();
        entry.push_str(&format!("rules = [{}]\n", rules_str.join(", ")));
    }

    if !skills.is_empty() {
        let skills_str: Vec<String> = skills.iter().map(|s| format!("\"{s}\"")).collect();
        entry.push_str(&format!("skills = [{}]\n", skills_str.join(", ")));
    }

    let new_content = format!("{content}{entry}");
    std::fs::write(&config_path, new_content)?;

    println!("added {package} to acman.toml:");
    for r in &rules {
        println!("  rule: {r}");
    }
    for s in &skills {
        println!("  skill: {s}");
    }

    println!();
    cmd_install().await?;

    Ok(())
}

async fn cmd_push(filter_package: Option<&str>) -> Result<()> {
    let config_path = find_config_path();
    let config = Config::load(&config_path)?;
    let project_root = std::env::current_dir()?;

    let mut any_changes = false;

    for (package_name, spec) in &config.packages {
        if let Some(filter) = filter_package {
            if package_name != filter {
                continue;
            }
        }

        println!("checking {package_name} for changes...");
        let fetched = fetch::fetch_package(package_name)
            .await
            .with_context(|| format!("failed to fetch {package_name}"))?;

        // Read local rules
        let mut local_rules: BTreeMap<String, String> = BTreeMap::new();
        for target_name in &config.project.targets {
            let paths = target::get_target_paths(target_name)?;
            let rules_dir = project_root.join(paths.rules_dir);
            for rule_name in &spec.rules {
                let path = rules_dir.join(format!("{rule_name}.md"));
                if path.exists() {
                    let content = std::fs::read_to_string(&path)?;
                    local_rules.insert(rule_name.clone(), content);
                }
            }
        }

        // Read local skills
        let mut local_skills: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for target_name in &config.project.targets {
            let paths = target::get_target_paths(target_name)?;
            let skills_dir = project_root.join(paths.skills_dir);
            for skill_name in &spec.skills {
                let skill_dir = skills_dir.join(skill_name);
                if skill_dir.exists() {
                    let mut files = BTreeMap::new();
                    read_skill_dir(&skill_dir, &skill_dir, &mut files)?;
                    if !files.is_empty() {
                        local_skills.insert(skill_name.clone(), files);
                    }
                }
            }
        }

        let mut changes = push::diff_rules(&local_rules, &fetched.rules);
        changes.extend(push::diff_skills(&local_skills, &fetched.skills));

        if changes.is_empty() {
            println!("  no changes");
            continue;
        }

        any_changes = true;
        println!("  {} file(s) changed:", changes.len());
        for change in &changes {
            println!("    - {}", change.repo_path);
        }

        let pr_url = push::create_pr(package_name, &fetched.commit_sha, &changes).await?;
        println!("  PR created: {pr_url}");
    }

    if !any_changes {
        println!("no changes to push.");
    }

    Ok(())
}

fn read_skill_dir(
    base: &std::path::Path,
    dir: &std::path::Path,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base)?.to_string_lossy().to_string();
        if path.is_file() {
            let content = std::fs::read_to_string(&path)?;
            files.insert(rel, content);
        } else if path.is_dir() {
            read_skill_dir(base, &path, files)?;
        }
    }
    Ok(())
}

fn cmd_list() -> Result<()> {
    let lock_path = find_lock_path();
    let lockfile = Lockfile::load(&lock_path)?;

    if lockfile.packages.is_empty() {
        println!("no packages installed. Run `acman install` first.");
        return Ok(());
    }

    let config_path = find_config_path();
    let config = if config_path.exists() {
        Some(Config::load(&config_path)?)
    } else {
        None
    };

    for (name, locked) in &lockfile.packages {
        println!("{name} ({})", &locked.commit[..7.min(locked.commit.len())]);

        if !locked.rules.is_empty() {
            println!("  rules:");
            for rule in &locked.rules {
                let override_marker = config
                    .as_ref()
                    .and_then(|c| c.packages.get(name))
                    .is_some_and(|spec| spec.overrides.contains_key(rule));
                let suffix = if override_marker { " (overridden)" } else { "" };
                println!("    - {rule}{suffix}");
            }
        }
        if !locked.skills.is_empty() {
            println!("  skills:");
            for skill in &locked.skills {
                println!("    - {skill}");
            }
        }
    }

    Ok(())
}
