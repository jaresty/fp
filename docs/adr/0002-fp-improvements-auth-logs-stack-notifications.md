# ADR 0002: fp Improvements — Auth, CI Logs, Stack Management, Notifications, and Agent-Native Compliance

**Status:** Partially Implemented — gh auth fallback (#1), check deduplication (#3), empty branch auto-fetch (#4), rebase-stack API inference (#5), macOS notifications (#7), resolved threads subcommand (#2), Buildkite log pipeline (#6) done. CODEOWNERS/ready-unverified (#8) done. Agent-native audit (#9) not yet implemented.
**Date:** 2026-05-06

---

## Context

Since ADR 0001, fp has been used in production agentic workflows. Feedback from those sessions has surfaced nine structural gaps where fp either suppresses useful state, exposes false affordances, or fails to satisfy agent-native CLI principles. These gaps fall into four clusters:

1. **Auth and credential discovery** — fp doesn't delegate to `gh` CLI when `GITHUB_TOKEN` is absent, causing silent stale results instead of a clear error
2. **State visibility** — resolved review threads are invisible; check-run deduplication takes stale failing runs over fresh passing ones; empty branch fields on tracked PRs break all downstream commands
3. **Stack management** — `rebase-stack` hard-depends on `fpstate.json` for branch relationships, making it non-functional when state is missing or stale; squash merges produce messy downstream diffs
4. **Output and notifications** — Buildkite logs are truncated at the wrong end (errors are at the bottom); `fp watch` is pull-only with no push notification; agent-native CLI principles (structured output, bounded responses, introspection, async-awareness) are unevenly applied

---

## Decisions

### 1. Auth: Delegate to `gh` CLI When `GITHUB_TOKEN` Is Absent

**Decision:** At startup, if `GITHUB_TOKEN` is unset or empty, fp calls `gh auth token 2>/dev/null` and uses its output as the token. If both are absent, fp prints a hard error naming both remediation paths before executing any API call:

```
fp: no GitHub credentials found.
  Option 1: export GITHUB_TOKEN=<token>
  Option 2: gh auth login
```

fp does not silently continue with empty credentials and return stale or empty results.

**Rationale:** `gh` CLI already solves the auth problem; fp treating them as separate creates a false affordance — commands appear to work but return wrong data. The error message satisfies trevinsays Principle 3 (errors that enumerate valid options).

**Rejected:** Adding a `--gh-auth` flag to opt in. The correct action should require no configuration.

---

### 2. Resolved Threads: Add `fp threads` Subcommand

**Decision:** Add a `threads` subcommand:

```
fp threads [--open | --resolved] [<pr-number>]
```

- `--open` (default): current behavior — threads requiring author action
- `--resolved`: threads with `resolved: true`, including who resolved them, when, and what commit was pushed after the thread was opened

`fp watch` additionally emits an event when a thread transitions from open to resolved during a watch cycle.

**Rationale:** Resolved threads are the audit trail proving concerns were addressed. Making them invisible forces users to leave fp and re-examine GitHub, breaking the convergence loop. The subcommand separation keeps `fp status` clean while giving the resolved view a first-class surface.

**Rejected:** `fp status --show-resolved`. Status output is already dense; resolved threads are an audit action distinct from the readiness task list.

---

### 3. Check-Run Deduplication: Last-Write-Wins by Name

**Decision:** After fetching check-runs, fp groups them by `name`, retains only the entry with the latest `started_at` (using highest array index as tiebreaker for identical timestamps), then evaluates pass/fail against the deduplicated set. The same logic applies to commit statuses: group by `context`, keep latest.

**Rationale:** GitHub's check-runs API returns all historical runs. A stale failed run followed by a fresh passing re-run currently causes fp to surface a FixCi task for a failure that no longer exists. The fix must happen at the data layer (before task generation), not the display layer, because task generation consumes the same raw list.

**Rejected:** Deduplication at display layer only. FixCi task generation uses the raw list; display-only deduplication leaves the task generator broken.

---

### 4. Empty Branch: Enforce at Track Time, Auto-Fetch as Fallback

**Decision:** When `fp track <PR>` is called, fp attempts to populate the branch field in this order:

1. `--branch <name>` flag (explicit)
2. `gh pr view <PR> --json headRefName` (auto-fetch if gh auth is available)
3. If both fail: block the track and emit `fp track <PR> --branch <branch-name>` as the corrective action

fp additionally provides `fp repair <PR>` to backfill the branch field on already-tracked PRs that are missing it.

**Rationale:** An empty branch field is a latent failure — the track command succeeds but every downstream command that needs the branch silently breaks. The root cause is that the field has no enforcement at the point where it can be set. Auto-fetch covers the common case; the block covers the edge case when auth is unavailable.

**Rejected:** Hard-requiring `--branch` at all times. Too strict when auth is temporarily unavailable but will be restored.

---

### 5. Rebase-Stack: Infer Branch Relationships from GitHub API, Drop `fpstate.json` Dependency

**Decision:** `rebase-stack` no longer requires `fpstate.json` to determine stack order. Instead, it queries each tracked PR's `base.ref` from the GitHub API at command time and builds the dependency graph dynamically. The rebase executes in topological order derived from that graph.

Additionally, fp detects squash-merged base branches by comparing the tree SHA of the merge commit to the PR's head commit tree SHA. When a squash merge is detected, fp rebases downstream branches onto the squash commit's equivalent tree rather than the raw merge commit, producing clean downstream diffs.

When a PR was merged via the GitHub UI (rather than through fp's awareness), fp detects the divergent commit graph and surfaces a warning:

```
warning: PR #N appears to have been squash-merged outside fp.
Downstream branches have been rebased onto the equivalent tree.
Verify the result is correct before pushing.
```

**Rationale:** `rebase-stack` exposing a command that silently fails when state is absent is the definition of a false affordance. The stateless approach (GitHub API as source of truth for base relationships) works across machines and survives state file deletion. Squash-merge awareness addresses the "messy downstream diffs" problem by making fp understand the merge topology rather than blindly rebasing onto a commit that looks unrelated.

**Rejected:** Keeping local state and adding a repair command. Local state is inherently machine-specific and ephemeral; the command's reliability should not depend on it.

**Rejected:** fp owning the merge itself. Merge is outside fp's scope (ADR 0001). fp handles the rebase that follows a merge; it does not perform the merge.

---

### 6. Buildkite Logs: Structured Extraction Pipeline

**Decision:** `fp context <task-id>` replaces raw log truncation with a multi-stage extraction pipeline for Buildkite CI tasks:

1. Fetch the full log (or the Buildkite artifacts API if log is stored as an artifact)
2. Extract:
   - Failing step name
   - Last 100 lines of the failing step's output
   - All lines matching error/exception patterns (`Error:`, `FAILED`, `panic:`, exception stacktraces)
3. Return structured output:
   ```json
   {
     "step": "primary / rspec",
     "error_lines": [...],
     "context_lines": [...],
     "log_url": "https://buildkite.com/...",
     "full_log_available": true
   }
   ```
4. `log_url` is always included so the user can escape to the full log
5. `fp context <task-id> --full-log` streams or saves the complete raw log for cases where the extracted summary is insufficient (e.g., the failure is in the middle of output, not at the end, or spans multiple steps)

`--summarize` flag (opt-in) passes the extracted output through an LLM summarization pass for further compression. This is off by default due to added latency and cost.

`--full-log` writes to a temp file and prints the path, rather than dumping to stdout, since full Buildkite logs can be megabytes. The path is also included in `--json` output as `full_log_path`.

**Rationale:** Buildkite failures appear at the end of logs, not the beginning — front-truncation discards exactly the relevant content. Structured extraction converts the log from a text blob into a navigable artifact. The `log_url` field satisfies the principle that bounded responses must include an escape hatch to the full content.

**Rejected:** Raw truncation with a larger byte limit. The problem is not the limit size; it's truncating from the wrong end with no structure.

---

### 7. Mac System Notifications: Push on Watch Cycle State Transitions

**Decision:** `fp watch` detects the following state transitions per watch cycle and sends a macOS system notification:

- CI status changes to `fail` (new failure on a branch)
- CI status changes to `pass` (all required checks now passing)
- New review request received
- PR approved
- New thread opened requiring author response

Notifications use `osascript -e 'display notification "..." with title "fp: #<PR>"'`, which has no dependencies. An optional `--notifier terminal-notifier` flag enables richer notifications (click to open PR URL) when `terminal-notifier` is installed. On non-macOS systems, notifications are silently skipped.

**Rationale:** fp watch is currently pull-only — the user must observe it actively. The primary executor (an LLM agent) may be idle waiting for CI; a push notification enables immediate response without polling loops. This satisfies the "two-way I/O" principle from agent-native CLI design.

**Rejected:** Requiring `terminal-notifier` as a hard dependency. `osascript` is available on all macOS versions with no installation; it is the correct default.

---

### 8. CODEOWNERS: Surface Eligibility Uncertainty, Introduce `ready-unverified` State

**Decision:** fp introduces a `ready-unverified` state distinct from `ready`. A PR is `ready-unverified` when:

- The standard readiness criteria (ADR 0001) are met, AND
- fp cannot confirm that the approving reviewer is eligible under CODEOWNERS rules (because the GitHub team membership API returned 403/404 or the CODEOWNERS file requires a team fp cannot resolve)

When `ready-unverified`, fp emits:

```
warning: fp cannot verify CODEOWNERS eligibility for this approval.
Confirm that <reviewer> is a required reviewer for the changed files before merging.
```

If fp can successfully resolve team membership and the approver is not eligible, fp surfaces an `AwaitingReview` task indicating the specific team required.

**Rationale:** fp currently reports `ready` for PRs where the approval may not count under CODEOWNERS, creating a false terminal state. The `ready-unverified` state preserves the signal that the PR looks complete while being honest about what fp cannot verify. This is preferable to either false confidence or over-blocking.

**Rejected:** Full CODEOWNERS enforcement as a hard block. The GitHub team membership API frequently returns 403 for write-access tokens; enforcing would cause fp to block on PRs it cannot evaluate.

**Rejected:** Ignoring CODEOWNERS entirely and trusting GitHub's merge button to enforce it. fp marking a PR `ready` when it may not be creates false confidence in the convergence state.

---

### 9. Agent-Native CLI Compliance: Systematic Surface Audit

**Decision:** fp is audited against the trevinsays 10 agent-native CLI principles and the following gaps are closed:

| Principle | Gap | Fix |
|---|---|---|
| 2 — Structured output | `--json` missing on `threads`, `watch` events, `notifications` | Add `--json` to all subcommands |
| 3 — Errors that enumerate | Auth failure, missing state, and empty branch don't name corrective actions | Enumerated error messages on all failure paths (covered by decisions 1, 4 above) |
| 5 — Bounded responses | Log fetching is unbounded | Structured extraction pipeline (decision 6 above) |
| 7 — Three-layer introspection | No `agent-context` output or capability manifest | Add `fp agent-context --json` returning available commands, required auth, and current tracked PRs |
| 8 — Async-aware execution | No `--wait` flag on `fp watch` | Add `fp watch --wait-for <condition>` that blocks until a terminal state (e.g., `--wait-for ci-pass`, `--wait-for ready`) |
| 9 — Persistent identity | No profile system; auth and repo config must be re-supplied each session | Add `fp profile save/load` for named bundles of auth + repo config |

Principles 1 (non-interactive by default), 4 (safe retries), 6 (cross-CLI vocabulary), and 10 (two-way I/O feedback) are already satisfied or addressed by decisions above.

---

## Consequences

### Positive

- **Auth failures are immediately visible** instead of silently producing stale data. LLM agents can diagnose and fix the problem without a human debugging session.
- **Resolved threads become auditable** through a first-class command, enabling post-merge review verification.
- **Check-run state is always current** — no more FixCi tasks for failures that have already been re-run successfully.
- **Rebase-stack works across machines** without a state file, removing the most frequent cause of `rebase-stack` failure in production.
- **Buildkite failures are directly diagnosable** from within fp without context-switching to the Buildkite UI.
- **Notifications enable reactive rather than polling-based workflows** for both LLM agents and human operators.
- **CODEOWNERS gaps are surfaced** rather than silently misrepresented as ready.
- **fp satisfies the agent-native CLI principles** needed for reliable use as an MCP tool in LLM agent loops.

### Negative

- **`rebase-stack` now requires GitHub API availability** for every invocation (to fetch base branch relationships). Previously it could run offline against local state. In practice this is not a regression since `rebase-stack` failure modes with missing local state were worse.
- **Squash-merge detection via tree SHA comparison adds one API call per PR** in the stack during rebase-stack. This is acceptable latency for an infrequent operation.
- **`ready-unverified` introduces a new terminal-ish state** that downstream consumers (MCP callers, watch scripts) must handle. The JSON schema for PR status gains a new value.
- **Buildkite-specific extraction logic** must be maintained as a separate code path from generic check-run handling. Future CI backends would need equivalent extractors.
- **`osascript` notifications on macOS** require that fp is running in a context with notification permissions. In headless CI environments, notification calls will silently fail — which is the correct behavior, but should be documented.
