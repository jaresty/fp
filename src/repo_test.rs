#[cfg(test)]
mod tests {
    #[test]
    fn worktree_governs_parse_github_remote_pub_https() {
        let result = crate::worktree::parse_github_remote_pub("https://github.com/owner/repo.git");
        assert_eq!(
            result,
            Some(("owner".to_string(), "repo".to_string())),
            "worktree::parse_github_remote_pub must parse https remote"
        );
    }

    #[test]
    fn worktree_governs_parse_github_remote_pub_ssh() {
        let result = crate::worktree::parse_github_remote_pub("git@github.com:owner/repo.git");
        assert_eq!(
            result,
            Some(("owner".to_string(), "repo".to_string())),
            "worktree::parse_github_remote_pub must parse ssh remote"
        );
    }

    #[test]
    fn worktree_governs_detect_repo_with_cwd() {
        let cwd = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = crate::worktree::detect_repo_with_cwd(cwd);
        assert!(result.is_some(), "worktree::detect_repo_with_cwd must detect repo from manifest dir");
    }
}
