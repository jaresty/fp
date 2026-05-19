#[cfg(test)]
mod tests {
    #[test]
    fn worktree_governs_check_branch_lock_returns_some_when_lock_exists() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let lock_dir = tmp.path().join("worktrees").join("feat-branch");
        std::fs::create_dir_all(&lock_dir).unwrap();
        let lock_path = lock_dir.join("fp-lock");
        let mut f = std::fs::File::create(&lock_path).unwrap();
        writeln!(f, r#"{{"id":"test","pid":99999999,"kind":"agent"}}"#).unwrap();
        let result = crate::worktree::check_branch_lock(tmp.path(), "feat-branch");
        assert!(result.is_some(), "worktree::check_branch_lock must return Some when lock file exists: got None");
    }

    #[test]
    fn worktree_governs_require_repo_errors_when_none() {
        let result = crate::worktree::require_repo(None);
        assert!(result.is_err(), "worktree::require_repo(None) must return Err");
        assert!(
            result.unwrap_err().to_string().contains("no GitHub remote"),
            "error must mention 'no GitHub remote'"
        );
    }

    #[test]
    fn worktree_governs_require_repo_returns_pair_when_some() {
        let result = crate::worktree::require_repo(Some(("alice".into(), "proj".into())));
        assert!(result.is_ok(), "worktree::require_repo(Some) must succeed");
        let (owner, repo) = result.unwrap();
        assert_eq!(owner, "alice");
        assert_eq!(repo, "proj");
    }

    #[test]
    fn worktree_governs_git_dir_returns_path_in_repo() {
        // This test only works when run inside the fp repo itself
        let result = crate::worktree::git_dir();
        assert!(result.is_ok(), "worktree::git_dir must succeed inside a git repo");
        let path = result.unwrap();
        assert!(path.exists(), "worktree::git_dir must return an existing path");
    }
}
