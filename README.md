# acman

Agent Config Manager. Manages AI coding agent configs (rules, skills) across projects.

It fetches config packages from GitHub repos and writes them to the correct locations for each target agent (Claude Code, Cursor, etc.). Per-project overrides are applied to frontmatter only, keeping config content in sync with upstream.

## Install

```
cargo install --git https://github.com/milkyskies/acman.git
```

Or from a local clone:

```
cargo install --path .
```

## Private repos

To fetch from private GitHub repos (and to use `acman push`), set `GITHUB_TOKEN` in your environment:

```
export GITHUB_TOKEN="$(gh auth token)"
```

Add this to your `~/.zshrc` or `~/.bashrc` to set it permanently.

## Usage

```
acman init                  # create acman.toml in current directory
acman add <user/repo>       # fetch repo, discover rules/skills, add to config, and install
acman install               # fetch packages, apply overrides, write to target locations
acman pull                  # alias for install
acman update                # alias for install
acman push                  # push local changes back upstream as PRs
acman push <user/repo>      # push changes for a specific package only
acman list                  # show installed configs and their override status
```

## Workflow

```
acman init
acman add milkyskies/api-rules    # fetches repo, populates config with all rules/skills, installs
```

Edit `acman.toml` to remove rules/skills you don't want. Add overrides for the ones you keep. Then `acman install` to apply.

If you edit a rule or skill locally, `acman push` diffs the markdown body against upstream (ignoring frontmatter) and opens a PR with your changes. If a PR is already open, it updates it.

To add a new rule or skill to an upstream repo: add the name to `acman.toml`, create the file locally, then `acman push`.

Removing a rule or skill from `acman.toml` and running `acman install` will delete the local file. Only acman-managed files are deleted — your own files are never touched.

## Config

`acman.toml`:

```toml
[project]
targets = ["claude"]

[packages."milkyskies/base-rules"]
rules = ["claude-development", "frontend-structure"]
skills = ["scaffold-resource", "update-rule"]

[packages."milkyskies/api-rules"]
rules = ["api-patterns", "error-handling"]
skills = ["scaffold-resource"]

[packages."milkyskies/api-rules".overrides.api-patterns]
paths = ["apps/api/**"]
```

You must list which rules and skills you want from each package. Overrides are optional and only modify YAML frontmatter — the markdown body is never touched.

## Package repo structure

A package repo organizes configs like this:

```
rules/
  api-patterns.md
  error-handling.md
skills/
  scaffold-resource/
    SKILL.md
  update-rule/
    SKILL.md
```

## Targets

| Target | Rules output | Skills output |
|--------|-------------|---------------|
| claude | .claude/rules/ | .claude/skills/ |

More targets coming later.

## How install works

1. Read `acman.toml`
2. For each package, fetch the repo tarball from GitHub
3. Include only the listed rules and skills
4. Merge any frontmatter overrides
5. Write to the target locations
6. Remove any previously installed files no longer in the config
7. Write `acman.lock` with commit SHAs for reproducibility
