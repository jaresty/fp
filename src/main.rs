mod model;
mod tasks;
mod store;
mod github;
mod ci;
mod stack;

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

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use github::{GithubClient, detect_repo, resolve_github_token, resolve_track_branch, fetch_resolved_threads, fetch_open_threads, agent_context_manifest};
use store::{Store, TrackedPr};
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
    /// Create a draft PR for the current branch and start tracking it
    Create {
        /// PR title
        title: String,
        /// Base branch (default: main)
        #[arg(long, default_value = "main")]
        base: String,
    },
    /// Rebase all tracked PRs in stack order onto their parent branches
    RebaseStack,
    /// Install the fp Claude Code skill into ~/.claude/skills/fp/SKILL.md
    InstallSkills {
        /// Alternative output path (overrides default ~/.claude/skills/fp/SKILL.md)
        #[arg(long)]
        path: Option<std::path::PathBuf>,
    },
}

/// Send a macOS system notification via osascript. Silently ignores errors on non-macOS or
/// headless environments where notifications are unavailable.
/// Falsify exemption: fire-and-forget subprocess with no observable return value in the test
/// process — no artifact type can detect absence of the osascript call without a subprocess harness.
fn notify_macos(message: &str) {
    let script = format!(
        r#"display notification "{}" with title "fp""#,
        message.replace('"', "'")
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
}

fn git_dir() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        anyhow::bail!("not in a git repository");
    }
    let path = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(path))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let git_dir = git_dir()?;
    let store = Store::open(&git_dir);

    match cli.command {
        Commands::Ls { json } => {
            let state = store.load()?;
            if json {
                let items: Vec<_> = state.prs.values().collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                if state.prs.is_empty() {
                    println!("No tracked PRs. Use `fp track <pr>` to add one.");
                } else {
                    let mut prs: Vec<_> = state.prs.values().collect();
                    prs.sort_by_key(|p| p.number);
                    for pr in prs {
                        println!("#{} {} ({})", pr.number, pr.title, pr.branch);
                    }
                }
            }
        }

        Commands::Status { pr, json, all } => {
            let state = store.load()?;
            let token = std::env::var("GITHUB_TOKEN").ok();
            let repo = detect_repo();

            let fetch = |number: u64, _branch: &str| -> Option<crate::model::PrState> {
                if let (Some(tok), Some((owner, repo_name))) = (&token, &repo) {
                    let client = GithubClient::new(tok.clone());
                    client.fetch_pr(owner, repo_name, number).ok()
                } else {
                    None
                }
            };

            if all {
                let mut prs: Vec<_> = state.prs.values().collect();
                prs.sort_by_key(|p| p.number);
                for tracked in prs {
                    let pr_state = fetch(tracked.number, &tracked.branch)
                        .unwrap_or_else(|| crate::model::PrState {
                            number: tracked.number,
                            title: tracked.title.clone(),
                            branch: tracked.branch.clone(),
                            draft: false, approved: false,
                            checks: vec![], threads: vec![],
                        });
                    let tasks = generate_tasks(&pr_state);
                    if json {
                        println!("{}", serde_json::to_string_pretty(&tasks).unwrap());
                    } else if tasks.is_empty() {
                        println!("PR #{} {} — ready", tracked.number, tracked.title);
                    } else {
                        println!("PR #{} {} — {} task(s)", tracked.number, tracked.title, tasks.len());
                    }
                }
            } else {
                let number = pr.context("specify a PR number or use --all")?;

                let tracked = state.prs.get(&number)
                    .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", number, number))?;

                let pr_state = fetch(tracked.number, &tracked.branch)
                    .unwrap_or_else(|| crate::model::PrState {
                        number: tracked.number,
                        title: tracked.title.clone(),
                        branch: tracked.branch.clone(),
                        draft: false, approved: false,
                        checks: vec![], threads: vec![],
                    });
                let task_list = generate_tasks(&pr_state);

                if json {
                    println!("{}", serde_json::to_string_pretty(&task_list)?);
                } else {
                    if task_list.is_empty() {
                        println!("PR #{} is ready.", number);
                    } else {
                        println!("PR #{} — {} task(s):", number, task_list.len());
                        for t in &task_list {
                            let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
                            println!("  {} {:?}: {}", flag, t.task_type, t.description);
                        }
                    }
                }
            }
        }

        Commands::Track { pr, title, branch } => {
            let (title, resolved_branch) = {
                let token = resolve_github_token().ok();
                let repo = detect_repo();
                let (fetched_title, fetched_branch) = if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                    let client = GithubClient::new(tok);
                    client.fetch_pr_metadata(&owner, &repo_name, pr).ok()
                        .map(|(t, b)| (Some(t), Some(b)))
                        .unwrap_or((None, None))
                } else {
                    (None, None)
                };
                let resolved_title = title.or(fetched_title).unwrap_or_else(|| format!("PR #{}", pr));
                let resolved_branch = resolve_track_branch(branch, fetched_branch, pr)?;
                (resolved_title, resolved_branch)
            };
            store.track(TrackedPr { number: pr, title: title.clone(), branch: resolved_branch })?;
            println!("Tracking PR #{} — {}", pr, title);
        }

        Commands::Untrack { pr } => {
            store.untrack(pr)?;
            println!("Untracked PR #{}", pr);
        }

        Commands::Reply { pr, thread_id, message } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            let posted = client.reply_to_comment(&owner, &repo_name, pr, thread_id, &message)?;
            println!("Replied to thread #{}: {}", thread_id, posted);
        }

        Commands::Comment { pr, message } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            let url = client.post_pr_comment(&owner, &repo_name, pr, &message)?;
            println!("Comment posted: {}", url);
        }

        Commands::Watch { once, interval } => {
            let mut prev_tasks: std::collections::HashMap<u64, Vec<tasks::Task>> = std::collections::HashMap::new();
            loop {
                let state = store.load()?;
                let token = std::env::var("GITHUB_TOKEN").ok();
                let repo = detect_repo();
                let mut prs: Vec<_> = state.prs.values().collect();
                prs.sort_by_key(|p| p.number);

                for tracked in &prs {
                    let pr_state = if let (Some(tok), Some((owner, repo_name))) = (&token, &repo) {
                        let client = GithubClient::new(tok.clone());
                        client.fetch_pr(owner, repo_name, tracked.number).ok()
                    } else {
                        None
                    }.unwrap_or_else(|| model::PrState {
                        number: tracked.number,
                        title: tracked.title.clone(),
                        branch: tracked.branch.clone(),
                        draft: false, approved: false,
                        checks: vec![], threads: vec![],
                    });
                    let curr = generate_tasks(&pr_state);

                    let prev = prev_tasks.get(&tracked.number).map(|v| v.as_slice()).unwrap_or(&[]);
                    let (new, resolved) = task_diff(prev, &curr);

                    if prev_tasks.contains_key(&tracked.number) {
                        for t in &new {
                            let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
                            println!("+ PR #{} {} {:?}: {}", tracked.number, flag, t.task_type, t.description);
                            notify_macos(&format!("PR #{}: {}", tracked.number, t.description));
                        }
                        for t in &resolved {
                            println!("✓ PR #{} resolved {:?}: {}", tracked.number, t.task_type, t.description);
                        }
                    } else {
                        if curr.is_empty() {
                            println!("PR #{} {} — ready", tracked.number, tracked.title);
                        } else {
                            println!("PR #{} {} — {} task(s)", tracked.number, tracked.title, curr.len());
                            for t in &curr {
                                let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
                                println!("  {} {:?}: {}", flag, t.task_type, t.description);
                            }
                        }
                    }
                    prev_tasks.insert(tracked.number, curr);
                }

                if once { break; }
                std::thread::sleep(std::time::Duration::from_secs(interval));
            }
        }

        Commands::Create { title, base } => {
            let token = resolve_github_token()?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;

            // Get current branch
            let out = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .context("failed to run git")?;
            let head_branch = String::from_utf8(out.stdout)?.trim().to_string();

            let client = GithubClient::new(token);
            let pr_state = client.create_pr(&owner, &repo_name, &title, &head_branch, &base, true)?;
            store.track(TrackedPr {
                number: pr_state.number,
                title: pr_state.title.clone(),
                branch: pr_state.branch.clone(),
            })?;
            println!("Created PR #{}: {} ({})", pr_state.number, pr_state.title, pr_state.branch);
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

        Commands::RebaseStack => {
            let state = store.load()?;
            if state.prs.is_empty() {
                println!("No tracked PRs.");
                return Ok(());
            }

            // Collect tracked branches
            let branches: Vec<String> = state.prs.values().map(|p| p.branch.clone()).collect();

            // Detect stack topology from git
            let work_dir = stack::resolve_work_dir(&git_dir)?;

            let parent_of = stack::detect_parent_of(&branches, &work_dir)?;
            let result = stack::rebase_stack(&branches, &parent_of, &work_dir)?;

            for branch in &result.rebased {
                println!("✓ rebased {}", branch);
            }
            for branch in &result.conflicts {
                println!("✗ conflict on {} — resolve manually", branch);
            }
            if result.rebased.is_empty() && result.conflicts.is_empty() {
                println!("Stack is already up to date.");
            }
        }

        Commands::Context { pr, hint, full_log } => {
            let state = store.load()?;
            let token = std::env::var("GITHUB_TOKEN").ok();
            let repo = detect_repo();

            let tracked = state.prs.get(&pr)
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
                draft: false, approved: false,
                checks: vec![], threads: vec![],
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
                    println!("Check: {} ({:?})", check.name, check.status);
                    if let Some(url) = &check.details_url {
                        let provider = ci::parse_ci_provider(url);
                        let token = resolve_github_token().unwrap_or_default();
                        let log_client = ci::CiLogClient::new(token);
                        if full_log {
                            // Write the full raw log to a temp file
                            match log_client.fetch_raw_log(&provider) {
                                Ok(raw) => {
                                    let tmp = std::env::temp_dir().join(format!("fp-log-{}-{}.txt", pr, hint.replace('/', "-")));
                                    std::fs::write(&tmp, &raw)?;
                                    println!("full_log_path: {}", tmp.display());
                                }
                                Err(e) => println!("  Log URL: {}\n  (fetch failed: {})", url, e),
                            }
                        } else {
                            match log_client.fetch_logs(&provider) {
                                Ok(logs) => println!("{}", logs),
                                Err(e) => println!("  Log URL: {}\n  (fetch failed: {})", url, e),
                            }
                        }
                    } else {
                        println!("  No details URL available");
                    }
                } else {
                    println!("Check '{}' not found in PR #{}", hint, pr);
                }
            }
        }

        Commands::Threads { pr, resolved, json } => {
            let state = store.load()?;
            let token = resolve_github_token().ok();
            let repo = detect_repo();

            let tracked = state.prs.get(&pr)
                .with_context(|| format!("PR #{} is not tracked. Run `fp track {}`", pr, pr))?;

            let pr_state = if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                let client = GithubClient::new(tok);
                client.fetch_pr(&owner, &repo_name, pr).ok()
            } else {
                None
            }.unwrap_or_else(|| model::PrState {
                number: tracked.number, title: tracked.title.clone(), branch: tracked.branch.clone(),
                draft: false, approved: false, checks: vec![], threads: vec![],
            });

            let threads: Vec<&model::Thread> = if resolved {
                fetch_resolved_threads(&pr_state.threads)
            } else {
                fetch_open_threads(&pr_state.threads)
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&threads)?);
            } else {
                let label = if resolved { "resolved" } else { "open" };
                if threads.is_empty() {
                    println!("No {} threads on PR #{}.", label, pr);
                } else {
                    println!("PR #{} — {} {} thread(s):", pr, threads.len(), label);
                    for t in threads {
                        print!("  #{} ({:?})", t.id, t.state);
                        if let (Some(f), Some(l)) = (&t.file, t.line) { print!(" {}:{}", f, l); }
                        println!("\n    {}", t.body);
                    }
                }
            }
        }

        Commands::AgentContext { json } => {
            let manifest = agent_context_manifest();
            if json {
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            } else {
                println!("fp agent-context — run with --json for machine-readable output");
                println!("auth: GITHUB_TOKEN or gh auth login");
                println!("commands: ls, status, track, untrack, watch, reply, context, threads, create, rebase-stack");
            }
        }
    }

    Ok(())
}


