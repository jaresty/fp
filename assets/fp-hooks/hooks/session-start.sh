#!/usr/bin/env sh
# fp worktree context injection — emitted at session start so the agent
# knows which PR lives in which worktree before touching any files.
set -e

fp_bin=$(command -v fp 2>/dev/null) || exit 0

worktree_map=$("$fp_bin" ls --json 2>/dev/null) || exit 0
[ -z "$worktree_map" ] && exit 0

cat <<CONTEXT
<system-reminder>
fp worktree map (use absolute paths when editing PR files):
$worktree_map

Rule: before editing files for a PR, enter its worktree with: fp switch <pr>
Or address files directly by their absolute worktree path shown above.
</system-reminder>
CONTEXT
