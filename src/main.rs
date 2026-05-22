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
pub mod process_store;
pub mod app_config;
pub mod feature;
pub mod date;

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
#[cfg(test)]
mod process_store_test;
#[cfg(test)]
mod app_config_test;
#[cfg(test)]
mod feature_test;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use github::{GithubClient, detect_repo, resolve_github_token, resolve_track_branch};
use store::{Store, PrCache};


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
        /// Skip lifecycle prompts; apply safe defaults silently
        #[arg(long)]
        non_interactive: bool,
    },
    /// Remove the lock on a worktree branch so it can be switched to again
    Unlock {
        /// Branch name (not PR number)
        branch: String,
    },
    /// Manage feature envelopes (groups of PRs activated together)
    Feature {
        #[command(subcommand)]
        subcommand: FeatureCommands,
    },
    /// Manage app lifecycle configs (bootstrap, teardown, health check)
    App {
        #[command(subcommand)]
        subcommand: AppCommands,
    },
    /// Manage per-PR overrides (config assignment, etc.)
    Pr {
        #[command(subcommand)]
        subcommand: PrCommands,
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

#[derive(Subcommand)]
enum FeatureCommands {
    /// Create a new feature envelope
    New { name: String },
    /// Add a PR to a feature envelope (auto-tracks if untracked)
    Add { name: String, pr: u64, #[arg(long, action = clap::ArgAction::Append, value_name = "CONFIG")] config: Vec<String> },
    /// Declare a baseline app config dependency (no PR required)
    AddDep { name: String, app_config: String },
    /// List feature envelopes and their member PRs
    List {
        /// Show only envelopes with at least one live instance
        #[arg(long)]
        running: bool,
    },
    /// Show health status of all member PRs in a feature envelope
    Status {
        /// Feature envelope name
        name: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Bootstrap all PRs in a feature envelope
    Up {
        /// Feature envelope name
        name: String,
        /// Tear down conflicting running features without prompting
        #[arg(long)]
        yes: bool,
        /// Abort if any conflicting running feature is detected
        #[arg(long)]
        no: bool,
        /// Kill any unmanaged process via teardown before bootstrapping
        #[arg(long)]
        force: bool,
    },
    /// Tear down all PRs in a feature envelope
    Down {
        /// Feature envelope name
        name: String,
    },
    /// Re-run bootstrap for ephemeral members without teardown
    Rebuild {
        /// Feature envelope name
        name: String,
        /// Rebuild only this PR
        #[arg(long)]
        pr: Option<u64>,
    },
    /// Remove a PR from a feature envelope
    Remove {
        /// Feature envelope name
        name: String,
        /// PR number to remove
        pr: u64,
    },
    /// Remove a baseline app config dependency from a feature envelope
    RemoveDep {
        /// Feature envelope name
        name: String,
        /// App config name to remove
        app_config: String,
    },
    /// Set the e2e test command for a feature envelope
    SetTest {
        /// Feature envelope name
        name: String,
        /// Shell command to run as the e2e test
        command: String,
    },
    /// Run the e2e test command for a feature envelope
    Test {
        /// Feature envelope name
        name: String,
    },
    /// Show process logs for a feature envelope
    Logs {
        /// Feature envelope name
        name: String,
        /// Follow (tail -f) all log files
        #[arg(long, short)]
        follow: bool,
    },
}

#[derive(Subcommand)]
enum AppCommands {
    /// Define (create or update) a named app config with lifecycle commands
    DefineConfig {
        /// Name for this config (e.g. payments-api)
        name: String,
        /// Command to start the app
        #[arg(long)]
        bootstrap: String,
        /// Command to stop the app
        #[arg(long)]
        teardown: String,
        /// How long to wait for startup (e.g. 60s)
        #[arg(long, default_value = "60s")]
        startup_timeout: String,
        /// Optional health-check command (exit 0 = healthy)
        #[arg(long)]
        health_check: Option<String>,
        /// App exits immediately after install (e.g. Chrome extension); health-check required
        #[arg(long, default_value_t = false)]
        ephemeral: bool,
        /// Path to the main worktree to use when no PR owns this app config slot
        #[arg(long)]
        main_worktree: Option<String>,
    },
    /// Assign a named app config to all PRs on a repo
    SetConfig {
        /// Repository slug (e.g. acme/payments-api)
        repo: String,
        /// Name of the app config to assign
        config_name: String,
    },
    /// List all defined app configs
    List,
}

#[derive(Subcommand)]
enum PrCommands {
    /// Bootstrap the app for a single PR
    Up {
        /// PR number
        pr: u64,
        /// Override app config(s) to use (repeatable); defaults to configs bound to the PR
        #[arg(long = "config")]
        configs: Vec<String>,
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

use worktree::{git_dir, repo_root, require_repo};

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
                let ps = process_store::ProcessStateStore::open(&git_dir);
                let app_store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
                print!("{}", commands::cmd_status_all(client_ref, &store, Some(&ps), Some(&app_store), &git_dir, &owner, &repo_name, json)?);
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

        Commands::Switch { pr, id, force, adopt, non_interactive } => {
            let ps = process_store::ProcessStateStore::open(&git_dir);
            let app_cfg_store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
            let wt_path = commands::cmd_switch(&store, &ps, &app_cfg_store, &git_dir, pr, &id, force, adopt, non_interactive, &std::env::current_dir()?)?;
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
            let token = std::env::var("GITHUB_TOKEN").ok();
            let repo = detect_repo();
            let (client, owner, repo_name): (Option<GithubClient>, String, String) =
                if let (Some(tok), Some((o, r))) = (token, repo) {
                    (Some(GithubClient::new(tok)), o, r)
                } else {
                    (None, String::new(), String::new())
                };
            let out = commands::cmd_watch(
                client.as_ref().map(|c| c as &dyn github::GithubClientTrait),
                &owner, &repo_name,
                &store, &git_dir, once, interval, json, wait_for,
            )?;
            print!("{}", out);
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

        Commands::Feature { subcommand } => {
            let ps = process_store::ProcessStateStore::open(&git_dir);
            match subcommand {
                FeatureCommands::New { name } => {
                    let out = commands::cmd_feature_new(&ps, &name)?;
                    println!("{}", out);
                }
                FeatureCommands::Add { name, pr, config } => {
                    let store = Store::open(&git_dir);
                    let out = commands::cmd_feature_add(&ps, &store, &name, pr, config)?;
                    println!("{}", out);
                }
                FeatureCommands::List { running } => {
                    let out = if running {
                        commands::cmd_feature_list_running(&ps)?
                    } else {
                        commands::cmd_feature_list(&ps)?
                    };
                    println!("{}", out);
                }
                FeatureCommands::Status { name, json } => {
                    let app_store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
                    if json {
                        let out = commands::cmd_feature_status(&ps, &app_store, &name, true, &std::env::current_dir()?)?;
                        println!("{}", out);
                        return Ok(());
                    }
                    let (client, owner, repo_name) = if let (Ok(tok), Some((o, r))) = (resolve_github_token(), detect_repo()) {
                        let c: Box<dyn github::GithubClientTrait> = Box::new(GithubClient::new(tok));
                        (Some(c), o, r)
                    } else { (None, String::new(), String::new()) };
                    let repo_root = crate::worktree::main_repo_root(&std::env::current_dir()?)?;
                    let out = commands::cmd_feature_status_with_client(&ps, &app_store, &name, client.as_deref(), &owner, &repo_name, &repo_root)?;
                    println!("{}", out);
                }
                FeatureCommands::Up { name, yes, no, force } => {
                    let app_store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
                    let repo_root = crate::worktree::main_repo_root(&std::env::current_dir()?)?;
                    if force {
                        let state = ps.load()?;
                        for rec in state.records.values().filter(|r| r.feature_envelope.as_deref() == Some(&name)) {
                            let wt = std::path::Path::new(&rec.worktree);
                            for cfg_name in &rec.app_config_names {
                                if let Ok(Some(cfg)) = app_store.load_app_config(cfg_name) {
                                    let _ = crate::feature::teardown_pr(&ps, &cfg, rec.pr, wt, "", "");
                                }
                            }
                        }
                    }
                    let out = commands::cmd_feature_up_checked(&ps, &app_store, &name, yes, no, &repo_root)?;
                    println!("{}", out);
                }
                FeatureCommands::Down { name } => {
                    let app_store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
                    let repo_root = crate::worktree::main_repo_root(&std::env::current_dir()?)?;
                    let out = commands::cmd_feature_down(&ps, &app_store, &name, &repo_root)?;
                    println!("{}", out);
                }
                FeatureCommands::Rebuild { name, pr } => {
                    let app_store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
                    let repo_root = crate::worktree::main_repo_root(&std::env::current_dir()?)?;
                    let out = commands::cmd_feature_rebuild(&ps, &app_store, &name, pr, &repo_root)?;
                    println!("{}", out);
                }
                FeatureCommands::AddDep { name, app_config } => {
                    let out = commands::cmd_feature_add_dep(&ps, &name, &app_config)?;
                    println!("{}", out);
                }
                FeatureCommands::Remove { name, pr } => {
                    let out = commands::cmd_feature_remove(&ps, &name, pr)?;
                    print!("{}", out);
                }
                FeatureCommands::RemoveDep { name, app_config } => {
                    let out = commands::cmd_feature_remove_dep(&ps, &name, &app_config)?;
                    print!("{}", out);
                }
                FeatureCommands::SetTest { name, command } => {
                    let out = commands::cmd_feature_set_test(&ps, &name, &command)?;
                    print!("{}", out);
                }
                FeatureCommands::Test { name } => {
                    let repo_root = crate::worktree::main_repo_root(&std::env::current_dir()?)?;
                    let out = commands::cmd_feature_test(&ps, &name, &repo_root)?;
                    println!("{}", out);
                }
                FeatureCommands::Logs { name, follow } => {
                    let out = commands::cmd_feature_logs(&ps, &name, follow)?;
                    if !out.is_empty() { println!("{}", out); }
                }
            }
        }

        Commands::App { subcommand } => {
            let store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
            match subcommand {
                AppCommands::DefineConfig { name, bootstrap, teardown, startup_timeout, health_check, ephemeral, main_worktree } => {
                    let out = commands::cmd_app_define_config(&store, &name, &bootstrap, &teardown, &startup_timeout, health_check.as_deref(), ephemeral, main_worktree.as_deref())?;
                    println!("{}", out);
                }
                AppCommands::SetConfig { repo, config_name } => {
                    let out = commands::cmd_app_set_config(&store, &repo, &config_name)?;
                    println!("{}", out);
                }
                AppCommands::List => {
                    let out = commands::cmd_app_list(&store)?;
                    println!("{}", out);
                }
            }
        }

        Commands::Pr { subcommand } => {
            let store = app_config::AppConfigStore::open(app_config::AppConfigStore::default_path()?);
            match subcommand {
                PrCommands::Up { pr, configs } => {
                    let ps = process_store::ProcessStateStore::open(&git_dir);
                    if configs.is_empty() {
                        let out = commands::cmd_pr_up(&ps, &store, pr)?;
                        println!("{}", out);
                    } else {
                        let cfg_refs: Vec<&str> = configs.iter().map(|s| s.as_str()).collect();
                        let out = commands::cmd_pr_up_with_configs(&ps, &store, pr, &cfg_refs)?;
                        println!("{}", out);
                    }
                }
            }
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
            let ps = process_store::ProcessStateStore::open(&git_dir);
            print!("{}", commands::cmd_merge(&client, &owner, &repo_name, pr, commands::MergeContext { store: &store, dir: &repo_root()?, git_dir: &git_dir, merge_method }, &ps)?);
        }

        Commands::RebaseStack { pr: rebase_from_pr, verbose } => {
            let (client, owner, repo_name) = if let (Ok(tok), Some((o, r))) = (resolve_github_token(), detect_repo()) {
                let c: Box<dyn github::GithubClientTrait> = Box::new(GithubClient::new(tok));
                (Some(c), o, r)
            } else {
                (None, String::new(), String::new())
            };
            print!("{}", commands::cmd_rebase_stack(
                client.as_deref(), &owner, &repo_name, &store, &repo_root()?, &git_dir, rebase_from_pr,
                &|msg| if verbose { eprintln!("{}", msg) },
            )?);
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
