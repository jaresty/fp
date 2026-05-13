# ADR-004: `fp merge` — Downstream Rebase on Merge + Progress Logging

**Status:** Accepted

## Context

Two feedback items converge here. The job: merge a PR and immediately have the rest of the
stack in a correct state, without a separate manual step.

1. `fp merge` does not rebase downstream PRs after a merge, leaving the stack in an
   inconsistent state that users must resolve manually.
2. Neither `fp merge` nor `fp rebase-stack` emit progress as they operate, making long
   operations indistinguishable from hangs.

## Decision

1. `fp merge` automatically triggers the equivalent of `fp rebase-stack` for all PRs whose
   base was the just-merged branch, making merge + downstream rebase atomic from the user's
   perspective.
2. Before executing any destructive downstream push, `fp` confirms with the user or requires
   an explicit `--push` flag.
3. Both `fp merge` and `fp rebase-stack` emit a structured progress line before each step:
   fetching, identifying downstream branches, rebasing `<branch>`, pushing `<branch>`, etc.

## Consequences

- **Positive:** users get the full job done in one command; progress output removes ambiguity
  about hang vs. slow operation.
- **Negative:** automatic force-push of downstream branches is destructive; confirmation or
  opt-in flag is required to avoid surprises.
- **Open question:** how does `fp` determine the ordered set of downstream PRs to rebase when
  the stack has multiple levels?
