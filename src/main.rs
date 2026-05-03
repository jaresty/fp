mod model;
mod tasks;
mod store;
mod github;

#[cfg(test)]
mod tasks_test;
#[cfg(test)]
mod store_test;
#[cfg(test)]
mod github_test;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use github::{GithubClient, detect_repo};
use store::{Store, TrackedPr};
use tasks::generate_tasks;

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
            let title = title.unwrap_or_else(|| format!("PR #{}", pr));
            let branch = branch.unwrap_or_default();
            store.track(TrackedPr { number: pr, title: title.clone(), branch })?;
            println!("Tracking PR #{} — {}", pr, title);
        }

        Commands::Untrack { pr } => {
            store.untrack(pr)?;
            println!("Untracked PR #{}", pr);
        }
    }

    Ok(())
}
