# fp — PR convergence loop

`fp` tracks your open PRs and tells you exactly what's blocking each one from merging: failing CI, open review threads, missing approval. It fetches live state from GitHub and surfaces a normalized task list so an LLM agent (or you) always knows what to do next.

It also manages **stacked PRs** and **git worktrees**, so you can work on multiple branches simultaneously and rebase a whole stack with one command.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/jaresty/fp/main/install.sh | sh
```

Supports macOS (arm64, x86_64) and Linux (x86_64). Installs to `/usr/local/bin/fp`.

### From source

```sh
cargo install --git https://github.com/jaresty/fp
```

## Setup

Set your GitHub token:

```sh
export GITHUB_TOKEN=ghp_...
```

Or save a named profile for easy switching:

```sh
fp profile save work --token ghp_... --repo myorg/myrepo
fp profile load work
```

Install the `fps` shell shortcut (wraps `fp switch` with `cd`):

```sh
fp install-shell   # fish, zsh, or bash
```

## Tracking PRs

```sh
# Track a PR
fp track 42

# List all tracked PRs
fp ls

# Stop tracking a PR
fp untrack 42
```

## Status and tasks

```sh
# Show tasks blocking a single PR
fp status 42

# Show all tracked PRs and their tasks
fp status --all

# Watch for changes (polls every 30s)
fp watch
fp watch --once
fp watch --interval 60
```

## Task types

| Task | Blocking | Meaning |
|------|----------|---------|
| `fix_ci` | yes | A required CI check is failing |
| `respond_thread` | yes | An open or stale review thread needs a reply |
| `merge_conflict` | yes | The branch has a merge conflict |
| `rebase_on_parent` | yes | Parent PR has new commits — run `fp rebase-stack` |
| `awaiting_ci` | no | A required CI check is still running |
| `awaiting_review` | no | No approving review yet |
| `mark_ready` | no | PR is still in draft — run `fp ready` |
| `ready_unverified` | no | Approval received but CODEOWNERS eligibility could not be verified |

An empty task list means the PR is ready to merge.

## Working on PRs

```sh
# Create a new branch + worktree (then use fp create to open the PR)
fp new feat/my-feature
fp new feat/my-feature --base develop

# Create a draft PR for the current branch and start tracking it
fp create "My feature title"
fp create "My feature" --base develop --body "Description here"

# Switch to a PR's worktree (creates it if needed); cd into it with fps
fps 42              # requires fp install-shell
fp switch 42 <id>   # <id> = session identifier for the lock

# Unlock a branch so another session can switch to it
fp unlock feat/my-feature

# Mark a draft PR ready for review
fp ready 42
```

## Review threads

```sh
# Show review threads for a PR
fp threads 42

# Reply to a thread and mark it addressed
fp reply 42 <thread-id> "Fixed in the latest commit."

# Mark a thread resolved locally (without posting)
fp resolve 42 <thread-id>
```

## CI and context

```sh
# Show full context for a specific task (CI logs, thread body)
fp context 42 ci/test
fp context 42 thread:<thread-id>

# Show check run results for a commit SHA
fp checks <sha>
```

## Stacked PRs

```sh
# Rebase the full stack in dependency order
fp rebase-stack

# Rebase only a specific PR and its descendants
fp rebase-stack 42

# Merge a PR and automatically rebase downstream tracked branches
fp merge 42
fp merge 42 --squash
fp merge 42 --rebase

# Stack a new PR on top of an existing one
fp create "Child feature" --base feat/parent-branch

# Insert current branch before or after an existing PR in the stack
fp create "Mid feature" --insert-after 41
fp create "Mid feature" --restack-before 42
```

## Other commands

```sh
# Edit a PR's title or body
fp edit 42 --title "New title"

# Post a general comment (not a thread reply)
fp comment 42 "LGTM overall."

# Print machine-readable context for LLM agent consumption
fp agent-context

# Print the main repo root (works from inside a worktree)
fp root

# Install the fp Claude Code skill
fp install-skills
```

## Environment variables

| Variable | Required | Purpose |
|----------|----------|---------|
| `GITHUB_TOKEN` | for live status | GitHub personal access token |
| `BUILDKITE_TOKEN` | optional | Fetch Buildkite CI logs in `fp context` |
