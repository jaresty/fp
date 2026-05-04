mod model;
mod tasks;
mod store;
mod github;
mod ci;

#[cfg(test)]
mod tasks_test;
#[cfg(test)]
mod store_test;
#[cfg(test)]
mod github_test;
#[cfg(test)]
mod ci_test;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use github::{GithubClient, detect_repo};
use store::{State, Store, TrackedPr};
use tasks::{generate_tasks, task_diff};

fn apply_thread_states(mut pr_state: model::PrState, store_state: &State) -> model::PrState {
    for thread in &mut pr_state.threads {
        let key = format!("{}:{}", pr_state.number, thread.id);
        if let Some(&stored) = store_state.thread_states.get(&key) {
            thread.state = stored;
        }
    }
    pr_state
}

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
    /// Reply to a PR review thread and mark it as addressed
    Reply {
        /// PR number
        pr: u64,
        /// Thread (comment) ID
        thread_id: u64,
        /// Reply message body
        message: String,
    },
    /// Mark a PR review thread as resolved (local state only)
    Resolve {
        /// PR number
        pr: u64,
        /// Thread (comment) ID
        thread_id: u64,
    },
    /// Show full context for a task (check logs URL, thread body, etc.)
    Context {
        /// PR number
        pr: u64,
        /// Context hint from task output (check name or thread:<id>)
        hint: String,
    },
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

            let fetch = |number: u64, branch: &str| -> Option<crate::model::PrState> {
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
                    let pr_state = apply_thread_states(fetch(tracked.number, &tracked.branch)
                        .unwrap_or_else(|| crate::model::PrState {
                            number: tracked.number,
                            title: tracked.title.clone(),
                            branch: tracked.branch.clone(),
                            draft: false, approved: false,
                            checks: vec![], threads: vec![],
                        }), &state);
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

                let pr_state = apply_thread_states(fetch(tracked.number, &tracked.branch)
                    .unwrap_or_else(|| crate::model::PrState {
                        number: tracked.number,
                        title: tracked.title.clone(),
                        branch: tracked.branch.clone(),
                        draft: false, approved: false,
                        checks: vec![], threads: vec![],
                    }), &state);
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
            let (title, branch) = match (title, branch) {
                (Some(t), Some(b)) => (t, b),
                (t_opt, b_opt) => {
                    let token = std::env::var("GITHUB_TOKEN").ok();
                    let repo = detect_repo();
                    if let (Some(tok), Some((owner, repo_name))) = (token, repo) {
                        let client = GithubClient::new(tok);
                        match client.fetch_pr_metadata(&owner, &repo_name, pr) {
                            Ok((fetched_title, fetched_branch)) => (
                                t_opt.unwrap_or(fetched_title),
                                b_opt.unwrap_or(fetched_branch),
                            ),
                            Err(_) => (
                                t_opt.unwrap_or_else(|| format!("PR #{}", pr)),
                                b_opt.unwrap_or_default(),
                            ),
                        }
                    } else {
                        (
                            t_opt.unwrap_or_else(|| format!("PR #{}", pr)),
                            b_opt.unwrap_or_default(),
                        )
                    }
                }
            };
            store.track(TrackedPr { number: pr, title: title.clone(), branch })?;
            println!("Tracking PR #{} — {}", pr, title);
        }

        Commands::Untrack { pr } => {
            store.untrack(pr)?;
            println!("Untracked PR #{}", pr);
        }

        Commands::Reply { pr, thread_id, message } => {
            let token = std::env::var("GITHUB_TOKEN")
                .context("GITHUB_TOKEN not set")?;
            let (owner, repo_name) = detect_repo()
                .context("could not detect GitHub repo from git remote")?;
            let client = GithubClient::new(token);
            let posted = client.reply_to_comment(&owner, &repo_name, thread_id, &message)?;
            store.set_thread_state(pr, thread_id, model::ThreadState::Addressed)?;
            println!("Replied to thread #{}: {}", thread_id, posted);
        }

        Commands::Resolve { pr, thread_id } => {
            store.set_thread_state(pr, thread_id, model::ThreadState::Resolved)?;
            println!("Thread #{} marked as resolved (local state)", thread_id);
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
                    let pr_state = apply_thread_states(pr_state, &state);
                    let curr = generate_tasks(&pr_state);

                    let prev = prev_tasks.get(&tracked.number).map(|v| v.as_slice()).unwrap_or(&[]);
                    let (new, resolved) = task_diff(prev, &curr);

                    if prev_tasks.contains_key(&tracked.number) {
                        for t in &new {
                            let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
                            println!("+ PR #{} {} {:?}: {}", tracked.number, flag, t.task_type, t.description);
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

        Commands::Context { pr, hint } => {
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
                    println!("  {}", thread.body);
                } else {
                    println!("Thread #{} not found in PR #{}", thread_id, pr);
                }
            } else {
                if let Some(check) = pr_state.checks.iter().find(|c| c.name == hint) {
                    println!("Check: {} ({:?})", check.name, check.status);
                    if let Some(url) = &check.details_url {
                        let provider = ci::parse_ci_provider(url);
                        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
                        let log_client = ci::CiLogClient::new(token);
                        match log_client.fetch_logs(&provider) {
                            Ok(logs) => println!("{}", logs),
                            Err(e) => println!("  Log URL: {}\n  (fetch failed: {})", url, e),
                        }
                    } else {
                        println!("  No details URL available");
                    }
                } else {
                    println!("Check '{}' not found in PR #{}", hint, pr);
                }
            }
        }
    }

    Ok(())
}
