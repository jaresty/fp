pub fn check_merge_base(merged_base: &str, has_downstream: bool) -> anyhow::Result<()> {
    if merged_base.is_empty() && has_downstream {
        anyhow::bail!("could not determine merge base — downstream PRs were not rebased; rebase manually with: git rebase --onto <new-main-tip> <old-parent-tip> <branch>");
    }
    Ok(())
}

pub fn resolve_merge_base(fetched: &str, stored: &str) -> String {
    if !fetched.is_empty() { fetched.to_string() } else { stored.to_string() }
}

pub fn resolve_track_branch(
    explicit: Option<String>,
    fetched: Option<String>,
    pr_number: u64,
) -> anyhow::Result<String> {
    if let Some(b) = explicit.filter(|s| !s.is_empty()) { return Ok(b); }
    if let Some(b) = fetched.filter(|s| !s.is_empty()) { return Ok(b); }
    anyhow::bail!(
        "fp: could not determine branch for PR #{}.\nRun: fp track {} --branch <branch-name>",
        pr_number, pr_number
    )
}
