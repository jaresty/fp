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

    #[test]
    fn worktree_governs_check_branch_lock_returns_none_when_no_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        assert!(crate::worktree::check_branch_lock(&git_dir, "feat/foo").is_none(),
            "no lock file must return None");
    }

    #[test]
    fn worktree_governs_check_branch_lock_returns_alive_warning_for_live_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        let lp = crate::worktree::lock_path(&git_dir, "feat/foo");
        std::fs::create_dir_all(lp.parent().unwrap()).unwrap();
        let live_pid = std::process::id();
        crate::worktree::write_lock(&lp, live_pid, "agent", "my-session").unwrap();
        let result = crate::worktree::check_branch_lock(&git_dir, "feat/foo");
        assert!(result.is_some(), "live lock must return Some");
        let msg = result.unwrap();
        assert!(msg.contains("alive"), "live lock warning must say 'alive', got: {}", msg);
        assert!(msg.contains("feat/foo"), "warning must contain branch name, got: {}", msg);
    }

    #[test]
    fn worktree_governs_check_branch_lock_returns_dead_warning_with_unlock_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        let lp = crate::worktree::lock_path(&git_dir, "feat/bar");
        std::fs::create_dir_all(lp.parent().unwrap()).unwrap();
        crate::worktree::write_lock(&lp, 99999999, "agent", "old-session").unwrap();
        let result = crate::worktree::check_branch_lock(&git_dir, "feat/bar");
        assert!(result.is_some(), "dead lock must return Some");
        let msg = result.unwrap();
        assert!(msg.contains("fp unlock"), "dead lock warning must mention 'fp unlock', got: {}", msg);
    }

    #[test]
    fn worktree_governs_locked_subtree_returns_transitive_descendants() {
        let locked: std::collections::HashSet<String> = ["feat/a".to_string()].into();
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/b".to_string(), Some("feat/a".to_string())),
            ("feat/c".to_string(), Some("feat/b".to_string())),
            ("feat/d".to_string(), Some("main".to_string())),
        ].into();
        let result = crate::worktree::locked_subtree(&locked, &parent_of);
        assert!(result.contains("feat/b"), "b is child of locked a");
        assert!(result.contains("feat/c"), "c is grandchild of locked a");
        assert!(!result.contains("feat/d"), "d has unrelated parent");
        assert!(!result.contains("feat/a"), "locked branch itself not in subtree");
    }

    #[test]
    fn worktree_governs_locked_subtree_empty_when_no_descendants() {
        let locked: std::collections::HashSet<String> = ["feat/a".to_string()].into();
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/b".to_string(), Some("main".to_string())),
        ].into();
        let result = crate::worktree::locked_subtree(&locked, &parent_of);
        assert!(result.is_empty(), "no descendants when nothing chains from locked");
    }

    #[test]
    fn worktree_governs_branch_in_main_worktree_warning_contains_adopt_hint() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        std::process::Command::new("git").args(["commit", "--allow-empty", "-m", "init"])
            .current_dir(tmp.path())
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .output().unwrap();
        let head = std::process::Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"]).current_dir(tmp.path()).output().unwrap();
        let branch = String::from_utf8(head.stdout).unwrap().trim().to_string();
        let warn = crate::worktree::branch_in_main_worktree_warning(&branch, tmp.path()).expect("should return Some when branch is checked out");
        assert!(warn.contains("--adopt"), "warning must contain --adopt, got: {}", warn);
        assert!(warn.contains(&branch), "warning must contain branch name, got: {}", warn);
    }

    #[test]
    fn worktree_governs_branch_in_main_worktree_warning_returns_none_when_not_checked_out() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        std::process::Command::new("git").args(["commit", "--allow-empty", "-m", "init"])
            .current_dir(tmp.path())
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .output().unwrap();
        assert!(crate::worktree::branch_in_main_worktree_warning("feat/other-branch", tmp.path()).is_none());
    }

    #[test]
    fn worktree_governs_fix_worktree_branch_checks_out_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit","--allow-empty","-m","init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let default_branch = String::from_utf8(
            std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap().stdout
        ).unwrap().trim().to_string();
        std::process::Command::new("git").args(["checkout","-b","feat/other"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd2 = std::process::Command::new("git");
        cmd2.args(["commit","--allow-empty","-m","other"]).current_dir(tmp.path());
        for (k,v) in &env { cmd2.env(k,v); }
        cmd2.output().unwrap();
        crate::worktree::fix_worktree_branch(tmp.path(), &default_branch, false).unwrap();
        let head_after = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap();
        assert_eq!(String::from_utf8(head_after.stdout).unwrap().trim(), &default_branch);
    }

    #[test]
    fn worktree_governs_fix_worktree_branch_with_force_discards_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
        std::fs::write(tmp.path().join("shared.txt"), "original").unwrap();
        std::process::Command::new("git").args(["add","shared.txt"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit","-m","init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let default_branch = String::from_utf8(
            std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap().stdout
        ).unwrap().trim().to_string();
        std::process::Command::new("git").args(["checkout","-b","feat/other"]).current_dir(tmp.path()).output().unwrap();
        std::fs::write(tmp.path().join("shared.txt"), "other-branch-content").unwrap();
        std::process::Command::new("git").args(["add","shared.txt"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd2 = std::process::Command::new("git");
        cmd2.args(["commit","-m","other"]).current_dir(tmp.path());
        for (k,v) in &env { cmd2.env(k,v); }
        cmd2.output().unwrap();
        std::fs::write(tmp.path().join("shared.txt"), "local-modification").unwrap();
        let no_force = crate::worktree::fix_worktree_branch(tmp.path(), &default_branch, false);
        assert!(no_force.is_err(), "should fail without --force when dirty");
        crate::worktree::fix_worktree_branch(tmp.path(), &default_branch, true).unwrap();
        let head = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).current_dir(tmp.path()).output().unwrap();
        assert_eq!(String::from_utf8(head.stdout).unwrap().trim(), &default_branch);
    }

    #[test]
    fn worktree_governs_main_repo_root_returns_main_root_from_inside_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let main_root = tmp.path().join("myrepo");
        std::fs::create_dir(&main_root).unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init"]).current_dir(&main_root).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit","--allow-empty","-m","init"]).current_dir(&main_root);
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let wt_path = tmp.path().join("myrepo-worktrees/feat/branch");
        std::fs::create_dir_all(&wt_path).unwrap();
        std::process::Command::new("git").args(["checkout","-b","feat/branch"]).current_dir(&main_root).output().unwrap();
        std::process::Command::new("git").args(["checkout","-b","other"]).current_dir(&main_root).output().unwrap();
        let mut cmd2 = std::process::Command::new("git");
        cmd2.args(["commit","--allow-empty","-m","other"]).current_dir(&main_root);
        for (k,v) in &env { cmd2.env(k,v); }
        cmd2.output().unwrap();
        std::process::Command::new("git")
            .args(["worktree","add", wt_path.to_str().unwrap(), "feat/branch"])
            .current_dir(&main_root).output().unwrap();
        let result = crate::worktree::main_repo_root(&wt_path).unwrap();
        let expected = main_root.canonicalize().unwrap();
        assert_eq!(result.canonicalize().unwrap(), expected);
    }

    #[test]
    fn worktree_governs_untrack_and_cleanup_removes_lock_for_merged_pr() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store_dir = git_dir.join("fp_store");
        std::fs::create_dir_all(&store_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let branch = "feature/my-branch";
        store.track(42).unwrap();
        store.update_cache(crate::store::PrCache { number: 42, title: "test".into(), branch: branch.into(), base: "main".into() }).unwrap();
        let lp = crate::worktree::lock_path(&git_dir, branch);
        if let Some(parent) = lp.parent() { std::fs::create_dir_all(parent).unwrap(); }
        std::fs::write(&lp, b"fake lock").unwrap();
        crate::worktree::untrack_and_cleanup(&store, tmp.path(), &git_dir, 42, branch).unwrap();
        assert!(!store.load().unwrap().tracked.contains(&42), "PR must be untracked after cleanup");
        assert!(!lp.exists(), "lock file must be removed by untrack_and_cleanup");
    }
}
