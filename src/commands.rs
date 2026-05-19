pub fn unlock_message(branch: &str) -> String {
    format!("Unlocked branch '{}'", branch)
}

pub fn agent_context_text(pr_count: usize) -> String {
    format!(
        "fp agent-context — run with --json for machine-readable output\nauth: GITHUB_TOKEN or gh auth login\ncommands: ls, status, track, untrack, watch, reply, context, threads, create, rebase-stack\ntracked PRs: {}",
        pr_count
    )
}
