use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;

pub fn stack_order(branches: &[String], parent_of: &HashMap<String, Option<String>>) -> Vec<String> {
    let mut children: HashMap<Option<&str>, Vec<&str>> = HashMap::new();
    for branch in branches {
        let parent = parent_of.get(branch).and_then(|p| p.as_deref());
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
}

/// Rebase each branch onto its parent's current tip, in stack_order.
/// Root branches (no parent) rebase onto origin/<base_of[branch]>.
/// Fetches origin before rebasing to ensure remote refs are current.
pub fn rebase_stack(branches: &[String], parent_of: &HashMap<String, Option<String>>, base_of: &HashMap<String, String>, dir: &Path) -> Result<RebaseResult> {
    // Bail if a rebase is already in progress — user must resolve first
    if dir.join(".git").join("REBASE_HEAD").exists() {
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

        // Checkout the branch
        let checkout = std::process::Command::new("git")
            .args(["checkout", branch])
            .current_dir(dir)
            .output()?;
        if !checkout.status.success() {
            conflicts.push(format!("{}: checkout failed", branch));
            continue;
        }

        // Rebase onto parent
        let rebase = std::process::Command::new("git")
            .args(["rebase", parent])
            .current_dir(dir)
            .output()?;

        if rebase.status.success() {
            let push = std::process::Command::new("git")
                .args(["push", "origin", branch, "--force-with-lease"])
                .current_dir(dir)
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
            break;
        }
    }

    Ok(RebaseResult { conflicts, rebased })
}

/// Rebase `branch` onto `new_base`, cutting away commits from `old_base_sha` (the pre-merge tip).
/// Squash-safe: uses --onto so only commits unique to `branch` are replanted.
/// Force-pushes after a successful rebase.
pub fn rebase_onto_after_merge(branch: &str, old_base_sha: &str, new_base: &str, dir: &Path) -> Result<()> {
    let git = |args: &[&str]| {
        std::process::Command::new("git").args(args).current_dir(dir).output()
    };
    let checkout = git(&["checkout", branch])?;
    anyhow::ensure!(checkout.status.success(), "checkout {} failed: {}", branch, String::from_utf8_lossy(&checkout.stderr));
    let rebase = git(&["rebase", "--onto", new_base, old_base_sha, branch])?;
    if !rebase.status.success() {
        git(&["rebase", "--abort"]).ok();
        anyhow::bail!("rebase --onto {} {} {} failed: {}", new_base, old_base_sha, branch, String::from_utf8_lossy(&rebase.stderr));
    }
    let push = git(&["push", "--force-with-lease"])?;
    anyhow::ensure!(push.status.success(), "force-push of {} failed: {}", branch, String::from_utf8_lossy(&push.stderr));
    Ok(())
}

pub fn resolve_work_dir(_git_dir: &Path) -> Result<std::path::PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    let path = String::from_utf8(out.stdout)?.trim().to_string();
    Ok(std::path::PathBuf::from(path))
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
pub fn detect_parent_of(branches: &[String], dir: &Path) -> Result<HashMap<String, Option<String>>> {
    let tips: HashMap<String, String> = branches.iter()
        .map(|b| Ok((b.clone(), git_rev_parse(b, dir)?)))
        .collect::<Result<_>>()?;

    let mut parent_of: HashMap<String, Option<String>> = HashMap::new();

    for branch in branches {
        let mut best_parent: Option<String> = None;
        let mut best_depth = 0usize;

        for candidate in branches {
            if candidate == branch { continue; }
            let mb = git_merge_base(candidate, branch, dir)?;
            let candidate_tip = tips.get(candidate).unwrap();

            if &mb == candidate_tip {
                // candidate is an ancestor of branch — measure depth as commit count
                let out = std::process::Command::new("git")
                    .args(["rev-list", "--count", &format!("{}..{}", candidate, branch)])
                    .current_dir(dir)
                    .output()?;
                let depth: usize = String::from_utf8(out.stdout)?.trim().parse().unwrap_or(0);
                // pick the closest ancestor (smallest depth > 0)
                if best_parent.is_none() || depth < best_depth {
                    best_parent = Some(candidate.clone());
                    best_depth = depth;
                }
            }
        }
        parent_of.insert(branch.clone(), best_parent);
    }

    Ok(parent_of)
}
