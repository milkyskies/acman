# acman

Agent Config Manager. Manages AI coding agent directives (rules, skills) across projects.

It fetches directive packages from GitHub repos and writes them to the correct locations for each target agent (Claude Code, Cursor, etc.). Per-project overrides are applied to frontmatter only, keeping directive content in sync with upstream.

## Install

```
cargo install --path .
```

## Usage

```
acman init                  # create acman.toml in current directory
acman install               # fetch all packages, apply overrides, write to target locations
acman update                # re-fetch from upstream and reapply
acman add <user/repo>       # add a package to acman.toml
acman list                  # show installed directives and their override status
```

## Config

`acman.toml`:

```toml
[project]
targets = ["claude"]

# pull all directives from a package
[packages]
onesc/base-rules = "latest"

# pick specific directives and apply overrides
[packages.onesc/api-rules]
rules = ["api-patterns", "error-handling"]
skills = ["scaffold-resource"]

[packages.onesc/api-rules.overrides.api-patterns]
paths = ["apps/api/**"]
```

`"latest"` fetches everything from the repo's default branch. The table form lets you filter to specific rules/skills and define frontmatter overrides.

Overrides only modify YAML frontmatter. The markdown body is never touched.

## Package repo structure

A package repo organizes directives like this:

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
