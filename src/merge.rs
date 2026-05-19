pub fn check_merge_base(merged_base: &str, has_downstream: bool) -> anyhow::Result<()> {
    if merged_base.is_empty() && has_downstream {
        anyhow::bail!("could not determine merge base — downstream PRs were not rebased; rebase manually with: git rebase --onto <new-main-tip> <old-parent-tip> <branch>");
    }
    Ok(())
}

pub fn resolve_merge_base(fetched: &str, stored: &str) -> String {
    if !fetched.is_empty() { fetched.to_string() } else { stored.to_string() }
}
