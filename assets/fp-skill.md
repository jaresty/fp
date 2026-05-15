---
name: fp
description: Use fp (fixpoint) as the actuator when helping a user drive their PRs to merge. fp surfaces blocking tasks, fetches CI logs, manages review threads, and rebases stacked branches. Run fp commands to observe real state before advising on any PR work.
when_to_use: "Is the user asking about PR status, CI failures, review threads, stacked branches, or how to get a PR merged?"
requires:
  - fp CLI installed and on PATH
  - GITHUB_TOKEN environment variable set
  - git repository with GitHub remote
---

# fp Skill — PR Convergence Loop

## MANDATORY: Run fp Before Advising

Running fp commands before giving any advice is **not optional**.

- Do NOT describe what might be blocking a PR without first running `fp status <pr>`.
- Do NOT suggest how to fix a CI failure without first running `fp context <pr> <check-name>` to read the actual log.
- Do NOT advise on review threads without first running `fp status <pr>` to see open threads.
- Do NOT recommend rebasing without first running `fp status --all` to see all tracked PRs.
- Do NOT proceed if `GITHUB_TOKEN` is not set — tell the user to set it before running any fp command.

If you find yourself drafting advice without having run `fp status`, stop and run it first.

## What fp Does

fp observes PR state (checks, threads, approvals) and surfaces the exact actions needed to move a PR to merge. Claude's role is to execute those actions using fp commands — not to reason about state that fp can already observe.

## Command Reference

```sh
# State observation
fp ls [--json]                          # list tracked PRs
fp status <pr> [--json]                 # tasks blocking this PR
fp status --all [--json]                # all tracked PRs
fp context <pr> <hint>                  # log tail for check, or thread body
                                        # hint: exact check name (e.g. ci/test)
                                        #       or thread:<id> (e.g. thread:42)
fp context <pr> <check> --full-log      # write full raw log to temp file, print path
fp checks <sha>                         # show check run results for a specific commit SHA
fp threads <pr> [--resolved] [--json]   # list review threads (open by default)
fp agent-context [--json]               # print capability manifest with tracked PRs

# Thread management
fp reply <pr> <thread_id> "<message>"   # post reply, mark thread Addressed
fp resolve <pr> <thread_id>             # mark Resolved locally (no GitHub post)
fp comment <pr> "<text>"                # post top-level PR comment (not a thread reply)

# Stack management
fp rebase-stack                         # rebase each tracked branch onto parent tip
fp merge <pr>                           # merge PR via GitHub API, auto-detect merge method,
                                        # rebase full downstream stack after merge

# PR creation and editing
fp create "<title>" [--base <branch>]   # create draft PR for current branch
fp create "<title>" --demo <url>        # create PR and inject ## Demo section with image
fp create "<title>" --demo <file>       # upload local image file, inject URL into ## Demo
fp create "<title>" --demo <url> --demo <url2>  # multiple demos, numbered
fp edit <pr> [--title "<t>"] [--body "<b>"]     # update PR title and/or body
fp edit <pr> --demo <url>               # append/replace ## Demo section in PR body
fp edit <pr> --demo <file>              # upload local image file, inject into PR body
fp track <pr>                           # track PR (auto-fetches metadata via API)
fp track <pr> --title "..." --branch "..."  # track PR manually
fp untrack <pr>                         # stop tracking (also removes worktree if present)
fp ready <pr>                           # mark draft PR as ready for review
fp switch <pr>                          # print worktree path for PR (create if needed); use shell wrapper to cd
fp switch <pr> --force                  # skip dirty-check on current worktree
fp root                                 # print main repo root (works from inside a worktree)
fp install-shell                        # install fps shell function (auto-detects fish/zsh/bash)
fp install-shell --shell fish           # install for specific shell
fp install-shell --print                # print function to stdout without writing

# Monitoring
fp watch [--once] [--interval <secs>]   # poll tracked PRs, print task diffs
fp watch --json                         # emit JSON event objects per cycle
fp watch --wait-for ci-pass             # block until all CI tasks cleared
fp watch --wait-for ready               # block until no blocking tasks remain

# Auth and config profiles
fp profile save <name> --token <tok> --repo <owner/repo>  # save named auth+repo bundle
fp profile load <name>                                     # print export commands for profile
```

## Task Types

| Task | Blocking | Meaning | Action |
|------|----------|---------|--------|
| `FixCi` | **yes** | A required check is failing | `fp context <pr> <check>` → read log → fix → push |
| `RespondThread` | **yes** | An open or stale review thread needs a response | `fp context <pr> thread:<id>` → `fp reply` or `fp resolve` |
| `MergeConflict` | **yes** | PR has a merge conflict | Rebase branch locally, resolve conflicts, push |
| `AwaitingCi` | no | A required check is pending | `fp watch --once` to re-check |
| `AwaitingReview` | no | PR not approved yet | Wait |
| `MarkReady` | no | Draft PR is green — suggest marking ready | `fp ready <pr>` |
| `ReadyUnverified` | no | PR looks ready but CODEOWNERS eligibility unverifiable | Confirm reviewer manually before merging |

## Decision Protocol

When a user asks "what's blocking my PR" or "how do I get this merged":

1. Verify `GITHUB_TOKEN` is set — if not, stop and tell the user.
2. Run `fp status <pr>` — read every task in the output.
3. For each **blocking** task: run `fp context <pr> <hint>` to get specifics.
4. For `FixCi`: read the log tail, identify the failure, implement the fix, push.
5. For `RespondThread`: read the thread body, draft a reply, run `fp reply <pr> <id> "<message>"`.
6. Run `fp watch --once` to confirm state updated after each action.
7. Repeat from step 2 until `fp status` reports no blocking tasks.

Never skip step 2. Never advise based on assumed state.

## Worked Example

User: "My PR #7 is stuck, what do I do?"

```sh
# Step 1: get current state
fp status 7
# Output:
# PR #7 — 2 task(s):
#   [blocking] FixCi: Fix failing check: ci/test
#   [blocking] RespondThread: Respond to thread #88

# Step 2: read the CI log
fp context 7 ci/test
# Output: last 50 lines of the failing job log

# Step 3: fix the failing test, push, then check thread
fp context 7 thread:88
# Output: Thread #88 (Open)
#   src/lib.rs:42
#   "This function doesn't handle the empty input case"

# Step 4: reply to thread
fp reply 7 88 "Good catch — added handling for empty input in the same commit."

# Step 5: confirm state
fp watch --once
# Output: ✓ PR #7 resolved RespondThread: Respond to thread #88
```

## Waiting for CI in Agentic Loops

Use `--wait-for` to block until a terminal state rather than polling manually:

```sh
# Push a fix, then wait for CI to pass before continuing
git push
fp watch --wait-for ci-pass --interval 30
# fp exits when no FixCi or AwaitingCi tasks remain

# Wait until PR is fully ready to merge (no blocking tasks)
fp watch --wait-for ready
```

## Stack Workflow

All branches in the stack must be tracked first:

```sh
fp track 5    # base PR
fp track 6    # PR stacked on #5
fp track 7    # PR stacked on #6
fp rebase-stack
# Output:
# ✓ rebased feat/step-2
# ✓ rebased feat/step-3
```

Conflicts are reported by branch name. Resolve manually, then re-run `fp rebase-stack`.

## Agent-Context Manifest

`fp agent-context --json` returns a machine-readable manifest:
```json
{
  "name": "fp",
  "auth_required": "GITHUB_TOKEN env var or gh CLI",
  "commands": [...],
  "tracked_prs": [{"number": 7, "title": "feat: ...", "branch": "feat/..."}]
}
```

Use this to introspect fp capabilities and current state programmatically.

## Environment Variables

- `GITHUB_TOKEN` — required for all API calls. If absent, fp falls back to `gh auth token`. If both absent, fp errors with remediation options.
- `BUILDKITE_TOKEN` — required for Buildkite log content. If a Buildkite check fails and this is unset, tell the user.
- `GITHUB_USER_SESSION` — required for `--demo <file>` image uploads. fp auto-extracts this from Chrome/Firefox/Safari if unset. If upload fails with a session error, tell the user to set it from browser DevTools (Application → Cookies → github.com → user_session).

## Worktree Workflow

`fp switch <pr>` manages git worktrees so multiple PRs can be worked on in parallel without manual checkout.

```sh
# One-time setup: install the fps shell function
fp install-shell        # auto-detects fish/zsh/bash and writes the function

fps 42      # enter worktree for PR #42 (created if needed)
fps 87      # switch to PR #87 worktree
fps root    # return to main repo root from anywhere
fp status --all   # shows 🔒 lock indicator next to PRs with active worktrees
fp watch          # also shows lock status per PR
fp untrack 42     # removes worktree + cleans up lock
```

Guards:
- Aborts if current worktree has uncommitted changes (use `--force` to override)
- Aborts if target worktree is locked by another live process (parallel agent collision prevention)
- Stale locks (dead PID) are cleared automatically

Lock files live in `.git/worktrees/<branch>/fp-lock` — never committed.

## Write to Disk

This skill is written to `.claude/skills/fp/SKILL.md` inside the current git repository by running:

```sh
fp install-skills
```
