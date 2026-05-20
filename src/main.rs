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
pub mod merge;
pub mod upload;
pub mod platform;
pub mod commands;

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
#[cfg(test)]
mod threads_test;
#[cfg(test)]
mod repo_test;
#[cfg(test)]
mod merge_test;
#[cfg(test)]
mod lifecycle_test;
#[cfg(test)]
mod upload_test;
#[cfg(test)]
mod notify_ext_test;
#[cfg(test)]
mod commands_test;
#[cfg(test)]
mod profile_test;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use github::{GithubClient, detect_repo, resolve_github_token, resolve_track_branch};
use store::{Store, PrCache};
use tasks::{generate_tasks, task_diff};


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
        /// Print debug information about parent detection and rebase decisions
        #[arg(long)]
        verbose: bool,
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

pub use display::watch_notification_messages;


pub use shell::{fps_function_content, fps_install_path, detect_shell};

pub use display::{format_watch_initial_state, format_pr_status_all_entry, format_watch_event_json};

pub use worktree::{branch_in_main_worktree_warning, check_not_checked_out_in_main};

pub use merge::{check_merge_base, resolve_merge_base};

pub use stack::stack_tree_order;

pub use worktree::{check_branch_lock};

pub use worktree::{locked_subtree, subtree_branches, worktree_branch_mismatch, fix_worktree_branch};

pub use display::{format_adopt_message, format_new_worktree_output, repo_header, format_single_pr_status, format_worktree_add_error, format_conflict_hint};

use platform::notify_macos_titled;

use worktree::{git_dir, repo_root, untrack_and_cleanup, require_repo};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let git_dir = git_dir()?;
    let store = Store::open(&git_dir);

    match cli.command {
        Commands::Ls { json } => {
            let (owner, repo_name) = require_repo(detect_repo())?;
            print!("{}", commands::cmd_ls(&store, &owner, &repo_name, json)?);
        }

        Commands::Status { pr, json, all } => {
            let token = std::env::var("GITHUB_TOKEN").ok();
            let (owner, repo_name) = require_repo(detect_repo())?;
            let client = token.as_ref().map(|t| GithubClient::new(t.clone()));
            let client_ref: Option<&dyn github::GithubClientTrait> = client.as_ref().map(|c| c as &dyn github::GithubClientTrait);
            if all {
                print!("{}", commands::cmd_status_all(client_ref, &store, &git_dir, &owner, &repo_name, json)?);
            } else {
                let number = pr.context("specify a PR number or use --all")?;
                println!("{}", commands::cmd_status_one(client_ref, &store, &git_dir, &owner, &repo_name, number, json)?);
            }
        }

        Commands::Track { pr, title, branch } => {
            let (resolved_title, fetched_branch, base) = if let (Some(tok), Some((owner, repo_name))) = (resolve_github_token().ok(), detect_repo()) {
                let client = GithubClient::new(tok);
                commands::cmd_track(&client, &owner, &repo_name, pr, title.clone(), branch.clone())?
            } else {
                (title.clone().unwrap_or_else(|| format!("PR #{}", pr)), String::new(), String::new())
            };
            let resolved_branch = resolve_track_branch(branch, Some(fetched_branch).filter(|b| !b.is_empty()), pr)?;
            store.track(pr)?;
            store.update_cache(PrCache { number: pr, title: resolved_title.clone(), branch: resolved_branch, base })?;
            println!("Tracking PR #{} — {}", pr, resolved_title);
        }

        Commands::Untrack { pr } => {
            println!("{}", commands::cmd_untrack(&store, &repo_root()?, &git_dir, pr)?);
        }

        Commands::Switch { pr, id, force, adopt } => {
            let wt_path = commands::cmd_switch(&store, &git_dir, pr, &id, force, adopt)?;
            println!("{}", wt_path.display());
        }

        Commands::Reply { pr, thread_id, message } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            println!("{}", commands::cmd_reply(&client, &owner, &repo_name, pr, thread_id, &message)?);
        }

        Commands::Ready { pr } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            println!("{}", commands::cmd_ready(&client, &owner, &repo_name, pr)?);
        }

        Commands::Comment { pr, message } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            println!("{}", commands::cmd_comment(&client, &owner, &repo_name, pr, &message)?);
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
                let base_body = match body {
                    Some(ref b) => b.clone(),
                    None => client.fetch_pr_body(&owner, &repo_name, pr)?,
                };
                Some(github::inject_demo_section(&base_body, &demo_urls))
            };
            println!("{}", commands::cmd_edit(&client, &owner, &repo_name, pr, title, final_body, vec![])?);
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
            let out = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .context("failed to run git")?;
            let head_branch = String::from_utf8(out.stdout)?.trim().to_string();
            let client = GithubClient::new(token);
            let final_body = if demo.is_empty() {
                body
            } else {
                let demo_urls = resolve_demo_urls(&client, &owner, &repo_name, &demo)?;
                Some(github::inject_demo_section(body.as_deref().unwrap_or(""), &demo_urls))
            };
            println!("{}", commands::cmd_create(&client, &owner, &repo_name, &store, &head_branch, &repo_root()?, commands::CreateOpts { title, base, body: final_body, restack_before, insert_after })?);
        }

        Commands::New { branch, base } => {
            print!("{}", commands::cmd_new(&branch, &base, &repo_root()?)?);
        }

        Commands::Root => {
            println!("{}", repo_root()?.display());
        }

        Commands::Unlock { branch } => {
            let lp = worktree::lock_path(&git_dir, &branch);
            worktree::remove_lock(&lp)?;
            println!("{}", commands::unlock_message(&branch));
        }

        Commands::InstallShell { shell, print } => {
            let shell_name = shell.unwrap_or_else(detect_shell);
            let content = commands::install_shell_content(&shell_name)?;
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
            commands::install_skills(&skill_path)?;
            println!("Installed fp skill to {}", skill_path.display());
        }

        Commands::Merge { pr, squash, rebase, merge_commit } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            let explicit_method: Option<&str> = if squash { Some("squash") } else if rebase { Some("rebase") } else if merge_commit { Some("merge") } else { None };
            let mut state = store.load()?;
            let resolved_method: String;
            let merge_method: &str = if let Some(m) = explicit_method {
                m
            } else {
                resolved_method = github::resolve_merge_method(&client, &owner, &repo_name, &mut state.cached_merge_methods)?;
                store.save(&state)?;
                &resolved_method
            };
            print!("{}", commands::cmd_merge(&client, &owner, &repo_name, pr, commands::MergeContext { store: &store, dir: &repo_root()?, git_dir: &git_dir, merge_method })?);
        }

        Commands::RebaseStack { pr: rebase_from_pr, verbose } => {
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
                let cached_base_of: std::collections::HashMap<String, String> = state.tracked_prs().iter().map(|p| (p.branch.clone(), p.base.clone())).collect();
                let parent_of = stack::detect_parent_of(&all_branches, &main_root, &cached_base_of, &|_| {})?;

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
            let tracked_set: std::collections::HashSet<String> = all_branches.iter().cloned().collect();
            let cached_base_of = commands::normalize_base_of(
                state.tracked_prs().iter().map(|p| (p.branch.clone(), p.base.clone())).collect(),
                &tracked_set,
            );
            let debug_fn: Box<dyn Fn(&str)> = if verbose {
                Box::new(|s: &str| eprintln!("[fp verbose] {}", s))
            } else {
                Box::new(|_: &str| {})
            };
            let parent_of = stack::detect_parent_of(&all_branches, &main_root, &cached_base_of, debug_fn.as_ref())?;

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
            // Normalize: if a PR's declared base is not a tracked branch, use "main" instead.
            let rebase_pr_numbers: Vec<u64> = state.tracked.iter().copied().collect();
            let base_of: std::collections::HashMap<String, String> =
                if let (Ok(token), Some((owner, repo_name))) = (resolve_github_token(), detect_repo()) {
                    commands::normalize_base_of(
                        GithubClient::new(token).fetch_prs_as_map(&owner, &repo_name, &rebase_pr_numbers)
                            .into_values().map(|p| (p.branch, p.base)).collect(),
                        &tracked_set,
                    )
                } else {
                    std::collections::HashMap::new()
                };

            let result = stack::rebase_stack(&branches, &parent_of, &base_of, &main_root, &|msg| eprintln!("{}", msg), debug_fn.as_ref())?;

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
            state.cache.get(&pr)
                .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;
            if let (Some(tok), Some((owner, repo_name))) = (std::env::var("GITHUB_TOKEN").ok(), detect_repo()) {
                let client = GithubClient::new(tok);
                // full_log CI path: fetch raw log and write to tmp file before displaying
                if full_log && !hint.starts_with("thread:") {
                    let pr_state = client.fetch_pr(&owner, &repo_name, pr)?;
                    if let Some(check) = pr_state.checks.iter().find(|c| c.name == hint) {
                        let status = format!("{:?}", check.status);
                        let output = if let Some(url) = &check.details_url {
                            let provider = ci::parse_ci_provider(url);
                            let log_token = resolve_github_token().unwrap_or_default();
                            let log_client = ci::CiLogClient::new(log_token);
                            match log_client.fetch_raw_log(&provider) {
                                Ok(raw) => {
                                    let tmp = std::env::temp_dir().join(format!("fp-log-{}-{}.txt", pr, hint.replace('/', "-")));
                                    std::fs::write(&tmp, &raw)?;
                                    ci::format_check_output(&check.name, &status, None, Some(&tmp.to_string_lossy()), None)
                                }
                                Err(e) => ci::format_check_output(&check.name, &status, None, None, Some(&e.to_string())),
                            }
                        } else {
                            format!("Check: {} ({})\n  No details URL available\n", check.name, status)
                        };
                        print!("{}", output);
                    } else {
                        println!("Check '{}' not found in PR #{}", hint, pr);
                    }
                } else {
                    print!("{}", commands::cmd_context(&client, &owner, &repo_name, pr, &hint, false)?);
                }
            } else {
                println!("No GITHUB_TOKEN — checks and threads unavailable for PR #{}", pr);
            }
        }

        Commands::Checks { sha } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo().context("could not detect GitHub repo")?;
            let client = GithubClient::new(token);
            print!("{}", commands::cmd_checks(&client, &owner, &repo_name, &sha)?);
        }

        Commands::Threads { pr, resolved, json } => {
            let token = resolve_github_token().ok();
            let (owner, repo_name) = detect_repo().unwrap_or_default();
            let client = token.as_ref().map(|t| GithubClient::new(t.clone()));
            let client_ref: Option<&dyn github::GithubClientTrait> = client.as_ref().map(|c| c as &dyn github::GithubClientTrait);
            print!("{}", commands::cmd_threads(client_ref, &store, &owner, &repo_name, pr, resolved, json)?);
        }

        Commands::AgentContext { json } => {
            let state = store.load()?;
            let prs: Vec<_> = state.tracked_prs().into_iter().cloned().collect();
            let manifest = github::agent_context_manifest_with_prs(&prs);
            if json {
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            } else {
                println!("{}", commands::agent_context_text(prs.len()));
            }
        }

        Commands::Profile { action, name, token, repo } => {
            println!("{}", commands::cmd_profile(&profile::profiles_path(), &action, &name, token, repo)?);
        }
    }

    Ok(())
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

}
