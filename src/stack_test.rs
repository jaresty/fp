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
        let parent_of = detect_parent_of(&branches, path, &HashMap::new()).unwrap();

        // feat/base should have no parent in our branch set (its parent is main, not in set)
        assert_eq!(parent_of.get("feat/base"), Some(&None));
        // feat/top's parent should be feat/base
        assert_eq!(parent_of.get("feat/top"), Some(&Some("feat/base".to_string())));
    }

    // DP2: detect_parent_of identifies force-pushed parent even though its new tip is not in child's history
    #[test]
    fn detect_parent_of_finds_parent_after_force_push() {
        use std::process::Command;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(path)
                .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
                .output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);

        // root commit on main
        std::fs::write(path.join("root.txt"), "root").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "root"]);

        // grandparent: branch from main, add commit GP
        git(&["checkout", "-b", "grandparent"]);
        std::fs::write(path.join("gp.txt"), "gp").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "GP"]);

        // parent: branch from grandparent, add commit P
        git(&["checkout", "-b", "parent"]);
        std::fs::write(path.join("p.txt"), "p").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P"]);

        // child: branch from parent, add 2 commits C1, C2
        git(&["checkout", "-b", "child"]);
        std::fs::write(path.join("c1.txt"), "c1").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C1"]);
        std::fs::write(path.join("c2.txt"), "c2").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C2"]);

        // Simulate force-push of parent: reset parent to a new commit on grandparent (different SHA)
        // This is what happens when parent is rebased: its tip changes, child still has old tip
        git(&["checkout", "parent"]);
        git(&["reset", "--hard", "grandparent"]);
        std::fs::write(path.join("p_new.txt"), "p-rebased").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P-rebased"]);
        // Now parent's new tip is NOT in child's history (force-pushed)
        // child still contains old P commit in its history

        git(&["checkout", "child"]);

        let branches = vec!["grandparent".to_string(), "parent".to_string(), "child".to_string()];
        let parent_of = detect_parent_of(&branches, path, &HashMap::new()).unwrap();

        // child's parent should be "parent", not "grandparent" — even though parent was force-pushed
        assert_eq!(
            parent_of.get("child"),
            Some(&Some("parent".to_string())),
            "detect_parent_of must identify force-pushed 'parent' as child's parent, not grandparent. got: {:?}",
            parent_of.get("child")
        );
    }

    // DP3: detect_parent_of uses declared base_of over git topology.
    // Topology: main -> grandparent -> child (depth 2 from grandparent, depth 1 from parent NOT in history).
    // parent is force-pushed to a new commit NOT in child's history.
    // Without base_of: topology picks grandparent (depth 2 from mb, but grandparent is direct ancestor).
    // Wait — that's the DP2 test. Here we use a simpler case:
    // main -> grandparent (2 commits) -> child (1 commit from grandparent).
    // parent is a sibling of grandparent (also from main), not in child's ancestry.
    // Topology: grandparent wins (it IS in child's ancestry with depth 1).
    // Declared base_of: child -> parent (the sibling).
    // Test: with base_of, parent wins over the topologically-correct grandparent.
    #[test]
    fn detect_parent_of_uses_declared_base_over_topology() {
        use std::process::Command;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                   ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        let git = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(path);
            for (k, v) in &env { c.env(k, v); }
            c.output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["commit", "--allow-empty", "-m", "root"]);
        // grandparent: 1 commit from main
        git(&["checkout", "-b", "grandparent"]);
        git(&["commit", "--allow-empty", "-m", "gp"]);
        // child: 1 commit from grandparent (topology: grandparent is direct parent)
        git(&["checkout", "-b", "child"]);
        git(&["commit", "--allow-empty", "-m", "child"]);
        // parent: branch from main (sibling of grandparent, NOT in child's ancestry)
        git(&["checkout", "main"]);
        git(&["checkout", "-b", "parent"]);
        git(&["commit", "--allow-empty", "-m", "p"]);

        let branches = vec!["grandparent".to_string(), "parent".to_string(), "child".to_string()];

        // Without base_of: topology must pick grandparent (it IS child's direct git ancestor).
        let parent_of_topo = detect_parent_of(&branches, path, &HashMap::new()).unwrap();
        assert_eq!(
            parent_of_topo.get("child"),
            Some(&Some("grandparent".to_string())),
            "topology alone must pick grandparent as child's parent, got: {:?}",
            parent_of_topo.get("child")
        );

        // With base_of declaring child -> parent: declared must win over topology.
        let mut base_of = HashMap::new();
        base_of.insert("child".to_string(), "parent".to_string());
        let parent_of = detect_parent_of(&branches, path, &base_of).unwrap();
        assert_eq!(
            parent_of.get("child"),
            Some(&Some("parent".to_string())),
            "declared base_of must override topology — expected parent, got: {:?}",
            parent_of.get("child")
        );
    }

    // RS0: main_repo_root returns an absolute, existing directory
    #[test]
    fn main_repo_root_returns_absolute_path() {
        let cwd = std::env::current_dir().unwrap();
        let dir = crate::worktree::main_repo_root(&cwd).unwrap();
        assert!(dir.is_absolute(), "main_root must be absolute, got: {:?}", dir);
        assert!(dir.is_dir(), "main_root must be an existing directory, got: {:?}", dir);
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

        // Simulate an in-progress rebase by creating the rebase-merge directory
        std::fs::create_dir_all(path.join(".git").join("rebase-merge")).unwrap();

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
    // RS_ROOT2: root branch with TWO own commits must retain both after rebase when main advances.
    // Bug: the --onto logic fired incorrectly for root branches (parent = origin/main).
    // origin/main is not in the branch's ancestry after advancing, so parent_is_ancestor=false,
    // causing old_upstream = oldest own commit and replaying only the newer commit.
    #[test]
    fn rebase_stack_root_branch_retains_all_commits_when_main_advances() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                   ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        let git_env = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(path);
            for (k,v) in &env { c.env(k,v); }
            c.output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: commit M
        std::fs::write(path.join("m.txt"), "m").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // feat/work: TWO own commits (C1 then C2) on top of main
        git(&["checkout", "-b", "feat/work"]);
        std::fs::write(path.join("c1.txt"), "c1").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "C1"]);
        std::fs::write(path.join("c2.txt"), "c2").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "C2"]);
        git(&["push", "-u", "origin", "feat/work"]);

        // main advances (X) — origin/main is no longer in feat/work's ancestry
        git(&["checkout", "main"]);
        std::fs::write(path.join("x.txt"), "x").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "X"]);
        git(&["push", "origin", "main"]);
        git(&["checkout", "main"]);

        let parent_of: std::collections::HashMap<String, Option<String>> =
            [("feat/work".to_string(), None)].into_iter().collect();
        let base_of: std::collections::HashMap<String, String> =
            [("feat/work".to_string(), "main".to_string())].into_iter().collect();

        let result = rebase_stack(
            &["feat/work".to_string()], &parent_of, &base_of, path, &|_| {}
        ).unwrap();
        assert!(result.conflicts.is_empty(), "no conflicts expected: {:?}", result.conflicts);

        // feat/work must have BOTH C1 and C2 on top of new main
        let log = Command::new("git")
            .args(["log", "--oneline", "origin/main..feat/work"])
            .current_dir(path).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        let count = log_str.trim().lines().count();
        assert_eq!(count, 2,
            "feat/work must retain both C1 and C2, got: {}", log_str);
        assert!(log_str.contains(" C1"), "C1 must be present: {}", log_str);
        assert!(log_str.contains(" C2"), "C2 must be present: {}", log_str);
    }

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
    // RS14: rebase_stack rerun after parent conflict-resolution: child should not replay parent commits.
    // Scenario (rerun case):
    //   1. fp rebase-stack → feat/base conflicts with new main → user resolves → A' (different patch)
    //   2. feat/base is now at A' (rebased, pushed). feat/child is still based on old A_orig.
    //   3. fp rebase-stack reruns with both branches.
    //      feat/base: already on origin/main → no-op.
    //      feat/child: plain `git rebase feat/base` finds merge-base=M, replays A_orig+B → CONFLICT on A_orig.
    //      With --onto fix: detects feat/base is not ancestor of feat/child, finds A_orig as oldest
    //      exclusive commit, uses `git rebase --onto feat/base A_orig` → replays only B → SUCCESS.
    #[test]
    fn rebase_stack_rerun_after_parent_conflict_resolution() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();
        let git_env = |args: &[&str], env: &[(&str,&str)]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(path);
            for (k,v) in env { c.env(k,v); }
            c.output().unwrap()
        };
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                   ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main M: base.txt exists (so there's a file for the conflict), b.txt absent
        std::fs::write(path.join("base.txt"), "original_content\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M"], &env);
        git(&["push", "-u", "origin", "main"]);

        // feat/base A_orig: modifies base.txt in a way that will conflict with M2
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("base.txt"), "original_content\nA_addition\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "A"], &env);
        git(&["push", "-u", "origin", "feat/base"]);

        // feat/child B: creates an entirely separate file (no dependency on base.txt content)
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("base.txt"), "original_content\nA_addition\n").unwrap();
        std::fs::write(path.join("b.txt"), "B_content\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "B"], &env);
        git(&["push", "-u", "origin", "feat/child"]);

        // main M2: changes base.txt in a way that conflicts with A_orig's context
        git(&["checkout", "main"]);
        std::fs::write(path.join("base.txt"), "updated_content\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M2"], &env);
        git(&["push", "origin", "main"]);

        // === Simulate "previous fp rebase-stack run that conflicted on feat/base" ===
        // User manually rebased feat/base with conflict resolution (keeping A_addition).
        // This produces A' with DIFFERENT patch-id than A_orig (conflict changed context).
        git(&["checkout", "feat/base"]);
        git(&["fetch", "origin"]);
        let rebase_out = git(&["rebase", "origin/main"]);
        if !rebase_out.status.success() {
            // Resolve: keep A_addition under new updated_content
            std::fs::write(path.join("base.txt"), "updated_content\nA_addition\n").unwrap();
            git(&["add", "base.txt"]);
            git_env(&["rebase", "--continue"], &[
                ("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t"),
                ("GIT_EDITOR","true"),
            ]);
        }
        git(&["push", "--force-with-lease", "origin", "feat/base"]);

        // Back to main for worktree creation
        git(&["checkout", "main"]);

        // === Rerun: call rebase_stack with BOTH branches (as fp rebase-stack would) ===
        // feat/base is already rebased (A'), no-op rebase expected.
        // feat/child is still based on A_orig — this is what needs --onto to succeed.
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/base".to_string(), None),
            ("feat/child".to_string(), Some("feat/base".to_string())),
        ].into_iter().collect();
        let base_of: std::collections::HashMap<String, String> = [
            ("feat/base".to_string(), "main".to_string()),
            ("feat/child".to_string(), "main".to_string()),
        ].into_iter().collect();
        let branches = vec!["feat/base".to_string(), "feat/child".to_string()];

        let result = crate::stack::rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(),
            "rerun should succeed with no conflicts: {:?}", result.conflicts);

        // feat/child should have exactly 1 commit on top of feat/base: B (not A+B)
        let log = Command::new("git")
            .args(["log", "--oneline", "feat/base..feat/child"])
            .current_dir(path).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        let commit_count = log_str.trim().lines().count();
        assert_eq!(commit_count, 1,
            "feat/child should have exactly 1 commit on top of feat/base (B only), got: {}", log_str);
        assert!(log_str.contains(" B"), "the one commit should be B, got: {}", log_str);
    }

    // RS13: rebase_stack uses --onto <parent> <pre_parent_sha> to avoid replaying parent commits
    // when the parent was rebased with conflict resolution (changing its patch-id).
    // Without --onto, git falls back to merge-base(main), tries to replay A+B, and A conflicts
    // because parent already contains a modified version A' with a different patch.
    #[test]
    fn rebase_stack_uses_onto_when_parent_was_force_pushed() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();
        let git_env = |args: &[&str], env: &[(&str,&str)]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(path);
            for (k,v) in env { c.env(k,v); }
            c.output().unwrap()
        };
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: shared.txt = "line1\nline2\n"
        std::fs::write(path.join("shared.txt"), "line1\nline2\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M"], &env);
        git(&["push", "-u", "origin", "main"]);

        // feat/base: appends "A\n" to shared.txt (commit A)
        git(&["checkout", "-b", "feat/base"]);
        std::fs::write(path.join("shared.txt"), "line1\nline2\nA\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "A"], &env);
        git(&["push", "-u", "origin", "feat/base"]);

        // feat/child stacked on feat/base: appends "B\n" (commit B)
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("shared.txt"), "line1\nline2\nA\nB\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "B"], &env);
        git(&["push", "-u", "origin", "feat/child"]);

        // main advances: inserts "X\n" between line1 and line2 — causes conflict on rebase
        git(&["checkout", "main"]);
        std::fs::write(path.join("shared.txt"), "line1\nX\nline2\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M2"], &env);
        git(&["push", "origin", "main"]);

        // Rebase feat/base onto new main WITH CONFLICT RESOLUTION.
        // The "A" commit originally added "A\n" after "line2\n".
        // After conflict resolution the file becomes "line1\nX\nline2\nA\n" — different patch than original.
        git(&["checkout", "feat/base"]);
        git(&["fetch", "origin"]);
        // Start the rebase — it will conflict on shared.txt
        let rebase_out = git(&["rebase", "origin/main"]);
        if !rebase_out.status.success() {
            // Resolve conflict: accept the "A\n" line after new main content
            std::fs::write(path.join("shared.txt"), "line1\nX\nline2\nA\n").unwrap();
            git(&["add", "shared.txt"]);
            git_env(&["rebase", "--continue"], &[
                ("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t"),
                ("GIT_EDITOR","true"),
            ]);
        }
        git(&["push", "--force-with-lease", "origin", "feat/base"]);

        // Return to main so worktrees can be created
        git(&["checkout", "main"]);

        // Only rebase feat/child — feat/base is already done above
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/child".to_string(), Some("feat/base".to_string())),
        ].into_iter().collect();
        let base_of: std::collections::HashMap<String, String> = [
            ("feat/child".to_string(), "main".to_string()),
        ].into_iter().collect();
        let branches = vec!["feat/child".to_string()];

        let result = crate::stack::rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(), "no conflicts expected: {:?}", result.conflicts);

        // feat/child should have exactly 1 commit on top of feat/base: B (not A+B)
        let log = Command::new("git")
            .args(["log", "--oneline", "feat/base..feat/child"])
            .current_dir(path).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        let commit_count = log_str.trim().lines().count();
        assert_eq!(commit_count, 1,
            "feat/child should have exactly 1 commit on top of feat/base (B only), got: {}", log_str);
        assert!(log_str.contains(" B"), "the one commit should be B, got: {}", log_str);
    }

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

    // RS15: rebase_stack uses fork-point to find old_upstream when parent has multiple
    // rewritten commits AND P2_old's patch genuinely conflicts with P2' state.
    // Scenario: P1_old adds "runner_v1\n"; P2_old renames it to "runner_v2\n".
    // Conflict resolution on P1: resolves to "runner_resolved\n" (different from runner_v1).
    // P2_old tries to rename "runner_v1\n" → "runner_v2\n" but the file has "runner_resolved\n"
    // → CONFLICT when replaying wrong old_upstream (P1_old).
    // With --fork-point old_upstream = P2_old (old feat/child base), only C replayed → clean.
    #[test]
    fn rebase_stack_fully_rebased_parent_with_conflict_resolution_replays_only_child_commit() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                   ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        let git_env = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(path);
            for (k, v) in &env { c.env(k, v); }
            c.output().unwrap()
        };
        let git_env_cont = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(args).current_dir(path)
                .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
                .env("GIT_EDITOR", "true");
            c.output().unwrap()
        };

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // main: M — runner.sh = "base\n"
        std::fs::write(path.join("runner.sh"), "base\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // feat/parent: P1 adds runner_v1; P2 renames runner_v1 to runner_v2
        git(&["checkout", "-b", "feat/parent"]);
        // P1: adds runner_v1 line
        std::fs::write(path.join("runner.sh"), "base\nrunner_v1\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "P1"]);
        // P2: renames runner_v1 → runner_v2
        std::fs::write(path.join("runner.sh"), "base\nrunner_v2\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "P2"]);
        git(&["push", "-u", "origin", "feat/parent"]);

        // feat/child: C only adds c.txt (no runner.sh dependency)
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("c.txt"), "c\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "C"]);
        git(&["push", "-u", "origin", "feat/child"]);

        // main advances: M2 changes runner.sh context so P1 conflicts
        // M2: changes "base\n" to "base\nupdated\n"
        git(&["checkout", "main"]);
        std::fs::write(path.join("runner.sh"), "base\nupdated\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M2"]);
        git(&["push", "origin", "main"]);

        // Rebase feat/parent onto new main with conflict resolution.
        // P1 patch (base\n→base\nrunner_v1\n) conflicts with M2 (base\n→base\nupdated\n).
        // Resolution: "base\nupdated\nrunner_resolved\n" — runner_v1 renamed to runner_resolved.
        // P2 patch (runner_v1→runner_v2) now conflicts (runner_v1 not present).
        // Resolution for P2: keep runner_v2 as the final name.
        git(&["checkout", "feat/parent"]);
        git(&["fetch", "origin"]);
        let rebase_out = git(&["rebase", "origin/main"]);
        if !rebase_out.status.success() {
            // Resolve P1: keep updated and use runner_resolved instead of runner_v1
            std::fs::write(path.join("runner.sh"), "base\nupdated\nrunner_resolved\n").unwrap();
            git(&["add", "runner.sh"]);
            let cont = git_env_cont(&["rebase", "--continue"]);
            if !cont.status.success() {
                // Resolve P2: rename runner_resolved to runner_v2 (or just set runner_v2)
                std::fs::write(path.join("runner.sh"), "base\nupdated\nrunner_v2\n").unwrap();
                git(&["add", "runner.sh"]);
                git_env_cont(&["rebase", "--continue"]);
            }
        }
        git(&["push", "--force-with-lease", "origin", "feat/parent"]);

        // Verify feat/parent now has 2 commits on top of new main
        let parent_log = Command::new("git")
            .args(["log", "--oneline", "origin/main..feat/parent"])
            .current_dir(path).output().unwrap();
        let parent_log_str = String::from_utf8_lossy(&parent_log.stdout);
        assert_eq!(parent_log_str.trim().lines().count(), 2,
            "feat/parent should have 2 commits on top of new main, got: {}", parent_log_str);

        // Return to main for worktree creation
        git(&["checkout", "main"]);

        // Rebase feat/child: parent = feat/parent (both commits fully rewritten)
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/child".to_string(), Some("feat/parent".to_string())),
        ].into_iter().collect();
        let base_of: std::collections::HashMap<String, String> = [
            ("feat/child".to_string(), "main".to_string()),
        ].into_iter().collect();
        let branches = vec!["feat/child".to_string()];

        let result = crate::stack::rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(),
            "no conflicts expected — only C should be replayed via fork-point, got: {:?}", result.conflicts);

        // feat/child must have exactly 1 commit on top of feat/parent (C only)
        let log = Command::new("git")
            .args(["log", "--oneline", "feat/parent..feat/child"])
            .current_dir(path).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        let commit_count = log_str.trim().lines().count();
        assert_eq!(commit_count, 1,
            "feat/child should have exactly 1 commit on top of feat/parent (C only), got: {}", log_str);
        assert!(log_str.contains(" C"), "the one commit should be C, got: {}", log_str);
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

    // RS-REMOTE: rebase_stack uses --onto when parent is a remote tracking branch that was force-pushed.
    // Uses a rename-then-modify pattern so git cannot "already applied" skip the old parent commit —
    // the conflict resolution renames the content, making the old patch non-applicable and producing
    // a real conflict if --onto is not used.
    #[test]
    fn rebase_stack_onto_when_remote_parent_force_pushed() {
        use std::process::Command;
        use tempfile::TempDir;

        let remote_dir = TempDir::new().unwrap();
        Command::new("git").args(["init", "--bare", "-b", "main"])
            .current_dir(remote_dir.path()).output().unwrap();

        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(path).output().unwrap();
        let git_env = |args: &[&str], env: &[(&str, &str)]| {
            let mut c = Command::new("git"); c.args(args).current_dir(path);
            for (k, v) in env { c.env(k, v); } c.output().unwrap()
        };
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        git(&["remote", "add", "origin", remote_dir.path().to_str().unwrap()]);

        // M: runner.sh = "#!/bin/bash\nrun_v1()\n"
        std::fs::write(path.join("runner.sh"), "#!/bin/bash\nrun_v1()\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M"], &env);
        git(&["push", "-u", "origin", "main"]);

        // feat/parent P1: renames run_v1 → run_v2
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(path.join("runner.sh"), "#!/bin/bash\nrun_v2()\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "P1-rename-v1-to-v2"], &env);
        git(&["push", "-u", "origin", "feat/parent"]);

        // feat/child stacked on feat/parent: adds c.txt (unique, no conflict with runner.sh)
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(path.join("c.txt"), "child only\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "C"], &env);
        git(&["push", "-u", "origin", "feat/child"]);

        // main advances: also renames run_v1 → run_v3 (different name → conflict with P1)
        git(&["checkout", "main"]);
        std::fs::write(path.join("runner.sh"), "#!/bin/bash\nrun_v3()\n").unwrap();
        git(&["add", "."]);
        git_env(&["commit", "-m", "M2-rename-v1-to-v3"], &env);
        git(&["push", "origin", "main"]);

        // Rebase feat/parent onto new main with conflict resolution:
        // resolution uses run_resolved() — changes the content, so old P1's patch won't "already apply"
        git(&["checkout", "feat/parent"]);
        git(&["fetch", "origin"]);
        let r = git(&["rebase", "origin/main"]);
        if !r.status.success() {
            std::fs::write(path.join("runner.sh"), "#!/bin/bash\nrun_resolved()\n").unwrap();
            git(&["add", "runner.sh"]);
            git_env(&["rebase", "--continue"], &[
                ("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),
                ("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t"),
                ("GIT_EDITOR","true"),
            ]);
        }
        git(&["push", "--force-with-lease", "origin", "feat/parent"]);
        git(&["fetch", "origin"]);  // update origin/feat/parent reflog

        // Return to main
        git(&["checkout", "main"]);

        // Rebase feat/child with REMOTE parent origin/feat/parent
        let parent_of: std::collections::HashMap<String, Option<String>> = [
            ("feat/child".to_string(), Some("origin/feat/parent".to_string())),
        ].into_iter().collect();
        let base_of: std::collections::HashMap<String, String> = [
            ("feat/child".to_string(), "main".to_string()),
        ].into_iter().collect();
        let branches = vec!["feat/child".to_string()];

        let result = rebase_stack(&branches, &parent_of, &base_of, path, &|_| {}).unwrap();
        assert!(result.conflicts.is_empty(),
            "no conflicts expected when rebasing child onto force-pushed remote parent, got: {:?}", result.conflicts);

        // feat/child should have exactly 1 commit on top of origin/feat/parent: C only, not P1+C
        let log = Command::new("git")
            .args(["log", "--oneline", "origin/feat/parent..feat/child"])
            .current_dir(path).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        let commit_count = log_str.trim().lines().count();
        assert_eq!(commit_count, 1,
            "feat/child must have exactly 1 commit on top of origin/feat/parent (C only), got: {}", log_str);
    }

    #[test]
    fn stack_governs_stack_tree_order() {
        use crate::store::PrCache;
        let root = PrCache { number: 1, title: "root".into(), branch: "feat/root".into(), base: "main".into() };
        let child = PrCache { number: 2, title: "child".into(), branch: "feat/child".into(), base: "feat/root".into() };
        let prs = vec![&root, &child];
        let order = crate::stack::stack_tree_order(&prs);
        assert_eq!(order[0].0, 1, "root must come first");
        assert_eq!(order[1].0, 2, "child must come second");
        assert!(order[1].1.contains("└─"), "child must have indent prefix, got: {:?}", order[1].1);
    }
}
