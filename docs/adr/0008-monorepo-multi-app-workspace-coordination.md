# ADR-008: Monorepo Multi-App Workspace Coordination

**Status:** Proposed

## Context

fp currently models a workspace as a single app's context and issues one change request (CR)
per app. This creates compounding problems when working in a git monorepo with multiple
independently startable applications:

1. **No per-feature CR grouping.** A feature requiring changes across App A and App B
   produces two CRs with no expressed relationship. There is no way to activate "this
   feature" as a unit.

2. **No lifecycle management.** fp has no awareness of how to start or stop an app within
   its worktree. Starting, health-checking, and stopping apps is done manually, outside fp.

3. **No conflict detection.** Two worktrees for the same app, or two apps sharing a port,
   can collide silently. fp has no way to warn before activating a set of apps that would
   conflict with something already running.

fp does not own the applications. The teams building those apps are not fp users. Any
lifecycle configuration must therefore live outside the app repositories, in fp's own config
store, authored by the fp operator.

**Primary user model:** The primary consumer of fp commands is an LLM agent, not a human
typing interactively. All commands must support non-interactive operation. Interactive prompts
are surfaced only when stdout is a TTY; otherwise the safe default is applied silently and
the decision is reported in structured output.

## Decision

Introduce two new primitives: **named app configs** (lifecycle definitions, stored outside
repos) and **feature envelopes** (groupings of CRs activated together). fp takes ownership
of the process lifecycle — bootstrap, health check, teardown — using these configs.

---

### Primitive 1: Named App Configs

App configs live in `~/.fp/config.toml` (or equivalent fp state store), not in the
application repo. Each config defines how to start and stop one app and how to verify
that the correct instance is alive.

```toml
[app-configs.payments-api]
bootstrap       = "docker-compose up -d"
teardown        = "docker-compose down"
startup_timeout = "60s"
# health_check omitted → fp uses automatic detection (see Health Check section)

[app-configs.checkout-service]
bootstrap       = "npm start"
teardown        = "pkill -f checkout-service"
startup_timeout = "30s"
# health_check omitted → fp uses PID liveness
```

Config is per-fp-operator, not per-team. Two developers can have different `payments-api`
configs reflecting their local environment.

#### Assignment

A named config is assigned to a repo, and inherited by all PRs on that repo:

```
fp app set-config <repo> <config-name>      # all PRs on this repo use this config
fp pr set-config <pr#>   <config-name>      # override for one specific PR
```

---

### Primitive 2: Feature Envelopes

A feature envelope is a named set of PRs that fp activates together for local testing.

```
fp feature new <name>              # create a feature envelope
fp feature add <name> <pr#>        # add a PR to the feature (auto-tracks if not tracked)
fp feature up   <name>             # bootstrap all PRs in the feature
fp feature down <name>             # tear down all PRs in the feature
fp feature list                    # list features and their member PRs
fp feature list --running          # list only features with live instances
fp feature status <name>           # health-check all members
```

`fp feature add` automatically tracks any untracked PR before adding it to the envelope.
This removes the requirement that users run `fp track` separately before building a feature.

**Single-member envelopes** are permitted. They carry the same lifecycle tracking, health
checking, and conflict detection as multi-member envelopes. `fp pr up <pr#>` is syntactic
sugar for creating and immediately activating a single-member feature envelope.

**A feature envelope is a local coordination primitive, not a GitHub entity.** Each member
PR continues to progress through its own review and merge cycle independently. fp does not
create a cross-app PR or cross-app branch. The feature envelope exists only in fp's local
state.

Convention-based grouping is offered as a zero-cost supplement when stdout is a TTY: when
branch names share a common prefix (e.g. `feat/payments-*`), fp surfaces a suggestion to
group them into a feature envelope. This suggestion is suppressed in non-interactive mode.

---

### Lifecycle Execution

When fp runs any lifecycle command (bootstrap, teardown, health_check), it injects the
following environment variables:

| Variable               | Value                                                |
|------------------------|------------------------------------------------------|
| `FP_INSTANCE`          | `fp-<org>-<repo>-<pr-number>` (URL-safe, lowercased) |
| `FP_WORKTREE`          | Absolute path to the worktree                        |
| `FP_PR`                | PR number                                            |
| `COMPOSE_PROJECT_NAME` | Same as `FP_INSTANCE`                                |

`FP_INSTANCE` uses the full `org/repo` slug (e.g. `acme/payments-api` → `fp-acme-payments-api-123`)
to ensure uniqueness across repos with the same name. `COMPOSE_PROJECT_NAME` is injected
automatically into all lifecycle commands. Any bootstrap command that invokes `docker-compose`
is therefore automatically namespaced per PR instance with no changes to the bootstrap
command or compose file required.

**Caveat:** Bootstrap scripts that unset or override environment variables before calling
`docker-compose` will not inherit `COMPOSE_PROJECT_NAME`. Such scripts must set it
explicitly or use `$FP_INSTANCE` directly.

---

### Non-Interactive Operation

All lifecycle commands that would prompt interactively when stdout is a TTY have
non-interactive counterparts:

```
fp feature up <name> --yes         # tear down conflicting features without prompting
fp feature up <name> --no          # abort if any conflict is detected
fp switch <pr#> --non-interactive  # skip all lifecycle prompts; apply safe defaults
fp rebase-stack --non-interactive  # warn via structured output only; do not block
```

When stdout is not a TTY (i.e. fp is invoked by an LLM agent or script), fp behaves as
if `--non-interactive` were passed. The safe default for each prompt is:

| Prompt | Non-interactive default |
|--------|------------------------|
| Tear down conflicting feature? | No — report conflict, abort activation |
| Leave running instance up on switch? | Yes — leave it up |
| Start feature on switch to member PR? | No — do not start |
| Continue rebase with stale instance? | Yes — continue, emit structured warning |

Structured warnings and decisions are always included in JSON output (`--json` flag) so
LLM agents can observe and act on them.

---

### Process State Store

The current fp Store struct (tracked PR numbers, PR cache) does not include process state.
This ADR requires a new **process state store** alongside the existing store, persisting:

- Which PRs have been activated via `fp feature up` or `fp pr up`
- The expected branch name at activation time (for branch-drift detection)
- The PID of the bootstrap process group (for direct-process liveness)
- The feature envelope each PR belongs to (if any)

This is a lightweight append-only file (e.g. `~/.fp/process-state.json`) separate from
the main store to isolate volatile runtime state from stable PR metadata. It is read by
`fp status`, `fp feature status`, and `fp watch`; written by `fp feature up/down` and
`fp pr up`.

On worktree deletion, process state entries referencing the deleted worktree path are
flagged as stale on next access and reported as `(worktree missing)` in status output
rather than causing an opaque error.

---

### Health Checking

A feature envelope member is **healthy** when all three dimensions pass. fp evaluates them
independently and reports each on the status surface:

| Dimension | Method | Unhealthy when |
|-----------|--------|----------------|
| Process alive | PID liveness or Docker volume filter | process dead or no matching container |
| Service responding | explicit `health_check` command | command exits non-zero |
| Branch correct | `git -C $FP_WORKTREE rev-parse --abbrev-ref HEAD` | branch name ≠ PR's branch |

The branch check is always performed — it is a git command (instant), requires no app
config, and works for all runtime types. The process liveness probe (PID check or Docker
volume filter) is subject to a **2-second cap per PR** — if it times out, the process
dimension reports `timeout` and the branch check still runs and reports independently. The
2-second cap applies only to the process liveness probe, not to the branch check.

fp records the expected branch name in the process state store when `fp feature up` is run.
A worktree that has drifted to a different branch is unhealthy in the same sense as a
crashed process: the running code no longer corresponds to the PR being tested.

**Distinction: branch drift vs. commit staleness**

Branch drift (the worktree HEAD is on a different branch name) is detected by the branch
check. Commit staleness (the correct branch, but tip has moved after a rebase while the
process still runs old code) is not detectable by branch name alone — it is surfaced only
by the rebase warning described in the `fp rebase-stack` section below.

Status output reflects all three dimensions:

```
$ fp feature status auth-refactor
  payments-api  (PR #123)  ✓ running  ✓ healthy  ✗ wrong branch (main ≠ feat/payments)
  checkout-svc  (PR #456)  ✓ running  ✓ healthy  ✓ branch ok
```

**Process liveness — precedence:**

**1. Explicit `health_check` command (always wins)**

If the app config specifies a `health_check` shell command, fp runs it. Exit 0 = healthy;
non-zero = unhealthy. All injected env vars are available.

**2. Docker volume filter (automatic default for compose-based apps)**

If no explicit health check is configured and fp detects the bootstrap command invokes
`docker-compose`, fp runs an exact-path volume filter:

```
docker ps --filter volume=<exact-worktree-path> --filter status=running -q
```

The filter matches containers whose bind mount source is exactly `$FP_WORKTREE` — not
a prefix match. This prevents false positives when one worktree path is a substring of
another (e.g. `/repos/payments/pr-12` and `/repos/payments/pr-123`).

If any running container has a bind mount exactly matching the worktree path, it is
identified as belonging to this instance. This requires no changes to the compose file
or application.

Failure modes:
- Apps with no bind mounts (self-contained image) must provide an explicit `health_check`.
- Bootstrap scripts that unset environment variables may fail to namespace containers via
  `COMPOSE_PROJECT_NAME`; the volume filter remains valid in that case if bind mounts exist.

Known caveat: if the app's compose file specifies `container_name:`, Docker does not apply
the project-name prefix and a second instance cannot start. fp warns at `fp feature up` if
it detects `container_name:` in a compose file within the worktree.

**3. PID liveness (fallback for direct processes)**

If neither of the above applies, fp checks whether the process spawned via bootstrap is
still alive (`kill -0 $FP_BOOTSTRAP_PID`). fp stores `FP_BOOTSTRAP_PID` in the process
state store at activation time.

Caveat: bootstrap commands that spawn children and exit (launcher scripts) will appear dead
immediately. For these, an explicit `health_check` is required.

---

### Integration with `fp status`

`fp status` reads the process state store in addition to fetching GitHub PR state. For any
PR that has a record in the process state store, `fp status` adds a health column:

```
$ fp status
  #123  feat/payments   ✓ CI  · 1 thread  | instance: ✓ running  ✗ wrong branch
  #456  feat/checkout   ✓ CI  · 0 threads | instance: ✓ running  ✓ healthy
  #789  feat/auth       ✗ CI  · 2 threads | (no instance)
```

PRs with no process state record show `(no instance)`. The health column is absent for PRs
that have never been activated via `fp feature up` or `fp pr up`, to avoid noise.

`fp status --json` includes the health state as structured fields alongside existing task
output, enabling LLM agents to observe and act on lifecycle state.

`fp watch` surfaces the same signal alongside CI status for a unified view of remote (CI)
and local (running instance) state.

**Multi-repo note:** The health column appears for any PR in the process state store,
regardless of which repo `fp status` is invoked for. PRs from other repos in the same
feature envelope are visible via `fp feature status <name>`, not `fp status`.

---

### Conflict Detection and Teardown Gate

Before executing any activation (`fp feature up`, `fp pr up`, or single-PR startup from
`fp switch`), fp health-checks all other known feature envelopes. If any member of a
different feature is currently live, fp responds based on mode:

**Interactive (TTY):**
```
Feature "payments-v2" has running instances:
  payments-api (PR #123) — live
  checkout-service (PR #98) — live

Tear them down before starting "auth-refactor"? [y/N]
```

**Non-interactive:** fp aborts activation and exits non-zero with a structured error:
```json
{ "error": "conflict", "blocking_feature": "payments-v2", "blocking_prs": [123, 98] }
```

The LLM agent can then call `fp feature down payments-v2 --yes` before retrying.

On teardown confirmation (or `--yes` flag), fp runs teardown for each live instance in the
conflicting feature before proceeding with bootstrap.

**Partial bootstrap failure:** If bootstrap succeeds for some PRs in a feature but fails
for others, fp reports which succeeded and which failed, leaves the successful instances
running, and exits non-zero. The user or agent is expected to call `fp feature down <name>`
to clean up before retrying. fp does not automatically roll back successful bootstraps.

---

### Integration with `fp switch`

`fp switch` and `fp feature up/down` are orthogonal: `fp switch` manages **git context**
(which worktree is active for editing), while `fp feature up/down` manages **process
lifecycle** (what is running). A feature can be running without the user being switched
to any of its member PRs.

`fp switch` gains lifecycle awareness as a non-breaking layer. All lifecycle prompts are
subject to non-interactive mode (suppressed when stdout is not a TTY; safe default applied).

**Switching away from a PR that has a live instance (TTY):**
```
$ fp switch 456
  payments-api (PR #123) is running — leave it up? [Y/n]
```
Non-interactive default: leave it up. fp switch completes regardless.

**Switching to a PR that belongs to a feature envelope (TTY):**
```
$ fp switch 456
  PR #456 is part of feature "auth-refactor" (payments-api, checkout-service).
  Start the full feature? [y/N]
  Start just this PR?    [y/N]
  Skip startup           [Enter]
```
Non-interactive default: skip startup. The switch itself completes; no processes started.

Whichever startup path is chosen — full feature or single PR — the same conflict detection
and teardown gate applies before bootstrap begins. Single-PR startup via `fp switch` does
not bypass conflict detection.

**Switching to a PR with no app config assigned:** Silent. Behaves exactly as today.

**Switching to a PR with an already-live instance:** fp notes it and skips the startup
prompt.

**Health-check timeout during switch:** fp caps the liveness probe at 2 seconds. If it
times out, the prompt is skipped and the switch completes. Timeout is reported in `--json`
output.

---

### Impact on Existing Commands

#### `fp switch` — teardown side effect (CLASH → resolved)
See Integration with `fp switch` above. Key constraint: teardown never happens silently
or by default; `fp switch` always completes regardless of lifecycle choice.

#### `fp context` — single-active-PR assumption (CLASH → resolved)
`fp context` retains its current single-value contract: it returns the PR the user last
switched to (the editing context). It never returns a list. The set of currently running
PRs is a separate concept accessible only via `fp feature list --running` or
`fp feature status <name>`. These two concepts are kept separate in the data model and
in command output.

#### `fp merge` — envelope staleness and missing post-merge hook (CLASH → resolved)
`fp merge` gains a post-merge lifecycle callback. On merge, fp checks the feature envelope
store and removes the merged PR from any envelope it belongs to:
```
Merged PR #123. Removed from feature "auth-refactor" (2 members remaining).
```
If the envelope becomes empty, fp prompts (TTY) or auto-deletes (non-interactive) and
reports the deletion in structured output.

For merges performed outside fp (GitHub UI, `gh pr merge`, etc.): `fp feature status`
validates envelope members against GitHub PR state on each invocation. Closed PRs are
flagged as `(merged — remove with: fp feature remove <name> <pr#>)` rather than
causing silent staleness.

#### `fp rebase-stack` — live instance staleness (CLASH → resolved)
Before rebasing, fp performs a single health-check pass across all worktrees in the stack.
Worktrees with live instances are identified upfront (not per-worktree during rebase).
In TTY mode, fp prompts once for all affected instances. In non-interactive mode, fp
continues and emits structured warnings for each stale instance:
```json
{ "warning": "stale_instance", "pr": 123, "app": "payments-api" }
```
On completion, fp emits a restart reminder (TTY) or structured field (non-interactive).

#### `fp watch` — lifecycle visibility (BENEFIT)
Additive. Users without app configs see no change to watch output.

#### `fp merge` — tested-state traceability (BENEFIT)
Additive advisory only. "This PR has no recorded live run" is a soft warning, not a block.
Surfaced in JSON output as `"live_run_recorded": false`.

#### `fp reply` — no interaction (NEUTRAL)
No changes required.

---

## Implementation Stages

The full design touches the process state store, three new command families, and four
existing commands. Shipping everything at once maximises regression risk. The following
staged sequence delivers value incrementally while keeping existing behavior intact at
every stage boundary.

### Hard Dependencies (must not be violated by any staging)

1. **Process state store gates everything.** Every lifecycle feature reads or writes
   `~/.fp/process-state.json`. Nothing that depends on it can ship before it exists.
   If `fp feature up` ships before the store, activations are unrecorded, conflict
   detection silently passes, and health checks query nothing.

2. **Health check gates conflict detection.** The teardown gate is meaningless without
   a working health check. These two must ship in the same stage.

3. **Named app configs gate lifecycle execution.** `fp feature up` is a no-op until
   a config is assigned to the repo/PR. The config system ships before or with lifecycle
   execution, never after.

4. **`fp feature add` auto-track must ship with `fp feature add`.** Without it, adding
   an untracked PR to an envelope causes an opaque lookup failure on `fp feature up`.

### Risk Constraints on Existing Command Changes

- **`fp switch` and the process state store must not ship in the same release.** `fp switch`
  is a core command; a store read bug would break the most-used workflow with no fallback.

- **`fp status` health column must have a hard fail-open fallback.** Any store read failure
  silently omits the health column and continues. This must be enforced at the code level.

- **`fp merge` envelope cleanup must never affect the merge exit code.** Envelope write
  failures are logged and reported but do not fail the merge.

- **`fp rebase-stack` health-check pass must be capped at the stack level and fail-open.**
  A 5-PR stack at 2 seconds per PR costs 10 seconds before rebasing begins. Cap the total
  health-check pass, not per-PR. On timeout, emit a structured warning and proceed.

### Staged Delivery Sequence

**Stage 0 — Process state store** *(foundation; no user-visible surface)*
Implement `~/.fp/process-state.json` read/write with schema for: activated PRs, expected
branch at activation time, PID of bootstrap process group, feature envelope membership.
No commands consume it yet. Zero user-visible change. This stage is permanent
infrastructure — it cannot be reverted once later stages depend on it.

**Stage 1 — Named app configs and assignment** *(additive; no lifecycle execution)*
Implement `~/.fp/config.toml`, `fp app set-config`, `fp pr set-config`. Users can define
and assign configs. Nothing executes them yet. Zero risk to existing commands.

**Stage 2 — `fp pr up`, `fp feature up/down`, health check, conflict detection** *(new commands only)*
Implement bootstrap/teardown/health-check loop and the teardown gate as entirely new
commands. Existing commands are untouched. Health check (all three dimensions) and conflict
detection ship together in this stage — they are mutually dependent. The process state store
is now populated on activation. Feature envelope create/add/list also ships here.
`fp feature add` auto-tracking of untracked PRs ships here.

**Stage 3 — `fp feature status` and `fp feature list --running`** *(new read surface)*
Pure reads of the process state store. No writes to existing commands. Fully additive.

**Stage 4 — `fp status` health column** *(additive modification to existing command)*
Add the health column with hard fail-open: store read failure silently omits the column.
Ship only after Stage 3 is proven stable so the store read path is known-good before
`fp status` depends on it. `--json` output includes health state as structured fields.

**Stage 5 — `fp switch` lifecycle awareness** *(lowest-risk existing command modification)*
Add lifecycle prompts and feature-membership surfacing to `fp switch`. Non-blocking by
design — the switch always completes regardless of lifecycle choice. Ship after Stage 4
so the health check is known-good before `fp switch` depends on it.

**Stage 6 — `fp rebase-stack` warning** *(existing command modification with fail-open)*
Add a stack-level health-check pass before rebasing, with a stack-level total timeout and
fail-open behavior: on timeout, emit a structured warning and proceed with the rebase.
The rebase logic is unchanged.

**Stage 7 — `fp merge` envelope cleanup** *(existing command modification with failure isolation)*
Add post-merge envelope cleanup with hard failure isolation: merge exit code is never
affected by envelope write failures. Validation of closed-PR members in `fp feature status`
covers the gap for merges performed outside `fp merge`.

### Deferred (not part of initial delivery)

- **`fp watch` health integration** — benefit, not correctness. Defer until Stages 0–4 stable.
- **Convention-based branch-prefix grouping suggestions** — low value, noise risk. Defer.
- **Cross-repo `fp status` envelope display** — complexity without urgency. Defer.

---

## Alternatives Considered

**Port reservation at CR creation.** Rejected: fp does not own application configuration
and cannot guarantee apps bind to assigned ports.

**Central resource registry / OS flock.** Superseded by the teardown gate.

**Compound workspace model / named workspace sets.** Superseded. fp already has one
worktree per PR on disk; the missing primitive was lifecycle management, not workspace
abstraction.

**Workspace stack (LIFO push/pop).** Rejected: LIFO does not support simultaneous
multi-app activation.

**CR tagging.** Rejected: tags carry no semantic weight and cannot serve as an activation
unit.

**Namespace isolation per app.** Deferred. macOS network namespace support is limited.
The design is compatible with future migration.

## Consequences

- `fp feature` and `fp pr up` introduce new top-level commands and persisted entity types.
- A new **process state store** (`~/.fp/process-state.json`) is required alongside the
  existing Store struct. This is a schema addition, not a migration of existing data.
- `fp app set-config`, `fp pr set-config` require a config registry in fp's state store.
- `FP_INSTANCE` uses the full `org/repo` slug to ensure uniqueness across same-named repos.
- `COMPOSE_PROJECT_NAME` is injected automatically and silently into all lifecycle commands.
- Bootstrap scripts that unset environment variables must set `COMPOSE_PROJECT_NAME`
  explicitly using `$FP_INSTANCE`.
- Bootstrap commands using launcher scripts (spawn-and-exit) must provide an explicit
  `health_check` command.
- All interactive prompts are suppressed when stdout is not a TTY; safe defaults apply.
  All decisions and warnings are included in `--json` output for LLM agent consumption.
- `fp feature add` auto-tracks untracked PRs; `fp track` is no longer a prerequisite
  for feature envelope membership.
- Single-app, single-PR workflows are unaffected: a CR without a feature envelope and
  without an app config behaves exactly as today.
- Merges performed outside `fp merge` (GitHub UI, `gh`) require manual envelope cleanup
  via `fp feature remove <name> <pr#>`; `fp feature status` detects and flags these.
