# ADR-003: Stacked PR Rebase Strategy — `--onto` vs. Plain Rebase

**Status:** Accepted

## Context

When a user's stacked PR workflow involves a parent branch that has been merged to `main`,
`fp rebase-stack` currently runs `git rebase <parent>`. This fails the job of advancing a
child branch to a clean state after its parent lands: `git rebase <parent>` replays commits
already present in `main`, producing conflicts or duplicate commits.

The correct primitive is `git rebase --onto <new-parent> <old-parent-sha> <child-branch>`.

A second invariant is also expected: the file diff of a child PR vs. its parent should be
semantically unchanged after a rebase (modulo base movement). `fp` does not currently verify
or enforce this, so silent corruption is possible.

## Decision

1. `fp rebase-stack` detects when a parent branch has been merged to `main` and switches to
   `git rebase --onto origin/main <old-parent-sha> <child-branch>` instead of the plain rebase.
2. After every rebase, `fp` verifies that the diff of the child vs. its new base matches the
   diff vs. its old base. Any divergence is surfaced as a warning before force-pushing.
3. `fp` must persist or infer the pre-merge parent SHA (e.g., from the PR's recorded base ref
   before merge) to construct the `--onto` invocation.

## Consequences

- **Positive:** stacked PR users get the correct outcome without manual intervention; the
  invariant check prevents silent diff corruption.
- **Negative:** requires tracking the pre-merge parent SHA as state; adds latency for the
  invariant diff check on every rebase.
- **Risk:** if the invariant check is slow, it must be gated behind a flag or made async so
  it does not block the push.
