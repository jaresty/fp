# fp — PR convergence loop

`fp` tracks your open PRs and tells you exactly what's blocking each one from merging: failing CI, open review threads, missing approval. It fetches live state from GitHub and surfaces a normalized task list so an LLM agent (or you) always knows what to do next.

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

## Usage

```sh
# Track a PR (auto-fetches title and branch when GITHUB_TOKEN is set)
fp track 42

# Show what's blocking a PR
fp status 42

# Show all tracked PRs
fp status --all

# Watch for changes (polls every 30s)
fp watch
fp watch --once      # fetch once and exit
fp watch --interval 60

# Get full context for a task (fetches CI logs, thread body)
fp context 42 ci/test          # check by name
fp context 42 thread:999       # review thread by id

# Reply to a review thread (posts to GitHub + marks addressed)
fp reply 42 999 "Fixed in the latest commit."

# Mark a thread resolved locally
fp resolve 42 999

# List all tracked PRs
fp ls

# Stop tracking a PR
fp untrack 42
```

## Task types

| Task | Blocking | Meaning |
|------|----------|---------|
| `fix_ci` | yes | A required CI check is failing |
| `respond_thread` | yes | An open or stale review thread needs a reply |
| `awaiting_ci` | no | A required CI check is still running |
| `awaiting_review` | no | No approving review yet |

An empty task list means the PR is ready to merge.

## Environment variables

| Variable | Required | Purpose |
|----------|----------|---------|
| `GITHUB_TOKEN` | for live status | GitHub personal access token |
| `BUILDKITE_TOKEN` | optional | Fetch Buildkite CI logs in `fp context` |
