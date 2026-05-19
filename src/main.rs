mod model;
mod tasks;
mod store;
mod github;
mod ci;
mod stack;
mod profile;
mod worktree;
pub mod display;
pub mod credentials;
pub mod agent;
pub mod shell;

#[cfg(test)]
mod tasks_test;
#[cfg(test)]
mod store_test;
#[cfg(test)]
mod github_test;
#[cfg(test)]
mod ci_test;
#[cfg(test)]
mod stack_test;
#[cfg(test)]
mod notify_test;
#[cfg(test)]
mod agent_test;
#[cfg(test)]
mod display_test;
#[cfg(test)]
mod credentials_test;
#[cfg(test)]
mod agent_manifest_test;
#[cfg(test)]
mod shell_test;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use github::{GithubClient, detect_repo, resolve_github_token, resolve_track_branch, fetch_open_threads, format_open_threads, format_resolved_threads};
use store::{Store, PrCache};
use tasks::{generate_tasks, task_diff};

const FP_SKILL: &str = include_str!("../assets/fp-skill.md");


#[derive(Parser)]
#[command(name = "fp", about = "PR convergence loop")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all tracked PRs with status summary
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Show tasks blocking readiness for a PR
    Status {
        /// PR number (defaults to current branch's PR if tracked)
        pr: Option<u64>,
        #[arg(long)]
        json: bool,
        /// Show all tracked PRs
        #[arg(long)]
        all: bool,
    },
    /// Add a PR to the tracked set
    Track {
        pr: u64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        branch: Option<String>,
    },
    /// Remove a PR from the tracked set
    Untrack { pr: u64 },
    /// Poll all tracked PRs and print task changes
    Watch {
        /// Fetch once and exit
        #[arg(long)]
        once: bool,
        /// Poll interval in seconds (default: 30)
        #[arg(long, default_value = "30")]
        interval: u64,
        /// Emit JSON event objects per cycle
        #[arg(long)]
        json: bool,
        /// Block until condition is met: ci-pass, ready
        #[arg(long)]
        wait_for: Option<String>,
    },
    /// Mark a draft PR as ready for review
    Ready {
        /// PR number
        pr: u64,
    },
    /// Post a general comment on a PR (not a thread reply)
    Comment {
        /// PR number
        pr: u64,
        /// Comment body
        message: String,
    },
    /// Reply to a PR review thread and mark it as addressed
    Reply {
        /// PR number
        pr: u64,
        /// Thread (comment) ID
        thread_id: u64,
        /// Reply message body
        message: String,
    },
    /// Show full context for a task (check logs URL, thread body, etc.)
    Context {
        /// PR number
        pr: u64,
        /// Context hint from task output (check name or thread:<id>)
        hint: String,
        /// Write the full raw log to a temp file and print its path
        #[arg(long)]
        full_log: bool,
    },
    /// Show review threads for a PR
    Threads {
        /// PR number
        pr: u64,
        /// Show resolved threads (default: open/stale only)
        #[arg(long)]
        resolved: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print machine-readable capability manifest for agent consumption
    AgentContext {
        #[arg(long)]
        json: bool,
    },
    /// Save or load a named profile (auth + repo config bundle)
    Profile {
        /// save <name> or load <name>
        action: String,
        /// Profile name
        name: String,
        /// GitHub token (required for save)
        #[arg(long)]
        token: Option<String>,
        /// Repository (owner/repo, required for save)
        #[arg(long)]
        repo: Option<String>,
    },
    /// Show check run results for a specific commit SHA (useful for reviewing failures after pushing a new commit)
    Checks {
        /// Commit SHA to fetch check runs for
        sha: String,
    },
    /// Create a draft PR for the current branch and start tracking it
    Create {
        /// PR title
        title: String,
        /// Base branch (default: main)
        #[arg(long, default_value = "main")]
        base: String,
        /// PR description body
        #[arg(long)]
        body: Option<String>,
        /// Attach a demo (URL or file path); repeatable
        #[arg(long, value_name = "FILE_OR_URL")]
        demo: Vec<String>,
        /// Insert current branch before this PR: rebase that PR onto current branch
        #[arg(long)]
        restack_before: Option<u64>,
        /// Insert current branch after this PR: rebase the PR that follows it onto current branch
        #[arg(long)]
        insert_after: Option<u64>,
    },
    /// Edit a PR's title and/or body
    Edit {
        /// PR number
        pr: u64,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New body
        #[arg(long)]
        body: Option<String>,
        /// Attach a demo (URL or file path); repeatable
        #[arg(long, value_name = "FILE_OR_URL")]
        demo: Vec<String>,
    },
    /// Merge a PR via the GitHub API and rebase downstream tracked branches
    Merge {
        /// PR number to merge
        pr: u64,
        /// Merge method: squash, rebase, or merge (default: repo default)
        #[arg(long)]
        squash: bool,
        #[arg(long)]
        rebase: bool,
        #[arg(long, name = "merge")]
        merge_commit: bool,
    },
    /// Rebase tracked PRs in stack order onto their parent branches
    RebaseStack {
        /// Optional PR number to start from — rebase only that PR and its descendants
        pr: Option<u64>,
    },
    /// Create a new branch and worktree without creating a PR (use fp create afterwards)
    New {
        /// New branch name
        branch: String,
        /// Base branch to branch from (default: main)
        #[arg(long, default_value = "main")]
        base: String,
    },
    /// Switch to the worktree for a tracked PR (creates if needed)
    Switch {
        /// PR number
        pr: u64,
        /// Session identifier for the lock (e.g. agent name or session ID)
        id: String,
        /// Skip dirty-check on current worktree
        #[arg(long)]
        force: bool,
        /// Move branch from main worktree to an fp worktree (checks out main in main worktree)
        #[arg(long)]
        adopt: bool,
    },
    /// Remove the lock on a worktree branch so it can be switched to again
    Unlock {
        /// Branch name (not PR number)
        branch: String,
    },
    /// Print the main repo root directory (works from inside a worktree)
    Root,
    /// Install the fps shell function for the current shell (fish, zsh, or bash)
    InstallShell {
        /// Shell to install for (default: auto-detect from $SHELL)
        #[arg(long)]
        shell: Option<String>,
        /// Print the function to stdout instead of writing to disk
        #[arg(long)]
        print: bool,
    },
    /// Install the fp Claude Code skill into ~/.claude/skills/fp/SKILL.md
    InstallSkills {
        /// Alternative output path (overrides default ~/.claude/skills/fp/SKILL.md)
        #[arg(long)]
        path: Option<std::path::PathBuf>,
    },
}

/// Resolves `--demo` arguments to CDN URLs. URL strings pass through; file paths are uploaded
/// via the GitHub asset upload API. Returns error for file paths if upload is unavailable.
fn resolve_demo_urls(client: &github::GithubClient, owner: &str, repo: &str, demos: &[String]) -> anyhow::Result<Vec<String>> {
    let mut urls = Vec::new();
    for demo in demos {
        if demo.starts_with("http://") || demo.starts_with("https://") {
            urls.push(demo.clone());
        } else {
            let session = std::env::var("GITHUB_USER_SESSION")
                .or_else(|_| {
                    let db = std::env::var("CHROME_COOKIES_PATH")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|_| {
                            dirs::home_dir().unwrap_or_default()
                                .join("Library/Application Support/Google/Chrome/Default/Cookies")
                        });
                    github::extract_github_session_from_browser_with_chrome_db(&db)
                })
                .map_err(|_| anyhow::anyhow!(
                    "no GitHub session found — set GITHUB_USER_SESSION env var or log into GitHub in a supported browser (Chrome, Firefox, Safari)"
                ))?;
            let path = std::path::Path::new(demo);
            let url = github::github_upload_image(path, owner, repo, client, &session)?;
            urls.push(url);
        }
    }
    Ok(urls)
}

/// Send a macOS system notification via osascript. Silently ignores errors on non-macOS or
/// headless environments where notifications are unavailable.
/// Falsify exemption: fire-and-forget subprocess with no observable return value in the test
/// process — no artifact type can detect absence of the osascript call without a subprocess harness.
pub fn watch_notification_messages(
    pr: u64,
    new: &[tasks::Task],
    resolved: &[tasks::Task],
) -> Vec<(String, String)> {
    let title = format!("fp: #{}", pr);
    let mut msgs = Vec::new();
    for t in resolved {
        match t.task_type {
            tasks::TaskType::FixCi => msgs.push((title.clone(), format!("CI passing: {}", t.context_hint))),
            tasks::TaskType::AwaitingReview => msgs.push((title.clone(), "PR approved".into())),
            _ => {}
        }
    }
    for t in new {
        match t.task_type {
            tasks::TaskType::RespondThread => msgs.push((title.clone(), format!("New review thread: {}", t.description))),
            tasks::TaskType::FixCi => msgs.push((title.clone(), format!("CI failing: {}", t.context_hint))),
            _ => {}
        }
    }
    msgs
}


pub use shell::{fps_function_content, fps_install_path, detect_shell};

pub use display::{format_watch_initial_state, format_pr_status_all_entry, format_watch_event_json};

pub use worktree::{branch_in_main_worktree_warning, check_not_checked_out_in_main};

/// Errors when merged_base is empty and there are downstream PRs that need rebasing.
pub fn check_merge_base(merged_base: &str, has_downstream: bool) -> Result<()> {
    if merged_base.is_empty() && has_downstream {
        anyhow::bail!("could not determine merge base — downstream PRs were not rebased; rebase manually with: git rebase --onto <new-main-tip> <old-parent-tip> <branch>");
    }
    Ok(())
}

/// Returns `fetched` when non-empty (GitHub API is authoritative), else falls back to `stored`.
pub fn resolve_merge_base(fetched: &str, stored: &str) -> String {
    if !fetched.is_empty() { fetched.to_string() } else { stored.to_string() }
}

pub use stack::stack_tree_order;

/// Returns a skip-warning if branch has any lock (live or dead), None if clear.
pub fn check_branch_lock(git_dir: &std::path::Path, branch: &str) -> Option<String> {
    let lp = worktree::lock_path(git_dir, branch);
    let lock = worktree::read_lock(&lp)?;
    let my_pid = worktree::session_anchor_pid();
    if worktree::lock_is_live(&lock) {
        Some(format!(
            "⚠ skipping {} — locked by {} (pid {}, alive; your pid: {}). \
            Steal only after confirming the other process has finished: fp unlock {}",
            branch, lock.id, lock.pid, my_pid, branch
        ))
    } else {
        Some(format!(
            "⚠ skipping {} — dead lock from {} (pid {} no longer running; your pid: {}). \
            Safe to steal: fp unlock {}",
            branch, lock.id, lock.pid, my_pid, branch
        ))
    }
}

pub use worktree::{locked_subtree, subtree_branches, worktree_branch_mismatch, fix_worktree_branch};

pub use display::{format_adopt_message, format_new_worktree_output, repo_header, format_single_pr_status, format_worktree_add_error, format_conflict_hint};

pub fn require_repo(repo: Option<(String, String)>) -> Result<(String, String)> {
    repo.ok_or_else(|| anyhow::anyhow!("no GitHub remote detected — cannot determine which repository these PRs belong to"))
}


fn notify_macos_titled(title: &str, msg: &str) {
    let script = format!(
        r#"display notification "{}" with title "{}""#,
        msg.replace('"', "'"),
        title.replace('"', "'")
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
}

fn git_dir() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    worktree::common_git_dir(&cwd).context("not in a git repository")
}

fn repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    worktree::main_repo_root(&cwd).context("not in a git repository")
}

fn cleanup_pr_worktree(repo_root: &std::path::Path, git_dir: &std::path::Path, branch: &str) {
    let wt_path = worktree::worktree_path(repo_root, branch);
    if wt_path.exists() {
        let remove = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", wt_path.to_str().unwrap_or("")])
            .output();
        if remove.map(|o| o.status.success()).unwrap_or(false) {
            let _ = std::process::Command::new("git").args(["worktree", "prune"]).output();
        }
    }
    let lp = worktree::lock_path(git_dir, branch);
    let _ = worktree::remove_lock(&lp);
}

fn untrack_and_cleanup(store: &Store, repo_root: &std::path::Path, git_dir: &std::path::Path, number: u64, branch: &str) -> Result<()> {
    store.untrack(number)?;
    cleanup_pr_worktree(repo_root, git_dir, branch);
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let git_dir = git_dir()?;
    let store = Store::open(&git_dir);

    match cli.command {
        Commands::Ls { json } => {
            let (owner, repo_name) = require_repo(detect_repo())?;
            let state = store.load()?;
            if json {
                let items = state.tracked_prs();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                println!("{}", repo_header(&owner, &repo_name));
                if state.tracked.is_empty() {
                    println!("No tracked PRs. Use `fp track <pr>` to add one.");
                } else {
                    let prs = state.tracked_prs();
                    for (number, prefix) in stack_tree_order(&prs) {
                        if let Some(pr) = state.cache.get(&number) {
                            println!("{}#{} {} ({})", prefix, pr.number, pr.title, pr.branch);
                        }
                    }
                }
            }
        }

        Commands::Status { pr, json, all } => {
            let state = store.load()?;
            let token = std::env::var("GITHUB_TOKEN").ok();
            let repo = require_repo(detect_repo())?;

            let fetch = |number: u64, _branch: &str| -> Option<crate::model::PrState> {
                if let Some(tok) = &token {
                    let client = GithubClient::new(tok.clone());
                    client.fetch_pr(&repo.0, &repo.1, number).ok()
                } else {
                    None
                }
            };

            if all {
                if !json { println!("{}", repo_header(&repo.0, &repo.1)); }
                let pr_numbers: Vec<u64> = state.tracked.iter().copied().collect();

                let fetched: std::collections::HashMap<u64, crate::model::PrState> =
                    if let Some(tok) = &token {
                        GithubClient::new(tok.clone()).fetch_prs_as_map(&repo.0, &repo.1, &pr_numbers)
                    } else {
                        std::collections::HashMap::new()
                    };

                // Refresh cache from API so tree order uses current base branches
                let new_cache: std::collections::HashMap<u64, PrCache> = fetched.values()
                    .filter(|p| state.tracked.contains(&p.number))
                    .map(|p| (p.number, PrCache { number: p.number, title: p.title.clone(), branch: p.branch.clone(), base: p.base.clone() }))
                    .collect();
                let _ = store.replace_cache(new_cache);
                let state = store.load()?;

                let prs = state.tracked_prs();
                let tree_order = stack_tree_order(&prs);
                for (number, prefix) in tree_order {
                    let cached = match state.cache.get(&number) { Some(t) => t, None => continue };
                    let mut pr_state = fetched.get(&number).cloned()
                        .unwrap_or_else(|| crate::model::PrState {
                            number: cached.number,
                            title: cached.title.clone(),
                            branch: cached.branch.clone(),
                            base: cached.base.clone(), head_sha: "".into(), draft: false, approved: false,
                            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(),
                        });
                    if let (Some(tok), Some((owner, repo_name))) = (&token, detect_repo())
                        && let Some(parent) = prs.iter().find(|p| p.branch == cached.base).and_then(|p| fetched.get(&p.number))
                        && !parent.head_sha.is_empty() && !pr_state.head_sha.is_empty() {
                        pr_state.needs_parent_rebase = GithubClient::new(tok.clone())
                            .is_head_behind_base(&owner, &repo_name, &parent.head_sha, &pr_state.head_sha);
                    }
                    let tasks = generate_tasks(&pr_state);
                    let lock = worktree::lock_status(&git_dir, &cached.branch)
                        .map(|s| format!("  {}", s))
                        .unwrap_or_default();
                    if json {
                        println!("{}", serde_json::to_string_pretty(&tasks).unwrap());
                    } else {
                        print!("{}", format_pr_status_all_entry(&prefix, cached.number, &cached.title, &tasks, &lock));
                    }
                }
            } else {
                let number = pr.context("specify a PR number or use --all")?;

                let cached = state.cache.get(&number)
                    .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", number, number))?;

                let mut pr_state_single = fetch(cached.number, &cached.branch)
                    .unwrap_or_else(|| crate::model::PrState {
                        number: cached.number,
                        title: cached.title.clone(),
                        branch: cached.branch.clone(),
                        base: cached.base.clone(), head_sha: "".into(), draft: false, approved: false,
                        checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(),
                    });
                if let (Some(tok), Some((owner, repo_name))) = (&token, detect_repo())
                    && let Some(parent) = state.tracked_prs().into_iter().find(|p| p.branch == pr_state_single.base).and_then(|p| fetch(p.number, &p.branch))
                    && !parent.head_sha.is_empty() && !pr_state_single.head_sha.is_empty() {
                    pr_state_single.needs_parent_rebase = GithubClient::new(tok.clone())
                        .is_head_behind_base(&owner, &repo_name, &parent.head_sha, &pr_state_single.head_sha);
                }
                let task_list = generate_tasks(&pr_state_single);
                let lock = worktree::lock_status(&git_dir, &cached.branch);

                if json {
                    println!("{}", serde_json::to_string_pretty(&task_list)?);
                } else {
                    println!("{}", format_single_pr_status(number, &task_list, lock.as_deref()));
                }
            }
        }

        Commands::Track { pr, title, branch } => {
            let (title, resolved_branch, base) = {
                let token = resolve_github_token().ok();
                let repo = detect_repo();
                let (fetched_title, fetched_branch, fetched_base) = if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                    let client = GithubClient::new(tok);
                    client.fetch_pr_metadata(&owner, &repo_name, pr).ok()
                        .map(|(t, b)| {
                            let base = client.fetch_pr_base(&owner, &repo_name, pr).unwrap_or_default();
                            (Some(t), Some(b), base)
                        })
                        .unwrap_or((None, None, String::new()))
                } else {
                    (None, None, String::new())
                };
                let resolved_title = title.or(fetched_title).unwrap_or_else(|| format!("PR #{}", pr));
                let resolved_branch = resolve_track_branch(branch, fetched_branch, pr)?;
                (resolved_title, resolved_branch, fetched_base)
            };
            store.track(pr)?;
            store.update_cache(PrCache { number: pr, title: title.clone(), branch: resolved_branch, base })?;
            println!("Tracking PR #{} — {}", pr, title);
        }

        Commands::Untrack { pr } => {
            let branch = {
                let state = store.load()?;
                state.cache.get(&pr).map(|t| t.branch.clone())
            };
            if let Some(branch) = branch {
                untrack_and_cleanup(&store, &repo_root()?, &git_dir, pr, &branch)?;
            } else {
                store.untrack(pr)?;
            }
            println!("Untracked PR #{}", pr);
        }

        Commands::Switch { pr, id, force, adopt } => {
            let state = store.load()?;
            let cached = state.cache.get(&pr)
                .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;
            let branch = cached.branch.clone();
            let root = repo_root()?;

            if !force && worktree::repo_is_dirty(&std::env::current_dir()?)? {
                anyhow::bail!("current worktree has uncommitted changes — commit, stash, or use --force to override");
            }

            // Check if branch is in main worktree
            let head_out = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&root)
                .output()?;
            let current_head = String::from_utf8(head_out.stdout)?.trim().to_string();
            if current_head == branch {
                if adopt {
                    // Move branch to fp worktree: checkout main in main worktree first
                    let checkout = std::process::Command::new("git")
                        .args(["checkout", "main"])
                        .current_dir(&root)
                        .output()?;
                    anyhow::ensure!(checkout.status.success(), "git checkout main failed: {}",
                        String::from_utf8_lossy(&checkout.stderr));
                    print!("{}", format_adopt_message(&branch));
                } else {
                    check_not_checked_out_in_main(&branch, &root)?;
                }
            }
            worktree::check_target_lock(&git_dir, &branch)?;

            let wt_path = worktree::worktree_path(&root, &branch);
            if !wt_path.exists() {
                let out = std::process::Command::new("git")
                    .args(["worktree", "add", wt_path.to_str().unwrap_or(""), &branch])
                    .output()?;
                anyhow::ensure!(out.status.success(), "{}",
                    format_worktree_add_error(&String::from_utf8_lossy(&out.stderr), &branch, pr));
            } else if worktree_branch_mismatch(&wt_path, &branch)? {
                fix_worktree_branch(&wt_path, &branch, force)
                    .with_context(|| format!("worktree at {} is on wrong branch — use --force to discard local changes and fix it", wt_path.display()))?;
            }

            let lp = worktree::lock_path(&git_dir, &branch);
            worktree::write_lock(&lp, worktree::session_anchor_pid(), "agent", &id)?;
            println!("{}", wt_path.display());
        }

        Commands::Reply { pr, thread_id, message } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token.clone());
            let pr_state = client.fetch_pr(&owner, &repo_name, pr)?;
            let thread = pr_state.threads.iter().find(|t| t.id == thread_id)
                .context(format!("thread #{} not found on PR #{}", thread_id, pr))?;
            let posted = client.reply_to_thread(&owner, &repo_name, pr, thread, &message)?;
            println!("Replied to thread #{}: {}", thread_id, posted);
        }

        Commands::Ready { pr } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            client.mark_pr_ready(&owner, &repo_name, pr)?;
            println!("PR #{} marked as ready for review.", pr);
        }

        Commands::Comment { pr, message } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            let url = client.post_pr_comment(&owner, &repo_name, pr, &message)?;
            println!("Comment posted: {}", url);
        }

        Commands::Edit { pr, title, body, demo } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            let final_body: Option<String> = if demo.is_empty() {
                body
            } else {
                let demo_urls = resolve_demo_urls(&client, &owner, &repo_name, &demo)?;
                // Fetch current PR body if --body not provided, so we don't discard it
                let base_body = match body {
                    Some(ref b) => b.clone(),
                    None => client.fetch_pr_body(&owner, &repo_name, pr)?,
                };
                Some(github::inject_demo_section(&base_body, &demo_urls))
            };
            client.update_pr(&owner, &repo_name, pr, title.as_deref(), final_body.as_deref())?;
            println!("✓ PR #{} updated", pr);
        }

        Commands::Watch { once, interval, json, wait_for } => {
            let mut prev_tasks: std::collections::HashMap<u64, Vec<tasks::Task>> = std::collections::HashMap::new();
            loop {
                let state = store.load()?;
                let token = std::env::var("GITHUB_TOKEN").ok();
                let repo = detect_repo();
                let pr_numbers: Vec<u64> = state.tracked.iter().copied().collect();
                let fetched: std::collections::HashMap<u64, model::PrState> =
                    if let (Some(tok), Some((owner, repo_name))) = (&token, &repo) {
                        let client = GithubClient::new(tok.clone());
                        client.fetch_prs_parallel(owner, repo_name, &pr_numbers)
                            .into_iter().map(|p| (p.number, p)).collect()
                    } else {
                        std::collections::HashMap::new()
                    };

                // Refresh cache from API
                let new_cache: std::collections::HashMap<u64, PrCache> = fetched.values()
                    .filter(|p| state.tracked.contains(&p.number))
                    .map(|p| (p.number, PrCache { number: p.number, title: p.title.clone(), branch: p.branch.clone(), base: p.base.clone() }))
                    .collect();
                let _ = store.replace_cache(new_cache);
                let state = store.load()?;

                let prs = state.tracked_prs();
                let tree_prefixes: std::collections::HashMap<u64, String> =
                    stack_tree_order(&prs).into_iter().collect();

                let mut all_tasks: Vec<tasks::Task> = Vec::new();
                for cached in &prs {
                    let mut pr_state = fetched.get(&cached.number).cloned()
                        .unwrap_or_else(|| model::PrState {
                            number: cached.number,
                            title: cached.title.clone(),
                            branch: cached.branch.clone(),
                            base: cached.base.clone(), head_sha: "".into(), draft: false, approved: false,
                            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(),
                        });
                    if let (Some(tok), Some((owner, repo_name))) = (&token, &repo)
                        && let Some(parent) = prs.iter().find(|p| p.branch == pr_state.base).and_then(|p| fetched.get(&p.number))
                        && !parent.head_sha.is_empty() && !pr_state.head_sha.is_empty() {
                        pr_state.needs_parent_rebase = GithubClient::new(tok.clone())
                            .is_head_behind_base(owner, repo_name, &parent.head_sha, &pr_state.head_sha);
                    }
                    let curr = generate_tasks(&pr_state);
                    all_tasks.extend(curr.clone());

                    let prev = prev_tasks.get(&cached.number).map(|v| v.as_slice()).unwrap_or(&[]);
                    let (new, resolved) = task_diff(prev, &curr);

                    if prev_tasks.contains_key(&cached.number) {
                        if json {
                            println!("{}", format_watch_event_json(cached.number, &new, &resolved));
                        } else {
                            for t in &new {
                                let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
                                println!("+ PR #{} {} {:?}: {}", cached.number, flag, t.task_type, t.description);
                            }
                            for t in &resolved {
                                println!("✓ PR #{} resolved {:?}: {}", cached.number, t.task_type, t.description);
                            }
                            for (title, msg) in watch_notification_messages(cached.number, &new, &resolved) {
                                notify_macos_titled(&title, &msg);
                            }
                        }
                    } else {
                        let lock = worktree::lock_status(&git_dir, &cached.branch);
                        let prefix = tree_prefixes.get(&cached.number).cloned().unwrap_or_default();
                        print!("{}{}", prefix, format_watch_initial_state(cached.number, &cached.title, &curr, json, lock.as_deref(), &prefix));
                    }
                    prev_tasks.insert(cached.number, curr);
                }

                if let Some(ref condition) = wait_for {
                    if tasks::is_wait_condition_met(condition, &all_tasks) {
                        break;
                    }
                } else if once {
                    break;
                }
                if !once { std::thread::sleep(std::time::Duration::from_secs(interval)); }
            }
        }

        Commands::Create { title, base, body, demo, restack_before, insert_after } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;

            // Get current branch
            let out = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .context("failed to run git")?;
            let head_branch = String::from_utf8(out.stdout)?.trim().to_string();
            let main_root = repo_root()?;

            let client = GithubClient::new(token);
            let final_body = if demo.is_empty() {
                body
            } else {
                let demo_urls = resolve_demo_urls(&client, &owner, &repo_name, &demo)?;
                Some(github::inject_demo_section(body.as_deref().unwrap_or(""), &demo_urls))
            };
            let pr_state = client.create_pr_with_body(&owner, &repo_name, &title, &head_branch, &base, true, final_body.as_deref())?;
            store.track(pr_state.number)?;
            store.update_cache(PrCache {
                number: pr_state.number,
                title: pr_state.title.clone(),
                branch: pr_state.branch.clone(),
                base: pr_state.base.clone(),
            })?;
            println!("Created PR #{}: {} ({})", pr_state.number, pr_state.title, pr_state.branch);

            // --restack-before <pr>: rebase that PR's branch onto current branch
            if let Some(target_pr) = restack_before {
                let target_branch = client.fetch_pr_metadata(&owner, &repo_name, target_pr)?.1;
                let old_base = client.fetch_pr_base(&owner, &repo_name, target_pr)?;
                stack::rebase_branch_onto(&target_branch, &old_base, &head_branch, &main_root)?;
                client.update_pr_base(&owner, &repo_name, target_pr, &head_branch)?;
                println!("Restacked PR #{} onto {} (rebased {} --onto {})", target_pr, head_branch, target_branch, head_branch);
            }

            // --insert-after <pr>: find the PR whose base is <pr>'s branch, rebase it onto current branch
            if let Some(anchor_pr) = insert_after {
                let anchor_branch = client.fetch_pr_metadata(&owner, &repo_name, anchor_pr)?.1;
                let state = store.load()?;
                // Find tracked PR whose base is anchor_branch
                let next_pr = state.tracked_prs().into_iter()
                    .find(|p| {
                        client.fetch_pr_base(&owner, &repo_name, p.number)
                            .ok().as_deref() == Some(&anchor_branch)
                    })
                    .cloned();
                if let Some(next) = next_pr {
                    let next_branch = next.branch.clone();
                    let next_pr_num = next.number;
                    stack::rebase_branch_onto(&next_branch, &anchor_branch, &head_branch, &main_root)?;
                    client.update_pr_base(&owner, &repo_name, next_pr_num, &head_branch)?;
                    println!("Inserted {} between PR #{} and PR #{}", head_branch, anchor_pr, next_pr_num);
                } else {
                    println!("No tracked PR found with base {}; nothing to restack", anchor_branch);
                }
            }
        }

        Commands::New { branch, base } => {
            let root = repo_root()?;
            let wt_path = worktree::worktree_path(&root, &branch);
            std::process::Command::new("git")
                .args(["fetch", "origin", &base])
                .output()?;
            let out = std::process::Command::new("git")
                .args(["worktree", "add", wt_path.to_str().unwrap_or(""), "-b", &branch, &format!("origin/{}", base)])
                .output()?;
            anyhow::ensure!(out.status.success(), "git worktree add failed: {}", String::from_utf8_lossy(&out.stderr));
            print!("{}", format_new_worktree_output(&wt_path, &branch));
        }

        Commands::Root => {
            println!("{}", repo_root()?.display());
        }

        Commands::Unlock { branch } => {
            let lp = worktree::lock_path(&git_dir, &branch);
            worktree::remove_lock(&lp)?;
            println!("Unlocked branch '{}'", branch);
        }

        Commands::InstallShell { shell, print } => {
            let shell_name = shell.unwrap_or_else(detect_shell);
            let content = fps_function_content(&shell_name)
                .ok_or_else(|| anyhow::anyhow!("unsupported shell: {}. Supported: fish, zsh, bash", shell_name))?;
            if print {
                println!("{}", content);
            } else {
                let dest = fps_install_path(&shell_name)
                    .ok_or_else(|| anyhow::anyhow!("cannot determine install path for shell: {}", shell_name))?;
                if shell_name == "fish" {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&dest, content)?;
                    println!("Installed fps to {}", dest.display());
                } else {
                    // For zsh/bash append to rc file only if not already present
                    let existing = std::fs::read_to_string(&dest).unwrap_or_default();
                    if existing.contains("fps()") {
                        println!("fps already installed in {}", dest.display());
                    } else {
                        let mut f = std::fs::OpenOptions::new().append(true).create(true).open(&dest)?;
                        use std::io::Write;
                        writeln!(f, "\n{}", content)?;
                        println!("Appended fps to {}", dest.display());
                    }
                }
            }
        }

        Commands::InstallSkills { path } => {
            let skill_path = match path {
                Some(p) => p,
                None => {
                    let home = dirs::home_dir().context("could not determine home directory")?;
                    home.join(".claude").join("skills").join("fp").join("SKILL.md")
                }
            };
            if let Some(parent) = skill_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&skill_path, FP_SKILL)?;
            println!("Installed fp skill to {}", skill_path.display());
        }

        Commands::Merge { pr, squash, rebase, merge_commit } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);

            // Determine merge method: explicit flag, or auto-detect from repo settings
            let explicit_method: Option<&str> = if squash {
                Some("squash")
            } else if rebase {
                Some("rebase")
            } else if merge_commit {
                Some("merge")
            } else {
                None
            };
            let mut state = store.load()?;
            let resolved_method: String;
            let merge_method: &str = if let Some(m) = explicit_method {
                m
            } else {
                resolved_method = github::resolve_merge_method(&client, &owner, &repo_name, &mut state.cached_merge_methods)?;
                store.save(&state)?;
                &resolved_method
            };

            // Fetch the PR's head branch and base before merging (need head_sha for rebase --onto)
            let (head_sha, fetched_base_ref) = client.fetch_pr_head_sha_and_base(&owner, &repo_name, pr)?;

            // Perform the merge
            let merge_sha = client.merge_pr(&owner, &repo_name, pr, Some(merge_method))?;
            println!("✓ merged PR #{} ({})", pr, merge_sha);

            // Find the tracked PR to get the head branch name, then rebase downstream
            if let Some(cached_pr) = state.cache.get(&pr) {
                let merged_branch = cached_pr.branch.clone();
                let merged_base = resolve_merge_base(&fetched_base_ref, &cached_pr.base);
                let main_root = repo_root()?;

                let branch_base_of: std::collections::HashMap<String, String> = state.tracked_prs()
                    .iter()
                    .filter(|p| p.number != pr)
                    .map(|p| (p.branch.clone(), p.base.clone()))
                    .collect();
                let has_downstream = branch_base_of.values().any(|parent| parent == &merged_branch);
                if let Err(e) = check_merge_base(&merged_base, has_downstream) {
                    println!("✗ {}", e);
                } else {
                    let errors = stack::rebase_downstream_stack(
                        &merged_branch, &head_sha, &merged_base, &branch_base_of, &main_root, &|b| {
                            println!("  rebasing {}...", b);
                        }
                    );
                    for e in &errors {
                        println!("✗ {}", e);
                    }
                    if errors.is_empty() {
                        println!("✓ rebased downstream stack onto {}", merged_base);
                    }
                }
            }

            // Untrack the merged PR
            store.untrack(pr)?;
            println!("✓ untracked PR #{}", pr);
        }

        Commands::RebaseStack { pr: rebase_from_pr } => {
            let mut state = store.load()?;
            if state.tracked.is_empty() {
                println!("No tracked PRs.");
                return Ok(());
            }

            let main_root = repo_root()?;

            // Handle merged PRs: rebase their children onto the merge target, then untrack
            if let (Ok(token), Some((owner, repo_name))) = (resolve_github_token(), detect_repo()) {
                let client = GithubClient::new(token);
                let all_branches: Vec<String> = state.tracked_prs().iter().map(|p| p.branch.clone()).collect();
                let parent_of = stack::detect_parent_of(&all_branches, &main_root)?;

                let mut merged_prs: Vec<(u64, String)> = Vec::new();
                for pr in state.tracked_prs() {
                    if client.fetch_pr_is_merged(&owner, &repo_name, pr.number).unwrap_or(false) {
                        let (head_sha, base_ref) = client.fetch_pr_head_sha_and_base(&owner, &repo_name, pr.number)?;
                        // Find children of this branch and rebase them onto base_ref
                        for (branch, parent) in &parent_of {
                            if parent.as_deref() == Some(&pr.branch) {
                                if let Some(warn) = check_branch_lock(&git_dir, branch) {
                                    println!("{}", warn);
                                    continue;
                                }
                                match stack::rebase_onto_after_merge(branch, &head_sha, &base_ref, &main_root) {
                                    Ok(()) => println!("✓ rebased {} onto {} (merged PR #{})", branch, base_ref, pr.number),
                                    Err(e) => println!("✗ failed to rebase {} after merge: {}", branch, e),
                                }
                            }
                        }
                        merged_prs.push((pr.number, pr.branch.clone()));
                    }
                }
                for (number, branch) in merged_prs {
                    untrack_and_cleanup(&store, &repo_root()?, &git_dir, number, &branch)?;
                    println!("✓ untracked merged PR #{}", number);
                }
                state = store.load()?;
            }

            // Rebase remaining open PRs
            let all_branches: Vec<String> = state.tracked_prs().iter().map(|p| p.branch.clone()).collect();
            if all_branches.is_empty() {
                return Ok(());
            }
            let parent_of = stack::detect_parent_of(&all_branches, &main_root)?;

            // If a starting PR is given, restrict to that branch and its descendants
            let branches: Vec<String> = if let Some(from_pr) = rebase_from_pr {
                let start_branch = state.cache.get(&from_pr)
                    .with_context(|| format!("PR #{} is not tracked", from_pr))?
                    .branch.clone();
                subtree_branches(&start_branch, &parent_of, &all_branches)
            } else {
                all_branches
            };

            // Skip branches checked out in main worktree (and locked branches) and their descendants
            let directly_locked: std::collections::HashSet<String> = branches.iter()
                .filter_map(|b| {
                    branch_in_main_worktree_warning(b, &main_root)
                        .or_else(|| check_branch_lock(&git_dir, b))
                        .map(|w| { println!("{}", w); b.clone() })
                })
                .collect();
            let also_blocked = locked_subtree(&directly_locked, &parent_of);
            for b in &also_blocked {
                println!("⚠ skipping {} — parent branch is locked", b);
            }
            let branches: Vec<String> = branches.into_iter()
                .filter(|b| !directly_locked.contains(b) && !also_blocked.contains(b))
                .collect();

            // Build base_of from parallel fetch (reuses PrState.base, no extra API calls)
            let rebase_pr_numbers: Vec<u64> = state.tracked.iter().copied().collect();
            let base_of: std::collections::HashMap<String, String> =
                if let (Ok(token), Some((owner, repo_name))) = (resolve_github_token(), detect_repo()) {
                    GithubClient::new(token).fetch_prs_as_map(&owner, &repo_name, &rebase_pr_numbers)
                        .into_values().map(|p| (p.branch, p.base)).collect()
                } else {
                    std::collections::HashMap::new()
                };

            let result = stack::rebase_stack(&branches, &parent_of, &base_of, &main_root, &|msg| eprintln!("{}", msg))?;

            for branch in &result.rebased {
                println!("✓ rebased {}", branch);
            }
            for branch in &result.conflicts {
                println!("✗ conflict on {} — resolve manually", branch);
                let hint = format_conflict_hint(branch, &state.cache);
                if !hint.is_empty() { println!("{}", hint); }
            }
            if let Some(status) = &result.status_output {
                println!("\ngit status:\n{}", status);
            }
            for warn in &result.invariant_warnings {
                println!("⚠ {}", warn);
            }
            if result.rebased.is_empty() && result.conflicts.is_empty() {
                println!("Stack is already up to date.");
            }
        }

        Commands::Context { pr, hint, full_log } => {
            let state = store.load()?;
            let token = std::env::var("GITHUB_TOKEN").ok();
            let repo = detect_repo();

            let tracked = state.cache.get(&pr)
                .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;

            let pr_state = if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                let client = GithubClient::new(tok);
                client.fetch_pr(&owner, &repo_name, pr).ok()
            } else {
                None
            }.unwrap_or_else(|| model::PrState {
                number: tracked.number,
                title: tracked.title.clone(),
                branch: tracked.branch.clone(),
                base: "".into(), head_sha: "".into(), draft: false, approved: false,
                checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(),
            });

            if let Some(stripped) = hint.strip_prefix("thread:") {
                let thread_id: u64 = stripped.parse().context("invalid thread id")?;
                if let Some(thread) = pr_state.threads.iter().find(|t| t.id == thread_id) {
                    println!("Thread #{} ({:?})", thread.id, thread.state);
                    if let (Some(file), Some(line)) = (&thread.file, thread.line) {
                        println!("  {}:{}", file, line);
                    }
                    println!("  @{}: {}", thread.author, thread.body);
                    for (author, body) in &thread.replies {
                        println!("  > @{}: {}", author, body);
                    }
                } else {
                    println!("Thread #{} not found in PR #{}", thread_id, pr);
                }
            } else {
                if let Some(check) = pr_state.checks.iter().find(|c| c.name == hint) {
                    let status = format!("{:?}", check.status);
                    let output = if let Some(url) = &check.details_url {
                        let provider = ci::parse_ci_provider(url);
                        let token = resolve_github_token().unwrap_or_default();
                        let log_client = ci::CiLogClient::new(token);
                        match log_client.fetch_raw_log(&provider) {
                            Ok(raw) if full_log => {
                                let tmp = std::env::temp_dir().join(format!("fp-log-{}-{}.txt", pr, hint.replace('/', "-")));
                                std::fs::write(&tmp, &raw)?;
                                ci::format_check_output(&check.name, &status, None, Some(&tmp.to_string_lossy()), None)
                            }
                            Ok(raw) => ci::format_check_output(&check.name, &status, Some(&raw), None, None),
                            Err(e) => ci::format_check_output(&check.name, &status, None, None, Some(&e.to_string())),
                        }
                    } else {
                        format!("Check: {} ({})\n  No details URL available\n", check.name, status)
                    };
                    print!("{}", output);
                } else {
                    println!("Check '{}' not found in PR #{}", hint, pr);
                }
            }
        }

        Commands::Checks { sha } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo().context("could not detect GitHub repo")?;
            let client = GithubClient::new(token);
            let checks = client.fetch_checks_for_sha(&owner, &repo_name, &sha)?;
            if checks.is_empty() {
                println!("No check runs found for {}", sha);
            } else {
                for check in &checks {
                    let status = match check.status {
                        model::CheckStatus::Pass => "✓",
                        model::CheckStatus::Fail => "✗",
                        model::CheckStatus::Pending => "⏳",
                    };
                    let url = check.details_url.as_deref().unwrap_or("(no url)");
                    println!("{} {} — {}", status, check.name, url);
                }
            }
        }

        Commands::Threads { pr, resolved, json } => {
            let state = store.load()?;
            let token = resolve_github_token().ok();
            let repo = detect_repo();

            let tracked = state.cache.get(&pr)
                .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;

            if resolved {
                if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                    let client = GithubClient::new(tok);
                    let threads = client.fetch_resolved_threads_graphql(&owner, &repo_name, pr)?;
                    print!("{}", format_resolved_threads(pr, &threads, json));
                } else {
                    println!("No GitHub credentials — cannot fetch resolved threads.");
                }
            } else {
                let pr_state = if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                    let client = GithubClient::new(tok);
                    client.fetch_pr(&owner, &repo_name, pr).ok()
                } else {
                    None
                }.unwrap_or_else(|| model::PrState {
                    number: tracked.number, title: tracked.title.clone(), branch: tracked.branch.clone(),
                    base: "".into(), head_sha: "".into(), draft: false, approved: false, checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(),
                });
                let threads = fetch_open_threads(&pr_state.threads);
                print!("{}", format_open_threads(pr, &threads, json));
            }
        }

        Commands::AgentContext { json } => {
            let state = store.load()?;
            let prs: Vec<_> = state.tracked_prs().into_iter().cloned().collect();
            let manifest = github::agent_context_manifest_with_prs(&prs);
            if json {
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            } else {
                println!("fp agent-context — run with --json for machine-readable output");
                println!("auth: GITHUB_TOKEN or gh auth login");
                println!("commands: ls, status, track, untrack, watch, reply, context, threads, create, rebase-stack");
                println!("tracked PRs: {}", prs.len());
            }
        }

        Commands::Profile { action, name, token, repo } => {
            let profiles_path = dirs_path();
            match action.as_str() {
                "save" => {
                    let tok = token.ok_or_else(|| anyhow::anyhow!("--token required for profile save"))?;
                    let r = repo.ok_or_else(|| anyhow::anyhow!("--repo required for profile save"))?;
                    profile::save_profile(&profiles_path, &name, &tok, &r)?;
                    println!("Profile '{}' saved.", name);
                }
                "load" => {
                    let p = profile::load_profile(&profiles_path, &name)?;
                    println!("export GITHUB_TOKEN={}", p.github_token);
                    println!("# repo: {}", p.repo);
                }
                _ => anyhow::bail!("unknown profile action '{}'; use save or load", action),
            }
        }
    }

    Ok(())
}

fn dirs_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".config")
        .join("fp")
        .join("profiles.json")
}



#[cfg(test)]
mod tests {
    use super::*;

    // ADR-007: resolve_demo_urls with file path and no session env var falls back to browser,
    // and if browser also has no cookie, error mentions GITHUB_USER_SESSION
    #[test]
    fn resolve_demo_urls_file_path_errors_naming_github_user_session_when_no_cookie_found() {
        // SAFETY: single-threaded test, no concurrent env access
        unsafe {
            std::env::remove_var("GITHUB_USER_SESSION");
            // Point Chrome DB to nonexistent path so Keychain is never accessed in tests
            std::env::set_var("CHROME_COOKIES_PATH", "/nonexistent/chrome/Cookies");
        }
        let client = github::GithubClient::with_base_url("tok".into(), "http://localhost:1".into());
        let result = resolve_demo_urls(&client, "owner", "repo", &["some_image.png".to_string()]);
        unsafe { std::env::remove_var("CHROME_COOKIES_PATH"); }
        assert!(result.is_err(), "expected error when no session and no Chrome DB");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("GITHUB_USER_SESSION"),
            "error must mention GITHUB_USER_SESSION, got: {}", msg);
    }

    #[test]
    fn fps_function_fish_contains_root_dispatch() {
        let content = fps_function_content("fish").expect("fish must be supported");
        assert!(content.contains("fp root"), "fps fish function must dispatch 'root' to fp root, got: {}", content);
        assert!(content.contains("fp switch"), "fps fish function must dispatch other args to fp switch, got: {}", content);
    }

    #[test]
    fn fps_function_zsh_contains_root_dispatch() {
        let content = fps_function_content("zsh").expect("zsh must be supported");
        assert!(content.contains("fp root"), "fps zsh function must dispatch 'root' to fp root, got: {}", content);
        assert!(content.contains("fp switch"), "fps zsh function must dispatch other args to fp switch, got: {}", content);
    }

    #[test]
    fn install_shell_fish_path_is_functions_dir() {
        let path = fps_install_path("fish").expect("fish install path must exist");
        assert!(
            path.to_string_lossy().contains(".config/fish/functions/fps.fish"),
            "fish install path must be ~/.config/fish/functions/fps.fish, got: {}",
            path.display()
        );
    }

    #[test]
    fn install_shell_unsupported_returns_none() {
        assert!(fps_function_content("ksh").is_none(), "unsupported shell must return None");
        assert!(fps_install_path("ksh").is_none(), "unsupported shell install path must return None");
    }

    #[test]
    fn check_branch_lock_returns_none_when_no_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        assert!(check_branch_lock(&git_dir, "feat/foo").is_none(),
            "no lock file must return None");
    }

    #[test]
    fn check_branch_lock_returns_alive_warning_for_live_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        let lp = worktree::lock_path(&git_dir, "feat/foo");
        std::fs::create_dir_all(lp.parent().unwrap()).unwrap();
        let live_pid = std::process::id(); // current process is alive
        worktree::write_lock(&lp, live_pid, "agent", "my-session").unwrap();
        let result = check_branch_lock(&git_dir, "feat/foo");
        assert!(result.is_some(), "live lock must return Some");
        let msg = result.unwrap();
        assert!(msg.contains("alive"), "live lock warning must say 'alive', got: {}", msg);
        assert!(msg.contains("feat/foo"), "warning must contain branch name, got: {}", msg);
    }

    #[test]
    fn check_branch_lock_returns_dead_warning_with_unlock_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        let lp = worktree::lock_path(&git_dir, "feat/bar");
        std::fs::create_dir_all(lp.parent().unwrap()).unwrap();
        worktree::write_lock(&lp, 99999999, "agent", "old-session").unwrap(); // dead pid
        let result = check_branch_lock(&git_dir, "feat/bar");
        assert!(result.is_some(), "dead lock must return Some");
        let msg = result.unwrap();
        assert!(msg.contains("fp unlock"), "dead lock warning must mention 'fp unlock', got: {}", msg);
    }

    #[test]
    fn locked_subtree_returns_transitive_descendants() {
        let locked: std::collections::HashSet<String> = ["feat/a".to_string()].into();
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/b".to_string(), Some("feat/a".to_string())),
            ("feat/c".to_string(), Some("feat/b".to_string())),
            ("feat/d".to_string(), Some("main".to_string())),
        ].into();
        let result = locked_subtree(&locked, &parent_of);
        assert!(result.contains("feat/b"), "b is child of locked a");
        assert!(result.contains("feat/c"), "c is grandchild of locked a");
        assert!(!result.contains("feat/d"), "d has unrelated parent");
        assert!(!result.contains("feat/a"), "locked branch itself not in subtree");
    }

    #[test]
    fn locked_subtree_empty_when_no_descendants() {
        let locked: std::collections::HashSet<String> = ["feat/a".to_string()].into();
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/b".to_string(), Some("main".to_string())),
        ].into();
        let result = locked_subtree(&locked, &parent_of);
        assert!(result.is_empty(), "no descendants when nothing chains from locked");
    }

    #[test]
    fn fix_worktree_branch_checks_out_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit","--allow-empty","-m","init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        // detect actual default branch name (main or master depending on git config)
        let default_branch = String::from_utf8(
            std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap().stdout
        ).unwrap().trim().to_string();
        // create and checkout feat/other so we can switch back
        std::process::Command::new("git").args(["checkout","-b","feat/other"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd2 = std::process::Command::new("git");
        cmd2.args(["commit","--allow-empty","-m","other"]).current_dir(tmp.path());
        for (k,v) in &env { cmd2.env(k,v); }
        cmd2.output().unwrap();
        let head_before = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap();
        assert_eq!(String::from_utf8(head_before.stdout).unwrap().trim(), "feat/other");
        fix_worktree_branch(tmp.path(), &default_branch, false).unwrap();
        let head_after = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap();
        assert_eq!(String::from_utf8(head_after.stdout).unwrap().trim(), &default_branch, "should have checked out {}", default_branch);
    }

    #[test]
    fn fix_worktree_branch_with_force_discards_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        // create shared.txt on default branch
        std::fs::write(tmp.path().join("shared.txt"), "original").unwrap();
        std::process::Command::new("git").args(["add","shared.txt"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit","-m","init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let default_branch = String::from_utf8(
            std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap().stdout
        ).unwrap().trim().to_string();
        std::process::Command::new("git").args(["checkout","-b","feat/other"]).current_dir(tmp.path()).output().unwrap();
        // commit a different version of shared.txt on feat/other so branches diverge
        std::fs::write(tmp.path().join("shared.txt"), "other-branch-content").unwrap();
        std::process::Command::new("git").args(["add","shared.txt"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd2 = std::process::Command::new("git");
        cmd2.args(["commit","-m","other"]).current_dir(tmp.path());
        for (k,v) in &env { cmd2.env(k,v); }
        cmd2.output().unwrap();
        // now locally modify shared.txt — git checkout will refuse (would overwrite local change)
        std::fs::write(tmp.path().join("shared.txt"), "local-modification").unwrap();
        // without force, checkout should fail (local modification conflicts)
        let no_force = fix_worktree_branch(tmp.path(), &default_branch, false);
        assert!(no_force.is_err(), "should fail without --force when dirty");
        // with force, should succeed and discard changes
        fix_worktree_branch(tmp.path(), &default_branch, true).unwrap();
        let head = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap();
        assert_eq!(String::from_utf8(head.stdout).unwrap().trim(), &default_branch);
    }

    #[test]
    fn branch_in_main_worktree_warning_contains_adopt_hint() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        std::process::Command::new("git").args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path()).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap();
        let head = std::process::Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"]).current_dir(tmp.path()).output().unwrap();
        let branch = String::from_utf8(head.stdout).unwrap().trim().to_string();
        let warn = branch_in_main_worktree_warning(&branch, tmp.path()).expect("should return Some when branch is checked out");
        assert!(warn.contains("--adopt"), "warning must contain --adopt, got: {}", warn);
        assert!(warn.contains(&branch), "warning must contain branch name, got: {}", warn);
    }

    #[test]
    fn branch_in_main_worktree_warning_returns_none_when_not_checked_out() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        std::process::Command::new("git").args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path()).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap();
        assert!(branch_in_main_worktree_warning("feat/other-branch", tmp.path()).is_none());
    }

    #[test]
    fn main_repo_root_returns_main_root_from_inside_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let main_root = tmp.path().join("myrepo");
        std::fs::create_dir(&main_root).unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init"]).current_dir(&main_root).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit","--allow-empty","-m","init"]).current_dir(&main_root);
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        // create a worktree
        let wt_path = tmp.path().join("myrepo-worktrees/feat/branch");
        std::fs::create_dir_all(&wt_path).unwrap();
        std::process::Command::new("git").args(["checkout","-b","feat/branch"]).current_dir(&main_root).output().unwrap();
        // re-check out main so we can add worktree for feat/branch
        std::process::Command::new("git").args(["checkout","-b","other"]).current_dir(&main_root).output().unwrap();
        let mut cmd2 = std::process::Command::new("git");
        cmd2.args(["commit","--allow-empty","-m","other"]).current_dir(&main_root);
        for (k,v) in &env { cmd2.env(k,v); }
        cmd2.output().unwrap();
        std::process::Command::new("git")
            .args(["worktree","add", wt_path.to_str().unwrap(), "feat/branch"])
            .current_dir(&main_root).output().unwrap();
        // main_repo_root must return main root even when called with a worktree path as cwd
        let result = worktree::main_repo_root(&wt_path).unwrap();
        let expected = main_root.canonicalize().unwrap();
        assert_eq!(result.canonicalize().unwrap(), expected,
            "main_repo_root must return main repo root even when called from inside a worktree");
    }

    fn checked_out_in_main_error_mentions_adopt() {
        use std::path::PathBuf;
        let tmp = tempfile::tempdir().unwrap();
        // init a bare-minimum git repo with a branch checked out
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        std::process::Command::new("git").args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path()).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap();
        // HEAD is main/master — use that branch name
        let head = std::process::Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"]).current_dir(tmp.path()).output().unwrap();
        let branch = String::from_utf8(head.stdout).unwrap().trim().to_string();
        let err = check_not_checked_out_in_main(&branch, tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--adopt"), "error must mention --adopt, got: {}", msg);
    }

    #[test]
    fn format_adopt_message_contains_adopted_and_branch() {
        let msg = format_adopt_message("feat/my-branch");
        assert!(msg.contains("Adopted"), "must contain 'Adopted', got: {}", msg);
        assert!(msg.contains("feat/my-branch"), "must contain branch name, got: {}", msg);
    }

    #[test]
    fn format_new_worktree_output_contains_path_and_fps_hint() {
        let path = std::path::Path::new("/projects/vivaa-worktrees/feature/foo");
        let out = format_new_worktree_output(path, "feature/foo");
        assert!(out.contains("Created worktree at"), "must contain 'Created worktree at', got: {}", out);
        assert!(out.contains("/projects/vivaa-worktrees/feature/foo"), "must contain path, got: {}", out);
        assert!(out.contains("fps feature/foo"), "must contain fps hint, got: {}", out);
    }

    fn make_pr(number: u64, branch: &str, base: &str) -> store::PrCache {
        store::PrCache { number, title: format!("PR {}", number), branch: branch.into(), base: base.into() }
    }

    #[test]
    fn format_pr_status_all_entry_shows_tasks_inline() {
        let tasks = vec![
            tasks::Task { pr: 7, task_type: tasks::TaskType::FixCi, description: "Fix ci/test".into(), blocking: true, context_hint: "".into() },
            tasks::Task { pr: 7, task_type: tasks::TaskType::AwaitingReview, description: "Waiting for review".into(), blocking: false, context_hint: "".into() },
        ];
        let out = format_pr_status_all_entry("", 7, "My PR", &tasks, "");
        assert!(out.contains("PR #7"), "must contain PR number, got: {}", out);
        assert!(out.contains("[blocking]"), "must show blocking flag, got: {}", out);
        assert!(out.contains("Fix ci/test"), "must show task description, got: {}", out);
        assert!(out.contains("[waiting]"), "must show waiting flag, got: {}", out);
        assert!(out.contains("Waiting for review"), "must show waiting task, got: {}", out);
    }

    #[test]
    fn format_pr_status_all_entry_shows_ready_when_no_tasks() {
        let out = format_pr_status_all_entry("", 3, "Clean PR", &[], "");
        assert!(out.contains("ready"), "must say ready when no tasks, got: {}", out);
        assert!(!out.contains("[blocking]"), "must not show blocking when no tasks, got: {}", out);
    }

    #[test]
    fn format_pr_status_all_entry_respects_prefix() {
        let tasks = vec![tasks::Task { pr: 4, task_type: tasks::TaskType::FixCi, description: "fix".into(), blocking: true, context_hint: "".into() }];
        let out = format_pr_status_all_entry("  └─ ", 4, "Child PR", &tasks, "");
        assert!(out.starts_with("  └─ "), "must start with prefix, got: {}", out);
    }

    #[test]
    fn format_pr_status_all_entry_task_lines_do_not_start_with_tree_connector() {
        let tasks = vec![tasks::Task { pr: 4, task_type: tasks::TaskType::FixCi, description: "fix".into(), blocking: true, context_hint: "".into() }];
        let out = format_pr_status_all_entry("  └─ ", 4, "Child PR", &tasks, "");
        let task_lines: Vec<&str> = out.lines().skip(1).collect();
        assert!(!task_lines.is_empty(), "must have task lines");
        for line in &task_lines {
            assert!(!line.contains("└─"), "task lines must not contain tree connector, got: {}", line);
        }
    }

    fn stack_tree_order_root_has_no_indent() {
        let root = make_pr(1, "feature-a", "main");
        let prs = vec![&root];
        let result = stack_tree_order(&prs);
        assert_eq!(result.len(), 1);
        assert!(!result[0].1.contains("└─"), "root PR must have no └─ indent, got: {:?}", result[0].1);
    }

    #[test]
    fn stack_tree_order_child_has_indent_prefix() {
        let root = make_pr(1, "feature-a", "main");
        let child = make_pr(2, "feature-b", "feature-a");
        let prs = vec![&root, &child];
        let result = stack_tree_order(&prs);
        assert_eq!(result.len(), 2);
        let child_entry = result.iter().find(|(n, _)| *n == 2).unwrap();
        assert!(child_entry.1.contains("└─"), "child PR must contain └─, got: {:?}", child_entry.1);
    }

    #[test]
    fn stack_tree_order_child_follows_parent() {
        let root = make_pr(1, "feature-a", "main");
        let child = make_pr(2, "feature-b", "feature-a");
        let prs = vec![&child, &root]; // child listed first in input
        let result = stack_tree_order(&prs);
        let root_pos = result.iter().position(|(n, _)| *n == 1).unwrap();
        let child_pos = result.iter().position(|(n, _)| *n == 2).unwrap();
        assert!(root_pos < child_pos, "root must appear before its child in output");
    }

    #[test]
    fn check_merge_base_errors_with_no_base_message_when_empty_and_has_downstream() {
        let result = check_merge_base("", true);
        assert!(result.is_err(), "check_merge_base must error when base empty and downstream exists");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("could not determine merge base"),
            "error must contain 'could not determine merge base', got: {}", msg);
    }

    #[test]
    fn check_merge_base_ok_when_empty_and_no_downstream() {
        assert!(check_merge_base("", false).is_ok(),
            "check_merge_base must succeed when base empty but no downstream PRs");
    }

    #[test]
    fn check_merge_base_ok_when_base_non_empty() {
        assert!(check_merge_base("main", true).is_ok(),
            "check_merge_base must succeed when base is non-empty");
    }

    #[test]
    fn resolve_merge_base_prefers_fetched_when_both_non_empty() {
        assert_eq!(resolve_merge_base("main", "old-base"), "main",
            "resolve_merge_base must return fetched when both are non-empty");
    }

    #[test]
    fn resolve_merge_base_uses_fetched_when_stored_empty() {
        assert_eq!(resolve_merge_base("main", ""), "main",
            "resolve_merge_base must return fetched when stored is empty");
    }

    #[test]
    fn resolve_merge_base_falls_back_to_stored_when_fetched_empty() {
        assert_eq!(resolve_merge_base("", "stored-base"), "stored-base",
            "resolve_merge_base must fall back to stored when fetched is empty");
    }

    #[test]
    fn repo_header_contains_owner_slash_repo() {
        let h = repo_header("alice", "myproject");
        assert!(h.contains("alice/myproject"), "repo_header must contain owner/repo, got: {}", h);
    }

    #[test]
    fn require_repo_errors_with_no_github_remote_message_when_none() {
        let result = require_repo(None);
        assert!(result.is_err(), "require_repo(None) must return Err");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no GitHub remote"), "error must contain 'no GitHub remote', got: {}", msg);
    }

    #[test]
    fn require_repo_returns_owner_and_repo_when_some() {
        let result = require_repo(Some(("alice".into(), "proj".into())));
        assert!(result.is_ok(), "require_repo(Some) must succeed");
        let (owner, repo) = result.unwrap();
        assert_eq!(owner, "alice");
        assert_eq!(repo, "proj");
    }

    #[test]
    fn untrack_and_cleanup_removes_lock_for_merged_pr() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store_dir = git_dir.join("fp_store");
        std::fs::create_dir_all(&store_dir).unwrap();
        let store = Store::open(&git_dir);

        let branch = "feature/my-branch";
        store.track(42).unwrap();
        store.update_cache(store::PrCache { number: 42, title: "test".into(), branch: branch.into(), base: "main".into() }).unwrap();
        assert!(store.load().unwrap().tracked.contains(&42), "PR must be tracked before cleanup");

        // Create a fake lock file
        let lp = worktree::lock_path(&git_dir, branch);
        if let Some(parent) = lp.parent() { std::fs::create_dir_all(parent).unwrap(); }
        std::fs::write(&lp, b"fake lock").unwrap();
        assert!(lp.exists(), "lock file must exist before cleanup");

        untrack_and_cleanup(&store, tmp.path(), &git_dir, 42, branch).unwrap();

        assert!(!store.load().unwrap().tracked.contains(&42), "PR must be untracked after cleanup");
        assert!(!lp.exists(), "lock file must be removed by untrack_and_cleanup");
    }

}
