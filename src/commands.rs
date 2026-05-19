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
