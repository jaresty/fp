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

        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        parent_of.insert("feat/top".to_string(), Some("feat/base".to_string()));

        let result = rebase_stack(&branches, &parent_of, path).unwrap();
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

        // After rebase_stack, feat/top should be rebased onto feat/base
        let branches = vec!["feat/base".to_string(), "feat/top".to_string()];
        let mut parent_of = std::collections::HashMap::new();
        parent_of.insert("feat/base".to_string(), None);
        parent_of.insert("feat/top".to_string(), Some("feat/base".to_string()));

        let result = rebase_stack(&branches, &parent_of, path).unwrap();
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
}
