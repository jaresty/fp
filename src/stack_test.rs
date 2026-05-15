#[cfg(test)]
mod tests {
    use crate::stack::{stack_order, detect_parent_of, rebase_stack};
    use std::collections::HashMap;

    // RS1: linear stack A <- B <- C returns [A, B, C] (parent first)
    #[test]
    fn linear_stack_ordered_parent_first() {
        let branches = vec!["feat/a".to_string(), "feat/b".to_string(), "feat/c".to_string()];
        let mut parent_of: HashMap<String, Option<String>> = HashMap::new();
        parent_of.insert("feat/a".into(), None);
        parent_of.insert("feat/b".into(), Some("feat/a".into()));
        parent_of.insert("feat/c".into(), Some("feat/b".into()));

        let ordered = stack_order(&branches, &parent_of);
        assert_eq!(ordered, vec!["feat/a", "feat/b", "feat/c"]);
    }

    // RS1: single branch returns itself
    #[test]
    fn single_branch_returns_self() {
        let branches = vec!["feat/a".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("feat/a".into(), None);
        let ordered = stack_order(&branches, &parent_of);
        assert_eq!(ordered, vec!["feat/a"]);
    }

    // RS1: branches with no parent relationship returned in stable order
    #[test]
    fn unrelated_branches_returned_stably() {
        let branches = vec!["feat/x".to_string(), "feat/y".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("feat/x".into(), None);
        parent_of.insert("feat/y".into(), None);
        let ordered = stack_order(&branches, &parent_of);
        assert_eq!(ordered.len(), 2);
    }

    // RS1: detect_parent_of finds linear parent via git merge-base in a real repo
    #[test]
    fn detect_parent_of_finds_linear_parent() {
        use std::process::Command;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        // Set up a minimal git repo with two branches
        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path)
                .output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);

        // main: commit A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);

        // feat/base: branch from main, add commit B
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);

        // feat/top: branch from feat/base, add commit C
        git(&["checkout", "-b", "feat/top"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);

        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let parent_of = detect_parent_of(&branches, path).unwrap();

        // feat/base should have no parent in our branch set (its parent is main, not in set)
        assert_eq!(parent_of.get("feat/base"), Some(&None));
        // feat/top's parent should be feat/base
        assert_eq!(parent_of.get("feat/top"), Some(&Some("feat/base".to_string())));
    }

    // RS0: resolve_work_dir returns an absolute, existing directory
    #[test]
    fn resolve_work_dir_returns_absolute_path() {
        let dir = crate::stack::resolve_work_dir(std::path::Path::new(".git")).unwrap();
        assert!(dir.is_absolute(), "work_dir must be absolute, got: {:?}", dir);
        assert!(dir.is_dir(), "work_dir must be an existing directory, got: {:?}", dir);
    }

    // MG1: rebase_onto_after_merge rebases child onto base using head_sha as cut point (squash-safe)
    #[test]
    fn rebase_onto_after_merge_rebases_child_onto_base() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path).output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/parent: commit B (will be "squash merged" into main)
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        let parent_sha = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // feat/child: branch from feat/parent, commit C
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/child"]);

        // Simulate squash merge: merge feat/parent into main as a squash commit
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash merge B"]);
        git(&["push", "origin", "main"]);

        // Now rebase_onto_after_merge should rebase feat/child onto main, cutting at parent_sha
        crate::stack::rebase_onto_after_merge("feat/child", &parent_sha, "main", path).unwrap();

        // feat/child should now be on top of main
        let child_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let main_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "main"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(child_parent, main_tip, "feat/child should be rebased onto main tip");
    }

    // RS3: rebase_stack force-pushes each rebased branch after successful rebase
    #[test]
    fn rebase_stack_pushes_after_rebase() {
        use std::process::Command;
        use tempfile::TempDir;

        // Set up a bare remote so push has somewhere to go
        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path).output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/base: branch from main, commit B, push
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // feat/top: branch from main (not feat/base!), commit C, push
        git(&["checkout", "main"]);
        git(&["checkout", "-b", "feat/top"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/top"]);

        // Return to main before rebase_stack — worktree approach requires main worktree not on any PR branch
        git(&["checkout", "main"]);

        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        parent_of.insert("feat/top".to_string(), Some("feat/base".to_string()));

        let result = rebase_stack(&branches, &parent_of, &std::collections::HashMap::new(), path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);

        // Verify feat/top was pushed to remote by checking remote tip matches local tip
        let local_tip = Command::new("git")
            .args(["rev-parse", "feat/top"])
            .current_dir(path).output().unwrap();
        let remote_tip = Command::new("git")
            .args(["rev-parse", "refs/heads/feat/top"])
            .current_dir(remote_dir.path()).output().unwrap();

        let local = String::from_utf8(local_tip.stdout).unwrap().trim().to_string();
        let remote = String::from_utf8(remote_tip.stdout).unwrap().trim().to_string();
        assert_eq!(local, remote, "remote feat/top should match local after force-push");
    }

    // RS8: rebase_stack returns error if a rebase is already in progress
    #[test]
    fn rebase_stack_errors_if_rebase_in_progress() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // Simulate an in-progress rebase by creating the REBASE_HEAD file
        std::fs::write(path.join(".git").join("REBASE_HEAD"), "fakasha").unwrap();

        let branches = vec!["main".to_string()];
        let parent_of = std::collections::HashMap::new();
        let base_of = std::collections::HashMap::new();
        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {});
        assert!(result.is_err(), "expected error when rebase in progress");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("rebase in progress"), "expected 'rebase in progress' in error, got: {}", msg);
    }

    // RS9: rebase_stack leaves repo in conflict state (no abort) when conflict occurs
    #[test]
    fn rebase_stack_does_not_abort_on_conflict() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A with conflict.txt = "main"
        std::fs::write(path.join("conflict.txt"), "main").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/base: branch, change conflict.txt = "base", push
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("conflict.txt"), "base").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // Add conflicting commit to origin/main: conflict.txt = "upstream"
        git(&["checkout", "main"]);
        std::fs::write(path.join("conflict.txt"), "upstream").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "X"]);
        git(&["push", "origin", "main"]);
        // Return to main before rebase_stack — worktree approach requires main worktree not on any PR branch
        git(&["checkout", "main"]);

        let branches = vec!["feat/base".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/base".to_string(), "main".to_string());

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert_eq!(result.conflicts, vec!["feat/base"], "expected feat/base in conflicts");

        // REBASE_HEAD must exist in the worktree admin dir — rebase was NOT aborted
        let wt_admin = path.join(".git").join("worktrees");
        let has_rebase_head = std::fs::read_dir(&wt_admin)
            .map(|entries| entries.filter_map(|e| e.ok())
                .any(|entry| entry.path().join("REBASE_HEAD").exists()))
            .unwrap_or(false);
        assert!(has_rebase_head,
            "expected REBASE_HEAD in a worktree admin dir (rebase left in progress, not aborted)");
    }

    // RS6: rebase_stack fetches origin before rebasing so remote-only commits are picked up
    #[test]
    fn rebase_stack_fetches_before_rebasing() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path).output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A, push
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/base: branch from main, commit B, push
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // Clone a second local repo to push a new commit to origin/main
        // without the first repo knowing about it (simulating remote-only progress)
        let dir2 = TempDir::new().unwrap();
        let path2 = dir2.path();
        Command::new("git").args(["clone", remote_dir.path().to_str().unwrap(), "."])
            .current_dir(path2).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(path2).output().unwrap();
        Command::new("git").args(["config", "user.name", "Test"]).current_dir(path2).output().unwrap();
        std::fs::write(path2.join("x.txt"), "x").unwrap();
        Command::new("git").args(["add", "."]).current_dir(path2).output().unwrap();
        Command::new("git").args(["commit", "-m", "X"]).current_dir(path2).output().unwrap();
        Command::new("git").args(["push", "origin", "main"]).current_dir(path2).output().unwrap();

        // Back in original repo: origin/main is stale (doesn't have commit X yet)
        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let branches = vec!["feat/base".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/base".to_string(), "main".to_string());

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);

        // feat/base should be on top of the remote commit X (only reachable via fetch)
        let base_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/base~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let origin_main_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "origin/main"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(base_parent, origin_main_tip, "feat/base should be rebased onto fetched origin/main (commit X)");
    }

    // RS7: rebase_stack uses API-provided base branch (base_of) not hardcoded main
    #[test]
    fn rebase_stack_uses_base_of_for_root_branch() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "develop"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path).output().unwrap()
        };

        git(&["init", "-b", "develop"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // develop: commit A, push
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "develop"]);

        // feat/base: branch from develop, commit B, push
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // Add commit X to origin/develop
        git(&["checkout", "develop"]);
        std::fs::write(path.join("x.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "X"]);
        git(&["push", "origin", "develop"]);
        // Return to develop before rebase_stack
        git(&["checkout", "develop"]);

        let branches = vec!["feat/base".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/base".to_string(), "develop".to_string());

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);

        let base_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/base~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let origin_develop_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "origin/develop"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(base_parent, origin_develop_tip, "feat/base should be rebased onto origin/develop not origin/main");
    }

    // RS5: rebase_stack rebases root branches onto origin/main when main has new commits
    #[test]
    fn rebase_stack_rebases_root_branch_onto_origin_main() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path).output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A, push
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/base: branch from main, commit B, push
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // Simulate upstream progress: add commit X to origin/main directly
        git(&["checkout", "main"]);
        std::fs::write(path.join("x.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "X"]);
        git(&["push", "origin", "main"]);

        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let branches = vec!["feat/base".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);

        let result = rebase_stack(&branches, &parent_of, &std::collections::HashMap::new(), path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);

        // feat/base should now be on top of origin/main (commit X)
        let base_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/base~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let origin_main_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "origin/main"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(base_parent, origin_main_tip, "feat/base should be rebased onto origin/main");
    }

    // RS2: rebase_stack rebases feat/top onto feat/base's current tip
    #[test]
    fn rebase_stack_rebases_in_order() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path)
                .output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/base: branch from main, commit B
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // feat/top: branch from main (not feat/base!), commit C
        git(&["checkout", "main"]);
        git(&["checkout", "-b", "feat/top"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/top"]);

        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        // After rebase_stack, feat/top should be rebased onto feat/base
        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        parent_of.insert("feat/top".to_string(), Some("feat/base".to_string()));

        let result = rebase_stack(&branches, &parent_of, &std::collections::HashMap::new(), path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts, got: {:?}", result.conflicts);

        // feat/top should now be on top of feat/base
        let top_tip = Command::new("git")
            .args(["rev-parse", "feat/top"])
            .current_dir(path).output().unwrap();
        let base_tip = Command::new("git")
            .args(["rev-parse", "feat/base"])
            .current_dir(path).output().unwrap();
        let top_parent = Command::new("git")
            .args(["rev-parse", "feat/top~1"])
            .current_dir(path).output().unwrap();

        let top_tip = String::from_utf8(top_tip.stdout).unwrap().trim().to_string();
        let base_tip = String::from_utf8(base_tip.stdout).unwrap().trim().to_string();
        let top_parent = String::from_utf8(top_parent.stdout).unwrap().trim().to_string();

        assert_ne!(top_tip, base_tip, "feat/top should have its own commit");
        assert_eq!(top_parent, base_tip, "feat/top's parent should be feat/base tip");
    }

    // ADR-004: rebase_downstream_stack rebases the full stack (A→B→C) when A merges
    #[test]
    fn rebase_downstream_stack_rebases_full_chain() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/parent: B
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        let parent_sha = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // feat/child: C on feat/parent
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/child"]);

        // feat/grandchild: D on feat/child
        git(&["checkout", "-b", "feat/grandchild"]);
        std::fs::write(path.join("d.txt"), "d").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "D"]);
        git(&["push", "--set-upstream", "origin", "feat/grandchild"]);

        // Squash merge feat/parent into main
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash: B"]);
        git(&["push", "origin", "main"]);

        // Build branch_base_of map: child → parent_branch
        let mut branch_base_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        branch_base_of.insert("feat/child".to_string(), "feat/parent".to_string());
        branch_base_of.insert("feat/grandchild".to_string(), "feat/child".to_string());

        // rebase_downstream_stack should rebase feat/child then feat/grandchild onto main
        let errors = crate::stack::rebase_downstream_stack(
            "feat/parent", &parent_sha, "main", &branch_base_of, path, &|_| {}
        );
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);

        // feat/child~1 should now be main tip
        let child_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let main_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "main"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(child_parent, main_tip, "feat/child should be on top of main");

        // feat/grandchild~1 should be feat/child tip (after rebase)
        let grandchild_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/grandchild~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let child_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(grandchild_parent, child_tip, "feat/grandchild should be on top of rebased feat/child");
    }

    // ADR-003: rebase_stack produces no invariant_warning on a clean rebase
    #[test]
    fn rebase_stack_no_invariant_warning_on_clean_rebase() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/base: independent change to b.txt
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        // feat/top: independent change to c.txt
        git(&["checkout", "-b", "feat/top"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/top"]);

        // Advance main with an independent commit (d.txt)
        git(&["checkout", "main"]);
        std::fs::write(path.join("d.txt"), "d").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "D"]);
        git(&["push", "origin", "main"]);
        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        parent_of.insert("feat/top".to_string(), Some("feat/base".to_string()));
        let base_of: std::collections::HashMap<String, String> =
            [("feat/base".to_string(), "main".to_string()),
             ("feat/top".to_string(), "main".to_string())].into_iter().collect();

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);
        assert!(
            result.invariant_warnings.is_empty(),
            "expected no invariant warnings on clean rebase, got: {:?}", result.invariant_warnings
        );
    }

    // ADR-003: rebase_stack uses --onto when parent branch ref is gone (merged)
    #[test]
    fn rebase_stack_uses_onto_when_parent_branch_is_deleted() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/parent: commit B
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);

        // feat/child: commit C on top of feat/parent
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/child"]);

        // Push feat/parent to remote (as a PR would be), then squash-merge it and
        // delete the remote branch — simulating GitHub's "auto-delete branch on merge".
        git(&["push", "--set-upstream", "origin", "feat/parent"]);
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash: B"]);
        git(&["push", "origin", "main"]);
        // Simulate remote branch auto-deletion on merge: delete feat/parent from remote
        git(&["push", "origin", "--delete", "feat/parent"]);
        // Fetch to update local tracking refs (origin/feat/parent will be pruned)
        git(&["fetch", "--prune", "origin"]);

        // Rebase stack with feat/child still listing feat/parent as parent
        let branches = vec!["feat/child".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/child".to_string(), Some("feat/parent".to_string()));
        let base_of: std::collections::HashMap<String, String> =
            [("feat/child".to_string(), "main".to_string())].into_iter().collect();

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts after --onto rebase: {:?}", result.conflicts);

        // feat/child should now be directly on top of origin/main
        let child_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let main_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "origin/main"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(child_parent, main_tip, "feat/child should be on top of origin/main after --onto rebase");
    }

    // ADR-004: rebase_stack emits progress messages via callback
    #[test]
    fn rebase_stack_emits_progress_via_callback() {
        use std::process::Command;
        use tempfile::TempDir;
        use std::sync::{Arc, Mutex};

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        git(&["checkout", "-b", "feat/x"]);
        std::fs::write(path.join("x.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "X"]);
        git(&["push", "--set-upstream", "origin", "feat/x"]);

        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let branches = vec!["feat/x".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/x".to_string(), None);
        let base_of: std::collections::HashMap<String, String> =
            [("feat/x".to_string(), "main".to_string())].into_iter().collect();

        let messages: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = Arc::clone(&messages);
        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|msg| {
            messages_clone.lock().unwrap().push(msg.to_string());
        }).unwrap();

        assert!(result.conflicts.is_empty());
        let msgs = messages.lock().unwrap();
        assert!(!msgs.is_empty(), "expected progress messages, got none");
        assert!(
            msgs.iter().any(|m| m.contains("feat/x")),
            "expected progress message mentioning feat/x, got: {:?}", msgs
        );
    }

    // RS10: push uses explicit 'origin <branch>' args (verifiable by pushing branch with no upstream configured)
    #[test]
    fn rebase_stack_push_uses_explicit_origin_and_branch() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/x: branch from main — push WITHOUT --set-upstream so no tracking ref is configured
        git(&["checkout", "-b", "feat/x"]);
        std::fs::write(path.join("x.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "X"]);
        // Deliberately push without --set-upstream (no upstream configured)
        git(&["push", "origin", "feat/x"]);

        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let branches = vec!["feat/x".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/x".to_string(), None);
        let base_of: std::collections::HashMap<String, String> =
            [("feat/x".to_string(), "main".to_string())].into_iter().collect();

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected push to succeed with explicit origin branch, got: {:?}", result.conflicts);

        // Verify remote has the branch
        let remote_tip = Command::new("git")
            .args(["rev-parse", "refs/heads/feat/x"])
            .current_dir(remote_dir.path()).output().unwrap();
        assert!(remote_tip.status.success(), "expected feat/x on remote after push with explicit origin");
    }

    // RS11: push failure stops processing of dependent branches
    // Uses a bare remote with a pre-receive hook that rejects all pushes,
    // so fetch works (rebase has valid origin/main) but push always fails.
    #[test]
    fn rebase_stack_stops_on_push_failure() {
        use std::process::Command;
        use tempfile::TempDir;

        // Bare remote with pre-receive hook rejecting all pushes
        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();
        let hook_path = remote_dir.path().join("hooks").join("pre-receive");
        std::fs::write(&hook_path, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        // Push main bypassing the hook (push directly to bare repo object)
        // by writing the ref manually — instead just init the remote with a commit
        // Workaround: temporarily remove the hook, push, then restore
        std::fs::remove_file(&hook_path).unwrap();
        git(&["push", "origin", "main"]);
        std::fs::write(&hook_path, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // feat/base: branch from main with a new commit — NOT pushed to remote before hook is active
        // so when rebase_stack tries to push it, the hook fires and rejects it
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);

        // feat/top: depends on feat/base
        git(&["checkout", "main"]);
        git(&["checkout", "-b", "feat/top"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);

        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        parent_of.insert("feat/top".to_string(), Some("feat/base".to_string()));
        let base_of: std::collections::HashMap<String, String> =
            [("feat/base".to_string(), "main".to_string())].into_iter().collect();

        // Record feat/top SHA before rebase_stack runs
        let top_sha_before = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/top"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // Hook now active — push will be rejected. Rebase of feat/base onto origin/main succeeds
        // (it's a no-op since feat/base is already based on main), but push fails.
        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();

        // feat/base push failed → feat/top should not have been touched (SHA unchanged)
        let top_sha_after = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/top"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(top_sha_before, top_sha_after,
            "expected feat/top SHA unchanged after feat/base push failed, result: {:?}", result);
        assert!(result.conflicts.iter().any(|c| c.contains("feat/base")),
            "expected feat/base in conflicts after push failure, got: {:?}", result.conflicts);
    }

    // RS12: git status output is captured in status_output on rebase conflict
    #[test]
    fn rebase_stack_captures_git_status_on_conflict() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        std::fs::write(path.join("conflict.txt"), "main").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("conflict.txt"), "base").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/base"]);

        git(&["checkout", "main"]);
        std::fs::write(path.join("conflict.txt"), "upstream").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "X"]);
        git(&["push", "origin", "main"]);
        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let branches = vec!["feat/base".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/base".to_string(), "main".to_string());

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert_eq!(result.conflicts, vec!["feat/base"]);
        let status = result.status_output.expect("expected status_output on conflict");
        assert!(!status.is_empty(), "expected non-empty git status output on conflict");
        assert!(status.contains("conflict.txt") || status.contains("rebase"),
            "expected conflict.txt or rebase in git status output, got: {}", status);
    }

    /// Verifies that after a parent branch is merged, rebase_downstream_stack rebases
    /// not just direct children but also grandchildren — the behavior the old one-level
    /// loop in fp merge did NOT provide.
    #[test]
    fn rebase_downstream_stack_rebases_grandchildren_not_just_direct_children() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: A
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        // feat/parent: B on main
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        let parent_sha = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // feat/child: C on feat/parent
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/child"]);

        // feat/grandchild: D on feat/child
        git(&["checkout", "-b", "feat/grandchild"]);
        std::fs::write(path.join("d.txt"), "d").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "D"]);
        git(&["push", "--set-upstream", "origin", "feat/grandchild"]);

        // Squash merge feat/parent into main
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash: B"]);
        git(&["push", "origin", "main"]);

        let main_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "main"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // branch_base_of mirrors what fp merge builds from state.prs
        let mut branch_base_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        branch_base_of.insert("feat/child".to_string(), "feat/parent".to_string());
        branch_base_of.insert("feat/grandchild".to_string(), "feat/child".to_string());

        let errors = crate::stack::rebase_downstream_stack(
            "feat/parent", &parent_sha, "main", &branch_base_of, path, &|_| {}
        );
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);

        // feat/child~1 == main tip
        let child_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(child_parent, main_tip, "feat/child should be on top of main after rebase");

        // feat/grandchild~1 == rebased feat/child tip (not original feat/child)
        let rebased_child_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let grandchild_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/grandchild~1"]).current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(grandchild_parent, rebased_child_tip,
            "feat/grandchild should be on top of rebased feat/child, not original — old one-level loop misses this");
    }

    /// Splice test: A→C becomes A→B→C after inserting B between A and C.
    /// Verifies that after rebase_stack with the updated parent_of:
    /// 1. C sits on top of B (not A) after rebase
    /// 2. C's three-dot diff vs B equals C's original three-dot diff vs A
    ///    (semantic content unchanged; only base moves)
    #[test]
    fn rebase_stack_splice_preserves_c_diff_after_inserting_b_between_a_and_c() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main (origin/main): commit M
        std::fs::write(path.join("m.txt"), "m").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "origin", "main"]);

        // feat/a: commit A on main (this is "branch A" in the splice)
        git(&["checkout", "-b", "feat/a"]);
        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "--set-upstream", "origin", "feat/a"]);

        // feat/c: commit C on feat/a — original stack is main→feat/a→feat/c
        git(&["checkout", "-b", "feat/c"]);
        std::fs::write(path.join("c.txt"), "c-content").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "--set-upstream", "origin", "feat/c"]);

        // Capture C's original semantic diff vs A (before splice)
        let pre_splice_diff = String::from_utf8(
            Command::new("git").args(["diff", "feat/a...feat/c", "--"])
                .current_dir(path).output().unwrap().stdout
        ).unwrap();
        assert!(!pre_splice_diff.is_empty(), "pre-splice diff should not be empty");

        // Now create feat/b by branching from feat/a (inserting B between A and C)
        git(&["checkout", "feat/a"]);
        git(&["checkout", "-b", "feat/b"]);
        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/b"]);

        // Updated stack: feat/a → feat/b → feat/c
        // parent_of reflects the splice: C's parent is now B
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/a".to_string(), None);
        parent_of.insert("feat/b".to_string(), Some("feat/a".to_string()));
        parent_of.insert("feat/c".to_string(), Some("feat/b".to_string()));

        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/a".to_string(), "main".to_string());
        base_of.insert("feat/b".to_string(), "main".to_string());
        base_of.insert("feat/c".to_string(), "main".to_string());

        let branches = vec!["feat/a".to_string(), "feat/b".to_string(), "feat/c".to_string()];

        // Return to main before rebase_stack
        git(&["checkout", "main"]);

        let result = crate::stack::rebase_stack(&branches, &parent_of, &base_of, path, &|_| {});
        let result = result.unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts, got: {:?}", result.conflicts);

        // Dimension 1: C sits on top of B after rebase
        let b_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/b"])
                .current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        let c_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/c~1"])
                .current_dir(path).output().unwrap().stdout
        ).unwrap().trim().to_string();
        assert_eq!(c_parent, b_tip, "C should sit on top of B after splice rebase");

        // Dimension 2: C's semantic diff vs B equals pre-splice diff vs A
        let post_splice_diff = String::from_utf8(
            Command::new("git").args(["diff", "feat/b...feat/c", "--"])
                .current_dir(path).output().unwrap().stdout
        ).unwrap();
        assert_eq!(post_splice_diff, pre_splice_diff,
            "C's semantic diff should be unchanged after splice rebase");
    }

    // DW1: rebase_stack does not checkout any branch in the main worktree
    #[test]
    fn rebase_stack_does_not_checkout_in_main_worktree() {
        use std::process::Command;
        use tempfile::TempDir;
        use std::fs;

        let base = TempDir::new().unwrap();
        let path = base.path().join("repo");
        fs::create_dir(&path).unwrap();
        let remote_dir = base.path().join("remote");
        fs::create_dir(&remote_dir).unwrap();

        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(&remote_dir).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&path).output().unwrap();
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.to_str().unwrap()]);

        fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        git(&["checkout", "-b", "feat/wt-test"]);
        fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/wt-test"]);

        // Return to main before calling rebase_stack
        git(&["checkout", "main"]);

        let head_before = String::from_utf8(
            Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        let branches = vec!["feat/wt-test".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/wt-test".to_string(), None);
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/wt-test".to_string(), "main".to_string());

        let result = rebase_stack(&branches, &parent_of, &base_of, &path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);

        let head_after = String::from_utf8(
            Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&path).output().unwrap().stdout
        ).unwrap().trim().to_string();

        assert_eq!(head_before, head_after,
            "main worktree HEAD must not change during rebase_stack, was {} before, {} after",
            head_before, head_after);
    }

    // DW2: rebase_stack creates a git worktree for each branch
    #[test]
    fn rebase_stack_creates_worktree_for_branch() {
        use std::process::Command;
        use tempfile::TempDir;
        use std::fs;

        let base = TempDir::new().unwrap();
        let path = base.path().join("repo");
        fs::create_dir(&path).unwrap();
        let remote_dir = base.path().join("remote");
        fs::create_dir(&remote_dir).unwrap();

        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(&remote_dir).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&path).output().unwrap();
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["remote", "add", "origin", remote_dir.to_str().unwrap()]);

        fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "origin", "main"]);

        git(&["checkout", "-b", "feat/wt-create"]);
        fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "B"]);
        git(&["push", "--set-upstream", "origin", "feat/wt-create"]);
        git(&["checkout", "main"]);

        let branches = vec!["feat/wt-create".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/wt-create".to_string(), None);
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/wt-create".to_string(), "main".to_string());

        let result = rebase_stack(&branches, &parent_of, &base_of, &path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "expected no conflicts: {:?}", result.conflicts);

        let wt = crate::worktree::worktree_path(&path, "feat/wt-create");
        assert!(wt.exists(), "worktree must be created at {:?}", wt);
    }
}
