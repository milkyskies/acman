use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use tempfile::TempDir;

pub struct FetchedPackage {
    pub commit_sha: String,
    pub rules: BTreeMap<String, String>,
    pub skills: BTreeMap<String, BTreeMap<String, String>>,
    pub _temp_dir: TempDir,
}

pub async fn fetch_package(repo: &str) -> Result<FetchedPackage> {
    let client = reqwest::Client::new();
    let token = std::env::var("GITHUB_TOKEN").ok();

    // Get the default branch's HEAD commit SHA
    let api_url = format!("https://api.github.com/repos/{repo}/commits?per_page=1");
    let mut req = client
        .get(&api_url)
        .header("User-Agent", "acman")
        .header("Accept", "application/vnd.github.v3+json");
    if let Some(t) = &token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let response = req
        .send()
        .await
        .with_context(|| format!("failed to fetch commits for {repo}"))?;

    let status = response.status();
    if !status.is_success() {
        match status.as_u16() {
            404 => anyhow::bail!("repository {repo} not found (if private, set GITHUB_TOKEN)"),
            403 => anyhow::bail!("GitHub API rate limit exceeded — try again later or set GITHUB_TOKEN"),
            401 => anyhow::bail!("GITHUB_TOKEN is invalid or expired"),
            _ => anyhow::bail!("GitHub API returned {status} for {repo}"),
        }
    }

    let commits: Vec<serde_json::Value> = response.json().await?;

    let commit_sha = commits
        .first()
        .and_then(|c| c["sha"].as_str())
        .unwrap_or("unknown")
        .to_string();

    // Fetch tarball
    let tarball_url = format!("https://api.github.com/repos/{repo}/tarball");
    let mut req = client
        .get(&tarball_url)
        .header("User-Agent", "acman")
        .header("Accept", "application/vnd.github.v3+json");
    if let Some(t) = &token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let response = req
        .send()
        .await
        .with_context(|| format!("failed to fetch tarball for {repo}"))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("failed to download {repo}: GitHub returned {status}");
    }

    let bytes = response.bytes().await?;

    let temp_dir = TempDir::new()?;
    let decoder = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(decoder);
    archive.unpack(temp_dir.path())?;

    // GitHub tarballs have a top-level directory like "user-repo-sha/"
    let top_dir = find_top_dir(temp_dir.path())?;

    let rules = read_rules(&top_dir)?;
    let skills = read_skills(&top_dir)?;

    Ok(FetchedPackage {
        commit_sha,
        rules,
        skills,
        _temp_dir: temp_dir,
    })
}

fn find_top_dir(base: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            return Ok(entry.path());
        }
    }
    anyhow::bail!("no top-level directory found in tarball");
}

fn read_rules(top_dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut rules = BTreeMap::new();
    let rules_dir = top_dir.join("rules");
    if rules_dir.exists() {
        for entry in std::fs::read_dir(&rules_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let mut content = String::new();
                std::fs::File::open(&path)?.read_to_string(&mut content)?;
                rules.insert(name, content);
            }
        }
    }
    Ok(rules)
}

fn read_skills(top_dir: &Path) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
    let mut skills = BTreeMap::new();
    let skills_dir = top_dir.join("skills");
    if skills_dir.exists() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let skill_name = path.file_name().unwrap().to_string_lossy().to_string();
                let mut files = BTreeMap::new();
                read_skill_files(&path, &path, &mut files)?;
                if !files.is_empty() {
                    skills.insert(skill_name, files);
                }
            }
        }
    }
    Ok(skills)
}

fn read_skill_files(
    base: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base)?.to_string_lossy().to_string();
        if path.is_file() {
            let mut content = String::new();
            std::fs::File::open(&path)?.read_to_string(&mut content)?;
            files.insert(rel, content);
        } else if path.is_dir() {
            read_skill_files(base, &path, files)?;
        }
    }
    Ok(())
}
