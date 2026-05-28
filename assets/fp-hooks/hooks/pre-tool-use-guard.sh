#!/usr/bin/env sh
# fp worktree guard — fires before every Edit/Write/NotebookEdit tool call.
# Reads the tool input path from CLAUDE_TOOL_INPUT_FILE_PATH (set by Claude Code).
# If the path is inside a known fp worktree, checks that it matches the PR the
# agent last switched to. Emits a warning if there is a mismatch.
set -e

fp_bin=$(command -v fp 2>/dev/null) || exit 0

file_path="${CLAUDE_TOOL_INPUT_FILE_PATH:-}"
[ -z "$file_path" ] && exit 0

# Find which worktree (if any) contains this file.
repo_root=$("$fp_bin" root 2>/dev/null) || exit 0
worktrees_dir="${repo_root}-worktrees"
[ -d "$worktrees_dir" ] || exit 0

case "$file_path" in
  "$worktrees_dir"/*)
    # The file is in some fp worktree — verify fp ls knows about it.
    rel="${file_path#"$worktrees_dir"/}"
    branch=$(echo "$rel" | cut -d'/' -f1-2)
    match=$("$fp_bin" ls --json 2>/dev/null | grep -c "\"$branch\"" || true)
    if [ "$match" -eq 0 ]; then
      echo "⚠ fp worktree guard: editing file in worktree '$branch' which is not tracked by fp. Run: fp ls" >&2
    fi
    ;;
esac

exit 0
