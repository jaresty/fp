pub fn unlock_message(branch: &str) -> String {
    format!("Unlocked branch '{}'", branch)
}

const FP_SKILL: &str = include_str!("../assets/fp-skill.md");

pub fn install_skills(path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, FP_SKILL)?;
    Ok(())
}

pub fn install_shell_content(shell: &str) -> anyhow::Result<String> {
    crate::shell::fps_function_content(shell)
        .ok_or_else(|| anyhow::anyhow!("unsupported shell: {}. Supported: fish, zsh, bash", shell))
}

pub fn agent_context_text(pr_count: usize) -> String {
    format!(
        "fp agent-context — run with --json for machine-readable output\nauth: GITHUB_TOKEN or gh auth login\ncommands: ls, status, track, untrack, watch, reply, context, threads, create, rebase-stack\ntracked PRs: {}",
        pr_count
    )
}


use anyhow::Context as _;

pub fn cmd_profile(
    profiles_path: &std::path::Path,
    action: &str,
    name: &str,
    token: Option<String>,
    repo: Option<String>,
) -> anyhow::Result<String> {
    match action {
        "save" => {
            let tok = token.ok_or_else(|| anyhow::anyhow!("--token required for profile save"))?;
            let r = repo.ok_or_else(|| anyhow::anyhow!("--repo required for profile save"))?;
            crate::profile::save_profile(profiles_path, name, &tok, &r)?;
            Ok(format!("Profile '{}' saved.", name))
        }
        "load" => {
            let p = crate::profile::load_profile(profiles_path, name)?;
            Ok(format!("export GITHUB_TOKEN={}\n# repo: {}", p.github_token, p.repo))
        }
        _ => anyhow::bail!("unknown profile action '{}'; use save or load", action),
    }
}

pub fn cmd_switch(
    store: &crate::store::Store,
    git_dir: &std::path::Path,
    pr: u64,
    id: &str,
    force: bool,
    adopt: bool,
) -> anyhow::Result<std::path::PathBuf> {
    let state = store.load()?;
    let cached = state.cache.get(&pr)
        .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;
    let branch = cached.branch.clone();
    let root = crate::worktree::main_repo_root(&std::env::current_dir()?)?;

    if !force && crate::worktree::repo_is_dirty(&std::env::current_dir()?)? {
        anyhow::bail!("current worktree has uncommitted changes — commit, stash, or use --force to override");
    }

    let head_out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&root)
        .output()?;
    let current_head = String::from_utf8(head_out.stdout)?.trim().to_string();
    if current_head == branch {
        if adopt {
            let checkout = std::process::Command::new("git")
                .args(["checkout", "main"])
                .current_dir(&root)
                .output()?;
            anyhow::ensure!(checkout.status.success(), "git checkout main failed: {}",
                String::from_utf8_lossy(&checkout.stderr));
            print!("{}", crate::display::format_adopt_message(&branch));
        } else {
            crate::worktree::check_not_checked_out_in_main(&branch, &root)?;
        }
    }
    crate::worktree::check_target_lock(git_dir, &branch)?;

    let wt_path = crate::worktree::worktree_path(&root, &branch);
    if !wt_path.exists() {
        let out = std::process::Command::new("git")
            .args(["worktree", "add", wt_path.to_str().unwrap_or(""), &branch])
            .output()?;
        anyhow::ensure!(out.status.success(), "{}",
            crate::display::format_worktree_add_error(&String::from_utf8_lossy(&out.stderr), &branch, pr));
    } else if crate::worktree::worktree_branch_mismatch(&wt_path, &branch)? {
        crate::worktree::fix_worktree_branch(&wt_path, &branch, force)
            .with_context(|| format!("worktree at {} is on wrong branch — use --force to discard local changes and fix it", wt_path.display()))?;
    }

    let lp = crate::worktree::lock_path(git_dir, &branch);
    crate::worktree::write_lock(&lp, crate::worktree::session_anchor_pid(), "agent", id)?;
    Ok(wt_path)
}

pub fn cmd_untrack(store: &crate::store::Store, repo_root: &std::path::Path, git_dir: &std::path::Path, pr: u64) -> anyhow::Result<String> {
    let branch = store.load()?.cache.get(&pr).map(|t| t.branch.clone());
    if let Some(branch) = branch {
        crate::worktree::untrack_and_cleanup(store, repo_root, git_dir, pr, &branch)?;
    } else {
        store.untrack(pr)?;
    }
    Ok(format!("Untracked PR #{}", pr))
}

pub fn cmd_ls(store: &crate::store::Store, owner: &str, repo: &str, json: bool) -> anyhow::Result<String> {
    let state = store.load()?;
    if json {
        return Ok(serde_json::to_string_pretty(&state.tracked_prs())?);
    }
    let mut out = crate::display::repo_header(owner, repo);
    out.push('\n');
    if state.tracked.is_empty() {
        out.push_str("No tracked PRs. Use `fp track <pr>` to add one.");
    } else {
        let prs = state.tracked_prs();
        for (number, prefix) in crate::stack::stack_tree_order(&prs) {
            if let Some(pr) = state.cache.get(&number) {
                out.push_str(&format!("{}#{} {} ({})\n", prefix, pr.number, pr.title, pr.branch));
            }
        }
    }
    Ok(out)
}
