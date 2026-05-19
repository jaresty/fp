use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;
use crate::worktree;
use crate::store::PrCache;

pub fn stack_tree_order(prs: &[&PrCache]) -> Vec<(u64, String)> {
    let branches: std::collections::HashSet<&str> = prs.iter().map(|p| p.branch.as_str()).collect();
    let mut result = Vec::new();
    fn visit(branch: &str, prs: &[&PrCache], depth: usize, result: &mut Vec<(u64, String)>) {
        let prefix = if depth == 0 { String::new() } else { "  ".repeat(depth - 1) + "  └─ " };
        if let Some(pr) = prs.iter().find(|p| p.branch == branch) {
            result.push((pr.number, prefix));
        }
        for child in prs.iter().filter(|p| p.base == branch) {
            visit(&child.branch.clone(), prs, depth + 1, result);
        }
    }
    let mut root_prs: Vec<&&PrCache> = prs.iter().filter(|p| !branches.contains(p.base.as_str())).collect();
    root_prs.sort_by_key(|p| p.number);
    for root in root_prs {
        visit(&root.branch.clone(), prs, 0, &mut result);
    }
    result
}

pub fn stack_order(branches: &[String], parent_of: &HashMap<String, Option<String>>) -> Vec<String> {
    let branch_set: std::collections::HashSet<&str> = branches.iter().map(String::as_str).collect();
    let mut children: HashMap<Option<&str>, Vec<&str>> = HashMap::new();
    for branch in branches {
        // If parent is not in the branches set, treat this branch as a root
        let parent = parent_of.get(branch)
            .and_then(|p| p.as_deref())
            .filter(|p| branch_set.contains(*p));
        children.entry(parent).or_default().push(branch.as_str());
    }

    let mut result = Vec::with_capacity(branches.len());
    let mut queue: Vec<&str> = children.remove(&None).unwrap_or_default();
    // stable: preserve input order within same level
    queue.sort_by_key(|b| branches.iter().position(|x| x == b));

    while !queue.is_empty() {
        let mut next_level: Vec<&str> = Vec::new();
        for b in &queue {
            result.push(b.to_string());
            if let Some(mut kids) = children.remove(&Some(b)) {
                kids.sort_by_key(|k| branches.iter().position(|x| x == k));
                next_level.extend(kids);
            }
        }
        queue = next_level;
    }
    result
}

#[derive(Debug)]
pub struct RebaseResult {
    pub conflicts: Vec<String>,
    pub rebased: Vec<String>,
    pub status_output: Option<String>,
    pub invariant_warnings: Vec<String>,
}

/// Rebase each branch onto its parent's current tip, in stack_order.
/// Root branches (no parent) rebase onto origin/<base_of[branch]>.
/// Fetches origin before rebasing to ensure remote refs are current.
pub fn rebase_stack(branches: &[String], parent_of: &HashMap<String, Option<String>>, base_of: &HashMap<String, String>, dir: &Path, progress: &dyn Fn(&str)) -> Result<RebaseResult> {
    // Bail if a rebase is already in progress — user must resolve first.
    // Check rebase-merge/rebase-apply directories rather than REBASE_HEAD: on Apple Git 2.50+,
    // REBASE_HEAD persists after a completed rebase, causing false positives.
    let git_dir = dir.join(".git");
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        anyhow::bail!("rebase in progress — resolve conflicts then run: git rebase --continue && fp rebase-stack");
    }

    // Fetch to get latest remote state before rebasing
    std::process::Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(dir)
        .output()?;

    let ordered = stack_order(branches, parent_of);
    let mut conflicts = Vec::new();
    let mut rebased = Vec::new();
    let mut status_output: Option<String> = None;
    let mut invariant_warnings = Vec::new();

    // Snapshot parent SHA for each branch before any rebasing begins.
    // Used for the diff invariant check: pre-rebase diff must use the original parent SHA.
    let pre_rebase_parent_shas: HashMap<String, String> = ordered.iter().filter_map(|branch| {
        let parent_ref = match parent_of.get(branch).and_then(|p| p.as_ref()) {
            Some(p) => p.as_str(),
            None => {
                let base = base_of.get(branch.as_str()).map(String::as_str).unwrap_or("main");
                return std::process::Command::new("git")
                    .args(["rev-parse", &format!("origin/{}", base)])
                    .current_dir(dir).output().ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| (branch.clone(), s.trim().to_string()));
            }
        };
        std::process::Command::new("git")
            .args(["rev-parse", parent_ref])
            .current_dir(dir).output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| (branch.clone(), s.trim().to_string()))
    }).collect();

    for branch in &ordered {
        let parent_owned: String;
        let parent = match parent_of.get(branch).and_then(|p| p.as_ref()) {
            Some(p) => p.as_str(),
            None => {
                let base = base_of.get(branch).map(String::as_str).unwrap_or("main");
                parent_owned = format!("origin/{}", base);
                &parent_owned
            }
        };

        progress(&format!("rebasing {} onto {}", branch, parent));

        // Capture pre-rebase diff using three-dot (symmetric diff from merge-base).
        // Use the snapshotted parent SHA (before any sibling rebase changed the ref).
        let pre_parent_sha = pre_rebase_parent_shas.get(branch.as_str()).cloned().unwrap_or_default();
        let pre_rebase_diff = if pre_parent_sha.is_empty() { String::new() } else {
            std::process::Command::new("git")
                .args(["diff", &format!("{}...{}", pre_parent_sha, branch), "--"])
                .current_dir(dir).output().ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default()
        };

        // Ensure a worktree exists for this branch; create it if absent.
        let wt_path = worktree::worktree_path(dir, branch);
        if !wt_path.exists() {
            let add = std::process::Command::new("git")
                .args(["worktree", "add", wt_path.to_str().unwrap_or(""), branch])
                .current_dir(dir)
                .output()?;
            if !add.status.success() {
                conflicts.push(format!("{}: worktree add failed: {}", branch,
                    String::from_utf8_lossy(&add.stderr).trim()));
                continue;
            }
        }

        // Detect whether the parent branch has been merged (and its remote branch deleted).
        // Two signals, either sufficient:
        // 1. git merge-base --is-ancestor <parent> origin/<base>: parent's tip is in base (regular merge)
        // 2. origin/<parent> remote tracking ref is gone after fetch (squash/rebase merge with auto-delete)
        let base_for_parent = base_of.get(branch.as_str()).map(String::as_str).unwrap_or("main");
        let origin_base = format!("origin/{}", base_for_parent);
        let parent_merged_into_base = !parent.starts_with("origin/") && {
            let parent_exists = std::process::Command::new("git")
                .args(["rev-parse", "--verify", parent])
                .current_dir(dir).output()?.status.success();
            if parent_exists {
                // Regular merge: parent's tip is an ancestor of origin/<base>
                let is_ancestor = std::process::Command::new("git")
                    .args(["merge-base", "--is-ancestor", parent, &origin_base])
                    .current_dir(dir).output()?.status.success();
                // Squash/rebase merge: remote branch is gone (use ls-remote for authoritative check)
                let ls_remote_out = std::process::Command::new("git")
                    .args(["ls-remote", "--exit-code", "--heads", "origin", parent])
                    .current_dir(dir).output()?;
                let remote_branch_gone = !ls_remote_out.status.success();
                is_ancestor || remote_branch_gone
            } else {
                // Local branch ref is gone — definitely merged
                true
            }
        };

        // Rebase onto parent — run from the branch's worktree so conflict state lands there.
        let rebase = if parent_merged_into_base {
            // Parent merged — use --onto to transplant only commits unique to this branch.
            // Find the oldest commit on branch not in origin/<base> — that's the former parent tip.
            let rev_list_out = std::process::Command::new("git")
                .args(["rev-list", &format!("{}..{}", origin_base, branch)])
                .current_dir(dir)
                .output()?;
            let old_parent_sha = String::from_utf8_lossy(&rev_list_out.stdout)
                .lines()
                .last()
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            std::process::Command::new("git")
                .args(["rebase", "--onto", &origin_base, &old_parent_sha])
                .current_dir(&wt_path)
                .output()?
        } else {
            // Only check for "parent was rewritten" when parent is a local branch.
            // Origin refs (origin/main etc.) are always forward-only — if origin/main is not
            // an ancestor of the branch, the branch simply needs to be rebased forward and
            // If parent is already an ancestor of branch, plain rebase is correct.
            // Otherwise (parent advanced or was force-pushed), use fork-point + --onto
            // to replay only the commits unique to branch.
            let parent_is_ancestor = std::process::Command::new("git")
                .args(["merge-base", "--is-ancestor", parent, branch])
                .current_dir(&wt_path)
                .output()?.status.success();
            if !parent_is_ancestor {
                // Find where branch diverged from parent's old history.
                // --fork-point uses the reflog to find the most recent parent tip that is
                // an ancestor of branch — this is the correct old_upstream when parent was
                // fully rebased (all SHAs changed). Falls back to rev-list | tail -1 which
                // works for the single-rewrite case but returns the wrong (oldest) commit
                // when parent had multiple commits all rewritten.
                let fork_point_out = std::process::Command::new("git")
                    .args(["merge-base", "--fork-point", parent, branch])
                    .current_dir(&wt_path)
                    .output();
                let old_upstream = fork_point_out
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        // Fallback: oldest commit in branch not reachable from parent
                        let rev_list_out = std::process::Command::new("git")
                            .args(["rev-list", &format!("{}..{}", parent, branch)])
                            .current_dir(&wt_path)
                            .output()
                            .unwrap_or_else(|_| std::process::Output {
                                status: std::process::ExitStatus::default(),
                                stdout: vec![],
                                stderr: vec![],
                            });
                        String::from_utf8_lossy(&rev_list_out.stdout)
                            .lines()
                            .last()
                            .map(str::trim)
                            .unwrap_or("")
                            .to_string()
                    });
                // Only use --onto if there are commits to replay after old_upstream.
                // If replay_count == 0, old_upstream == branch tip (independent branch being
                // newly stacked) — fall back to plain rebase which handles that correctly.
                let replay_count = if !old_upstream.is_empty() {
                    std::process::Command::new("git")
                        .args(["rev-list", "--count", &format!("{}..{}", old_upstream, branch)])
                        .current_dir(&wt_path)
                        .output().ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .and_then(|s| s.trim().parse::<usize>().ok())
                        .unwrap_or(0)
                } else { 0 };
                if !old_upstream.is_empty() && replay_count > 0 {
                    std::process::Command::new("git")
                        .args(["rebase", "--onto", parent, &old_upstream])
                        .current_dir(&wt_path)
                        .output()?
                } else {
                    std::process::Command::new("git")
                        .args(["rebase", parent])
                        .current_dir(&wt_path)
                        .output()?
                }
            } else {
                std::process::Command::new("git")
                    .args(["rebase", parent])
                    .current_dir(&wt_path)
                    .output()?
            }
        };

        if rebase.status.success() {
            // Invariant check: semantic diff (three-dot) should match pre-rebase diff
            let new_parent = if parent_merged_into_base { &origin_base } else { parent };
            let post_rebase_diff = std::process::Command::new("git")
                .args(["diff", &format!("{}...{}", new_parent, branch), "--"])
                .current_dir(&wt_path).output().ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            if !pre_rebase_diff.is_empty() && post_rebase_diff != pre_rebase_diff {
                invariant_warnings.push(format!(
                    "{}: diff vs parent changed after rebase — review carefully", branch
                ));
            }
            progress(&format!("pushing {}", branch));
            let push = std::process::Command::new("git")
                .args(["push", "origin", branch, "--force-with-lease"])
                .current_dir(&wt_path)
                .output()?;
            if push.status.success() {
                rebased.push(branch.clone());
            } else {
                conflicts.push(format!("{}: push failed", branch));
                eprintln!("Push failed for {} — dependent branches skipped.", branch);
                eprintln!("Retry with: fp rebase-stack");
                break;
            }
        } else {
            conflicts.push(branch.clone());
            eprintln!("Conflict on {} — resolve with:", branch);
            eprintln!("  git add <resolved files> && git rebase --continue");
            eprintln!("  fp rebase-stack");
            let status = std::process::Command::new("git")
                .args(["status"])
                .current_dir(&wt_path)
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string());
            if let Some(ref s) = status {
                eprintln!("{}", s);
            }
            status_output = status;
            break;
        }
    }

    Ok(RebaseResult { conflicts, rebased, status_output, invariant_warnings })
}

/// Rebase `branch` onto `new_base`, cutting away commits from `old_base_sha` (the pre-merge tip).
/// Squash-safe: uses --onto so only commits unique to `branch` are replanted.
/// Force-pushes after a successful rebase.
pub fn rebase_onto_after_merge(branch: &str, old_base_sha: &str, new_base: &str, dir: &Path) -> Result<()> {
    let wt_dir = worktree::find_worktree_path(branch, dir);
    let rebase_dir = wt_dir.as_deref().unwrap_or(dir);
    let git = |args: &[&str]| {
        std::process::Command::new("git").args(args).current_dir(rebase_dir).output()
    };
    if wt_dir.is_none() {
        let checkout = git(&["checkout", branch])?;
        anyhow::ensure!(checkout.status.success(), "checkout {} failed: {}", branch, String::from_utf8_lossy(&checkout.stderr));
    }
    let rebase = git(&["rebase", "--onto", new_base, old_base_sha, branch])?;
    if !rebase.status.success() {
        git(&["rebase", "--abort"]).ok();
        anyhow::bail!("rebase --onto {} {} {} failed: {}", new_base, old_base_sha, branch, String::from_utf8_lossy(&rebase.stderr));
    }
    let push = git(&["push", "--force-with-lease"])?;
    anyhow::ensure!(push.status.success(), "force-push of {} failed: {}", branch, String::from_utf8_lossy(&push.stderr));
    Ok(())
}

/// Rebase the full downstream stack after `merged_branch` (with tip `merged_sha`) is merged into `new_base`.
/// `branch_base_of` maps each branch to its immediate parent branch name.
/// Rebases all branches whose parent chain includes `merged_branch`, in topological order.
pub fn rebase_downstream_stack(
    merged_branch: &str,
    merged_sha: &str,
    new_base: &str,
    branch_base_of: &HashMap<String, String>,
    dir: &Path,
    progress: &dyn Fn(&str),
) -> Vec<String> {
    let mut errors = Vec::new();
    let direct_children: Vec<String> = branch_base_of.iter()
        .filter(|(_, parent)| parent.as_str() == merged_branch)
        .map(|(child, _)| child.clone())
        .collect();

    for child in direct_children {
        progress(&child);
        match rebase_onto_after_merge(&child, merged_sha, new_base, dir) {
            Ok(()) => {
                let new_child_sha = match std::process::Command::new("git")
                    .args(["rev-parse", &child])
                    .current_dir(dir)
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                {
                    Some(sha) if !sha.is_empty() => sha,
                    _ => {
                        errors.push(format!("{}: could not resolve new SHA after rebase", child));
                        continue;
                    }
                };
                let mut child_errors = rebase_downstream_stack(
                    &child, &new_child_sha, &child, branch_base_of, dir, progress,
                );
                errors.append(&mut child_errors);
            }
            Err(e) => errors.push(format!("{}: {}", child, e)),
        }
    }
    errors
}


fn git_rev_parse(branch: &str, dir: &Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", branch])
        .current_dir(dir)
        .output()?;
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

fn git_merge_base(a: &str, b: &str, dir: &Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["merge-base", a, b])
        .current_dir(dir)
        .output()?;
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

/// For each branch, find its parent among the other branches using git merge-base.
/// A branch B has parent A if merge-base(A, B) == tip(A) and A != B.
/// If no branch in the set is an ancestor, the branch is a root (None).
pub fn detect_parent_of(branches: &[String], dir: &Path, base_of: &HashMap<String, String>) -> Result<HashMap<String, Option<String>>> {
    let tips: HashMap<String, String> = branches.iter()
        .map(|b| Ok((b.clone(), git_rev_parse(b, dir)?)))
        .collect::<Result<_>>()?;

    let mut parent_of: HashMap<String, Option<String>> = HashMap::new();

    for branch in branches {
        if let Some(declared) = base_of.get(branch.as_str())
            && !declared.is_empty() && branches.iter().any(|b| b == declared) {
            parent_of.insert(branch.clone(), Some(declared.clone()));
            continue;
        }

        let mut best_parent: Option<String> = None;
        let mut best_depth = 0usize;
        let mut best_lead = 0usize;

        for candidate in branches {
            if candidate == branch { continue; }
            let mb = git_merge_base(candidate, branch, dir)?;
            let candidate_tip = tips.get(candidate).unwrap();

            // Depth from merge-base to branch tip (fewer = closer parent).
            // Using merge-base (not candidate tip) handles force-push: the parent's new
            // tip is not in the child's history, but the merge-base is still recent.
            let depth_out = std::process::Command::new("git")
                .args(["rev-list", "--count", &format!("{}..{}", mb, branch)])
                .current_dir(dir)
                .output()?;
            let depth: usize = String::from_utf8(depth_out.stdout)?.trim().parse().unwrap_or(0);
            if depth == 0 { continue; }

            // How far has the candidate advanced beyond the merge-base?
            // Tiebreaker: when two candidates share the same depth, prefer the one whose
            // tip is further past the merge-base. A force-pushed parent has new commits
            // beyond the shared merge-base; a stable grandparent at the merge-base has 0.
            // This correctly picks a force-pushed direct parent over a stable grandparent
            // even when both yield the same depth-from-mb to the child.
            let lead_out = std::process::Command::new("git")
                .args(["rev-list", "--count", &format!("{}..{}", mb, candidate_tip)])
                .current_dir(dir)
                .output()?;
            let lead: usize = String::from_utf8(lead_out.stdout)?.trim().parse().unwrap_or(0);

            let better = match best_parent {
                None => true,
                Some(_) => depth < best_depth || (depth == best_depth && lead > best_lead),
            };
            if better {
                best_parent = Some(candidate.clone());
                best_depth = depth;
                best_lead = lead;
            }
        }
        parent_of.insert(branch.clone(), best_parent);
    }

    Ok(parent_of)
}

/// Squash-safe single-branch rebase: rebase `branch` onto `new_base`, replacing `old_base`.
/// Uses the branch's worktree if one exists; otherwise checks out in the main worktree.
pub fn rebase_branch_onto(branch: &str, old_base: &str, new_base: &str, dir: &std::path::Path) -> anyhow::Result<()> {
    let wt_dir = worktree::find_worktree_path(branch, dir);
    let rebase_dir = wt_dir.as_deref().unwrap_or(dir);
    let git = |args: &[&str]| {
        std::process::Command::new("git").args(args).current_dir(rebase_dir).output()
    };
    if wt_dir.is_none() {
        let checkout = git(&["checkout", branch])?;
        anyhow::ensure!(checkout.status.success(), "failed to checkout {}: {}", branch, String::from_utf8_lossy(&checkout.stderr));
    }
    let rebase = git(&["rebase", "--onto", new_base, old_base, branch])?;
    if !rebase.status.success() {
        git(&["rebase", "--abort"]).ok();
        anyhow::bail!("rebase --onto {} {} {} failed: {}", new_base, old_base, branch, String::from_utf8_lossy(&rebase.stderr));
    }
    let push = git(&["push", "--force-with-lease"])?;
    anyhow::ensure!(push.status.success(), "force-push of {} failed: {}", branch, String::from_utf8_lossy(&push.stderr));
    Ok(())
}
