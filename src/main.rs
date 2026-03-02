mod config;
mod fetch;
mod frontmatter;
mod lock;
mod target;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

use config::{Config, PackageSpec};
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
    /// Add a package to acman.toml
    Add {
        /// Package in user/repo format
        package: String,
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
        Command::Add { package } => cmd_add(&package)?,
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
    let mut lockfile = Lockfile::default();

    for (package_name, spec) in &config.packages {
        println!("fetching {package_name}...");
        let fetched = fetch::fetch_package(package_name)
            .await
            .with_context(|| format!("failed to fetch {package_name}"))?;

        // Determine which rules and skills to include
        let (selected_rules, selected_skills, overrides): (
            Option<&Vec<String>>,
            Option<&Vec<String>>,
            BTreeMap<String, config::Override>,
        ) = match spec {
            PackageSpec::Simple(_) => (None, None, BTreeMap::new()),
            PackageSpec::Detailed(detail) => {
                let rules = if detail.rules.is_empty() {
                    None
                } else {
                    Some(&detail.rules)
                };
                let skills = if detail.skills.is_empty() {
                    None
                } else {
                    Some(&detail.skills)
                };
                (rules, skills, detail.overrides.clone())
            }
        };

        // Filter and process rules
        let mut processed_rules: BTreeMap<String, String> = BTreeMap::new();
        for (name, content) in &fetched.rules {
            if let Some(selected) = &selected_rules {
                if !selected.iter().any(|s| s == name) {
                    continue;
                }
            }
            let content = if let Some(ovr) = overrides.get(name.as_str()) {
                frontmatter::merge_frontmatter(content, &ovr.fields)?
            } else {
                content.clone()
            };
            processed_rules.insert(name.clone(), content);
        }

        // Filter skills
        let mut processed_skills: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (name, files) in &fetched.skills {
            if let Some(selected) = &selected_skills {
                if !selected.iter().any(|s| s == name) {
                    continue;
                }
            }
            processed_skills.insert(name.clone(), files.clone());
        }

        // Write to each target
        for target_name in &config.project.targets {
            let paths = target::get_target_paths(target_name)?;
            target::write_rules(&project_root, &paths, package_name, &processed_rules)?;
            target::write_skills(&project_root, &paths, package_name, &processed_skills)?;
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

    lockfile.save(&find_lock_path())?;
    println!("done.");
    Ok(())
}

fn cmd_add(package: &str) -> Result<()> {
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
    if content.contains(&format!("\"{package}\"")) || content.contains(&format!("[packages.{package}")) {
        anyhow::bail!("package {package} is already in acman.toml");
    }

    // Append to the packages section
    let new_content = format!("{content}{package} = \"latest\"\n");
    std::fs::write(&config_path, new_content)?;
    println!("added {package} to acman.toml");
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

        let has_overrides = config.as_ref().is_some_and(|c| {
            matches!(
                c.packages.get(name),
                Some(PackageSpec::Detailed(d)) if !d.overrides.is_empty()
            )
        });

        if !locked.rules.is_empty() {
            println!("  rules:");
            for rule in &locked.rules {
                let override_marker = if has_overrides
                    && config.as_ref().is_some_and(|c| {
                        matches!(
                            c.packages.get(name),
                            Some(PackageSpec::Detailed(d)) if d.overrides.contains_key(rule)
                        )
                    }) {
                    " (overridden)"
                } else {
                    ""
                };
                println!("    - {rule}{override_marker}");
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
