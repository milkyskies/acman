use anyhow::{Context, Result};
use std::collections::BTreeMap;

use crate::frontmatter::split_frontmatter;

pub struct FileChange {
    /// Path within the repo, e.g. "rules/api-patterns.md"
    pub repo_path: String,
    /// The new content to write (upstream frontmatter + local body)
    pub content: String,
}

/// Compare local rules/skills against upstream and return changes.
/// Only diffs the markdown body, ignoring frontmatter.
/// Produces new content = upstream frontmatter + local body.
pub fn diff_rules(
    local_rules: &BTreeMap<String, String>,
    upstream_rules: &BTreeMap<String, String>,
) -> Vec<FileChange> {
    let mut changes = Vec::new();

    for (name, local_content) in local_rules {
        let (_, local_body) = split_frontmatter(local_content);

        match upstream_rules.get(name) {
            Some(upstream_content) => {
                let (upstream_fm, upstream_body) = split_frontmatter(upstream_content);

                if local_body.trim() != upstream_body.trim() {
                    let new_content = if let Some(fm) = &upstream_fm {
                        format!("---\n{fm}\n---\n{local_body}")
                    } else {
                        local_body
                    };

                    changes.push(FileChange {
                        repo_path: format!("rules/{name}.md"),
                        content: new_content,
                    });
                }
            }
            None => {
                // New file — push local content as-is (strip override frontmatter, keep body)
                changes.push(FileChange {
                    repo_path: format!("rules/{name}.md"),
                    content: local_body,
                });
            }
        }
    }

    changes
}

pub fn diff_skills(
    local_skills: &BTreeMap<String, BTreeMap<String, String>>,
    upstream_skills: &BTreeMap<String, BTreeMap<String, String>>,
) -> Vec<FileChange> {
    let mut changes = Vec::new();

    for (skill_name, local_files) in local_skills {
        let upstream_files = upstream_skills.get(skill_name);

        for (file_name, local_content) in local_files {
            let (_, local_body) = split_frontmatter(local_content);

            let upstream_content = upstream_files.and_then(|f| f.get(file_name));

            match upstream_content {
                Some(upstream_content) => {
                    let (upstream_fm, upstream_body) = split_frontmatter(upstream_content);

                    if local_body.trim() != upstream_body.trim() {
                        let new_content = if let Some(fm) = &upstream_fm {
                            format!("---\n{fm}\n---\n{local_body}")
                        } else {
                            local_body
                        };

                        changes.push(FileChange {
                            repo_path: format!("skills/{skill_name}/{file_name}"),
                            content: new_content,
                        });
                    }
                }
                None => {
                    changes.push(FileChange {
                        repo_path: format!("skills/{skill_name}/{file_name}"),
                        content: local_body,
                    });
                }
            }
        }
    }

    changes
}

/// Create or update a branch, commit changes, and open or update a PR using the GitHub API.
pub async fn create_pr(
    repo: &str,
    base_sha: &str,
    changes: &[FileChange],
) -> Result<String> {
    let token = std::env::var("GITHUB_TOKEN")
        .context("GITHUB_TOKEN is required for acman push")?;

    let client = reqwest::Client::new();
    let branch_name = "acman/push";

    // 1. Try to create branch, or update it if it already exists
    let create_ref_url = format!("https://api.github.com/repos/{repo}/git/refs");
    let resp = client
        .post(&create_ref_url)
        .header("User-Agent", "acman")
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "ref": format!("refs/heads/{branch_name}"),
            "sha": base_sha
        }))
        .send()
        .await?;

    let status = resp.status();
    if status.as_u16() == 422 {
        // Branch exists — force update it to the latest base SHA
        let update_ref_url =
            format!("https://api.github.com/repos/{repo}/git/refs/heads/{branch_name}");
        let resp = client
            .patch(&update_ref_url)
            .header("User-Agent", "acman")
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "sha": base_sha,
                "force": true
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("failed to update branch on {repo}: {body}");
        }
    } else if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to create branch on {repo}: {status} {body}");
    }

    // 2. For each changed file, update via Contents API
    for change in changes {
        // Get current file SHA on the branch
        let contents_url = format!(
            "https://api.github.com/repos/{repo}/contents/{}?ref={branch_name}",
            change.repo_path
        );
        let resp = client
            .get(&contents_url)
            .header("User-Agent", "acman")
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;

        let file_sha = if resp.status().is_success() {
            let data: serde_json::Value = resp.json().await?;
            data["sha"].as_str().map(|s| s.to_string())
        } else {
            None
        };

        let action = if file_sha.is_some() { "update" } else { "add" };
        let mut body = serde_json::json!({
            "message": format!("{action} {}", change.repo_path),
            "content": base64_encode(&change.content),
            "branch": branch_name,
        });
        if let Some(sha) = file_sha {
            body["sha"] = serde_json::Value::String(sha);
        }

        let put_url = format!(
            "https://api.github.com/repos/{repo}/contents/{}",
            change.repo_path
        );
        let resp = client
            .put(&put_url)
            .header("User-Agent", "acman")
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("failed to update {}: {body}", change.repo_path);
        }
    }

    // 3. Check for existing open PR from this branch
    let search_url = format!(
        "https://api.github.com/repos/{repo}/pulls?head={}:{branch_name}&state=open",
        repo.split('/').next().unwrap_or("")
    );
    let resp = client
        .get(&search_url)
        .header("User-Agent", "acman")
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await?;

    if resp.status().is_success() {
        let prs: Vec<serde_json::Value> = resp.json().await?;
        if let Some(existing_pr) = prs.first() {
            // PR already exists — it's now updated with the new commits
            let pr_html_url = existing_pr["html_url"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            return Ok(pr_html_url);
        }
    }

    // 4. No existing PR — create one
    let changed_files: Vec<&str> = changes.iter().map(|c| c.repo_path.as_str()).collect();
    let pr_body = format!(
        "Updated by acman push.\n\nChanged files:\n{}",
        changed_files
            .iter()
            .map(|f| format!("- `{f}`"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let pr_url = format!("https://api.github.com/repos/{repo}/pulls");
    let resp = client
        .post(&pr_url)
        .header("User-Agent", "acman")
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "title": format!("acman: update {} file(s)", changes.len()),
            "body": pr_body,
            "head": branch_name,
            "base": "main",
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to create PR on {repo}: {body}");
    }

    let pr_data: serde_json::Value = resp.json().await?;
    let pr_html_url = pr_data["html_url"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    Ok(pr_html_url)
}

fn base64_encode(input: &str) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(input.as_bytes()).unwrap();
        encoder.finish().unwrap();
    }
    String::from_utf8(buf).unwrap()
}

// Minimal base64 encoder — avoids pulling in another crate
struct Base64Encoder<W: std::io::Write> {
    writer: W,
    buf: [u8; 3],
    buf_len: usize,
}

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl<W: std::io::Write> Base64Encoder<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            buf: [0; 3],
            buf_len: 0,
        }
    }

    fn flush_buf(&mut self) -> std::io::Result<()> {
        if self.buf_len == 0 {
            return Ok(());
        }
        let b = &self.buf;
        let out = match self.buf_len {
            3 => [
                B64_CHARS[(b[0] >> 2) as usize],
                B64_CHARS[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize],
                B64_CHARS[(((b[1] & 0x0F) << 2) | (b[2] >> 6)) as usize],
                B64_CHARS[(b[2] & 0x3F) as usize],
            ],
            2 => [
                B64_CHARS[(b[0] >> 2) as usize],
                B64_CHARS[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize],
                B64_CHARS[((b[1] & 0x0F) << 2) as usize],
                b'=',
            ],
            1 => [
                B64_CHARS[(b[0] >> 2) as usize],
                B64_CHARS[((b[0] & 0x03) << 4) as usize],
                b'=',
                b'=',
            ],
            _ => unreachable!(),
        };
        self.writer.write_all(&out)?;
        self.buf_len = 0;
        Ok(())
    }

    fn finish(mut self) -> std::io::Result<W> {
        self.flush_buf()?;
        Ok(self.writer)
    }
}

impl<W: std::io::Write> std::io::Write for Base64Encoder<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut i = 0;
        while i < data.len() {
            self.buf[self.buf_len] = data[i];
            self.buf_len += 1;
            i += 1;
            if self.buf_len == 3 {
                self.flush_buf()?;
            }
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_rules_no_change() {
        let mut local = BTreeMap::new();
        let mut upstream = BTreeMap::new();
        local.insert("test".to_string(), "---\npaths:\n- src/**\n---\nbody text".to_string());
        upstream.insert("test".to_string(), "---\ntitle: test\n---\nbody text".to_string());
        let changes = diff_rules(&local, &upstream);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_diff_rules_body_changed() {
        let mut local = BTreeMap::new();
        let mut upstream = BTreeMap::new();
        local.insert("test".to_string(), "---\npaths:\n- src/**\n---\nnew body".to_string());
        upstream.insert("test".to_string(), "---\ntitle: test\n---\nold body".to_string());
        let changes = diff_rules(&local, &upstream);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].repo_path, "rules/test.md");
        // Should have upstream frontmatter + local body
        assert!(changes[0].content.contains("title: test"));
        assert!(changes[0].content.contains("new body"));
        assert!(!changes[0].content.contains("paths"));
    }

    #[test]
    fn test_diff_rules_new_file() {
        let mut local = BTreeMap::new();
        let upstream = BTreeMap::new();
        local.insert("new-rule".to_string(), "---\npaths:\n- src/**\n---\nmy new rule body".to_string());
        let changes = diff_rules(&local, &upstream);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].repo_path, "rules/new-rule.md");
        // New file should have body only (frontmatter stripped since it's overrides)
        assert!(changes[0].content.contains("my new rule body"));
        assert!(!changes[0].content.contains("paths"));
    }

    #[test]
    fn test_diff_skills_new_skill() {
        let mut local = BTreeMap::new();
        let upstream = BTreeMap::new();
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".to_string(), "skill content here".to_string());
        local.insert("new-skill".to_string(), files);
        let changes = diff_skills(&local, &upstream);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].repo_path, "skills/new-skill/SKILL.md");
        assert!(changes[0].content.contains("skill content here"));
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode("hello"), "aGVsbG8=");
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("ab"), "YWI=");
        assert_eq!(base64_encode("abc"), "YWJj");
    }
}
