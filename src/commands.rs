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

pub fn cmd_status_one(
    client: Option<&dyn crate::github::GithubClientTrait>,
    store: &crate::store::Store,
    git_dir: &std::path::Path,
    owner: &str,
    repo: &str,
    pr_number: u64,
    json: bool,
) -> anyhow::Result<String> {
    let state = store.load()?;
    let cached = state.cache.get(&pr_number)
        .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr_number, pr_number))?;

    let mut pr_state = client
        .and_then(|c| c.fetch_pr(owner, repo, cached.number).ok())
        .unwrap_or_else(|| crate::model::PrState {
            number: cached.number, title: cached.title.clone(), branch: cached.branch.clone(),
            base: cached.base.clone(), head_sha: String::new(), draft: false, approved: false,
            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });

    if let Some(c) = client
        && let Some(parent) = state.tracked_prs().into_iter().find(|p| p.branch == pr_state.base)
            .and_then(|p| c.fetch_pr(owner, repo, p.number).ok())
        && !parent.head_sha.is_empty() && !pr_state.head_sha.is_empty() {
        pr_state.needs_parent_rebase = c.is_head_behind_base(owner, repo, &parent.head_sha, &pr_state.head_sha);
    }

    let tasks = crate::tasks::generate_tasks(&pr_state);
    let lock = crate::worktree::lock_status(git_dir, &cached.branch);

    if json {
        Ok(serde_json::to_string_pretty(&tasks)?)
    } else {
        Ok(crate::display::format_single_pr_status(pr_number, &tasks, lock.as_deref()))
    }
}

pub fn cmd_status_all(
    client: Option<&dyn crate::github::GithubClientTrait>,
    store: &crate::store::Store,
    git_dir: &std::path::Path,
    owner: &str,
    repo: &str,
    json: bool,
) -> anyhow::Result<String> {
    let state = store.load()?;
    let pr_numbers: Vec<u64> = state.tracked.iter().copied().collect();

    let fetched: std::collections::HashMap<u64, crate::model::PrState> = client
        .map(|c| c.fetch_prs_as_map(owner, repo, &pr_numbers))
        .unwrap_or_default();

    let new_cache: std::collections::HashMap<u64, crate::store::PrCache> = fetched.values()
        .filter(|p| state.tracked.contains(&p.number))
        .map(|p| (p.number, crate::store::PrCache { number: p.number, title: p.title.clone(), branch: p.branch.clone(), base: p.base.clone() }))
        .collect();
    let _ = store.replace_cache(new_cache);
    let state = store.load()?;

    let prs = state.tracked_prs();
    let tree_order = crate::stack::stack_tree_order(&prs);

    let mut out = String::new();
    if !json { out.push_str(&crate::display::repo_header(owner, repo)); out.push('\n'); }

    for (number, prefix) in tree_order {
        let cached = match state.cache.get(&number) { Some(t) => t, None => continue };
        let mut pr_state = fetched.get(&number).cloned().unwrap_or_else(|| crate::model::PrState {
            number: cached.number, title: cached.title.clone(), branch: cached.branch.clone(),
            base: cached.base.clone(), head_sha: String::new(), draft: false, approved: false,
            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });

        if let Some(c) = client
            && let Some(parent) = prs.iter().find(|p| p.branch == cached.base).and_then(|p| fetched.get(&p.number))
            && !parent.head_sha.is_empty() && !pr_state.head_sha.is_empty() {
            pr_state.needs_parent_rebase = c.is_head_behind_base(owner, repo, &parent.head_sha, &pr_state.head_sha);
        }

        let tasks = crate::tasks::generate_tasks(&pr_state);
        let lock = crate::worktree::lock_status(git_dir, &cached.branch)
            .map(|s| format!("  {}", s))
            .unwrap_or_default();

        if json {
            out.push_str(&serde_json::to_string_pretty(&tasks).unwrap());
            out.push('\n');
        } else {
            out.push_str(&crate::display::format_pr_status_all_entry(&prefix, cached.number, &cached.title, &tasks, &lock));
        }
    }
    Ok(out)
}

pub fn cmd_checks(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, sha: &str) -> anyhow::Result<String> {
    let checks = client.fetch_checks_for_sha(owner, repo, sha)?;
    if checks.is_empty() {
        return Ok(format!("No check runs found for {}", sha));
    }
    let mut out = String::new();
    for check in &checks {
        let status = match check.status {
            crate::model::CheckStatus::Pass => "✓",
            crate::model::CheckStatus::Fail => "✗",
            crate::model::CheckStatus::Pending => "⏳",
        };
        let url = check.details_url.as_deref().unwrap_or("(no url)");
        out.push_str(&format!("{} {} — {}\n", status, check.name, url));
    }
    Ok(out)
}

pub fn cmd_reply(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, pr: u64, thread_id: u64, message: &str) -> anyhow::Result<String> {
    let pr_state = client.fetch_pr(owner, repo, pr)?;
    let thread = pr_state.threads.iter().find(|t| t.id == thread_id)
        .with_context(|| format!("thread #{} not found on PR #{}", thread_id, pr))?;
    let posted = client.reply_to_thread(owner, repo, pr, thread, message)?;
    Ok(format!("Replied to thread #{}: {}", thread_id, posted))
}

pub fn cmd_ready(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, pr: u64) -> anyhow::Result<String> {
    client.mark_pr_ready(owner, repo, pr)?;
    Ok(format!("PR #{} marked as ready for review.", pr))
}

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

pub fn normalize_base_of(
    base_of: std::collections::HashMap<String, String>,
    tracked_branches: &std::collections::HashSet<String>,
) -> std::collections::HashMap<String, String> {
    base_of.into_iter()
        .map(|(branch, base)| {
            let normalized = if tracked_branches.contains(&base) { base } else { "main".to_string() };
            (branch, normalized)
        })
        .collect()
}

pub fn update_children_base(store: &crate::store::Store, merged_branch: &str, new_base: &str) -> anyhow::Result<()> {
    let state = store.load()?;
    for pr in state.tracked_prs() {
        if pr.base == merged_branch {
            store.update_cache(crate::store::PrCache {
                number: pr.number,
                title: pr.title.clone(),
                branch: pr.branch.clone(),
                base: new_base.to_string(),
            })?;
        }
    }
    Ok(())
}

pub fn cmd_comment(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, pr: u64, message: &str) -> anyhow::Result<String> {
    let url = client.post_pr_comment(owner, repo, pr, message)?;
    Ok(format!("Comment posted: {}", url))
}

pub fn cmd_threads(
    client: Option<&dyn crate::github::GithubClientTrait>,
    store: &crate::store::Store,
    owner: &str,
    repo: &str,
    pr: u64,
    resolved: bool,
    json: bool,
) -> anyhow::Result<String> {
    let state = store.load()?;
    let tracked = state.cache.get(&pr)
        .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;

    if resolved {
        if let Some(c) = client {
            let threads = c.fetch_resolved_threads_graphql(owner, repo, pr)?;
            return Ok(crate::display::format_resolved_threads(pr, &threads, json));
        }
        return Ok("No GitHub credentials — cannot fetch resolved threads.".into());
    }

    let pr_state = client
        .and_then(|c| c.fetch_pr(owner, repo, pr).ok())
        .unwrap_or_else(|| crate::model::PrState {
            number: tracked.number, title: tracked.title.clone(), branch: tracked.branch.clone(),
            base: "".into(), head_sha: "".into(), draft: false, approved: false,
            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
    let threads = crate::display::fetch_open_threads(&pr_state.threads);
    Ok(crate::display::format_open_threads(pr, &threads, json))
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

pub fn cmd_track(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, pr: u64, title: Option<String>, _branch: Option<String>) -> anyhow::Result<(String, String, String)> {
    let (fetched_title, fetched_branch) = client.fetch_pr_metadata(owner, repo, pr).unwrap_or_default();
    let base = client.fetch_pr_base(owner, repo, pr).unwrap_or_default();
    let resolved_title = title.unwrap_or_else(|| if fetched_title.is_empty() { format!("PR #{}", pr) } else { fetched_title });
    Ok((resolved_title, fetched_branch, base))
}

pub fn cmd_edit(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, pr: u64, title: Option<String>, body: Option<String>, _demo: Vec<String>) -> anyhow::Result<String> {
    client.update_pr(owner, repo, pr, title.as_deref(), body.as_deref())?;
    Ok(format!("✓ PR #{} updated", pr))
}

pub fn cmd_new(branch: &str, base: &str, dir: &std::path::Path) -> anyhow::Result<String> {
    let wt_path = crate::worktree::worktree_path(dir, branch);
    std::process::Command::new("git")
        .args(["fetch", "origin", base])
        .current_dir(dir).output()?;
    let out = std::process::Command::new("git")
        .args(["worktree", "add", wt_path.to_str().unwrap_or(""), "-b", branch, &format!("origin/{}", base)])
        .current_dir(dir).output()?;
    anyhow::ensure!(out.status.success(), "git worktree add failed: {}", String::from_utf8_lossy(&out.stderr));
    Ok(crate::display::format_new_worktree_output(&wt_path, branch))
}

pub struct CreateOpts {
    pub title: String,
    pub base: String,
    pub body: Option<String>,
    pub restack_before: Option<u64>,
    pub insert_after: Option<u64>,
}

pub fn cmd_create(
    client: &dyn crate::github::GithubClientTrait,
    owner: &str,
    repo: &str,
    store: &crate::store::Store,
    head_branch: &str,
    dir: &std::path::Path,
    opts: CreateOpts,
) -> anyhow::Result<String> {
    let CreateOpts { title, base, body, restack_before, insert_after } = opts;
    let pr_state = client.create_pr_with_body(owner, repo, &title, head_branch, &base, true, body.as_deref())?;
    store.track(pr_state.number)?;
    store.update_cache(crate::store::PrCache {
        number: pr_state.number,
        title: pr_state.title.clone(),
        branch: pr_state.branch.clone(),
        base: pr_state.base.clone(),
    })?;
    let mut out = format!("Created PR #{}: {} ({})", pr_state.number, pr_state.title, pr_state.branch);

    if let Some(target_pr) = restack_before {
        let target_branch = client.fetch_pr_metadata(owner, repo, target_pr)?.1;
        let old_base = client.fetch_pr_base(owner, repo, target_pr)?;
        crate::stack::rebase_branch_onto(&target_branch, &old_base, head_branch, dir)?;
        client.update_pr_base(owner, repo, target_pr, head_branch)?;
        out.push_str(&format!("\nRestacked PR #{} onto {} (rebased {} --onto {})", target_pr, head_branch, target_branch, head_branch));
    }

    if let Some(anchor_pr) = insert_after {
        let anchor_branch = client.fetch_pr_metadata(owner, repo, anchor_pr)?.1;
        let state = store.load()?;
        let next_pr = state.tracked_prs().into_iter()
            .find(|p| client.fetch_pr_base(owner, repo, p.number).ok().as_deref() == Some(&anchor_branch))
            .cloned();
        if let Some(next) = next_pr {
            let next_branch = next.branch.clone();
            let next_pr_num = next.number;
            crate::stack::rebase_branch_onto(&next_branch, &anchor_branch, head_branch, dir)?;
            client.update_pr_base(owner, repo, next_pr_num, head_branch)?;
            out.push_str(&format!("\nInserted {} between PR #{} and PR #{}", head_branch, anchor_pr, next_pr_num));
        } else {
            out.push_str(&format!("\nNo tracked PR found with base {}; nothing to restack", anchor_branch));
        }
    }

    Ok(out)
}

pub fn cmd_context(client: &dyn crate::github::GithubClientTrait, owner: &str, repo: &str, pr: u64, hint: &str, _full_log: bool) -> anyhow::Result<String> {
    let pr_state = client.fetch_pr(owner, repo, pr)?;
    if let Some(stripped) = hint.strip_prefix("thread:") {
        let thread_id: u64 = stripped.parse().map_err(|_| anyhow::anyhow!("invalid thread id"))?;
        if let Some(thread) = pr_state.threads.iter().find(|t| t.id == thread_id) {
            let mut out = format!("Thread #{} ({:?})\n", thread.id, thread.state);
            if let (Some(file), Some(line)) = (&thread.file, thread.line) {
                out.push_str(&format!("  {}:{}\n", file, line));
            }
            out.push_str(&format!("  @{}: {}\n", thread.author, thread.body));
            for (author, body) in &thread.replies {
                out.push_str(&format!("  > @{}: {}\n", author, body));
            }
            Ok(out)
        } else {
            Ok(format!("Thread #{} not found in PR #{}\n", thread_id, pr))
        }
    } else if let Some(check) = pr_state.checks.iter().find(|c| c.name == hint) {
        let status = format!("{:?}", check.status);
        Ok(crate::ci::format_check_output(&check.name, &status, None, None, None))
    } else {
        Ok(format!("Check '{}' not found in PR #{}\n", hint, pr))
    }
}

pub struct MergeContext<'a> {
    pub store: &'a crate::store::Store,
    pub dir: &'a std::path::Path,
    pub git_dir: &'a std::path::Path,
    pub merge_method: &'a str,
}

pub fn cmd_merge(
    client: &dyn crate::github::GithubClientTrait,
    owner: &str,
    repo: &str,
    pr: u64,
    ctx: MergeContext<'_>,
) -> anyhow::Result<String> {
    let MergeContext { store, dir, git_dir, merge_method } = ctx;
    let state = store.load()?;
    let (head_sha, fetched_base_ref) = client.fetch_pr_head_sha_and_base(owner, repo, pr)?;
    let merge_sha = client.merge_pr(owner, repo, pr, Some(merge_method))?;
    let mut out = format!("✓ merged PR #{} ({})\n", pr, merge_sha);

    if let Some(cached_pr) = state.cache.get(&pr) {
        let merged_branch = cached_pr.branch.clone();
        let merged_base = crate::merge::resolve_merge_base(&fetched_base_ref, &cached_pr.base);
        let branch_base_of: std::collections::HashMap<String, String> = state.tracked_prs()
            .iter().filter(|p| p.number != pr)
            .map(|p| (p.branch.clone(), p.base.clone())).collect();
        let has_downstream = branch_base_of.values().any(|parent| parent == &merged_branch);
        if let Err(e) = crate::merge::check_merge_base(&merged_base, has_downstream) {
            out.push_str(&format!("✗ {}\n", e));
        } else {
            let errors = crate::stack::rebase_downstream_stack(
                &merged_branch, &head_sha, &merged_base, &branch_base_of, dir,
                &|b| { let _ = b; },
            );
            for e in &errors { out.push_str(&format!("✗ {}\n", e)); }
            if errors.is_empty() { out.push_str(&format!("✓ rebased downstream stack onto {}\n", merged_base)); }
        }
        let _ = update_children_base(store, &merged_branch, &merged_base);
        let _ = crate::worktree::untrack_and_cleanup(store, dir, git_dir, pr, &merged_branch);
    }
    store.untrack(pr)?;
    out.push_str(&format!("✓ untracked PR #{}\n", pr));
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_rebase_stack(
    client: Option<&dyn crate::github::GithubClientTrait>,
    owner: &str,
    repo: &str,
    store: &crate::store::Store,
    dir: &std::path::Path,
    git_dir: &std::path::Path,
    from_pr: Option<u64>,
    on_progress: &dyn Fn(&str),
) -> anyhow::Result<String> {
    let mut state = store.load()?;
    if state.tracked.is_empty() {
        return Ok("No tracked PRs.\n".to_string());
    }

    let mut out = String::new();
    let debug_fn: Box<dyn Fn(&str)> = Box::new(|s: &str| on_progress(&format!("[fp verbose] {}", s)));

    // Handle merged PRs
    if let Some(client) = client {
        let all_branches: Vec<String> = state.tracked_prs().iter().map(|p| p.branch.clone()).collect();
        let cached_base_of: std::collections::HashMap<String, String> = state.tracked_prs().iter().map(|p| (p.branch.clone(), p.base.clone())).collect();
        let parent_of = crate::stack::detect_parent_of(&all_branches, dir, &cached_base_of, &|_| {})?;
        let mut merged_prs: Vec<(u64, String)> = Vec::new();
        on_progress("[fp] checking merged PRs (GitHub API)");
        for pr in state.tracked_prs() {
            on_progress(&format!("[fp] fetch_pr_is_merged PR #{} (GitHub API)", pr.number));
            let (is_merged, _) = client.fetch_pr_is_merged(owner, repo, pr.number).unwrap_or((false, None));
            if is_merged {
                on_progress(&format!("[fp] fetch head SHA for merged PR #{} (GitHub API)", pr.number));
                let (head_sha, base_ref) = client.fetch_pr_head_sha_and_base(owner, repo, pr.number)?;
                for (branch, parent) in &parent_of {
                    if parent.as_deref() == Some(&pr.branch) {
                        if let Some(warn) = crate::worktree::check_branch_lock(git_dir, branch) {
                            out.push_str(&format!("{}\n", warn)); continue;
                        }
                        match crate::stack::rebase_onto_after_merge(branch, &head_sha, &base_ref, dir) {
                            Ok(()) => out.push_str(&format!("✓ rebased {} onto {} (merged PR #{})\n", branch, base_ref, pr.number)),
                            Err(e) => out.push_str(&format!("✗ failed to rebase {} after merge: {}\n", branch, e)),
                        }
                    }
                }
                merged_prs.push((pr.number, pr.branch.clone()));
            }
        }
        for (number, branch) in merged_prs {
            crate::worktree::untrack_and_cleanup(store, dir, git_dir, number, &branch)?;
            out.push_str(&format!("✓ untracked merged PR #{}\n", number));
        }
        state = store.load()?;

        // Detect untracked squash-merged PRs whose tip is an ancestor of a tracked branch.
        // Scenario: parent PR was merged but not tracked by fp; child branch still has parent
        // commits in its history. Replaying those commits causes add/add conflicts. Fix: find the
        // squash commit on origin/main, look up the PR's head SHA via API, and use it as the
        // --onto cut point so only child-unique commits are replayed.
        let current_branches: Vec<String> = state.tracked_prs().iter().map(|p| p.branch.clone()).collect();
        // Use the oldest tracked branch tip date as the lower bound for squash detection.
        // Any squash merge of a parent PR must have happened after the branch's last commit —
        // if the branch has already been rebased past it, no action is needed. This gives a
        // tight bound that scales with how recently you've been working, regardless of when
        // the PR was opened or how deep the merge-base is.
        let min_tip_date: Option<String> = current_branches.iter().filter_map(|branch| {
            std::process::Command::new("git")
                .args(["log", "--format=%ci", "-1", branch])
                .current_dir(dir).output().ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }).min();
        if let Some(ref since_date) = min_tip_date {
            let squash_commits = crate::stack::squash_pr_numbers_since_date("origin/main", since_date, 200, dir);
            let untracked: Vec<u64> = squash_commits.into_iter()
                .filter(|(_, pr_num)| !state.tracked.contains(pr_num))
                .map(|(_, pr_num)| pr_num)
                .collect();
            on_progress(&format!("[fp] squash detection: {} untracked squash PR(s) to check via API", untracked.len()));
            for pr_num in untracked {
                on_progress(&format!("[fp] fetch head SHA for squash PR #{} (GitHub API)", pr_num));
                let Ok((head_sha, base_ref)) = client.fetch_pr_head_sha_and_base(owner, repo, pr_num) else { continue; };
                // Do not fetch head_sha from origin — git fetch of a bare SHA can hang on GitHub
                // when the remote branch has been deleted. The sha is available locally if the
                // branch was fetched by the git fetch origin call at the top of rebase_stack.
                for branch in &current_branches {
                    let cut_sha = std::process::Command::new("git")
                        .args(["merge-base", &head_sha, branch])
                        .current_dir(dir).output().ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    if let Some(cut) = cut_sha {
                        // If cut is on origin/main, head_sha and branch only share a main commit —
                        // unrelated PR. Skip. If cut is NOT on main, it's on the branch's own
                        // history — it's the correct exclusion point for the rebase.
                        let cut_on_main = std::process::Command::new("git")
                            .args(["merge-base", "--is-ancestor", &cut, "origin/main"])
                            .current_dir(dir).status().map(|s| s.success()).unwrap_or(false);
                        if cut_on_main { continue; }
                        if let Some(warn) = crate::worktree::check_branch_lock(git_dir, branch) {
                            out.push_str(&format!("{}\n", warn)); continue;
                        }
                        match crate::stack::rebase_onto_after_merge(branch, &cut, &base_ref, dir) {
                            Ok(()) => out.push_str(&format!("✓ rebased {} after untracked squash of PR #{}\n", branch, pr_num)),
                            Err(e) => out.push_str(&format!("✗ failed to rebase {} after squash of PR #{}: {}\n", branch, pr_num, e)),
                        }
                    }
                }
            }
        }
    }

    let all_branches: Vec<String> = state.tracked_prs().iter().map(|p| p.branch.clone()).collect();
    if all_branches.is_empty() { return Ok(out); }
    let tracked_set: std::collections::HashSet<String> = all_branches.iter().cloned().collect();
    let cached_base_of = normalize_base_of(
        state.tracked_prs().iter().map(|p| (p.branch.clone(), p.base.clone())).collect(),
        &tracked_set,
    );
    let parent_of = crate::stack::detect_parent_of(&all_branches, dir, &cached_base_of, debug_fn.as_ref())?;

    let branches: Vec<String> = if let Some(fp) = from_pr {
        let start_branch = state.cache.get(&fp)
            .with_context(|| format!("PR #{} is not tracked", fp))?.branch.clone();
        crate::worktree::subtree_branches(&start_branch, &parent_of, &all_branches)
    } else { all_branches };

    let directly_locked: std::collections::HashSet<String> = branches.iter()
        .filter_map(|b| {
            crate::worktree::branch_in_main_worktree_warning(b, dir)
                .or_else(|| crate::worktree::check_branch_lock(git_dir, b))
                .map(|w| { out.push_str(&format!("{}\n", w)); b.clone() })
        }).collect();
    let also_blocked = crate::worktree::locked_subtree(&directly_locked, &parent_of);
    for b in &also_blocked { out.push_str(&format!("⚠ skipping {} — parent branch is locked\n", b)); }
    let branches: Vec<String> = branches.into_iter()
        .filter(|b| !directly_locked.contains(b) && !also_blocked.contains(b)).collect();

    let base_of: std::collections::HashMap<String, String> = if let Some(c) = client {
        normalize_base_of(
            c.fetch_prs_as_map(owner, repo, &state.tracked.iter().copied().collect::<Vec<_>>())
                .into_values().map(|p| (p.branch, p.base)).collect(),
            &tracked_set,
        )
    } else { std::collections::HashMap::new() };

    let result = crate::stack::rebase_stack(&branches, &parent_of, &base_of, dir, &|msg| on_progress(msg), debug_fn.as_ref())?;

    for branch in &result.rebased { out.push_str(&format!("✓ rebased {}\n", branch)); }
    for branch in &result.conflicts {
        out.push_str(&format!("✗ conflict on {} — resolve manually\n", branch));
        let hint = crate::display::format_conflict_hint(branch, &state.cache);
        if !hint.is_empty() { out.push_str(&format!("{}\n", hint)); }
    }
    if let Some(status) = &result.status_output { out.push_str(&format!("\ngit status:\n{}\n", status)); }
    for warn in &result.invariant_warnings { out.push_str(&format!("⚠ {}\n", warn)); }
    if result.rebased.is_empty() && result.conflicts.is_empty() {
        out.push_str("Stack is already up to date.\n");
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_watch(
    client: Option<&dyn crate::github::GithubClientTrait>,
    owner: &str,
    repo_name: &str,
    store: &crate::store::Store,
    git_dir: &std::path::Path,
    once: bool,
    interval: u64,
    json: bool,
    wait_for: Option<String>,
) -> anyhow::Result<String> {
    use crate::tasks::{generate_tasks, task_diff, is_wait_condition_met};
    use crate::display::{format_watch_initial_state, format_watch_event_json, watch_notification_messages};
    use crate::platform::notify_macos_titled;
    use crate::stack::stack_tree_order;
    use crate::store::PrCache;
    use crate::model;

    let mut prev_tasks: std::collections::HashMap<u64, Vec<crate::tasks::Task>> = std::collections::HashMap::new();
    let mut out = String::new();

    loop {
        let state = store.load()?;
        let pr_numbers: Vec<u64> = state.tracked.iter().copied().collect();
        let fetched: std::collections::HashMap<u64, model::PrState> =
            if let Some(c) = client {
                c.fetch_prs_parallel(owner, repo_name, &pr_numbers)
                    .into_iter().map(|p| (p.number, p)).collect()
            } else {
                std::collections::HashMap::new()
            };

        let new_cache: std::collections::HashMap<u64, PrCache> = fetched.values()
            .filter(|p| state.tracked.contains(&p.number))
            .map(|p| (p.number, PrCache { number: p.number, title: p.title.clone(), branch: p.branch.clone(), base: p.base.clone() }))
            .collect();
        let _ = store.replace_cache(new_cache);
        let state = store.load()?;

        let prs = state.tracked_prs();
        let tree_prefixes: std::collections::HashMap<u64, String> =
            stack_tree_order(&prs).into_iter().collect();

        let mut all_tasks: Vec<crate::tasks::Task> = Vec::new();
        for cached in &prs {
            let mut pr_state = fetched.get(&cached.number).cloned()
                .unwrap_or_else(|| model::PrState {
                    number: cached.number,
                    title: cached.title.clone(),
                    branch: cached.branch.clone(),
                    base: cached.base.clone(), head_sha: "".into(), draft: false, approved: false,
                    checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
                    codeowners_eligibility: Default::default(), created_at: None,
                });
            if let Some(c) = client
                && let Some(parent) = prs.iter().find(|p| p.branch == pr_state.base).and_then(|p| fetched.get(&p.number))
                && !parent.head_sha.is_empty() && !pr_state.head_sha.is_empty() {
                pr_state.needs_parent_rebase = c.is_head_behind_base(owner, repo_name, &parent.head_sha, &pr_state.head_sha);
            }
            let curr = generate_tasks(&pr_state);
            all_tasks.extend(curr.clone());

            let prev = prev_tasks.get(&cached.number).map(|v| v.as_slice()).unwrap_or(&[]);
            let (new, resolved) = task_diff(prev, &curr);

            if prev_tasks.contains_key(&cached.number) {
                if json {
                    out.push_str(&format!("{}\n", format_watch_event_json(cached.number, &new, &resolved)));
                } else {
                    for t in &new {
                        let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
                        out.push_str(&format!("+ PR #{} {} {:?}: {}\n", cached.number, flag, t.task_type, t.description));
                    }
                    for t in &resolved {
                        out.push_str(&format!("✓ PR #{} resolved {:?}: {}\n", cached.number, t.task_type, t.description));
                    }
                    for (title, msg) in watch_notification_messages(cached.number, &new, &resolved) {
                        notify_macos_titled(&title, &msg);
                    }
                }
            } else {
                let lock = crate::worktree::lock_status(git_dir, &cached.branch);
                let prefix = tree_prefixes.get(&cached.number).cloned().unwrap_or_default();
                out.push_str(&format_watch_initial_state(cached.number, &cached.title, &curr, json, lock.as_deref(), &prefix));
            }
            prev_tasks.insert(cached.number, curr);
        }

        if once { break; }
        if let Some(ref condition) = wait_for
            && is_wait_condition_met(condition, &all_tasks) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(interval));
    }

    Ok(out)
}

pub fn cmd_app_set_config(store: &crate::app_config::AppConfigStore, repo: &str, config_name: &str) -> anyhow::Result<String> {
    store.set_repo_config(repo, config_name)?;
    Ok(format!("Assigned config '{}' to repo '{}'", config_name, repo))
}

pub fn cmd_pr_set_config(store: &crate::app_config::AppConfigStore, pr: u64, config_name: &str) -> anyhow::Result<String> {
    store.set_pr_config(pr, config_name)?;
    Ok(format!("Assigned config '{}' to PR #{}", config_name, pr))
}

pub fn cmd_feature_new(ps: &crate::process_store::ProcessStateStore, name: &str) -> anyhow::Result<String> {
    crate::feature::feature_new(ps, name)?;
    Ok(format!("Created feature envelope '{}'", name))
}

pub fn cmd_feature_list(ps: &crate::process_store::ProcessStateStore) -> anyhow::Result<String> {
    let list = crate::feature::feature_list(ps)?;
    if list.is_empty() {
        return Ok("No feature envelopes.".to_string());
    }
    let mut out = String::new();
    for f in &list {
        out.push_str(&format!("  {} ({} PR(s)): {}\n", f.name, f.prs.len(),
            f.prs.iter().map(|p| format!("#{}", p)).collect::<Vec<_>>().join(", ")));
    }
    Ok(out.trim_end().to_string())
}

pub fn cmd_feature_up(ps: &crate::process_store::ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str) -> anyhow::Result<String> {
    let msgs = crate::feature::feature_up(ps, config, name)?;
    if msgs.is_empty() {
        Ok(format!("Feature '{}' has no member PRs with app configs.", name))
    } else {
        Ok(msgs.join("\n"))
    }
}

pub fn cmd_feature_down(ps: &crate::process_store::ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str) -> anyhow::Result<String> {
    let msgs = crate::feature::feature_down(ps, config, name)?;
    if msgs.is_empty() {
        Ok(format!("Feature '{}' has no member PRs with app configs.", name))
    } else {
        Ok(msgs.join("\n"))
    }
}

pub fn cmd_feature_rebuild(ps: &crate::process_store::ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str, pr: Option<u64>) -> anyhow::Result<String> {
    let msgs = crate::feature::feature_rebuild(ps, config, name, pr)?;
    if msgs.is_empty() {
        Ok(format!("Feature '{}' has no matching ephemeral members.", name))
    } else {
        Ok(msgs.join("\n"))
    }
}

pub fn cmd_feature_list_running(ps: &crate::process_store::ProcessStateStore) -> anyhow::Result<String> {
    let list = crate::feature::feature_list_running(ps)?;
    if list.is_empty() {
        return Ok("No running feature envelopes.".to_string());
    }
    let mut out = String::new();
    for f in &list {
        out.push_str(&format!("  {} ({} PR(s)): {}\n", f.name, f.prs.len(),
            f.prs.iter().map(|p| format!("#{}", p)).collect::<Vec<_>>().join(", ")));
    }
    Ok(out.trim_end().to_string())
}

pub fn cmd_feature_status(
    ps: &crate::process_store::ProcessStateStore,
    config: &crate::app_config::AppConfigStore,
    name: &str,
) -> anyhow::Result<String> {
    let statuses = crate::feature::feature_status(ps, config, name)?;
    if statuses.is_empty() {
        return Ok(format!("Feature '{}' has no member PRs.", name));
    }
    let mut out = String::new();
    for s in &statuses {
        let pid = if s.pid_alive { "✓ running" } else { "✗ stopped" };
        let branch = if s.branch_ok { "✓ branch ok" } else { "✗ wrong branch" };
        let health = match s.service_healthy {
            Some(true) => " ✓ healthy",
            Some(false) => " ✗ unhealthy",
            None => "",
        };
        out.push_str(&format!("  PR #{}  {}{}  {}\n", s.pr, pid, health, branch));
    }
    Ok(out.trim_end().to_string())
}

pub fn cmd_pr_up(ps: &crate::process_store::ProcessStateStore, config: &crate::app_config::AppConfigStore, pr: u64, worktree: &str) -> anyhow::Result<String> {
    let app_cfg_name = config.get_pr_config(pr)?;
    let cfg = app_cfg_name.as_deref()
        .and_then(|n| config.load_app_config(n).ok().flatten())
        .ok_or_else(|| anyhow::anyhow!("no app config assigned to PR #{}", pr))?;
    let wt = std::path::Path::new(worktree);
    crate::feature::bootstrap_pr(ps, &cfg, pr, wt, "", "")?;
    Ok(format!("PR #{}: started ({})", pr, cfg.name))
}

pub fn cmd_feature_add_dep(ps: &crate::process_store::ProcessStateStore, name: &str, app_config: &str) -> anyhow::Result<String> {
    crate::feature::feature_add_dep(ps, name, app_config)?;
    Ok(format!("Added dep '{}' to feature '{}'", app_config, name))
}

pub fn cmd_feature_add(ps: &crate::process_store::ProcessStateStore, store: &crate::store::Store, name: &str, pr: u64) -> anyhow::Result<String> {
    crate::feature::feature_add(ps, store, name, pr)?;
    Ok(format!("Added PR #{} to feature '{}'", pr, name))
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_app_define_config(
    store: &crate::app_config::AppConfigStore,
    name: &str,
    bootstrap: &str,
    teardown: &str,
    startup_timeout: &str,
    health_check: Option<&str>,
    ephemeral: bool,
    main_worktree: Option<&str>,
) -> anyhow::Result<String> {
    store.save_app_config(crate::app_config::AppConfig {
        name: name.to_string(),
        bootstrap: bootstrap.to_string(),
        teardown: teardown.to_string(),
        startup_timeout: startup_timeout.to_string(),
        health_check: health_check.map(str::to_string),
        ephemeral,
        main_worktree: main_worktree.map(str::to_string),
    })?;
    Ok(format!("Defined app config '{}'", name))
}
