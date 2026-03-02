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

To fetch from private GitHub repos, set `GITHUB_TOKEN` in your environment:

```
export GITHUB_TOKEN="ghp_..."
```

You can add this to your `~/.zshrc` or `~/.bashrc` to set it permanently. This is the same token used by `gh` and other GitHub tools.

## Usage

```
acman init                  # create acman.toml in current directory
acman install               # fetch all packages, apply overrides, write to target locations
acman update                # re-fetch from upstream and reapply
acman add <user/repo>       # add a package to acman.toml
acman list                  # show installed configs and their override status
```

## Config

`acman.toml`:

```toml
[project]
targets = ["claude"]

# pull all configs from a package
[packages]
milkyskies/base-rules = "latest"

# pick specific configs and apply overrides
[packages.milkyskies/api-rules]
rules = ["api-patterns", "error-handling"]
skills = ["scaffold-resource"]

[packages.milkyskies/api-rules.overrides.api-patterns]
paths = ["apps/api/**"]
```

`"latest"` fetches everything from the repo's default branch. The table form lets you filter to specific rules/skills and define frontmatter overrides.

Overrides only modify YAML frontmatter. The markdown body is never touched.

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
3. Filter to selected rules/skills (or include all if `"latest"`)
4. Merge any frontmatter overrides
5. Write to the target locations
6. Write `acman.lock` with commit SHAs for reproducibility
