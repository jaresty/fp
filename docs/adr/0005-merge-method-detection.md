# ADR-005: `fp merge` — Preferred Merge Method Detection

**Status:** Accepted

## Context

The job: merge a PR through `fp` without needing to know the repo's branch protection
configuration. `fp merge` currently calls the GitHub merge endpoint with a default method.
Repos that require squash merges return `405 Method Not Allowed`. The user must pass
`--squash` explicitly, or the command fails with an opaque HTTP error.

## Decision

1. On first use against a repo (or on any `405` response), `fp` queries the repository's
   merge settings (`allow_squash_merge`, `allow_merge_commit`, `allow_rebase_merge`) and
   selects the preferred method automatically.
2. If only one method is allowed, use it. If multiple are allowed, prefer squash (most common
   default for modern repos); this preference should be configurable per-repo in `fp` config.
3. The resolved method is cached per repo so the API call is not repeated on every merge.

## Consequences

- **Positive:** `fp merge` works out-of-the-box against any repo configuration; 405 errors
  are eliminated.
- **Negative:** adds one GitHub API call on first merge per repo; "preferred" method is
  ambiguous when multiple are enabled and no explicit default is configured.
- **Risk:** cached method can become stale if repo settings change; cache should be
  invalidated on 405 or on explicit `fp config refresh`.
