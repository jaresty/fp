# ADR-007: PR Communication — Top-Level Comments, Description Editing, and Demo Upload

**Status:** Accepted

## Context

The job: manage all PR communication and metadata from the CLI without switching to the
GitHub web UI. Three gaps:

1. `fp reply` only handles numbered inline review threads. Top-level PR review body comments
   (e.g., "lgtm but has merge conflicts") cannot be replied to via `fp`.
2. There is no way to edit a PR's description after creation.
3. There is no way to attach a demo (GIF, video, screenshot) to a PR at creation time or
   after the fact.

## Decision

### Top-level comments

Add `fp comment <pr> "<text>"` to post a top-level PR comment via
`POST /repos/:owner/:repo/issues/:number/comments`.

### Description editing

Add `fp edit <pr>` to update a PR's title and/or body. When run interactively (TTY present),
open `$EDITOR` pre-populated with the current description. When run non-interactively, require
`--body "<text>"` and/or `--title "<text>"` flags.

### Demo upload

Add a repeatable `--demo <file-or-url>` flag available on both `fp create` and `fp edit`:

- **File path:** upload the file via the GitHub asset upload endpoint, obtain the CDN URL,
  inject as a markdown image.
- **URL:** inject directly without upload.
- **Multiple demos:** each `--demo` flag appends one entry; they are injected in order.
- **Injection format:** a `## Demo` section is appended to (or replaced in) the PR
  description containing `![Demo 1](<url>)`, `![Demo 2](<url>)`, etc.
- On `fp edit`, if a `## Demo` section already exists it is replaced; otherwise it is
  appended.

Supported file types for upload: GIF, PNG, JPG, MP4, WebM (GitHub-supported asset types).

## Consequences

- **Positive:** full PR lifecycle — comments, description, demos — is manageable from `fp`;
  no context switch to the web UI.
- **Negative:** `fp edit` with `$EDITOR` requires TTY detection; asset upload requires
  multipart form encoding, which is a different code path from the JSON REST calls `fp`
  uses elsewhere.
- **Risk:** GitHub's asset upload endpoint is undocumented (it powers the drag-to-upload UI);
  the API surface may change. Consider falling back to a paste-as-comment approach if the
  upload endpoint is unavailable.
