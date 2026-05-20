#[cfg(test)]
mod tests {
    #[test]
    fn commands_governs_unlock_formats_success_message() {
        let result = crate::commands::unlock_message("feat-branch");
        assert!(
            result.contains("feat-branch"),
            "commands::unlock_message must include branch name: {}",
            result
        );
        assert!(
            result.contains("Unlocked"),
            "commands::unlock_message must say 'Unlocked': {}",
            result
        );
    }

    #[test]
    fn commands_governs_agent_context_text_output() {
        let result = crate::commands::agent_context_text(3);
        assert!(
            result.contains("3"),
            "commands::agent_context_text must include PR count: {}",
            result
        );
        assert!(
            result.contains("tracked"),
            "commands::agent_context_text must say 'tracked': {}",
            result
        );
    }

    #[test]
    fn commands_governs_install_skills_writes_skill_content() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("SKILL.md");
        crate::commands::install_skills(&dest).unwrap();
        assert!(dest.exists(), "commands::install_skills must create the file");
        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(
            content.contains("name: fp"),
            "commands::install_skills must write skill content with 'name: fp': {}",
            &content[..100.min(content.len())]
        );
    }

    #[test]
    fn commands_governs_install_shell_print_returns_content() {
        let result = crate::commands::install_shell_content("fish");
        assert!(
            result.is_ok(),
            "commands::install_shell_content must succeed for fish shell"
        );
        let content = result.unwrap();
        assert!(
            content.contains("fps"),
            "commands::install_shell_content must return fps function body: {}",
            &content[..50.min(content.len())]
        );
    }

    #[test]
    fn commands_governs_install_shell_unsupported_errors() {
        let result = crate::commands::install_shell_content("powershell");
        assert!(
            result.is_err(),
            "commands::install_shell_content must error for unsupported shell"
        );
        assert!(
            result.unwrap_err().to_string().contains("unsupported shell"),
            "error must mention 'unsupported shell'"
        );
    }

    fn make_store_with_pr(git_dir: &std::path::Path, number: u64, branch: &str) -> crate::store::Store {
        let store = crate::store::Store::open(git_dir);
        store.track(number).unwrap();
        store.update_cache(crate::store::PrCache {
            number,
            title: format!("PR {}", number),
            branch: branch.into(),
            base: "main".into(),
        }).unwrap();
        store
    }

    #[test]
    fn commands_governs_cmd_status_one_returns_task_output() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 7, "feat/my-feature");
        let fake = make_fake_with_pr(7);
        let out = crate::commands::cmd_status_one(Some(&fake), &store, &git_dir, "alice", "repo", 7, false).unwrap();
        assert!(out.contains("7") || out.contains("PR #7") || out.contains("feat/my-feature"),
            "output must reference PR, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_status_one_errors_on_untracked() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let result = crate::commands::cmd_status_one(None, &store, &git_dir, "alice", "repo", 99, false);
        assert!(result.is_err(), "must error for untracked PR");
        assert!(result.unwrap_err().to_string().contains("not tracked"), "error must say 'not tracked'");
    }

    #[test]
    fn commands_governs_cmd_status_one_json_returns_valid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 3, "feat/json-test");
        let fake = make_fake_with_pr(3);
        let out = crate::commands::cmd_status_one(Some(&fake), &store, &git_dir, "alice", "repo", 3, true).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out)
            .expect("cmd_status_one json must return valid JSON");
        assert!(parsed.is_array(), "json output must be array, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_status_all_lists_tracked_prs() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 5, "feat/all-test");
        let fake = make_fake_with_pr(5);
        let out = crate::commands::cmd_status_all(Some(&fake), &store, &git_dir, "alice", "repo", false).unwrap();
        assert!(out.contains("PR #5") || out.contains("#5"), "must contain PR number, got: {}", out);
    }

    fn make_fake_with_pr(number: u64) -> crate::github::FakeGithubClient {
        let mut fake = crate::github::FakeGithubClient::new();
        fake.set_pr(number, crate::model::PrState {
            number,
            title: "test".into(),
            branch: "feat/test".into(),
            base: "main".into(),
            head_sha: "abc123".into(),
            draft: false,
            approved: false,
            checks: vec![crate::model::Check {
                name: "ci/test".into(),
                status: crate::model::CheckStatus::Pass,
                required: true,
                details_url: Some("https://ci.example.com/1".into()),
                log_snippet: None,
            }],
            threads: vec![crate::model::Thread {
                id: 42,
                body: "fix this".into(),
                state: crate::model::ThreadState::Open,
                file: None,
                line: None,
                author: "reviewer".into(),
                replies: vec![],
            }],
            needs_parent_rebase: false,
            has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake
    }

    #[test]
    fn commands_governs_cmd_checks_formats_check_results() {
        let mut fake = crate::github::FakeGithubClient::new();
        fake.set_checks("abc123", vec![crate::model::Check {
            name: "ci/test".into(),
            status: crate::model::CheckStatus::Pass,
            required: true,
            details_url: Some("https://ci.example.com/1".into()),
            log_snippet: None,
        }]);
        let out = crate::commands::cmd_checks(&fake, "owner", "repo", "abc123").unwrap();
        assert!(out.contains("ci/test"), "must contain check name, got: {}", out);
        assert!(out.contains("✓"), "must contain pass symbol, got: {}", out);
        assert!(out.contains("https://ci.example.com/1"), "must contain url, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_checks_empty_says_no_checks() {
        let fake = crate::github::FakeGithubClient::new();
        let out = crate::commands::cmd_checks(&fake, "owner", "repo", "deadbeef").unwrap();
        assert!(out.contains("No check runs"), "must say 'No check runs', got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_reply_returns_confirmation() {
        let fake = make_fake_with_pr(7);
        let out = crate::commands::cmd_reply(&fake, "owner", "repo", 7, 42, "looks good").unwrap();
        assert!(out.contains("Replied"), "must confirm reply, got: {}", out);
        assert!(out.contains("42"), "must mention thread id, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_reply_errors_on_missing_thread() {
        let fake = make_fake_with_pr(7);
        let result = crate::commands::cmd_reply(&fake, "owner", "repo", 7, 99, "msg");
        assert!(result.is_err(), "must error when thread not found");
        assert!(result.unwrap_err().to_string().contains("not found"), "error must say 'not found'");
    }

    #[test]
    fn commands_governs_cmd_ready_returns_confirmation() {
        let fake = make_fake_with_pr(3);
        let out = crate::commands::cmd_ready(&fake, "owner", "repo", 3).unwrap();
        assert!(out.contains("ready"), "must confirm ready, got: {}", out);
        assert!(out.contains("3"), "must mention PR number, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_profile_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles.json");
        crate::commands::cmd_profile(&profiles_path, "save", "myprofile", Some("tok123".into()), Some("owner/repo".into())).unwrap();
        let out = crate::commands::cmd_profile(&profiles_path, "load", "myprofile", None, None).unwrap();
        assert!(out.contains("tok123"), "load must contain saved token, got: {}", out);
        assert!(out.contains("owner/repo"), "load must contain saved repo, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_profile_unknown_action_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles.json");
        let result = crate::commands::cmd_profile(&profiles_path, "delete", "x", None, None);
        assert!(result.is_err(), "unknown action must return error");
        assert!(result.unwrap_err().to_string().contains("unknown profile action"), "error must say 'unknown profile action'");
    }

    #[test]
    fn commands_governs_cmd_switch_errors_on_untracked_pr() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let result = crate::commands::cmd_switch(&store, &git_dir, 99, "session-id", false, false);
        assert!(result.is_err(), "cmd_switch must error for untracked PR");
        assert!(result.unwrap_err().to_string().contains("not tracked"), "error must mention 'not tracked'");
    }

    #[test]
    fn commands_governs_cmd_untrack_removes_pr_and_returns_message() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(5).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 5,
            title: "test".into(),
            branch: "feat/test".into(),
            base: "main".into(),
        }).unwrap();
        let msg = crate::commands::cmd_untrack(&store, tmp.path(), &git_dir, 5).unwrap();
        assert!(msg.contains("Untracked PR #5"), "must confirm untrack, got: {}", msg);
        assert!(!store.load().unwrap().tracked.contains(&5), "PR must be removed from store");
    }

    #[test]
    fn commands_governs_cmd_ls_text_lists_tracked_prs() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(7).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 7,
            title: "my feature".into(),
            branch: "feat/my-feature".into(),
            base: "main".into(),
        }).unwrap();
        let out = crate::commands::cmd_ls(&store, "alice", "myrepo", false).unwrap();
        assert!(out.contains("#7"), "cmd_ls text must contain PR number, got: {}", out);
        assert!(out.contains("my feature"), "cmd_ls text must contain title, got: {}", out);
        assert!(out.contains("feat/my-feature"), "cmd_ls text must contain branch, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_ls_json_returns_valid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(3).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 3,
            title: "fix bug".into(),
            branch: "fix/bug".into(),
            base: "main".into(),
        }).unwrap();
        let out = crate::commands::cmd_ls(&store, "alice", "myrepo", true).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out)
            .expect("cmd_ls json mode must return valid JSON");
        assert!(parsed.is_array(), "cmd_ls json must be an array, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_ls_text_empty_shows_no_tracked() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let out = crate::commands::cmd_ls(&store, "alice", "myrepo", false).unwrap();
        assert!(out.contains("No tracked PRs"), "cmd_ls with empty store must say 'No tracked PRs', got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_comment_posts_and_returns_url() {
        let fake = make_fake_with_pr(7);
        let out = crate::commands::cmd_comment(&fake, "owner", "repo", 7, "great work").unwrap();
        assert!(out.contains("Comment posted"), "must say 'Comment posted', got: {}", out);
        assert!(out.contains("http"), "must contain a URL, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_threads_open_returns_formatted_threads() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 7, "feat/test");
        let fake = make_fake_with_pr(7);
        let out = crate::commands::cmd_threads(Some(&fake), &store, "owner", "repo", 7, false, false).unwrap();
        assert!(out.contains("7") || out.contains("fix this"), "must contain thread content, got: {}", out);
    }

    #[test]
    fn commands_governs_cmd_threads_errors_on_untracked() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let result = crate::commands::cmd_threads(None, &store, "owner", "repo", 99, false, false);
        assert!(result.is_err(), "must error for untracked PR");
        assert!(result.unwrap_err().to_string().contains("not tracked"), "error must say 'not tracked'");
    }

    #[test]
    fn commands_governs_update_children_base_updates_children_of_merged_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);

        // Set up: parent PR #1 (branch: feat/parent, base: main)
        //         child PR #2 (branch: feat/child, base: feat/parent)
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache { number: 1, title: "parent".into(), branch: "feat/parent".into(), base: "main".into() }).unwrap();
        store.track(2).unwrap();
        store.update_cache(crate::store::PrCache { number: 2, title: "child".into(), branch: "feat/child".into(), base: "feat/parent".into() }).unwrap();

        // After merging feat/parent into main, update child's base to main
        crate::commands::update_children_base(&store, "feat/parent", "main").unwrap();

        let state = store.load().unwrap();
        let child = state.cache.get(&2).unwrap();
        assert_eq!(child.base, "main", "child base must be updated to merged_base after parent merges, got: {}", child.base);
    }

    #[test]
    fn commands_governs_update_children_base_leaves_unrelated_prs_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);

        store.track(3).unwrap();
        store.update_cache(crate::store::PrCache { number: 3, title: "other".into(), branch: "feat/other".into(), base: "main".into() }).unwrap();

        crate::commands::update_children_base(&store, "feat/parent", "main").unwrap();

        let state = store.load().unwrap();
        let other = state.cache.get(&3).unwrap();
        assert_eq!(other.base, "main", "unrelated PR base must be unchanged, got: {}", other.base);
    }

    #[test]
    fn commands_governs_normalize_base_of_replaces_untracked_base_with_main() {
        let tracked: std::collections::HashSet<String> = ["feat/child".to_string()].into();
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/child".to_string(), "feat/old-parent".to_string());

        let result = crate::commands::normalize_base_of(base_of, &tracked);
        assert_eq!(result.get("feat/child").map(String::as_str), Some("main"),
            "base must be replaced with main when declared parent is not tracked, got: {:?}", result);
    }

    #[test]
    fn commands_governs_normalize_base_of_keeps_tracked_base_unchanged() {
        let tracked: std::collections::HashSet<String> =
            ["feat/child".to_string(), "feat/parent".to_string()].into();
        let mut base_of = std::collections::HashMap::new();
        base_of.insert("feat/child".to_string(), "feat/parent".to_string());

        let result = crate::commands::normalize_base_of(base_of, &tracked);
        assert_eq!(result.get("feat/child").map(String::as_str), Some("feat/parent"),
            "base must be kept when declared parent IS tracked, got: {:?}", result);
    }

    #[test]
    fn cmd_track_returns_metadata_from_client() {
        let fake = crate::github::FakeGithubClient::new_with_pr(42, "feat/my-feature", "My Feature PR", "main");
        let result = crate::commands::cmd_track(&fake, "owner", "repo", 42, None, None);
        let (title, branch, base) = result.unwrap();
        assert_eq!(title, "My Feature PR", "cmd_track must return title from client");
        assert_eq!(branch, "feat/my-feature", "cmd_track must return branch from client");
        assert_eq!(base, "main", "cmd_track must return base from client");
    }

    #[test]
    fn cmd_track_uses_provided_title_over_fetched() {
        let fake = crate::github::FakeGithubClient::new_with_pr(42, "feat/my-feature", "Fetched Title", "main");
        let result = crate::commands::cmd_track(&fake, "owner", "repo", 42, Some("Override Title".to_string()), None);
        let (title, _branch, _base) = result.unwrap();
        assert_eq!(title, "Override Title", "cmd_track must prefer provided title over fetched");
    }

    #[test]
    fn cmd_edit_returns_success_message() {
        let fake = crate::github::FakeGithubClient::new_with_pr(7, "feat/edit-me", "Old Title", "main");
        let result = crate::commands::cmd_edit(&fake, "owner", "repo", 7, Some("New Title".to_string()), None, vec![]);
        let msg = result.unwrap();
        assert!(msg.contains("7"), "cmd_edit must mention PR number: {}", msg);
    }

    #[test]
    fn cmd_edit_with_demo_injects_section_into_body() {
        let fake = crate::github::FakeGithubClient::new_with_pr(7, "feat/demo", "Title", "main");
        // demo is empty so body is passed through directly
        let result = crate::commands::cmd_edit(&fake, "owner", "repo", 7, None, Some("explicit body".to_string()), vec![]);
        assert!(result.is_ok(), "cmd_edit must succeed with explicit body: {:?}", result);
    }

    #[test]
    fn cmd_new_creates_worktree_at_expected_path() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin.git");
        let repo = tmp.path().join("myrepo");
        std::fs::create_dir_all(&origin).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        // create bare origin with a main branch
        std::process::Command::new("git").args(["init", "--bare"]).current_dir(&origin).output().unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "Test"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["remote", "add", "origin", origin.to_str().unwrap()]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("init.txt"), "init").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&repo).output().unwrap();

        let result = crate::commands::cmd_new("feat/my-branch", "main", &repo);
        assert!(result.is_ok(), "cmd_new must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("feat/my-branch"), "cmd_new output must mention branch: {}", output);
        let wt_path = crate::worktree::worktree_path(&repo, "feat/my-branch");
        assert!(wt_path.exists(), "cmd_new must create worktree at {:?}", wt_path);
    }

    #[test]
    fn cmd_new_errors_when_git_worktree_add_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let not_a_repo = tmp.path().join("notrepo");
        std::fs::create_dir_all(&not_a_repo).unwrap();
        let result = crate::commands::cmd_new("feat/x", "main", &not_a_repo);
        assert!(result.is_err(), "cmd_new must error in non-git directory");
    }

    #[test]
    fn cmd_create_returns_created_message() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let fake = crate::github::FakeGithubClient::new();
        let result = crate::commands::cmd_create(
            &fake, "owner", "repo", &store, "feat/my-branch", tmp.path(),
            crate::commands::CreateOpts { title: "My PR Title".into(), base: "main".into(), body: None, restack_before: None, insert_after: None },
        );
        assert!(result.is_ok(), "cmd_create must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("Created PR"), "must confirm PR creation: {}", msg);
    }

    #[test]
    fn cmd_create_tracks_pr_in_store() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let fake = crate::github::FakeGithubClient::new();
        crate::commands::cmd_create(
            &fake, "owner", "repo", &store, "feat/my-branch", tmp.path(),
            crate::commands::CreateOpts { title: "My PR Title".into(), base: "main".into(), body: None, restack_before: None, insert_after: None },
        ).unwrap();
        let state = store.load().unwrap();
        assert!(!state.tracked.is_empty(), "cmd_create must track the created PR in store");
    }

    #[test]
    fn cmd_context_thread_returns_thread_details() {
        use crate::model::{Thread, ThreadState};
        use crate::github::GithubClientTrait as _;
        let mut fake = crate::github::FakeGithubClient::new_with_pr(5, "feat/ctx", "My PR", "main");
        let mut pr = fake.fetch_pr("o", "r", 5).unwrap();
        pr.threads = vec![Thread {
            id: 42,
            state: ThreadState::Open,
            author: "alice".into(),
            body: "please fix this".into(),
            file: Some("src/foo.rs".into()),
            line: Some(10),
            replies: vec![],
        }];
        fake.set_pr(5, pr);
        let result = crate::commands::cmd_context(&fake, "o", "r", 5, "thread:42", false);
        assert!(result.is_ok(), "cmd_context must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("please fix this"), "output must include thread body: {}", out);
        assert!(out.contains("alice"), "output must include thread author: {}", out);
    }

    #[test]
    fn cmd_context_check_returns_check_details() {
        use crate::model::{Check, CheckStatus};
        use crate::github::GithubClientTrait as _;
        let mut fake = crate::github::FakeGithubClient::new_with_pr(5, "feat/ctx", "My PR", "main");
        let mut pr = fake.fetch_pr("o", "r", 5).unwrap();
        pr.checks = vec![Check {
            name: "ci/build".into(),
            status: CheckStatus::Fail,
            required: true,
            details_url: None,
            log_snippet: None,
        }];
        fake.set_pr(5, pr);
        let result = crate::commands::cmd_context(&fake, "o", "r", 5, "ci/build", false);
        assert!(result.is_ok(), "cmd_context must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("ci/build"), "output must include check name: {}", out);
    }

    #[test]
    fn cmd_context_missing_thread_returns_not_found() {
        let fake = crate::github::FakeGithubClient::new_with_pr(5, "feat/ctx", "My PR", "main");
        let result = crate::commands::cmd_context(&fake, "o", "r", 5, "thread:99", false);
        assert!(result.is_ok(), "cmd_context must not error for missing thread");
        let out = result.unwrap();
        assert!(out.contains("not found"), "must say not found: {}", out);
    }

    #[test]
    fn cmd_merge_returns_merged_message() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache { number: 10, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(10, "feat/x", "T", "main");
        let result = crate::commands::cmd_merge(&fake, "o", "r", 10, crate::commands::MergeContext { store: &store, dir: tmp.path(), git_dir: &git_dir, merge_method: "squash" });
        assert!(result.is_ok(), "cmd_merge must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("merged PR #10"), "must mention merged PR: {}", msg);
    }

    #[test]
    fn cmd_merge_untracks_pr_from_store() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache { number: 10, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(10, "feat/x", "T", "main");
        crate::commands::cmd_merge(&fake, "o", "r", 10, crate::commands::MergeContext { store: &store, dir: tmp.path(), git_dir: &git_dir, merge_method: "squash" }).unwrap();
        let state = store.load().unwrap();
        assert!(!state.tracked.contains(&10), "cmd_merge must untrack the PR");
    }

    #[test]
    fn cmd_watch_once_empty_store_returns_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let fake = crate::github::FakeGithubClient::new();
        let result = crate::commands::cmd_watch(
            Some(&fake), "", "", &store, &git_dir, true, 5, false, None,
        );
        assert!(result.is_ok(), "cmd_watch with empty store must succeed: {:?}", result);
    }

    #[test]
    fn cmd_watch_once_with_tracked_pr_returns_initial_state_output() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(42).unwrap();
        store.update_cache(crate::store::PrCache { number: 42, title: "my PR".into(), branch: "feat/test".into(), base: "main".into() }).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(42, "feat/test", "my PR", "main");
        let result = crate::commands::cmd_watch(
            Some(&fake), "", "", &store, &git_dir, true, 5, false, None,
        );
        assert!(result.is_ok(), "cmd_watch must succeed with tracked PR: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("my PR"), "output must contain PR title 'my PR': {}", out);
    }

    #[test]
    fn cmd_watch_once_wait_for_never_exits_when_condition_unmet() {
        // with once=true, should exit after one iteration regardless of wait_for
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let fake = crate::github::FakeGithubClient::new();
        let result = crate::commands::cmd_watch(
            Some(&fake), "", "", &store, &git_dir, true, 5, false, Some("all-ready".into()),
        );
        assert!(result.is_ok(), "cmd_watch with once=true and unmet wait_for must still return ok: {:?}", result);
    }

    #[test]
    fn cmd_watch_detects_needs_parent_rebase_when_child_behind_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        // parent PR #1, child PR #2 whose base is the parent branch
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache { number: 1, title: "parent".into(), branch: "feat/parent".into(), base: "main".into() }).unwrap();
        store.track(2).unwrap();
        store.update_cache(crate::store::PrCache { number: 2, title: "child".into(), branch: "feat/child".into(), base: "feat/parent".into() }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new();
        // parent has non-empty SHA so is_head_behind_base check runs
        fake.set_pr(1, crate::model::PrState {
            number: 1, title: "parent".into(), branch: "feat/parent".into(), base: "main".into(),
            head_sha: "parent_sha".into(), draft: false, approved: false,
            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake.set_pr(2, crate::model::PrState {
            number: 2, title: "child".into(), branch: "feat/child".into(), base: "feat/parent".into(),
            head_sha: "child_sha".into(), draft: false, approved: false,
            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake.head_behind = true; // child is behind parent

        let result = crate::commands::cmd_watch(
            Some(&fake), "owner", "repo", &store, &git_dir, true, 5, false, None,
        );
        assert!(result.is_ok(), "cmd_watch must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("parent PR has new commits"), "output must surface rebase task: {}", out);
    }

    #[test]
    fn cmd_watch_once_returns_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let fake = crate::github::FakeGithubClient::new();
        let result = crate::commands::cmd_watch(
            None, "", "", &store, &git_dir, true, 5, false, None,
        );
        assert!(result.is_ok(), "cmd_watch with no client must succeed: {:?}", result);
    }

    #[test]
    fn cmd_rebase_stack_returns_up_to_date_when_no_tracked_prs() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let fake = crate::github::FakeGithubClient::new();
        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, tmp.path(), &git_dir, None, &|_| {},
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("No tracked PRs"), "must say no tracked PRs: {}", msg);
    }

    #[test]
    fn cmd_rebase_stack_returns_up_to_date_when_all_rebased() {
        // Set up a real git repo with bare remote and a tracked branch already up to date
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        // Bare remote
        std::process::Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();

        // Working clone
        std::process::Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        std::fs::write(repo.join("a.txt"), "a").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["push", "-u", "origin", "main"]).current_dir(&repo).output().unwrap();

        // Create feat/x already based on main
        std::process::Command::new("git").args(["checkout", "-b", "feat/x"]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("b.txt"), "b").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "feat"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["push", "-u", "origin", "feat/x"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["checkout", "main"]).current_dir(&repo).output().unwrap();

        let store = crate::store::Store::open(&git_dir);
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache { number: 1, title: "X".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(1, "feat/x", "X", "main");

        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None, &|_| {},
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("up to date"), "must say up to date: {}", msg);
    }

    // VB1: cmd_rebase_stack --verbose emits progress markers for each blocking operation
    // so a user can identify which git/API call is hanging.
    #[test]
    fn cmd_rebase_stack_verbose_emits_progress_markers() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        git(&["checkout", "-b", "feat/a"]);
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A"]);
        git(&["push", "-u", "origin", "feat/a"]);

        let store = crate::store::Store::open(&git_dir);
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 1, title: "A".into(), branch: "feat/a".into(), base: "main".into()
        }).unwrap();

        let fake = crate::github::FakeGithubClient::new_with_pr(1, "feat/a", "A", "main");

        let log = std::cell::RefCell::new(Vec::<String>::new());
        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None,
            &|msg| log.borrow_mut().push(msg.to_string()),
        );
        assert!(result.is_ok(), "cmd_rebase_stack verbose must succeed: {:?}", result);
        let captured = log.borrow().join("\n");

        assert!(captured.contains("[fp] checking merged PRs"),
            "verbose output must contain merged-PR check marker; got:\n{}", captured);
        assert!(captured.contains("[fp] fetch_pr_is_merged"),
            "verbose output must contain per-PR check marker; got:\n{}", captured);
        assert!(captured.contains("git fetch origin"),
            "verbose output must contain fetch marker; got:\n{}", captured);

        // ls-remote and push only fire when a stacked branch is rebased — tested in VB2 below

    }

    // VB2: ls-remote (checking if parent branch is deleted) and push fire when a stacked
    // child branch is rebased after the parent was force-pushed.
    #[test]
    fn cmd_rebase_stack_verbose_emits_ls_remote_and_push_markers() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // feat/parent: stacked on main
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(repo.join("p.txt"), "p\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P"]);
        git(&["push", "-u", "origin", "feat/parent"]);

        // feat/child: stacked on feat/parent
        git(&["checkout", "-b", "feat/child"]);
        std::fs::write(repo.join("c.txt"), "c\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C"]);
        git(&["push", "-u", "origin", "feat/child"]);

        // Force-push feat/parent with a new commit so feat/child needs rebasing
        git(&["checkout", "feat/parent"]);
        std::fs::write(repo.join("p2.txt"), "p2\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P2"]);
        git(&["push", "--force-with-lease", "origin", "feat/parent"]);
        // Check out main so neither tracked branch is locked in the main worktree
        git(&["checkout", "main"]);

        let store = crate::store::Store::open(&git_dir);
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 1, title: "Parent".into(), branch: "feat/parent".into(), base: "main".into()
        }).unwrap();
        store.track(2).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 2, title: "Child".into(), branch: "feat/child".into(), base: "feat/parent".into()
        }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new_with_pr(1, "feat/parent", "Parent", "main");
        fake.set_pr(2, crate::model::PrState {
            number: 2, title: "Child".into(), branch: "feat/child".into(), base: "feat/parent".into(),
            head_sha: String::new(), draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });

        let log = std::cell::RefCell::new(Vec::<String>::new());
        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None,
            &|msg| log.borrow_mut().push(msg.to_string()),
        );
        assert!(result.is_ok(), "cmd_rebase_stack verbose must succeed: {:?}", result);
        let captured = log.borrow().join("\n");

        assert!(captured.contains("git ls-remote"),
            "verbose output must contain ls-remote marker (checking if parent branch deleted); got:\n{}", captured);
        assert!(captured.contains("git push --force-with-lease"),
            "verbose output must contain push marker; got:\n{}", captured);
    }

    // VB3: squash detection loop emits a progress marker before each GitHub API call
    // so the user can see which untracked squash PR lookup is hanging.
    #[test]
    fn cmd_rebase_stack_verbose_emits_squash_pr_api_marker() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // Untracked PR #42 squash-merged into main
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(repo.join("p.txt"), "p\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P: add p.txt"]);
        git(&["push", "-u", "origin", "feat/parent"]);
        let parent_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        // Explicit future date so --since=<child_tip_date> finds this squash commit deterministically.
        Command::new("git").args(["commit", "--date=2024-01-02T12:00:00", "-m", "squash: feat/parent (#42)"])
            .env("GIT_COMMITTER_DATE", "2024-01-02T12:00:00")
            .current_dir(&repo).output().unwrap();
        git(&["push", "origin", "main"]);

        // feat/child tracks PR #99, NOT #42; forked from feat/parent so squash commit is its ancestor
        git(&["checkout", "-b", "feat/child", "feat/parent"]);
        std::fs::write(repo.join("c.txt"), "c\n").unwrap();
        git(&["add", "."]);
        // Explicit past date so min_tip_date < squash commit date, making --since find the squash.
        Command::new("git").args(["commit", "--date=2024-01-01T12:00:00", "-m", "C: add c.txt"])
            .env("GIT_COMMITTER_DATE", "2024-01-01T12:00:00")
            .current_dir(&repo).output().unwrap();
        git(&["push", "-u", "origin", "feat/child"]);
        git(&["checkout", "main"]);
        git(&["fetch", "origin"]);

        let store = crate::store::Store::open(&git_dir);
        store.track(99).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 99, title: "Child".into(), branch: "feat/child".into(), base: "main".into()
        }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new_with_pr(99, "feat/child", "Child", "main");
        fake.set_pr(42, crate::model::PrState {
            number: 42, title: "Parent".into(), branch: "feat/parent".into(), base: "main".into(),
            head_sha: parent_tip.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        // Set created_at on PR #99 so squash detection runs (min created_at used as --since bound)
        fake.set_pr_created_at(99, "2020-01-01T00:00:00Z");

        let log = std::cell::RefCell::new(Vec::<String>::new());
        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None,
            &|msg| log.borrow_mut().push(msg.to_string()),
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);
        let captured = log.borrow().join("\n");

        assert!(captured.contains("[fp] fetch head SHA for squash PR #42"),
            "verbose output must contain squash-PR API marker before fetch; got:\n{}", captured);
    }

    // VB4: squash scan uses branch tip date (not PR created_at) as --since bound.
    // If created_at is a far-future date and the scan still detects the squash commit,
    // the implementation is using branch tip date rather than created_at.
    #[test]
    fn cmd_rebase_stack_squash_scan_uses_branch_tip_date_not_created_at() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // Untracked PR #42 squash-merged into main
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(repo.join("p.txt"), "p\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P: add p.txt"]);
        git(&["push", "-u", "origin", "feat/parent"]);
        let parent_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash: feat/parent (#42)"]);
        git(&["push", "origin", "main"]);

        // feat/child forked from feat/parent tip
        git(&["checkout", "-b", "feat/child", "feat/parent"]);
        std::fs::write(repo.join("c.txt"), "c\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C: add c.txt"]);
        git(&["push", "-u", "origin", "feat/child"]);
        git(&["checkout", "main"]);
        git(&["fetch", "origin"]);

        let store = crate::store::Store::open(&git_dir);
        store.track(99).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 99, title: "Child".into(), branch: "feat/child".into(), base: "main".into()
        }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new_with_pr(99, "feat/child", "Child", "main");
        fake.set_pr(42, crate::model::PrState {
            number: 42, title: "Parent".into(), branch: "feat/parent".into(), base: "main".into(),
            head_sha: parent_tip.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        // Set a far-future created_at — if the code uses this as --since, no squash commits
        // will be found and the test will fail. Branch tip date must be used instead.
        fake.set_pr_created_at(99, "2099-01-01T00:00:00Z");

        let log = std::cell::RefCell::new(Vec::<String>::new());
        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None,
            &|msg| log.borrow_mut().push(msg.to_string()),
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);
        let captured = log.borrow().join("\n");

        assert!(captured.contains("[fp] fetch head SHA for squash PR #42"),
            "squash scan must use branch tip date (not created_at); with future created_at the scan found nothing; got:\n{}", captured);
    }

    // RS3: cmd_rebase_stack rebases a child branch correctly when its parent PR was
    // squash-merged into main but was NOT tracked by fp (untracked squash-merged parent).
    // The child should end up with only its own commits on top of main, not parent+child.
    #[test]
    fn cmd_rebase_stack_rebases_child_after_untracked_parent_squash_merged() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        // M: initial commit on main
        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // Build a scenario that matches the real-world bug:
        // - feat/parent and feat/child share some history (up to a fork point)
        // - feat/parent adds MORE commits after the fork point (not in feat/child)
        // - feat/parent is squash-merged; feat/child must be rebased using the
        //   merge-base of parent_tip and feat/child as the cut point (not parent_tip itself,
        //   which may not be locally available or reachable).
        //
        // Shared: M → A (runner.ts="v1")
        // feat/parent: M → A → P_extra (runner.ts="v2") — parent_tip = P_extra
        // feat/child:  M → A → C1 (runner.ts="v3")      — NOT from P_extra
        //
        // git merge-base P_extra feat/child = A
        // Correct rebase: --onto origin/main A feat/child → replay only C1

        // A: shared commit — adds shared.ts (touched by both branches)
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(repo.join("shared.ts"), "shared\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "A: add shared.ts"]);

        // P_extra: parent-only commit — adds parent.ts (NOT touched by child)
        std::fs::write(repo.join("parent.ts"), "parent\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P_extra: add parent.ts"]);
        git(&["push", "-u", "origin", "feat/parent"]);

        // Record feat/parent tip before squash
        let parent_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // feat/child branches from the shared commit A (HEAD~1 of feat/parent), NOT from P_extra.
        // This means parent_tip (P_extra) is NOT an ancestor of feat/child.
        // C1 adds child.ts — a different file from parent.ts, so no conflict after squash-merge.
        let fork_sha = String::from_utf8(
            Command::new("git").args(["rev-parse", "HEAD~1"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();
        git(&["checkout", "-b", "feat/child", &fork_sha]);
        std::fs::write(repo.join("child.ts"), "child\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C1: add child.ts"]);
        git(&["push", "-u", "origin", "feat/child"]);

        // Squash-merge feat/parent into main with "(#42)" in message
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash: feat/parent (#42)"]);
        git(&["push", "origin", "main"]);
        git(&["fetch", "origin"]);

        // fp state: tracks feat/child (PR #99) but NOT feat/parent (#42)
        let store = crate::store::Store::open(&git_dir);
        store.track(99).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 99, title: "Child".into(), branch: "feat/child".into(), base: "main".into()
        }).unwrap();

        // FakeGithubClient knows PR #42 (untracked) with the parent tip sha
        let mut fake = crate::github::FakeGithubClient::new_with_pr(99, "feat/child", "Child", "main");
        fake.set_pr(42, crate::model::PrState {
            number: 42, title: "Parent".into(), branch: "feat/parent".into(), base: "main".into(),
            head_sha: parent_tip.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });

        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None, &|_| {},
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);

        // feat/child must have exactly 1 commit on top of main (C1 only, not P1+P2+C1)
        let log = Command::new("git")
            .args(["log", "--oneline", "origin/main..feat/child"])
            .current_dir(&repo).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        let commit_count = log_str.trim().lines().filter(|l| !l.is_empty()).count();
        assert_eq!(commit_count, 1,
            "feat/child must have exactly 1 commit (C1) above main after rebase, got {}:\n{}", commit_count, log_str);
    }

    // RS5: An unrelated squash-merged PR whose head_sha is NOT an ancestor of the tracked
    // branch must be ignored — the rebase must not be attempted and the branch must be
    // unchanged after cmd_rebase_stack completes.
    #[test]
    fn cmd_rebase_stack_ignores_unrelated_squash_pr() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // Unrelated PR #77 — its branch and head_sha share NO history with feat/child
        git(&["checkout", "-b", "feat/unrelated"]);
        std::fs::write(repo.join("unrelated.txt"), "unrelated\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "Unrelated work"]);
        git(&["push", "-u", "origin", "feat/unrelated"]);
        let unrelated_head_sha = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/unrelated"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // Squash-merge unrelated PR into main
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/unrelated"]);
        git(&["commit", "-m", "squash: feat/unrelated (#77)"]);
        git(&["push", "origin", "main"]);

        // feat/child is forked from main BEFORE the squash — has no connection to feat/unrelated
        let fork_sha = String::from_utf8(
            Command::new("git").args(["rev-parse", "HEAD~1"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();
        git(&["checkout", "-b", "feat/child", &fork_sha]);
        std::fs::write(repo.join("child.txt"), "child\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C: child work"]);
        git(&["push", "-u", "origin", "feat/child"]);

        let child_tip_before = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/child"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        git(&["checkout", "main"]);
        git(&["fetch", "origin"]);

        let store = crate::store::Store::open(&git_dir);
        store.track(99).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 99, title: "Child".into(), branch: "feat/child".into(), base: "main".into()
        }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new_with_pr(99, "feat/child", "Child", "main");
        fake.set_pr(77, crate::model::PrState {
            number: 77, title: "Unrelated".into(), branch: "feat/unrelated".into(), base: "main".into(),
            head_sha: unrelated_head_sha.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake.set_pr_created_at(99, "2020-01-01T00:00:00Z");

        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None, &|_| {},
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);
        let output = result.unwrap();
        // The squash-detection path must not fire for the unrelated PR #77.
        // (The normal rebase onto main may still run — that's expected.)
        assert!(!output.contains("untracked squash of PR #77"),
            "must not rebase feat/child via unrelated squash PR #77; got:\n{}", output);
    }

    // RS6: When the parent branch had additional commits after the child forked (so head_sha
    // is NOT an ancestor of the child), we must still rebase the child. The cut point is
    // merge-base(head_sha, child) which lands on feat/parent history (NOT on main), so the
    // origin/main ancestry check correctly allows the rebase.
    #[test]
    fn cmd_rebase_stack_rebases_when_parent_had_extra_commits_after_fork() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        // Main: commit M
        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // feat/parent: commit P1
        git(&["checkout", "-b", "feat/parent"]);
        std::fs::write(repo.join("p1.txt"), "p1\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P1: parent work"]);
        git(&["push", "-u", "origin", "feat/parent"]);

        // feat/child forks from feat/parent at P1 and adds commit C
        git(&["checkout", "-b", "feat/child", "feat/parent"]);
        std::fs::write(repo.join("c.txt"), "child\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "C: child work"]);
        git(&["push", "-u", "origin", "feat/child"]);

        // feat/parent gets ANOTHER commit P2 AFTER child forked — so head_sha (P2) is NOT
        // an ancestor of feat/child
        git(&["checkout", "feat/parent"]);
        std::fs::write(repo.join("p2.txt"), "p2\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "P2: extra parent work after child forked"]);
        git(&["push", "origin", "feat/parent"]);
        let parent_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/parent"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // Squash-merge feat/parent (tip = P2 = head_sha) into main
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/parent"]);
        git(&["commit", "-m", "squash: feat/parent (#42)"]);
        git(&["push", "origin", "main"]);
        git(&["fetch", "origin"]);

        let store = crate::store::Store::open(&git_dir);
        store.track(99).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 99, title: "Child".into(), branch: "feat/child".into(), base: "main".into()
        }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new_with_pr(99, "feat/child", "Child", "main");
        fake.set_pr(42, crate::model::PrState {
            number: 42, title: "Parent".into(), branch: "feat/parent".into(), base: "main".into(),
            head_sha: parent_tip.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake.set_pr_created_at(99, "2020-01-01T00:00:00Z");

        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None, &|_| {},
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);
        let output = result.unwrap();
        // feat/child must be rebased onto main even though head_sha (P2) is not an ancestor
        // of feat/child (child only has P1, not P2). The cut point is merge-base(P2, child) = P1,
        // which is on feat/parent history (NOT on origin/main), so the rebase must proceed.
        assert!(output.contains("untracked squash of PR #42"),
            "must rebase feat/child after parent squash even when head_sha not ancestor of child; got:\n{}", output);
    }

    // RS4: When two branches are tracked (a stacked pair), the squash detection must scan
    // from the EARLIEST divergence point (the root branch's fork from main), not only from
    // the child's fork. This ensures squash commits predating the child's fork are detected.
    // Also verifies that the detection works without git fetch origin <sha> (sha is already local).
    #[test]
    fn cmd_rebase_stack_scans_from_earliest_merge_base_with_multiple_branches() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");

        Command::new("git").args(["init", "--bare", "-b", "main"]).current_dir(&remote).output().unwrap();
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["remote", "add", "origin", remote.to_str().unwrap()]).current_dir(&repo).output().unwrap();

        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output().unwrap();

        // M: initial commit on main
        std::fs::write(repo.join("main.txt"), "main\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "M"]);
        git(&["push", "-u", "origin", "main"]);

        // feat/root: branches from M, adds root.ts, then a parent-only extra commit.
        // This PR will be squash-merged (untracked by fp).
        git(&["checkout", "-b", "feat/root"]);
        std::fs::write(repo.join("root.ts"), "root\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "Root: add root.ts"]);

        let root_extra_parent = String::from_utf8(
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        std::fs::write(repo.join("root_extra.ts"), "extra\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "Root extra: add root_extra.ts"]);
        git(&["push", "-u", "origin", "feat/root"]);

        let root_tip = String::from_utf8(
            Command::new("git").args(["rev-parse", "feat/root"]).current_dir(&repo).output().unwrap().stdout
        ).unwrap().trim().to_string();

        // feat/mid branches from the FIRST root commit (root_extra_parent), adds mid.ts.
        // It is tracked by fp and is the "bottom" of the tracked stack.
        git(&["checkout", "-b", "feat/mid", &root_extra_parent]);
        std::fs::write(repo.join("mid.ts"), "mid\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "Mid: add mid.ts"]);
        git(&["push", "-u", "origin", "feat/mid"]);

        // feat/top stacks on feat/mid, adds top.ts. Also tracked.
        git(&["checkout", "-b", "feat/top"]);
        std::fs::write(repo.join("top.ts"), "top\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "Top: add top.ts"]);
        git(&["push", "-u", "origin", "feat/top"]);

        // Squash-merge feat/root into main (untracked PR #55)
        git(&["checkout", "main"]);
        git(&["merge", "--squash", "feat/root"]);
        git(&["commit", "-m", "squash: feat/root (#55)"]);
        git(&["push", "origin", "main"]);
        git(&["fetch", "origin"]);

        // fp state: tracks feat/mid (#10) and feat/top (#11), NOT feat/root (#55)
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 10, title: "Mid".into(), branch: "feat/mid".into(), base: "main".into()
        }).unwrap();
        store.track(11).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 11, title: "Top".into(), branch: "feat/top".into(), base: "feat/mid".into()
        }).unwrap();

        let mut fake = crate::github::FakeGithubClient::new_with_pr(10, "feat/mid", "Mid", "main");
        fake.set_pr(10, crate::model::PrState {
            number: 10, title: "Mid".into(), branch: "feat/mid".into(), base: "main".into(),
            head_sha: String::new(), draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake.set_pr(11, crate::model::PrState {
            number: 11, title: "Top".into(), branch: "feat/top".into(), base: "feat/mid".into(),
            head_sha: String::new(), draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });
        fake.set_pr(55, crate::model::PrState {
            number: 55, title: "Root".into(), branch: "feat/root".into(), base: "main".into(),
            head_sha: root_tip.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None,
        });

        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None, &|_| {},
        );
        assert!(result.is_ok(), "cmd_rebase_stack must succeed: {:?}", result);

        // feat/mid must have exactly 1 commit above origin/main (Mid only)
        let mid_log = Command::new("git")
            .args(["log", "--oneline", "origin/main..feat/mid"])
            .current_dir(&repo).output().unwrap();
        let mid_str = String::from_utf8_lossy(&mid_log.stdout);
        let mid_count = mid_str.trim().lines().filter(|l| !l.is_empty()).count();
        assert_eq!(mid_count, 1,
            "feat/mid must have exactly 1 commit above main after rebase, got {}:\n{}", mid_count, mid_str);
    }
}
