# ADR-006: Task Surface Completeness — Merge Conflicts, Linter Errors, and Pagination

**Status:** Accepted — pagination done; MergeConflict task type + detection done; ESLint log parsing done.

## Context

Three feedback items share a root: tasks blocking the user are not surfaced by `fp status`.
The job: see everything blocking a PR in one place, without switching to GitHub or running
linters manually.

1. Merge conflicts are not surfaced as tasks — users discover them only when a push fails.
2. ESLint errors are not captured in `error_lines`; linter failures are invisible to `fp`.
3. `fp status` does not paginate all task-fetching calls, silently dropping items beyond a
   threshold (likely the position-30 cap matching the prior reviews pagination bug).

## Decision

1. `fp status` surfaces merge conflict state as a distinct task type:
   `conflict: <branch> has merge conflicts against <base>`.
2. The CI log parsing pipeline is extended to recognize ESLint's output format
   (`<file>:<line>:<col>  error  <message>  <rule>`) and surface each error as a task.
   The parser should be generic enough to support other structured linter formats.
3. All task-fetching calls (PR review threads, check runs, comments) are paginated to the
   full result set with no implicit position cap.

## Consequences

- **Positive:** users have a single pane of glass for all blocking items; no silent gaps.
- **Negative:** linter output parsing is format-specific; a generic structured-output mode
  is more maintainable than per-linter parsers.
- **Risk:** surfacing merge conflicts as tasks creates pressure to also resolve them via `fp`,
  which is a larger scope question deferred to a future ADR.
