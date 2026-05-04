# ADR 0001: fp System Architecture

**Status:** Accepted  
**Date:** 2026-05-03

---

## Context

Software delivery as an external contractor or agentic LLM worker involves a fragmented feedback loop: CI failures arrive asynchronously, reviewer comments accumulate across multiple PRs, stacked branches drift out of sync, and nothing enforces that all open loops get closed. The gap is not intelligence — it is completeness and sequencing. Things get missed. PRs stall.

The goal of `fp` is to close this gap by owning the PR convergence loop: creating PRs, monitoring all feedback signals, maintaining a normalized task list of what is blocking readiness, and keeping the stack clean continuously as work is pushed. Merge itself is outside `fp`'s scope — it happens by whatever external means is appropriate (auto-merge policy, a reviewer clicking the button, another tool). `fp`'s job is to ensure that when merge happens, nothing was missed.

The primary executor is an LLM agent (Claude Code or equivalent) operating in its normal agentic mode — editing files, running bash, committing, and pushing. `fp` does not restrict the LLM's working freedom. It surfaces what needs doing and keeps the stack rebased.

`fp` is built as a CLI first. The CLI is the complete, independently useful system. An MCP server layer — thin wrappers over CLI commands — is a later addition that enables LLM agents to call `fp` as a tool without shell access. All core logic lives in the CLI; MCP adds no new behavior.

---

## Decision

`fp` is a CLI and MCP server that owns the PR convergence loop. The LLM is a fully autonomous agentic worker. The boundary between them is defined precisely by what each owns.

### What fp owns

- **Normalized task list.** `fp` is the authoritative source of "what is currently open and needs to be addressed" across all PRs in the stack. It computes this from live state — CI results, thread states, approval status — and rescans after every push to any branch in the stack.
- **Continuous rebase cascade.** Whenever a fix is pushed to any PR in a stack, `fp` rebases all downstream PRs in git-ancestry order. This happens on every push, continuously as work progresses across the stack. Every branch always has a current base.
- **Stack topology.** `fp` tracks which PRs depend on which, computed from git ancestry (`git merge-base`), not from code host PR metadata. It detects when a downstream PR's base has drifted and triggers rebase automatically.

`fp` does not own or execute merge. Merge is a separate concern handled outside the system.

### What the LLM owns freely

- File editing, running code locally, committing, pushing.
- Deciding how to fix a CI failure or respond to a thread.
- Calling `fp` MCP tools to retrieve context and check current status.

Push is not gated by `fp`. Pushing is cheap and safe. CI is the real validator: results from pushed commits flow back to `fp` via code host API polling. After pushing, the LLM calls `get_tasks()` to see what remains open.

### PR tracking

`fp` maintains a local state file at `.git/fp/state.json` (inside `.git`, never committed). This file records which PRs `fp` is actively watching for this repository, along with cached state (thread states, last-polled timestamps) to avoid redundant API calls.

PRs enter the tracked set in two ways:
- Automatically, when created via `fp create`
- Explicitly, via `fp track <pr-number>`

PRs leave the tracked set when closed or merged on the code host (detected on next poll), or explicitly via `fp untrack <pr-number>`.

Stack relationships are not stored in the state file — they are computed from git ancestry on every invocation. The state file tracks individual PRs; `fp` derives stacks dynamically.

### CLI commands

`fp` is invoked as a CLI. All commands operate on the current git repository. Commands that reference a specific PR default to the PR associated with the current branch if one is tracked; otherwise they require an explicit PR number.

| Command | Behaviour |
|---|---|
| `fp init` | Authenticate with the code host and configure the local repo for `fp` use. |
| `fp create` | Create a draft PR for the current branch on the code host; auto-tracks it. |
| `fp track <pr>` | Add a PR to the tracked set. |
| `fp untrack <pr>` | Remove a PR from the tracked set. |
| `fp ls` | List all tracked PRs with their current status summary (ready / N tasks open). |
| `fp status` | Print the task list for the current branch's PR. Empty array means ready to merge. |
| `fp status --all` | Print task lists for all tracked PRs, grouped by stack, ordered bottom-up by git ancestry. |
| `fp watch` | Poll all tracked PRs continuously; print events as they arrive (new CI results, new comments, new approvals). Exits on interrupt. |
| `fp watch --once` | Poll once; print any new events since last poll; exit. |
| `fp rebase-stack` | Rebase all downstream tracked PRs in git-ancestry order onto the current branch tip. Reports conflicts as tasks rather than aborting silently. |
| `fp context <task-id>` | Print full context for one task: CI log verbatim, thread comment, list of relevant changed files. |

All commands support `--json` for machine-readable output. Human-readable output is the default.

`fp watch --once` is the primary entry point for automated use: poll, emit any new events as JSON, exit. An LLM or script calls this to check for new feedback without holding a long-running process.

### MCP server (future)

The MCP server is a thin protocol layer over the CLI commands. It adds no logic. Each MCP tool maps directly to a CLI command:

| MCP tool | CLI equivalent |
|---|---|
| `get_tasks()` | `fp status --json` |
| `poll_events()` | `fp watch --once --json` |
| `get_context(task_id)` | `fp context <task-id> --json` |
| `rebase_stack()` | `fp rebase-stack --json` |

The MCP server is built after the CLI is complete and verified. It is not required for the system to function — an LLM with shell access can call `fp` commands directly.

The LLM calls these tools to orient itself. When `get_tasks()` returns an empty array, `fp`'s job for this PR is done. Merge happens externally.

### Task schema

Each item returned by `get_tasks()` is a Task:

```json
{
  "id": "task-uuid",
  "type": "fix_ci | respond_thread | resolve_conflict | awaiting_review | awaiting_ci",
  "pr": { "number": 45, "branch": "fix/parser" },
  "blocking": true,
  "context_ref": "task-uuid",
  "done_when": "All required CI checks pass with no new failures introduced"
}
```

`blocking: true` means this task must be resolved before the PR is ready to merge. `awaiting_review` and `awaiting_ci` are informational — the LLM has done its part and is waiting on humans or CI to respond. The LLM does not act on non-blocking tasks; `fp` will surface new blocking tasks when the wait resolves.

### Internal state model

`fp` maintains the following state per PR, refreshed by polling the code host:

- **Checks:** each required and optional CI check, with status `pass | fail | pending`
- **Threads:** each review thread, with state `open | addressed | stale | resolved`
- **Approval:** whether at least one approving review is present
- **Draft status:** whether the PR is still a draft

**Thread state machine:**  
`open` → `addressed` (LLM posts a reply or fix) → `stale` (a subsequent commit touches the same file) → `resolved` (reviewer dismisses or closes the thread)

`fp` detects the `addressed → stale` transition automatically. A thread that was addressed but then had its surrounding code changed is re-surfaced as a new `respond_thread` task. This prevents silent regression of reviewer intent.

**Stack topology** is computed from git ancestry (`git merge-base`), not from code host PR relationships. The code host's "base branch" field is unreliable in stacked workflows.

### Readiness definition

A PR is **ready** (task list empty) when all of the following hold simultaneously against live state:

1. No required CI checks are failing or pending
2. No review threads are in state `open` or `stale`
3. At least one approving review is present
4. The PR is not in draft status

Stack readiness is per-PR, not recursive. Each PR in a stack has its own task list. `fp` keeps all PRs in the stack rebased so that each can be worked on and evaluated independently.

### Execution loop

`fp` manages multiple PRs simultaneously. A typical workflow involves a stack of related PRs, each receiving independent feedback that must be addressed before the stack can progress.

```
1.  fp watch --stack — poll all PRs in the stack; print events as they arrive
2.  Executor (LLM or human) runs fp status --stack — sees open tasks across all PRs
3.  Executor picks a task from any PR; works freely on that branch
    (edits files, runs code, commits, pushes)
4.  fp detects push to that branch; rebases all downstream PRs in git-ancestry order
5.  CI evaluates pushed commits on all affected branches; results arrive at code host
6.  fp polls code host; updates internal state for all PRs (checks, threads, approvals)
7.  Executor runs fp status --stack — sees updated task lists across all PRs
8.  Executor repeats steps 3–7, working on whichever PR has actionable tasks
9.  As each PR's task list empties, it is ready for merge
10. Merge happens externally; downstream PRs are already rebased and continue their loops
```

Step 4 is key: `fp` rebases downstream PRs immediately when a push lands on any branch in the stack, not at merge time. The cascade runs on every push.

### Host and CI abstraction

`fp` implements a host adapter pattern. Each code host (GitHub, GitLab) has a concrete adapter that translates host-specific API responses into `fp`'s internal event types: `CheckEvent`, `CommentEvent`, `ApprovalEvent`, `PREvent`. `fp`'s core logic never calls host APIs directly.

`fp` is CI-agnostic. It reads check/status results from the code host API. It does not know or care whether the underlying CI system is GitHub Actions, GitLab CI, CircleCI, or anything else.

---

## Consequences

### Positive

- **Completeness is a hard invariant.** `fp status` returning an empty task list is the only signal that a PR is ready. Nothing can be silently missed — `fp` rescans the full stack after every push to any branch.
- **LLM has maximum autonomy.** No artificial restrictions on commits, pushes, or local iteration. The LLM works at its natural pace.
- **CI is the validator.** No local test infrastructure is required in `fp`. The same CI that will gate merge validates intermediate pushes.
- **Thread staleness is tracked continuously.** The `addressed → stale` transition means reviewer intent is never silently discarded after a code change.
- **Stack stays clean automatically.** Downstream PRs are rebased on every push. The LLM never encounters a stale-base PR as a surprise.
- **Merge is decoupled.** `fp` has no opinion on how or when merge happens. It works with any merge strategy: auto-merge, manual, branch protection rules, another tool.
- **Host and CI portability.** The adapter pattern means `fp` can support GitHub and GitLab without changing core logic.

### Negative

- **CI latency is in the loop.** The LLM cannot get validation feedback instantly. After a push, it must wait for CI to complete before `get_tasks()` reflects the updated check state. For slow CI pipelines this increases iteration time.
- **`fp` requires polling the code host.** In the absence of webhooks, there is lag between a CI result arriving and `fp` surfacing it. Polling frequency is a tunable tradeoff between latency and API rate limits.
- **Rebase cascade can encounter conflicts.** When `fp` rebases a downstream PR after a push, it may encounter conflicts it cannot resolve automatically. These surface as `resolve_conflict` tasks, requiring LLM or human intervention before that branch's work can continue.
- **Stack topology from git requires a local clone.** `fp` must have access to a local git clone with up-to-date remote refs to compute ancestry correctly. This is a deployment constraint.
- **Tracked set requires explicit management.** PRs created outside `fp` must be manually added with `fp track`. There is no automatic discovery of PRs you did not create or track.
- **`fp` does not control merge.** If merge happens before all tasks are closed (e.g., a human merges manually), `fp` cannot prevent it. The guarantee is advisory, not enforcement.

---

## Rejected Alternatives

### fp gates or owns merge

Rejected. Merge is a separate concern with its own policies (branch protection, required reviews, auto-merge rules). `fp` owning merge would require it to understand and reimplement those policies, creating overlap and fragility. `fp`'s value is convergence, not gate-keeping. When `get_tasks()` returns empty, the PR is ready; how and when it merges is outside `fp`'s scope.

### Rebase cascade triggered at merge, not at push

Rejected. If downstream PRs are only rebased after merge, the LLM working on PR #3 will be working against a stale base for the entire duration that PR #2 is being fixed. Conflicts discovered at merge time are more expensive than conflicts discovered continuously during development. Rebasing on every push keeps the stack current and surfaces conflicts early.

### fp gates push, not just readiness

Rejected. Push is cheap and has no permanent consequence. Gating push would add friction to the LLM's working loop without improving safety. CI already validates pushed commits. The only state that matters for correctness is what is open at the time someone chooses to merge.

### Local test validation before each push

Rejected. Running a full local test suite duplicates what CI does and adds latency to the working loop. CI is more reliable (consistent environment, same system that gates merge) and already in the loop. Local validation is the LLM's own choice if it wants faster feedback before pushing.

### jj (Jujutsu) for stack management

Rejected. `fp` owns git rebase operations directly using standard git primitives (`git rebase`, `git merge-base`, `git log --graph`). Adding an external VCS tool dependency would introduce API instability, deployment complexity, and an unclear upgrade path. Stack operations here are mechanical (rebase downstream branch onto new tip of upstream) and do not require jj's ergonomic features.

### Static task list (snapshot at session start)

Rejected. In a stacked PR workflow, pushing a fix to PR #2 immediately changes the base of PR #3, which may cause new CI failures or stale previously-addressed threads. A snapshot taken at session start will be wrong after the first push. `get_tasks()` must rescan live state on every call to remain correct.

### Separate sandbox / workspace isolation enforced by fp

Rejected. Early design considered `fp` creating an isolated workspace and controlling LLM file access. This was rejected because it duplicates what a Claude Code agentic session already provides, adds significant complexity to `fp`, and conflicts with the principle that the LLM should have maximum working freedom. The LLM's workspace is its own session. `fp`'s role is feedback and stack management, not file access control.
