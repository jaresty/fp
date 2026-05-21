---
name: fp
description: Use fp (fixpoint) as the actuator when helping a user drive their PRs to merge. fp surfaces blocking tasks, fetches CI logs, manages review threads, and rebases stacked branches. Run fp commands to observe real state before advising on any PR work.
when_to_use: |
  Answer yes to any of:
  - Is the agent about to navigate into, edit code in, or run tests inside a PR branch?
  - Is the user asking about PR status, CI failures, review threads, or stacked branches?
  - Is the user asking how to get a PR merged or marked ready?
  - Is the agent about to use git worktree, git checkout, or cd to reach a PR branch?
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
- Do NOT enter a PR branch's worktree using `git worktree add` or `git checkout` directly — always use `fp switch <pr>` to get the canonical absolute path and create the worktree correctly.

If you find yourself drafting advice without having run `fp status`, stop and run it first.

## What fp Does

fp observes PR state (checks, threads, approvals) and surfaces the exact actions needed to move a PR to merge. Claude's role is to execute those actions using fp commands — not to reason about state that fp can already observe.

## Command Reference

```sh
# State observation
fp ls [--json]                          # list tracked PRs
fp status [<pr>] [--json]               # tasks blocking this PR (defaults to current branch's PR)
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
fp comment <pr> "<text>"                # post top-level PR comment (not a thread reply)

# Stack management
fp rebase-stack                         # rebase each tracked branch onto parent tip
fp merge <pr>                           # merge PR via GitHub API, auto-detect merge method,
                                        # rebase full downstream stack after merge;
                                        # removes PR from its feature envelope automatically
fp merge <pr> --squash                  # force squash merge
fp merge <pr> --rebase                  # force rebase merge
fp merge <pr> --merge                   # force merge commit

# App lifecycle configs (define how to start/stop/health-check an app)
fp app define-config <name> \
  --bootstrap "<cmd>" \                 # command to start the app in its worktree
  --teardown "<cmd>" \                  # command to stop the app
  --startup-timeout <dur> \             # how long to wait for startup (default: 60s)
  [--health-check "<cmd>"] \            # optional: exit 0 = healthy
  [--ephemeral] \                       # app exits immediately after install (health-check required)
  [--main-worktree <path>]              # path to use when no PR owns this config slot
fp app set-config <owner/repo> <name>   # assign a named app config to all PRs in a repo

# Single-PR app lifecycle
fp pr up <pr>                           # bootstrap the app for a single PR (uses its bound app config)
fp pr up <pr> --config <name>           # override app config at call time (repeatable: --config a --config b)

# Feature envelopes (multi-PR coordinated workspaces)
fp feature new <name>                   # create a named feature envelope
fp feature add <name> <pr>              # add PR to envelope; auto-tracks if not yet tracked
fp feature add <name> <pr> --config <app>           # bind one app config to this PR
fp feature add <name> <pr> --config <a> --config <b>  # bind multiple app configs (repeatable)
fp feature add-dep <name> <app>         # declare a baseline service dependency with no PR
                                        # bootstraps from the main repo root when no live PR
                                        # covers that app config (dep slot: pr=0, branch="")
fp feature up <name>                    # bootstrap all member PRs (start app processes)
                                        # dep slots run from main repo root if no PR covers them
fp feature up <name> --yes              # tear down conflicting running features without prompting
fp feature up <name> --no               # abort if any conflicting running feature is detected
fp feature down <name>                  # tear down all member PRs (stop app processes)
fp feature rebuild <name> [--pr <pr>]   # re-run bootstrap for ephemeral members without teardown
fp feature rebuild <name> --pr 0        # rebuild the main-branch dep slot specifically
fp feature status <name>                # health of all member PRs; flags merged PRs (GitHub API)
fp feature status <name> --json         # output as JSON (skips GitHub merged-PR check)
fp feature list                         # list all envelopes and members
fp feature list --running               # list envelopes with at least one live instance
fp feature remove <name> <pr>           # remove a PR from an envelope (deletes envelope if empty)
                                        # use when PR was merged outside fp merge

# Branch and worktree creation
fp new <branch> [--base <base>]         # create new branch + worktree without a PR (default base: main)
                                        # fetches origin/<base>, creates branch from it, prints worktree path

# PR creation and editing
fp create "<title>" [--base <branch>]   # create draft PR for current branch
fp create "<title>" --body "<text>"     # create PR with description body
fp create "<title>" --demo <url>        # create PR and inject ## Demo section with image
fp create "<title>" --demo <file>       # upload local image file, inject URL into ## Demo
fp create "<title>" --demo <url> --demo <url2>  # multiple demos, numbered
fp create "<title>" --restack-before <pr>  # insert new PR before <pr> in the stack
fp create "<title>" --insert-after <pr>    # insert new PR after <pr>, rebase what follows
fp edit <pr> [--title "<t>"] [--body "<b>"]     # update PR title and/or body
fp edit <pr> --demo <url>               # append/replace ## Demo section in PR body
fp edit <pr> --demo <file>              # upload local image file, inject into PR body
fp track <pr>                           # track PR (auto-fetches metadata via API)
fp track <pr> --title "..." --branch "..."  # track PR manually
fp untrack <pr>                         # stop tracking (also removes worktree if present)
fp ready <pr>                           # mark draft PR as ready for review
fp switch <pr> <id>                     # print worktree path for PR (create if needed); <id> is a session label (e.g. "claude-session-1")
fp switch <pr> <id> --force            # skip dirty-check on current worktree
fp switch <pr> <id> --adopt            # branch is checked out in main worktree — check out main there, create fp worktree
fp switch <pr> <id> --non-interactive  # skip all lifecycle prompts; apply safe defaults silently
fp unlock <branch>                      # remove the lock on a worktree branch so it can be switched to again
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

## Worked Example: Entering a Worktree to Fix Code

User: "Reproduce the CI failure in PR #42 locally and fix it."

```sh
# Step 1: get the worktree path (creates worktree if needed)
fp switch 42 claude-session-1 --force
# Output: /Users/me/projects/myrepo-worktrees/feat/my-branch

# Step 2: enter the worktree
EnterWorktree path=/Users/me/projects/myrepo-worktrees/feat/my-branch

# Step 3: get the failing check details
fp context 42 ci/test

# Step 4: fix, commit, push — then confirm
fp watch --wait-for ci-pass

# Step 5: exit worktree when done
ExitWorktree action=keep
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

Create new stacked branches from scratch:

```sh
fp new feat/step-1 --base main          # create branch + worktree from main
fps feat/step-1                         # switch in
# ... make changes, push ...
fp create "Step 1: ..."                 # create PR, now on feat/step-1

fp new feat/step-2 --base feat/step-1  # stack on top
fps feat/step-2
# ... make changes, push ...
fp create "Step 2: ..." --base feat/step-1
```

Track existing PRs and see the stack:

```sh
fp track 5    # base PR
fp track 6    # PR stacked on #5
fp track 7    # PR stacked on #6
fp ls
# Output:
# owner/repo
# #5 Step 1 title (feat/step-1)
#   └─ #6 Step 2 title (feat/step-2)
#       └─ #7 Step 3 title (feat/step-3)

fp rebase-stack
# ✓ rebased feat/step-2
# ✓ rebased feat/step-3
```

`fp ls`, `fp status --all`, and `fp watch` all show the stack tree with indented `└─` children.

Conflicts are reported by branch name. Resolve manually, then re-run `fp rebase-stack`.

## Feature Envelopes and Dep Slots

A **feature envelope** groups multiple PRs into a coordinated workspace. `fp feature up <name>` bootstraps all member PRs simultaneously — starting each app from its PR's worktree.

A **dep slot** (declared with `fp feature add-dep`) is a service that has no open PR — it runs from the main repo root instead. Use it when a feature depends on a service that hasn't changed (e.g., a shared backend) or when the artifact is built from the main branch.

Key behavior:
- Members with an open PR → bootstrap runs from that PR's worktree
- Dep slots (`pr=0`, `expected_branch=""`) → bootstrap runs from the main repo root
- `fp feature rebuild <name> --pr 0` re-runs the dep slot bootstrap without tearing down other members

### Worked Example: Chrome Extension + Backend PRs

Suppose your feature has a backend API PR and a Chrome extension. The extension is an **ephemeral build artifact** (it installs and exits), and you want to rebuild it from main while the backend PRs are running.

```sh
# Define app configs
fp app define-config backend \
  --bootstrap "docker compose up -d" \
  --teardown "docker compose down" \
  --health-check "curl -sf http://localhost:3000/health"

fp app define-config extension \
  --bootstrap "npm run build && npm run install-extension" \
  --teardown "echo done" \
  --ephemeral \
  --health-check "test -f dist/manifest.json"

# Build the feature envelope
fp feature new my-feature

# Add the backend PR (will run from its worktree)
fp feature add my-feature 42 --config backend

# Declare the extension as a dep slot (builds from main — no open PR)
fp feature add-dep my-feature extension

# Bring everything up
fp feature up my-feature
# Output:
#   ✓ bootstrapped backend (PR #42)
#   ✓ bootstrapped extension (main)

# Rebuild just the extension from main (after pulling changes)
fp feature rebuild my-feature --pr 0
# Output:
#   ✓ rebuilt extension (main)
```

The `--pr 0` flag targets the dep slot specifically, leaving the backend PR untouched.

### Health-Check Environment Variables

fp sets the following environment variables when running bootstrap, teardown, and health-check commands:

| Variable | Value |
|---|---|
| `FP_WORKTREE` | Absolute path of the worktree the app was started from |
| `FP_PR` | PR number (0 for dep slots) |
| `FP_INSTANCE` | Unique instance label (also set as `COMPOSE_PROJECT_NAME`) |
| `COMPOSE_PROJECT_NAME` | Same as `FP_INSTANCE` — scopes docker compose commands to this PR |

These variables are available to the **host shell** running the command, not inside containers. Use them on the host side of any command pipeline.

### Verifying Docker Volume Mounts

A service health-check that only tests liveness (`curl http://localhost:PORT/health`) will pass even if the container is mounted from the wrong directory (e.g., main instead of the PR worktree). To catch mount mismatches without changing the compose config, extend the health-check to verify the volume source using `docker inspect`:

```sh
fp app define-config backend \
  --bootstrap "docker compose up -d" \
  --teardown "docker compose down" \
  --health-check "curl -sf http://127.0.0.1:6363/health && \
    docker inspect \$(docker compose ps -q backend) \
      --format '{{range .Mounts}}{{.Source}} {{end}}' \
    | grep -qF \"$FP_WORKTREE\""
```

How this works:
- `docker compose ps -q backend` is scoped by `COMPOSE_PROJECT_NAME` (set by fp), so it finds the container for *this* PR's instance only
- `docker inspect ... --format '{{range .Mounts}}{{.Source}} {{end}}'` lists all host paths mounted into the container
- `grep -qF "$FP_WORKTREE"` checks that the PR's worktree is one of them — fails if the container is serving from main or another PR's directory

This gives a true mount-correct health signal without touching compose.yml.

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

fps 42 my-session   # enter worktree for PR #42 with session label (created if needed)
fps 87 my-session   # switch to PR #87 worktree
fps root            # return to main repo root from anywhere
fp ls               # shows owner/repo header + stack tree; errors if no GitHub remote
fp status --all     # shows owner/repo header + stack tree + task counts; errors if no GitHub remote
fp watch            # also shows lock status per PR
fp unlock <branch>  # explicitly remove a lock (required — locks are never auto-cleared)
fp untrack 42       # removes worktree + cleans up lock
```

Guards:
- Aborts if current worktree has uncommitted changes (use `--force` to override)
- Aborts if target worktree has any lock (live or dead) — must run `fp unlock <branch>` to clear it
- Lock PID is the session anchor: first ancestor with a TTY or first non-shell ancestor (durable across both terminal and agent contexts)

Lock files live in `.git/worktrees/<branch>/fp-lock` — never committed. Format: `{"pid": <n>, "kind": "agent", "id": "<session-label>"}`.

## Agent Worktree Protocol

**Criterion:** Every entry into a PR branch worktree must go through `fp switch <pr> <id>` — not `git worktree add` directly — so that lock files are written correctly, the path is canonical, and `fp status` shows the active lock. Non-compliance is observable: `fp status --all` will show no 🔒 lock for the PR.

**Lock lifecycle:**
1. `fp switch <pr> <id>` — writes lock with session label and durable anchor PID
2. Work in the worktree
3. `fp unlock <branch>` — explicitly removes the lock when done (or before another agent can take over)

If a lock shows `(dead)` in `fp status`, the previous session crashed — run `fp unlock <branch>` to clear it before switching.

When an agent needs to work inside a PR's worktree (fix CI, edit code, run tests):

1. Run `fp switch <pr> [--force]` — use `--force` when the current directory has untracked files.
   - The command prints an absolute canonical path. Capture it directly — no `..` resolution needed.
   - If the branch already has a worktree at a non-fp path, the error message says exactly how to relocate it.

2. Call `EnterWorktree path=<printed-path>`.

3. Do the work (fix CI, respond to review, run tests).

4. When done, call `ExitWorktree action=keep` (or `action=remove` if the branch is merged).

Anti-patterns:
- Do **not** guess the worktree path — always run `fp switch` first to get the canonical path.
- Do **not** try to resolve `..` segments — `fp switch` already prints an absolute path.
- Do **not** use `git checkout` inside a worktree — the branch is already checked out there.

## Write to Disk

This skill is written to `.claude/skills/fp/SKILL.md` inside the current git repository by running:

```sh
fp install-skills
```
